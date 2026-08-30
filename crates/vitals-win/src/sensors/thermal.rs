//! ACPI thermal zones.
//!
//! # What a thermal zone actually is
//!
//! Not a CPU temperature. An ACPI thermal zone is a firmware-defined region
//! with a trip point attached, and the sensor behind it is chosen by the
//! board vendor. On desktops it is usually the PCH; on laptops it is often a
//! skin-temperature sensor near the palm rest, which is the number the fan
//! curve cares about and has no fixed relationship to core temperature.
//!
//! It is therefore labelled as a thermal zone in the UI, never as "CPU". A
//! zone reading 45 °C while the cores are at 85 °C is not a bug — it is two
//! different sensors — but presenting the former as the latter would send a
//! user shopping for a cooler they do not need.
//!
//! # How it is read, and the two traps
//!
//! The data source is `MSAcpi_ThermalZoneTemperature` in the `root\WMI`
//! namespace, backed by the ACPI driver.
//!
//! 1. **It is access-controlled.** The `root\WMI` namespace grants
//!    `MSAcpi_ThermalZoneTemperature` only to administrators on most builds.
//!    Unelevated, the query returns `WBEM_E_ACCESS_DENIED` (`0x8004_1003`) —
//!    *not* an empty result set. Conflating the two would report "this
//!    machine has no thermal sensors" to every non-admin user, which is both
//!    wrong and unfixable from the user's side. The distinction is preserved
//!    all the way to [`ThermalAvailability`].
//! 2. **Many boards expose no zones at all.** A desktop whose firmware
//!    delegates thermal management entirely to the Super-I/O chip has zero
//!    instances even when elevated. That is genuinely "no data", and is
//!    reported as such rather than as a failure.
//!
//! # Why not WMI from Rust directly
//!
//! Binding COM/WMI pulls in `Win32_System_Wmi` plus the whole `IWbem*`
//! surface, and every call is tens of milliseconds — the entire system
//! sample budget is 30 ms. The reader here is therefore built around an
//! explicit TTL cache and a separate cadence; see
//! [`super::ThermalReader::sample_interval_hint`].

use vitals_core::capability::Capability;
use vitals_core::error::{Error, Result};
use vitals_core::units::Celsius;

use super::convert::decikelvin_to_celsius;
use super::reading::{Quality, SensorReading, SensorSource, SensorValue};

/// Why thermal data is or is not present.
///
/// Three states, not two. "Denied" and "none present" both yield an empty
/// list, but only one of them is fixable by elevating, and the UI must say
/// which.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThermalAvailability {
    /// Zones were read.
    Available,
    /// The namespace refused the query. Elevation would fix it.
    AccessDenied,
    /// The query succeeded and returned nothing: this firmware exposes no
    /// zones. Elevation would not help.
    NoZonesPresent,
    /// The ACPI WMI provider is not registered at all.
    ProviderMissing,
}

impl ThermalAvailability {
    /// Whether elevating could plausibly change the answer.
    #[must_use]
    pub const fn is_elevation_fixable(self) -> bool {
        matches!(self, Self::AccessDenied)
    }
}

/// One ACPI thermal zone.
#[derive(Debug, Clone)]
pub struct ThermalZone {
    /// ACPI instance name, e.g. `ACPI\ThermalZone\TZ00_0`.
    ///
    /// Opaque, board-specific, and the only stable identity a zone has.
    pub instance: String,
    pub temperature: Celsius,
    /// Temperature at which the firmware will force a shutdown, when the
    /// zone declares one.
    pub critical: Option<Celsius>,
    /// Whether the zone is under active (fan) rather than passive
    /// (throttling) control.
    pub active_cooling: Option<bool>,
}

impl ThermalZone {
    /// Renders this zone as a generic reading for the sensors list.
    #[must_use]
    pub fn as_reading(&self, index: usize) -> SensorReading {
        SensorReading::new(
            format!("acpi.tz.{index}"),
            format!("Thermal zone {index}"),
            SensorValue::Temperature(self.temperature),
            SensorSource::AcpiThermalZone,
            Quality::Measured,
        )
    }

    /// Headroom to the critical trip point, when one is declared.
    #[must_use]
    pub fn headroom(&self) -> Option<Celsius> {
        self.critical
            .map(|crit| Celsius(crit.0 - self.temperature.0))
    }
}

/// The outcome of one thermal scan.
#[derive(Debug, Clone)]
pub struct ThermalScan {
    pub zones: Vec<ThermalZone>,
    pub availability: ThermalAvailability,
}

impl ThermalScan {
    /// An empty scan carrying a reason.
    #[must_use]
    pub const fn unavailable(reason: ThermalAvailability) -> Self {
        Self {
            zones: Vec::new(),
            availability: reason,
        }
    }

    /// The zones as generic sensor readings.
    #[must_use]
    pub fn readings(&self) -> Vec<SensorReading> {
        self.zones
            .iter()
            .enumerate()
            .map(|(i, z)| z.as_reading(i))
            .collect()
    }

