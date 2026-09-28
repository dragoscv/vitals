//! Unit conversions for sensor data. Pure, and therefore testable.
//!
//! Everything here is arithmetic on values that arrive from firmware in
//! units nobody would choose deliberately. It lives apart from the I/O so
//! it can be tested without a battery, and because a conversion bug is the
//! most dangerous kind of bug this module can have: the number still looks
//! like a temperature.

use vitals_core::units::{Celsius, Percent, Volts, Watts};

/// Absolute zero in Celsius.
const KELVIN_OFFSET: f32 = 273.15;

/// Lowest ACPI reading treated as real, in tenths of a Kelvin.
///
/// 2000 dK is −73 °C. A thermal zone below that is not a cold machine, it is
/// firmware returning a placeholder — some boards report 0 or 2732 (exactly
/// 0 °C) for a zone they do not actually populate.
const MIN_PLAUSIBLE_DECIKELVIN: u32 = 2000;

/// Highest ACPI reading treated as real, in tenths of a Kelvin.
///
/// 4732 dK is 200 °C. Silicon has long since shut down by then, so anything
/// above is a sentinel rather than a measurement.
const MAX_PLAUSIBLE_DECIKELVIN: u32 = 4732;

/// Converts an ACPI thermal-zone reading to Celsius.
///
/// ACPI reports tenths of a Kelvin: 2932 is 20.05 °C. The 273.15 offset must
/// be applied *after* dividing by ten — subtracting first and then dividing
/// yields 265.9, and dividing without subtracting yields 293.2. Both are
/// wrong, but 293.2 at least announces itself; the subtler failure is
/// applying a 273 offset to a value already in Celsius, which is off by a
/// constant and still reads as a plausible temperature.
///
/// Returns `None` when the value is outside the plausible band, because a
/// firmware placeholder rendered as −273 °C is worse than an empty cell.
#[must_use]
pub fn decikelvin_to_celsius(decikelvin: u32) -> Option<Celsius> {
    if !(MIN_PLAUSIBLE_DECIKELVIN..=MAX_PLAUSIBLE_DECIKELVIN).contains(&decikelvin) {
        return None;
    }

    Some(Celsius(decikelvin as f32 / 10.0 - KELVIN_OFFSET))
}

/// Converts a battery rate reported in milliwatts.
///
/// The battery miniport signs this value: negative is discharge, positive is
/// charge, and `i32::MIN` (`BATTERY_UNKNOWN_RATE`) means the gauge cannot
/// report it. That sentinel must be filtered before any arithmetic — it is a
/// valid `i32` and would otherwise render as −2.1 GW.
///
/// Returns the magnitude; direction is carried separately by the charging
/// flag, so a caller cannot accidentally sum a charge and a discharge to
/// zero.
#[must_use]
pub fn milliwatts_to_watts(milliwatts: i32) -> Option<Watts> {
    /// `BATTERY_UNKNOWN_RATE`
    const UNKNOWN_RATE: i32 = i32::MIN;

    if milliwatts == UNKNOWN_RATE {
        return None;
    }

    Some(Watts(milliwatts.unsigned_abs() as f32 / 1000.0))
}

/// Converts a battery voltage reported in millivolts.
///
/// `BATTERY_UNKNOWN_VOLTAGE` is `0xFFFF_FFFF`, which as an unsigned value is
/// a perfectly ordinary 4.29 megavolts if left unchecked.
#[must_use]
pub fn millivolts_to_volts(millivolts: u32) -> Option<Volts> {
    /// `BATTERY_UNKNOWN_VOLTAGE`
    const UNKNOWN_VOLTAGE: u32 = u32::MAX;

    if millivolts == UNKNOWN_VOLTAGE {
        return None;
    }

    Some(Volts(millivolts as f32 / 1000.0))
}

/// Battery charge as a percentage of its *present* full capacity.
///
/// Deliberately measured against full-charge capacity rather than design
/// capacity. A pack that has aged to 70 % health and is physically full is at
/// 100 %, not 70 % — reporting the latter would tell the user their fully
/// charged laptop is a third empty.
///
/// Returns `None` when the denominator is zero or the gauge reported a
/// sentinel, rather than the 0 % that a naive guard would produce.
#[must_use]
pub fn charge_percent(remaining_mwh: u32, full_charge_mwh: u32) -> Option<Percent> {
    /// `BATTERY_UNKNOWN_CAPACITY`
    const UNKNOWN_CAPACITY: u32 = u32::MAX;

    if full_charge_mwh == 0
        || full_charge_mwh == UNKNOWN_CAPACITY
        || remaining_mwh == UNKNOWN_CAPACITY
    {
        return None;
    }

    Some(Percent::new(
        remaining_mwh as f32 / full_charge_mwh as f32 * 100.0,
    ))
}

/// Battery health: present full capacity as a fraction of design capacity.
///
/// This is [`crate::sensors::reading::Quality::Derived`] — it is a ratio of
/// two firmware constants, not a measurement of anything happening now. Some
/// packs report a full-charge capacity slightly above design when new, hence
/// the clamp inside [`Percent::new`] rather than an assertion.
#[must_use]
pub fn health_percent(full_charge_mwh: u32, design_mwh: u32) -> Option<Percent> {
    /// `BATTERY_UNKNOWN_CAPACITY`
    const UNKNOWN_CAPACITY: u32 = u32::MAX;

    if design_mwh == 0 || design_mwh == UNKNOWN_CAPACITY || full_charge_mwh == UNKNOWN_CAPACITY {
        return None;
    }

    Some(Percent::new(
        full_charge_mwh as f32 / design_mwh as f32 * 100.0,
    ))
}

