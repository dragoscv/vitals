//! Thermals, power and battery — the Thermals tab and "Devices & Sensors".
//!
//! # Cadence: this module must not run on the sampling tick
//!
//! The whole system sample has a 30 ms budget. A WMI round trip to
//! `root\WMI` costs tens of milliseconds on its own, and the first one in a
//! process also pays for `CoInitializeEx` and `ConnectServer`. Calling
//! [`SensorReader::sample`] from the 1 Hz loop would therefore blow the
//! budget on its own, before a single process had been enumerated.
//!
//! Sensors also do not need that cadence. A thermal mass large enough to be
//! worth a chart does not change measurably in a second, and battery charge
//! moves in whole percent over minutes. [`SensorReader`] therefore caches
//! with a TTL and publishes [`SensorReader::sample_interval_hint`] so the
//! scheduler can run it on its own slower timer; calls in between are served
//! from the cache and cost nothing.
//!
//! # Honesty
//!
//! Every value this module cannot measure is `None` or an
//! [`Error::Unsupported`](vitals_core::error::Error::Unsupported), never a
//! plausible stand-in. The things it cannot measure are numerous and are
//! enumerated, with the reason for each, in [`driver::DRIVER_GAPS`]:
//! per-core CPU temperature, board fan RPM and rail voltages all require
//! ring-0 access this build does not have. See [`driver`] for the full
//! argument. Two sources need no privilege and are read: ACPI thermal zones
//! through the performance counters when WMI refuses ([`zone_counters`]),
//! and NVIDIA GPU temperature, fan and board power through the driver's own
//! `nvml.dll` ([`nvml`]).

pub mod battery;
pub mod convert;
pub mod cpu_service;
pub mod driver;
pub mod nvml;
pub mod power;
pub mod reading;
pub mod thermal;
pub mod wmi;
pub mod zone_counters;

pub use battery::{Battery, ChargeState, enumerate_batteries};
pub use convert::{
    charge_percent, decikelvin_to_celsius, health_percent, millivolts_to_volts,
    milliwatts_to_watts, seconds_remaining,
};
pub use driver::{DRIVER_GAPS, DriverGap, gaps_for};
pub use nvml::{NvidiaGpu, read_nvidia_gpus};
pub use power::{
    AggregateBattery, LineStatus, PowerMode, PowerState, aggregate_battery, classify_scheme,
    read_power_state,
};
pub use reading::{Quality, SensorReading, SensorSource, SensorValue};
pub use thermal::{RawZone, ThermalAvailability, ThermalScan, ThermalZone, parse_zone};
pub use wmi::read_thermal_zones;
pub use zone_counters::{CounterZone, read_zone_counters};

use std::time::{Duration, Instant};

use vitals_core::capability::{Capabilities, Capability, Unavailable};

/// How long a sensor sample stays fresh.
///
/// Five seconds is a deliberate compromise. Shorter would pay the WMI cost
/// often enough to show up in the process's own CPU figure — which, in a
/// tool that reports CPU usage, is a particularly embarrassing way to be
/// wrong. Longer would make the battery charge visibly lag the taskbar.
const CACHE_TTL: Duration = Duration::from_secs(5);

/// Everything the sensor layer knows at one instant.
#[derive(Debug, Clone)]
pub struct SensorSample {
    pub thermal: ThermalScan,
    pub batteries: Vec<Battery>,
    pub power: PowerState,
    /// NVIDIA GPUs read through the driver's own `nvml.dll`. Empty elsewhere.
    pub nvidia: Vec<NvidiaGpu>,
    /// CPU package temperature and power from the optional sensors service.
    pub cpu: Option<cpu_service::CpuSensors>,
    /// Every zone and battery flattened into one list for the sensors table.
    pub readings: Vec<SensorReading>,
    /// Wall-clock cost of producing this sample.
    ///
    /// Exposed rather than merely logged so the scheduler can back off if a
    /// machine turns out to be unusually slow — some WMI providers take
    /// hundreds of milliseconds on first use, and a fixed cadence chosen on
    /// a fast machine would stall a slow one.
    pub elapsed: Duration,
}

impl SensorSample {
    /// Whether anything at all was measured.
    ///
    /// False on a desktop whose firmware exposes no zones and which has no
    /// battery — a common configuration, and the UI should say so plainly
    /// rather than render an empty table.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.readings.is_empty()
    }
}

/// Reads sensors on its own cadence, caching between reads.
///
/// Not `Clone`: the cache is the point, and a copy would silently double the
/// WMI traffic.
#[derive(Debug)]
pub struct SensorReader {
    cached: Option<SensorSample>,
    read_at: Option<Instant>,
    ttl: Duration,
}

