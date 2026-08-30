//! The sensor reading model.
//!
//! One shape for every physical quantity, because the Thermals tab and the
//! "Devices & Sensors" section both render a heterogeneous list: a chipset
//! temperature next to a battery charge rate next to a pack voltage. Giving
//! each quantity its own struct would push the union back into the UI.

use vitals_core::units::{Celsius, Percent, Rpm, Volts, Watts};

/// A measured physical quantity, tagged with its unit.
///
/// The unit lives in the enum rather than in a separate field so it is
/// impossible to construct a reading whose declared unit disagrees with its
/// value — the failure that turns a 42 °C chipset into a 42 W chipset
/// somewhere in the render path.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SensorValue {
    Temperature(Celsius),
    Power(Watts),
    Voltage(Volts),
    FanSpeed(Rpm),
    Charge(Percent),
}

impl SensorValue {
    /// The unit suffix, for a UI that formats readings generically.
    #[must_use]
    pub const fn unit(self) -> &'static str {
        match self {
            Self::Temperature(_) => "°C",
            Self::Power(_) => "W",
            Self::Voltage(_) => "V",
            Self::FanSpeed(_) => "RPM",
            Self::Charge(_) => "%",
        }
    }

    /// The raw magnitude, for charting.
    ///
    /// Deliberately *not* `Into<f32>`: an implicit conversion would let a
    /// caller mix a temperature and a wattage on one axis without noticing.
    #[must_use]
    pub const fn magnitude(self) -> f32 {
        match self {
            Self::Temperature(v) => v.0,
            Self::Power(v) => v.0,
            Self::Voltage(v) => v.0,
            Self::FanSpeed(v) => v.0 as f32,
            Self::Charge(v) => v.get(),
        }
    }
}

/// Where a reading came from.
///
/// Surfaced in the UI because provenance decides how much a number is worth.
/// An ACPI thermal zone is a firmware-defined abstraction that may describe
/// the chipset, the CPU package, or nothing in particular; a battery IOCTL
/// reads a gauge IC directly. Presenting both as an unqualified "temperature"
/// invites the user to trust them equally.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SensorSource {
    /// ACPI thermal zone, read through WMI `MSAcpi_ThermalZoneTemperature`.
    AcpiThermalZone,
    /// Battery miniport, read through `IOCTL_BATTERY_QUERY_*`.
    BatteryMiniport,
    /// `GetSystemPowerStatus` — the OS's coarse, cached view.
    SystemPowerStatus,
    /// A signed kernel driver talking to a Super-I/O or vendor bus.
    ///
    /// Never produced today; see [`crate::sensors::driver`].
    KernelDriver,
}

impl SensorSource {
    /// A short human label for the provenance column.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::AcpiThermalZone => "ACPI thermal zone",
            Self::BatteryMiniport => "battery miniport",
            Self::SystemPowerStatus => "OS power status",
            Self::KernelDriver => "kernel driver",
        }
    }
}

/// How much the number can be trusted.
///
/// The distinction exists because a derived figure and a measured one look
/// identical once rendered, and users make hardware decisions on the
/// difference. Battery *health*, for instance, is a ratio of two firmware
/// constants, not an instantaneous measurement of anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quality {
    /// Read from a sensor, unmodified beyond unit conversion.
    Measured,
    /// Computed from other measured values.
    Derived,
    /// Reported by firmware as a fixed characteristic, not sampled.
    ///
    /// Design capacity is the canonical case: it never changes and describes
    /// the pack as manufactured, not as it is now.
    Nameplate,
}

/// One reading from one sensor.
#[derive(Debug, Clone, PartialEq)]
pub struct SensorReading {
    /// Human label, e.g. `Thermal zone 0` or `Battery charge rate`.
    pub label: String,
    /// Stable identifier for charting across samples, e.g. `acpi.tz.0`.
    ///
    /// Distinct from `label`, which is localisable and may change; a chart
    /// keyed on the label would restart its history on a language switch.
    pub key: String,
    pub value: SensorValue,
    pub source: SensorSource,
    pub quality: Quality,
}

impl SensorReading {
    /// Constructs a reading.
    #[must_use]
    pub fn new(
        key: impl Into<String>,
        label: impl Into<String>,
        value: SensorValue,
        source: SensorSource,
        quality: Quality,
    ) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            value,
            source,
            quality,
        }
    }

    /// Whether this reading was measured rather than computed or nameplate.
    #[must_use]
    pub const fn is_measured(&self) -> bool {
        matches!(self.quality, Quality::Measured)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_matches_the_variant() {
        assert_eq!(SensorValue::Temperature(Celsius(40.0)).unit(), "°C");
        assert_eq!(SensorValue::Power(Watts(12.5)).unit(), "W");
        assert_eq!(SensorValue::FanSpeed(Rpm(900)).unit(), "RPM");
        assert_eq!(SensorValue::Voltage(Volts(12.1)).unit(), "V");
        assert_eq!(SensorValue::Charge(Percent::new(50.0)).unit(), "%");
    }

    #[test]
    fn magnitude_preserves_the_value() {
        assert!((SensorValue::Temperature(Celsius(41.5)).magnitude() - 41.5).abs() < f32::EPSILON);
        assert!((SensorValue::FanSpeed(Rpm(1200)).magnitude() - 1200.0).abs() < f32::EPSILON);
    }

    #[test]
    fn nameplate_is_not_measured() {
        let reading = SensorReading::new(
            "battery.design",
            "Design capacity",
            SensorValue::Power(Watts(50.0)),
            SensorSource::BatteryMiniport,
            Quality::Nameplate,
        );
        assert!(!reading.is_measured());
    }
}
