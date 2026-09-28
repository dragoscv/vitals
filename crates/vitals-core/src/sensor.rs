//! Hardware sensors.
//!
//! Kept generic rather than modelled per component, because the set of
//! sensors a machine exposes is wildly variable and unknowable ahead of time.
//! A rigid schema would silently drop the sensors that make a given
//! motherboard interesting.

use crate::ids::SensorId;
use crate::units::{Celsius, Hertz, Percent, Volts, Watts};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(rename_all = "camelCase")]
pub enum SensorKind {
    Temperature,
    Fan,
    Voltage,
    Power,
    Clock,
    Load,
    Current,
    Flow,
    Level,
    Throughput,
}

/// A reading, typed by unit so the UI can format and chart it correctly
/// without a lookup table keyed on sensor name.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(tag = "unit", content = "value", rename_all = "camelCase")]
pub enum SensorReading {
    Celsius(Celsius),
    Rpm(u32),
    Volts(Volts),
    Watts(Watts),
    Hertz(Hertz),
    Percent(Percent),
    Amps(f32),
    LitresPerHour(f32),
    Raw(f32),
}

impl SensorReading {
    /// The underlying scalar, for charting and threshold comparison.
    #[must_use]
    pub fn as_f32(self) -> f32 {
        match self {
            Self::Celsius(v) => v.get(),
            Self::Rpm(v) => v as f32,
            Self::Volts(v) => v.get(),
            Self::Watts(v) => v.get(),
            Self::Hertz(v) => v.get() as f32,
            Self::Percent(v) => v.get(),
            Self::Amps(v) | Self::LitresPerHour(v) | Self::Raw(v) => v,
        }
    }
}

/// Where a reading came from, so the UI can show provenance.
///
/// This matters more than it looks: readings from different sources disagree,
/// sometimes by 10 °C. Showing the user which backend produced a number is
/// how they resolve "why does this say something different to `HWiNFO`".
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(rename_all = "camelCase")]
pub enum SensorSource {
    /// OS-provided (WMI, `MSAcpi`, sysfs, `IOKit`).
    OperatingSystem,
    /// Vendor SDK (NVAPI, ADL, IGCL).
    VendorSdk,
    /// The optional `LibreHardwareMonitor` sidecar.
    Sidecar,
    /// Direct hardware access via a driver.
    Driver,
    /// A third-party plugin.
    Plugin,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(rename_all = "camelCase")]
pub struct Sensor {
    pub id: SensorId,
    /// Display name, e.g. "CPU Package".
    pub name: String,
    /// Owning hardware, e.g. "AMD Ryzen 9 7950X".
    pub hardware: String,
    pub kind: SensorKind,
    pub source: SensorSource,
    pub value: SensorReading,
    pub min: Option<SensorReading>,
    pub max: Option<SensorReading>,
    /// Manufacturer-critical threshold, when known.
    pub critical: Option<f32>,
    /// Whether the user can write to this (fan curves, power limits).
    pub writable: bool,
}

impl Sensor {
    /// Whether the reading is at or past the critical threshold.
    #[must_use]
    pub fn is_critical(&self) -> bool {
        self.critical
            .is_some_and(|limit| self.value.as_f32() >= limit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sensor(value: SensorReading, critical: Option<f32>) -> Sensor {
        Sensor {
            id: SensorId("cpu/temp/0".into()),
            name: "CPU Package".into(),
            hardware: "Test CPU".into(),
            kind: SensorKind::Temperature,
            source: SensorSource::OperatingSystem,
            value,
            min: None,
            max: None,
            critical,
            writable: false,
        }
    }

    #[test]
    fn reading_exposes_scalar_regardless_of_unit() {
        // Both values are exactly representable in f32, so an exact
        // comparison is meaningful here; an epsilon would hide a real bug.
        assert!((SensorReading::Celsius(Celsius(72.5)).as_f32() - 72.5).abs() < f32::EPSILON);
        assert!((SensorReading::Rpm(1200).as_f32() - 1200.0).abs() < f32::EPSILON);
    }

    #[test]
    fn critical_requires_a_known_threshold() {
        let s = sensor(SensorReading::Celsius(Celsius(105.0)), None);
        assert!(
            !s.is_critical(),
            "no threshold means no claim, not a false alarm"
        );
    }

    #[test]
    fn critical_triggers_at_and_above_threshold() {
        assert!(sensor(SensorReading::Celsius(Celsius(100.0)), Some(100.0)).is_critical());
        assert!(sensor(SensorReading::Celsius(Celsius(101.0)), Some(100.0)).is_critical());
        assert!(!sensor(SensorReading::Celsius(Celsius(99.0)), Some(100.0)).is_critical());
    }
}