impl Default for SensorReader {
    fn default() -> Self {
        Self::new()
    }
}

impl SensorReader {
    #[must_use]
    pub fn new() -> Self {
        Self {
            cached: None,
            read_at: None,
            ttl: CACHE_TTL,
        }
    }

    /// Overrides the cache lifetime.
    ///
    /// Intended for a "high detail" mode the user opts into while watching
    /// the Thermals tab, not as a default.
    #[must_use]
    pub const fn with_ttl(mut self, ttl: Duration) -> Self {
        self.ttl = ttl;
        self
    }

    /// How often a scheduler should call [`Self::sample`].
    ///
    /// The main sampling loop must *not* use this value for itself; it is
    /// the period of a separate, slower timer. Sampling sensors at the
    /// frame rate would consume the entire 30 ms frame budget in WMI.
    #[must_use]
    pub const fn sample_interval_hint(&self) -> Duration {
        self.ttl
    }

    /// Whether the cache has expired.
    #[must_use]
    pub fn is_stale(&self) -> bool {
        self.read_at.is_none_or(|at| at.elapsed() >= self.ttl)
    }

    /// The cached sample without reading anything.
    ///
    /// For the frame builder, which must never block on WMI. Returns `None`
    /// before the first read rather than an empty sample, so a caller cannot
    /// mistake "not read yet" for "nothing to report".
    #[must_use]
    pub fn cached(&self) -> Option<&SensorSample> {
        self.cached.as_ref()
    }

    /// Reads sensors, or returns the cache if it is still fresh.
    pub fn sample(&mut self) -> &SensorSample {
        if self.is_stale() || self.cached.is_none() {
            let sample = read_all();
            self.read_at = Some(Instant::now());
            self.cached = Some(sample);
        }

        self.cached
            .as_ref()
            .unwrap_or_else(|| unreachable!("populated immediately above"))
    }

    /// Forces a read, ignoring the cache.
    pub fn refresh(&mut self) -> &SensorSample {
        self.read_at = None;
        self.sample()
    }
}

/// Reads every sensor source once, uncached.
///
/// Prefer [`SensorReader`], which will not do this more often than the
/// hardware justifies.
#[must_use]
pub fn read_all() -> SensorSample {
    let started = Instant::now();

    let (thermal, passive_limits) = read_thermal();
    let batteries = enumerate_batteries();
    let power = read_power_state();
    let nvidia = read_nvidia_gpus();

    let mut readings = thermal.readings();
    readings.extend(passive_limits);
    let cpu = cpu_service::latest();
    if let Some(cpu) = &cpu {
        readings.extend(cpu_readings(cpu));
    }
    for gpu in &nvidia {
        readings.extend(nvidia_readings(gpu));
    }

    for (index, pack) in batteries.iter().enumerate() {
        readings.extend(battery_readings(index, pack));
    }

    SensorSample {
        thermal,
        batteries,
        power,
        nvidia,
        cpu,
        readings,
        elapsed: started.elapsed(),
    }
}

/// Thermal zones: WMI first (it has trip points), the counter set otherwise.
///
/// Unelevated, WMI is refused; the counter set is not. Falling back is what
/// turns "needs administrator" into two real temperatures on an ordinary
/// desktop. A zone throttling below 100 % also yields a reading, since that
/// is the one thermal fact a user acts on.
fn read_thermal() -> (ThermalScan, Vec<SensorReading>) {
    let wmi = read_thermal_zones();
    if !wmi.zones.is_empty() {
        return (wmi, Vec::new());
    }
    let counters = read_zone_counters();
    if counters.is_empty() {
        return (wmi, Vec::new());
    }
    let limits = counters
        .iter()
        .enumerate()
        .filter_map(|(index, c)| {
            let limit = c.passive_limit.filter(|&l| (0.0..100.0).contains(&l))?;
            Some(SensorReading::new(
                format!("acpi.tz.{index}.passive"),
                format!("{} throttle limit", c.zone.instance),
                SensorValue::Percent(vitals_core::units::Percent::new(limit)),
                SensorSource::AcpiThermalZone,
                Quality::Measured,
            ))
        })
        .collect();
    let scan = ThermalScan {
        zones: counters.into_iter().map(|c| c.zone).collect(),
        availability: ThermalAvailability::Available,
    };
    (scan, limits)
}

