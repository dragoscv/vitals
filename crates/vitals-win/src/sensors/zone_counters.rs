//! ACPI thermal zones through the performance counters, unelevated.
//!
//! `MSAcpi_ThermalZoneTemperature` (see [`super::wmi`]) is refused to a
//! standard user on most builds — the Devices screen said "needs
//! administrator" on every machine it was opened on. The same zones are also
//! published by the kernel as the `Thermal Zone Information` counter set,
//! which any user may read: measured on 2026-09-28 on a desktop where WMI
//! answered `WBEM_E_ACCESS_DENIED`, the counter returned both zones
//! (`\_TZ.TZ00` 3010, `\_TZ.TZ10` 2900).
//!
//! `High Precision Temperature` is in tenths of a kelvin, the same unit WMI
//! uses, so the same plausibility band ([`decikelvin_to_celsius`]) applies.
//! The plain `Temperature` counter is whole kelvin; it is not read.
//!
//! Critical trip points and active/passive cooling are not in the counter
//! set, so those fields are `None` here rather than guessed. `% Passive
//! Limit` below 100 means the firmware is throttling the zone; that is kept
//! and surfaced as a reading, because "the machine is slowing itself down to
//! cool" is the one thing a user needs to know about a thermal zone.
//!
//! One PDH query per call. Opening it costs a few milliseconds; this module
//! is only called on the sensor cadence (see [`super::SensorReader`]), never
//! on the sampling tick.

use windows::Win32::System::Performance::{
    PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_DOUBLE, PDH_HCOUNTER, PDH_HQUERY, PdhAddEnglishCounterW,
    PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterArrayW, PdhOpenQueryW,
};
use windows::core::{PCWSTR, w};

use super::convert::decikelvin_to_celsius;
use super::thermal::ThermalZone;

/// One zone as the counter set reports it.
#[derive(Debug, Clone, PartialEq)]
pub struct CounterZone {
    pub zone: ThermalZone,
    /// `% Passive Limit`: 100 when unthrottled, lower while the firmware
    /// is cooling the zone by slowing the processor.
    pub passive_limit: Option<f32>,
}

/// Reads every zone the counter set publishes. Empty when it has none.
#[must_use]
pub fn read_zone_counters() -> Vec<CounterZone> {
    let Some(query) = Query::open() else {
        return Vec::new();
    };
    let Some(temperature) = query.add(w!(
        "\\Thermal Zone Information(*)\\High Precision Temperature"
    )) else {
        return Vec::new();
    };
    let limit = query.add(w!("\\Thermal Zone Information(*)\\% Passive Limit"));

    // Instantaneous counters: one collection is a complete reading, unlike
    // the rate counters in `gpu::counters`, which need two.
    // SAFETY: `query.0` is a live handle owned by `query`.
    if unsafe { PdhCollectQueryData(query.0) } != 0 {
        return Vec::new();
    }

    let limits = limit.map(read_array).unwrap_or_default();
    read_array(temperature)
        .into_iter()
        .filter_map(|(instance, decikelvin)| {
            let zone = parse_counter_zone(&instance, decikelvin)?;
            let passive_limit = limits
                .iter()
                .find(|(name, _)| *name == instance)
                .map(|&(_, value)| value as f32);
            Some(CounterZone {
                zone,
                passive_limit,
            })
        })
        .collect()
}

/// Validates one counter value. Rejects the implausible, like WMI does.
#[must_use]
pub fn parse_counter_zone(instance: &str, decikelvin: f64) -> Option<ThermalZone> {
    if !decikelvin.is_finite() || decikelvin < 0.0 || decikelvin > f64::from(u32::MAX) {
        return None;
    }
    // Truncation is the intent: the value is an integer count of tenths.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let temperature = decikelvin_to_celsius(decikelvin.round() as u32)?;
    Some(ThermalZone {
        instance: display_name(instance),
        temperature,
        critical: None,
        active_cooling: None,
    })
}

/// `\_tz.tz00` → `TZ00`: the ACPI path prefix is noise to a reader.
fn display_name(instance: &str) -> String {
    instance
        .rsplit('.')
        .next()
        .unwrap_or(instance)
        .trim_start_matches('\\')
        .to_ascii_uppercase()
}

struct Query(PDH_HQUERY);

impl Query {
    fn open() -> Option<Self> {
        let mut query = PDH_HQUERY::default();
        // SAFETY: writes a handle on success; closed in `Drop`.
        (unsafe { PdhOpenQueryW(PCWSTR::null(), 0, &raw mut query) } == 0).then_some(Self(query))
    }

    /// English path so a Romanian Windows resolves it too (see `gpu::counters`).
    fn add(&self, path: PCWSTR) -> Option<PDH_HCOUNTER> {
        let mut counter = PDH_HCOUNTER::default();
        // SAFETY: `self.0` is live; `path` is a static wide string.
        (unsafe { PdhAddEnglishCounterW(self.0, path, 0, &raw mut counter) } == 0)
            .then_some(counter)
    }
}

impl Drop for Query {
    fn drop(&mut self) {
        // SAFETY: live handle, closed exactly once. Its counters go with it.
        unsafe { PdhCloseQuery(self.0) };
    }
}

/// `(instance, value)` for every instance of one counter.
fn read_array(counter: PDH_HCOUNTER) -> Vec<(String, f64)> {
    let mut size = 0_u32;
    let mut count = 0_u32;
    // SAFETY: a null buffer asks PDH for the size required.
    unsafe {
        PdhGetFormattedCounterArrayW(counter, PDH_FMT_DOUBLE, &raw mut size, &raw mut count, None);
    }
    if size == 0 {
        return Vec::new();
    }

    let items = (size as usize).div_ceil(size_of::<PDH_FMT_COUNTERVALUE_ITEM_W>()) + 1;
    let mut buffer = vec![PDH_FMT_COUNTERVALUE_ITEM_W::default(); items];
    // SAFETY: the buffer is at least `size` bytes and aligned for the item.
    let status = unsafe {
        PdhGetFormattedCounterArrayW(
            counter,
            PDH_FMT_DOUBLE,
            &raw mut size,
            &raw mut count,
            Some(buffer.as_mut_ptr()),
        )
    };
    if status != 0 {
        return Vec::new();
    }

    buffer
        .iter()
        .take((count as usize).min(buffer.len()))
        .filter_map(|item| {
            // SAFETY: PDH_FMT_DOUBLE was requested, so the union holds a double.
            let value = unsafe { item.FmtValue.Anonymous.doubleValue };
            // SAFETY: PDH null-terminates the instance name.
            let name = unsafe { item.szName.to_string() }.ok()?;
            Some((name, value))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_tenths_of_a_kelvin_the_way_the_counter_reports_them() {
        // The value measured on the development machine: 3010 → 27.85 °C.
        let zone = parse_counter_zone("\\_tz.tz00", 3010.0).expect("plausible");
        assert!((zone.temperature.0 - 27.85).abs() < 0.01);
        assert_eq!(zone.instance, "TZ00");
    }

    #[test]
    fn rejects_a_placeholder_rather_than_showing_absolute_zero() {
        assert!(parse_counter_zone("\\_tz.tz00", 0.0).is_none());
        assert!(parse_counter_zone("\\_tz.tz00", f64::NAN).is_none());
        assert!(parse_counter_zone("\\_tz.tz00", -5.0).is_none());
    }

    #[test]
    fn claims_no_trip_point_or_cooling_mode_it_did_not_read() {
        let zone = parse_counter_zone("\\_tz.tz10", 2900.0).expect("plausible");
        assert!(zone.critical.is_none());
        assert!(zone.active_cooling.is_none());
    }
}