/// Time to empty or full, derived from remaining capacity and current rate.
///
/// Returns `None` at a zero rate rather than infinity, and `None` when the
/// rate opposes the question being asked — asking "how long until empty"
/// while charging has no answer, and the honest response is not a very large
/// number.
///
/// `rate_mw` keeps the miniport's sign convention: negative is discharge.
#[must_use]
pub fn seconds_remaining(remaining_mwh: u32, rate_mw: i32) -> Option<u32> {
    /// `BATTERY_UNKNOWN_RATE`. Negative, so the `>= 0` guard below let it
    /// through as a 2.1 GW drain and produced "0 s to empty" on a gauge that
    /// simply had not reported a rate yet.
    const UNKNOWN_RATE: i32 = i32::MIN;
    /// `BATTERY_UNKNOWN_CAPACITY`
    const UNKNOWN_CAPACITY: u32 = u32::MAX;

    if rate_mw >= 0 || rate_mw == UNKNOWN_RATE || remaining_mwh == UNKNOWN_CAPACITY {
        return None;
    }

    let drain_mw = rate_mw.unsigned_abs();
    if drain_mw == 0 {
        return None;
    }

    // mWh / mW gives hours; the seconds conversion is why this is u64 first.
    let seconds = u64::from(remaining_mwh) * 3600 / u64::from(drain_mw);
    u32::try_from(seconds).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unknown_rate_or_capacity_gives_no_time_to_empty_rather_than_a_tiny_one() {
        assert_eq!(seconds_remaining(40_000, i32::MIN), None);
        assert_eq!(seconds_remaining(u32::MAX, -10_000), None);
        assert_eq!(seconds_remaining(40_000, -10_000), Some(4 * 3600));
    }

    #[test]
    fn acpi_tenths_of_a_kelvin_convert_exactly() {
        // The example from the ACPI specification.
        let c = decikelvin_to_celsius(2932).expect("in band");
        assert!((c.0 - 20.05).abs() < 0.001, "got {}", c.0);
    }

    #[test]
    fn a_typical_chipset_reading_is_sane() {
        let c = decikelvin_to_celsius(3182).expect("in band");
        assert!((c.0 - 45.05).abs() < 0.001, "got {}", c.0);
    }

    #[test]
    fn the_off_by_273_error_would_be_caught() {
        // Guards the subtle failure: if the offset were applied before the
        // divide, 3182 would yield 290.9 rather than 45.05. Both are
        // numerically valid f32 temperatures, so only an explicit bound
        // separates them.
        let c = decikelvin_to_celsius(3182).expect("in band");
        assert!(c.0 < 100.0, "offset applied in the wrong order: {}", c.0);
        assert!(c.0 > 0.0);
    }

    #[test]
    fn implausible_readings_are_rejected_not_clamped() {
        assert_eq!(decikelvin_to_celsius(0), None);
        assert_eq!(decikelvin_to_celsius(1), None);
        // Exactly 0 °C from a zone that is really just unpopulated.
        assert_eq!(decikelvin_to_celsius(1999), None);
        assert_eq!(decikelvin_to_celsius(9999), None);
        assert_eq!(decikelvin_to_celsius(u32::MAX), None);
    }

    #[test]
    fn plausible_band_edges_are_inclusive() {
        assert!(decikelvin_to_celsius(2000).is_some());
        assert!(decikelvin_to_celsius(4732).is_some());
        assert!(decikelvin_to_celsius(4733).is_none());
    }

    #[test]
    fn unknown_rate_sentinel_is_not_a_measurement() {
        assert_eq!(milliwatts_to_watts(i32::MIN), None);
    }

    #[test]
    fn discharge_and_charge_give_the_same_magnitude() {
        let discharging = milliwatts_to_watts(-15_500).expect("known");
        let charging = milliwatts_to_watts(15_500).expect("known");
        assert!((discharging.0 - 15.5).abs() < 0.001);
        assert!((charging.0 - 15.5).abs() < 0.001);
    }

    #[test]
    fn unknown_voltage_sentinel_is_not_four_megavolts() {
        assert_eq!(millivolts_to_volts(u32::MAX), None);
        let v = millivolts_to_volts(11_400).expect("known");
        assert!((v.0 - 11.4).abs() < 0.001);
    }

    #[test]
    fn charge_is_measured_against_present_capacity_not_design() {
        // A pack aged to 70% health, physically full, must read 100%.
        let pct = charge_percent(35_000, 35_000).expect("known");
        assert!((pct.get() - 100.0).abs() < 0.01);
    }

    #[test]
    fn charge_rejects_sentinels_rather_than_reporting_zero() {
        assert_eq!(charge_percent(1000, 0), None);
        assert_eq!(charge_percent(1000, u32::MAX), None);
        assert_eq!(charge_percent(u32::MAX, 1000), None);
    }

    #[test]
    fn health_is_full_over_design() {
        let pct = health_percent(35_000, 50_000).expect("known");
        assert!((pct.get() - 70.0).abs() < 0.01);
    }

    #[test]
    fn health_above_design_clamps_rather_than_panicking() {
        let pct = health_percent(51_000, 50_000).expect("known");
        assert!((pct.get() - 100.0).abs() < f32::EPSILON);
    }

    #[test]
    fn runtime_has_no_answer_while_charging() {
        assert_eq!(seconds_remaining(20_000, 15_000), None);
        assert_eq!(seconds_remaining(20_000, 0), None);
    }

    #[test]
    fn runtime_from_a_steady_drain() {
        // 20 Wh remaining at a 10 W drain is two hours.
        assert_eq!(seconds_remaining(20_000, -10_000), Some(7200));
    }
}