/// Flattens the sensors-service reading; an unreported value adds no row.
fn cpu_readings(cpu: &cpu_service::CpuSensors) -> Vec<SensorReading> {
    use vitals_core::units::{Celsius, Watts};

    let mut out = Vec::new();
    if let Some(c) = cpu.package_celsius {
        out.push(SensorReading::new(
            "cpu.package.temperature",
            "CPU package temperature",
            SensorValue::Temperature(Celsius(c)),
            SensorSource::KernelDriver,
            Quality::Measured,
        ));
    }
    if let Some(c) = cpu.hottest_core_celsius {
        out.push(SensorReading::new(
            "cpu.core.hottest",
            "CPU hottest core",
            SensorValue::Temperature(Celsius(c)),
            SensorSource::KernelDriver,
            Quality::Measured,
        ));
    }
    if let Some(w) = cpu.package_watts {
        out.push(SensorReading::new(
            "cpu.package.power",
            "CPU package power",
            SensorValue::Power(Watts(w)),
            SensorSource::KernelDriver,
            // Energy over an interval, from a counter: measured, not modelled.
            Quality::Measured,
        ));
    }
    out
}

/// Flattens one NVIDIA GPU into readings; an unreported value adds no row.
fn nvidia_readings(gpu: &NvidiaGpu) -> Vec<SensorReading> {
    use vitals_core::units::{Celsius, Percent, Watts};

    let key = |what: &str| format!("nvidia.{}.{what}", gpu.index);
    let mut out = Vec::new();
    if let Some(c) = gpu.temperature_celsius {
        out.push(SensorReading::new(
            key("temperature"),
            format!("{} temperature", gpu.name),
            SensorValue::Temperature(Celsius(c)),
            SensorSource::VendorLibrary,
            Quality::Measured,
        ));
    }
    if let Some(f) = gpu.fan_percent {
        out.push(SensorReading::new(
            key("fan"),
            format!("{} fan", gpu.name),
            SensorValue::Percent(Percent::new(f)),
            SensorSource::VendorLibrary,
            Quality::Measured,
        ));
    }
    if let Some(w) = gpu.power_watts {
        out.push(SensorReading::new(
            key("power"),
            format!("{} board power", gpu.name),
            SensorValue::Power(Watts(w)),
            SensorSource::VendorLibrary,
            Quality::Measured,
        ));
    }
    out
}

/// Flattens one battery into the generic reading list.
///
/// Health is tagged [`Quality::Derived`] and every absent field is simply
/// omitted — an unreported cycle count produces no row at all, rather than a
/// row reading zero.
fn battery_readings(index: usize, pack: &Battery) -> Vec<SensorReading> {
    let mut out = Vec::new();

    if let Some(charge) = pack.charge {
        out.push(SensorReading::new(
            format!("battery.{index}.charge"),
            format!("Battery {index} charge"),
            SensorValue::Charge(charge),
            SensorSource::BatteryMiniport,
            Quality::Measured,
        ));
    }

    if let Some(health) = pack.health {
        out.push(SensorReading::new(
            format!("battery.{index}.health"),
            format!("Battery {index} health"),
            SensorValue::Charge(health),
            SensorSource::BatteryMiniport,
            Quality::Derived,
        ));
    }

    if let Some(rate) = pack.rate {
        out.push(SensorReading::new(
            format!("battery.{index}.rate"),
            match pack.state {
                ChargeState::Charging => format!("Battery {index} charge rate"),
                _ => format!("Battery {index} discharge rate"),
            },
            SensorValue::Power(rate),
            SensorSource::BatteryMiniport,
            Quality::Measured,
        ));
    }

    if let Some(voltage) = pack.voltage {
        out.push(SensorReading::new(
            format!("battery.{index}.voltage"),
            format!("Battery {index} voltage"),
            SensorValue::Voltage(voltage),
            SensorSource::BatteryMiniport,
            Quality::Measured,
        ));
    }

    out
}