    /// The hottest zone, for a summary tile.
    ///
    /// `None` on an empty scan rather than a zero, because 0 °C rendered
    /// beside a fan curve reads as a working sensor on a very cold machine.
    #[must_use]
    pub fn hottest(&self) -> Option<&ThermalZone> {
        self.zones.iter().max_by(|a, b| {
            a.temperature
                .0
                .partial_cmp(&b.temperature.0)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    }

    /// # Errors
    ///
    /// [`Error::Unsupported`] when no zone could be read, so a caller that
    /// wants a hard failure rather than an empty list gets one that names
    /// [`Capability::Thermals`].
    pub fn require(&self) -> Result<&[ThermalZone]> {
        if self.zones.is_empty() {
            return Err(Error::Unsupported(Capability::Thermals));
        }
        Ok(&self.zones)
    }
}

/// A raw zone as the WMI provider reports it, before validation.
///
/// Separated from [`ThermalZone`] so the parsing rules can be tested without
/// a WMI connection — which is the only way to test them at all on a machine
/// whose firmware exposes no zones.
#[derive(Debug, Clone, Copy)]
pub struct RawZone {
    /// `CurrentTemperature`, in tenths of a Kelvin.
    pub current_decikelvin: u32,
    /// `CriticalTripPoint`, in tenths of a Kelvin. Zero when undeclared.
    pub critical_decikelvin: u32,
    /// `ActiveCount` > 0, meaning fan control is available for this zone.
    pub has_active_cooling: bool,
}

/// Validates and converts one raw zone.
///
/// Returns `None` when the current temperature is outside the plausible
/// band; a zone whose primary reading is a firmware placeholder has nothing
/// worth showing, even if its trip point parses.
#[must_use]
pub fn parse_zone(instance: &str, raw: RawZone) -> Option<ThermalZone> {
    let temperature = decikelvin_to_celsius(raw.current_decikelvin)?;

    Some(ThermalZone {
        instance: instance.to_owned(),
        temperature,
        // An undeclared trip point is zero, which converts to −273 °C. It
        // must be rejected by the same plausibility band rather than
        // rendered as an alarmingly cold shutdown threshold.
        critical: decikelvin_to_celsius(raw.critical_decikelvin),
        active_cooling: Some(raw.has_active_cooling),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_normal_zone_parses() {
        let zone = parse_zone(
            "ACPI\\ThermalZone\\TZ00_0",
            RawZone {
                current_decikelvin: 3182,
                critical_decikelvin: 3732,
                has_active_cooling: true,
            },
        )
        .expect("plausible");

        assert!((zone.temperature.0 - 45.05).abs() < 0.01);
        assert!((zone.critical.expect("declared").0 - 100.05).abs() < 0.01);
        assert_eq!(zone.active_cooling, Some(true));
    }

    #[test]
    fn an_undeclared_trip_point_is_none_not_minus_273() {
        let zone = parse_zone(
            "TZ01",
            RawZone {
                current_decikelvin: 3100,
                critical_decikelvin: 0,
                has_active_cooling: false,
            },
        )
        .expect("plausible");

        assert_eq!(zone.critical, None);
    }

    #[test]
    fn a_placeholder_zone_is_dropped_entirely() {
        assert!(
            parse_zone(
                "TZ02",
                RawZone {
                    current_decikelvin: 0,
                    critical_decikelvin: 3732,
                    has_active_cooling: false,
                },
            )
            .is_none()
        );
    }

    #[test]
    fn headroom_is_the_gap_to_the_trip_point() {
        let zone = parse_zone(
            "TZ00",
            RawZone {
                current_decikelvin: 3182,
                critical_decikelvin: 3732,
                has_active_cooling: true,
            },
        )
        .expect("plausible");

        assert!((zone.headroom().expect("declared").0 - 55.0).abs() < 0.01);
    }

    #[test]
    fn access_denied_is_distinguishable_from_no_hardware() {
        assert!(ThermalAvailability::AccessDenied.is_elevation_fixable());
        assert!(!ThermalAvailability::NoZonesPresent.is_elevation_fixable());
        assert!(!ThermalAvailability::ProviderMissing.is_elevation_fixable());
    }

    #[test]
    fn an_empty_scan_reports_unsupported_rather_than_zero() {
        let scan = ThermalScan::unavailable(ThermalAvailability::NoZonesPresent);
        assert!(scan.hottest().is_none());
        assert!(matches!(
            scan.require(),
            Err(Error::Unsupported(Capability::Thermals))
        ));
    }

    #[test]
    fn hottest_picks_the_maximum() {
        let scan = ThermalScan {
            zones: vec![
                parse_zone(
                    "a",
                    RawZone {
                        current_decikelvin: 3100,
                        critical_decikelvin: 0,
                        has_active_cooling: false,
                    },
                )
                .expect("plausible"),
                parse_zone(
                    "b",
                    RawZone {
                        current_decikelvin: 3400,
                        critical_decikelvin: 0,
                        has_active_cooling: false,
                    },
                )
                .expect("plausible"),
            ],
            availability: ThermalAvailability::Available,
        };

        assert_eq!(scan.hottest().expect("non-empty").instance, "b");
        assert_eq!(scan.readings().len(), 2);
    }
}