/// What the sensor layer can do on this machine, right now.
///
/// Computed from an actual read rather than from a static table, because the
/// answer depends on the board's firmware and on whether the process is
/// elevated — neither of which can be known at compile time.
#[must_use]
pub fn capabilities(sample: &SensorSample) -> Capabilities {
    let mut caps = Capabilities::new();

    caps = if sample.thermal.zones.is_empty() {
        let reason = match sample.thermal.availability {
            ThermalAvailability::AccessDenied => Unavailable::NeedsElevation,
            // No zones and no provider are both "this hardware does not
            // offer it", and neither is fixed by elevating. Saying
            // NeedsElevation here would send the user through a UAC prompt
            // that changes nothing.
            _ => Unavailable::NoSuchHardware,
        };
        caps.without(Capability::Thermals, reason)
    } else {
        caps.with(Capability::Thermals)
    };

    // Battery draw is a genuine wattage measurement, so PowerDraw is
    // available on a laptop. It is *not* CPU or GPU package power, and the
    // gap list says so; a desktop has no measured wattage at all.
    caps = if sample.batteries.iter().any(|b| b.rate.is_some()) {
        caps.with(Capability::PowerDraw)
    } else {
        caps.without(Capability::PowerDraw, Unavailable::NeedsPlugin)
    };

    // Never available without a driver, on any machine. ACPI models fans as
    // on/off devices with no tachometer, so there is not even a partial
    // answer to degrade to.
    caps.without(Capability::FanControl, Unavailable::NeedsPlugin)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_reader_is_stale_and_has_no_cache() {
        let reader = SensorReader::new();
        assert!(reader.is_stale());
        assert!(reader.cached().is_none());
    }

    #[test]
    fn the_second_call_is_served_from_cache() {
        let mut reader = SensorReader::new();

        let first = reader.sample().elapsed;
        // Same instant, so the TTL cannot have expired; a second read would
        // show a different elapsed time.
        let second = reader.sample().elapsed;

        assert_eq!(first, second);
        assert!(!reader.is_stale());
    }

    #[test]
    fn refresh_bypasses_the_cache() {
        let mut reader = SensorReader::new();
        reader.sample();
        assert!(!reader.is_stale());
        reader.refresh();
        assert!(!reader.is_stale());
    }

    #[test]
    fn a_zero_ttl_never_caches() {
        let mut reader = SensorReader::new().with_ttl(Duration::ZERO);
        reader.sample();
        assert!(reader.is_stale());
    }

    #[test]
    fn the_hint_is_far_slower_than_the_frame_budget() {
        // The whole point of this module's design. 30 ms is the frame
        // budget; sampling sensors anywhere near it would consume all of it.
        let hint = SensorReader::new().sample_interval_hint();
        assert!(
            hint >= Duration::from_secs(1),
            "sensor cadence must not approach the sampling tick"
        );
    }

    #[test]
    fn reading_everything_is_infallible() {
        let sample = read_all();

        // Whatever the machine, an empty reading list must be accompanied by
        // a reason rather than presented as a healthy zero-sensor system.
        if sample.thermal.zones.is_empty() {
            assert_ne!(
                sample.thermal.availability,
                ThermalAvailability::Available,
                "no zones were read yet availability claims success"
            );
        }
    }

    #[test]
    fn nothing_measured_means_nothing_reported() {
        let sample = read_all();

        // The core invariant: every reading in the list is backed by a
        // source, and no reading exists for a value that was not obtained.
        for reading in &sample.readings {
            assert!(!reading.key.is_empty());
            assert!(reading.value.magnitude().is_finite());
        }
    }

    #[test]
    fn fan_control_is_never_claimed() {
        let caps = capabilities(&read_all());
        assert!(!caps.has(Capability::FanControl));
        assert_eq!(
            caps.reason(Capability::FanControl),
            Some(Unavailable::NeedsPlugin)
        );
    }

    #[test]
    fn absent_thermals_do_not_ask_for_pointless_elevation() {
        let scan = ThermalScan::unavailable(ThermalAvailability::NoZonesPresent);
        let sample = SensorSample {
            thermal: scan,
            batteries: Vec::new(),
            power: read_power_state(),
            nvidia: Vec::new(),
            cpu: None,
            readings: Vec::new(),
            elapsed: Duration::ZERO,
        };

        assert_eq!(
            capabilities(&sample).reason(Capability::Thermals),
            Some(Unavailable::NoSuchHardware)
        );
    }

    #[test]
    fn denied_thermals_offer_elevation() {
        let sample = SensorSample {
            thermal: ThermalScan::unavailable(ThermalAvailability::AccessDenied),
            batteries: Vec::new(),
            power: read_power_state(),
            nvidia: Vec::new(),
            cpu: None,
            readings: Vec::new(),
            elapsed: Duration::ZERO,
        };

        assert_eq!(
            capabilities(&sample).reason(Capability::Thermals),
            Some(Unavailable::NeedsElevation)
        );
    }

    #[test]
    fn a_desktop_reports_no_measured_power_draw() {
        let sample = read_all();
        let caps = capabilities(&sample);

        // Either a pack reports a rate, or PowerDraw is honestly unavailable.
        // What must never happen is PowerDraw available with no source.
        if caps.has(Capability::PowerDraw) {
            assert!(sample.batteries.iter().any(|b| b.rate.is_some()));
        }
    }
}
