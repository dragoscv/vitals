//! Battery state from IOKit's `IOPowerSources`.
//!
//! macOS exposes a live battery reading through Core Foundation only — there
//! is no `sysctl` for it and `sysinfo` carries none — so this is a small
//! hand-written binding to three IOKit functions and a handful of Core
//! Foundation accessors, in the same style as [`crate::input`]'s CoreGraphics
//! call.
//!
//! Two things this file is careful about:
//!
//! * **Every object it creates is released.** `IOPSCopyPowerSourcesInfo` and
//!   `IOPSCopyPowerSourcesList` both hand back a `+1` reference, and the keys
//!   are `CFString`s created here because the IOKit keys are C strings that
//!   the header requires be converted before use. This runs once a second for
//!   as long as the machine is on, so a missing `CFRelease` is a leak that
//!   grows forever, not a tidiness issue — [`Owned`] makes the release
//!   unconditional.
//! * **An unmeasured field is `None`, never `0`.** A Mac mini or an iMac has
//!   no battery at all, and a battery that will not report its capacity is not
//!   "flat" — both withhold the whole reading rather than claiming certainty
//!   about zero. That is the repository's first principle, and the mapping
//!   that enforces it is [`battery_from_reading`], a pure function so it can
//!   be tested without a battery.

use std::ffi::{CStr, c_char, c_void};
use std::ptr;

use vitals_core::metrics::BatteryMetrics;
use vitals_core::units::Percent;

// -------------------------------------------------------------------------
// Foreign declarations
// -------------------------------------------------------------------------

/// Opaque Core Foundation object pointer.
type CFTypeRef = *const c_void;
/// Opaque Core Foundation array pointer.
type CFArrayRef = *const c_void;
/// Opaque Core Foundation dictionary pointer.
type CFDictionaryRef = *const c_void;
/// Opaque Core Foundation string pointer.
type CFStringRef = *const c_void;
/// Opaque Core Foundation boolean pointer.
type CFBooleanRef = *const c_void;
/// `CFTypeID`, an `unsigned long`.
type CFTypeId = usize;
/// `CFIndex`, a `long`.
type CFIndex = isize;
/// `Boolean` from `MacTypes.h`, an `unsigned char`.
type Boolean = u8;

/// `kCFStringEncodingUTF8`.
const CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;
/// `kCFNumberSInt64Type`.
const CF_NUMBER_SINT64: i32 = 4;

#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    /// A `+1` opaque snapshot of every power source, or null.
    fn IOPSCopyPowerSourcesInfo() -> CFTypeRef;
    /// A `+1` array of source handles, or null.
    fn IOPSCopyPowerSourcesList(blob: CFTypeRef) -> CFArrayRef;
    /// A **borrowed** dictionary for one source; must not be released.
    fn IOPSGetPowerSourceDescription(blob: CFTypeRef, ps: CFTypeRef) -> CFDictionaryRef;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(cf: CFTypeRef);
    fn CFArrayGetCount(array: CFArrayRef) -> CFIndex;
    fn CFArrayGetValueAtIndex(array: CFArrayRef, index: CFIndex) -> *const c_void;
    fn CFDictionaryGetValue(dict: CFDictionaryRef, key: *const c_void) -> *const c_void;
    fn CFGetTypeID(cf: CFTypeRef) -> CFTypeId;
    fn CFNumberGetTypeID() -> CFTypeId;
    fn CFNumberGetValue(number: CFTypeRef, the_type: i32, value: *mut c_void) -> Boolean;
    fn CFBooleanGetTypeID() -> CFTypeId;
    fn CFBooleanGetValue(boolean: CFBooleanRef) -> Boolean;
    fn CFStringGetTypeID() -> CFTypeId;
    fn CFStringCreateWithCString(
        alloc: CFTypeRef,
        c_str: *const c_char,
        encoding: u32,
    ) -> CFStringRef;
    fn CFStringGetCString(
        string: CFStringRef,
        buffer: *mut c_char,
        buffer_size: CFIndex,
        encoding: u32,
    ) -> Boolean;
}

// -------------------------------------------------------------------------
// Ownership
// -------------------------------------------------------------------------

/// A `+1` Core Foundation reference, released exactly once when it drops.
///
/// The sampler calls [`battery`] once a second for as long as the machine is
/// on, so anything this module allocates and does not release leaks forever.
/// Every object returned by a `Copy`/`Create` function is wrapped in one of
/// these; an object returned by a `Get` is borrowed and is not.
struct Owned(CFTypeRef);

impl Owned {
    /// The underlying reference, for passing to a Core Foundation call.
    fn raw(&self) -> CFTypeRef {
        self.0
    }
}

impl Drop for Owned {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: `self.0` is a `+1` reference this guard exclusively owns
            // and has not released; the null check covers the failure return
            // that a `Copy`/`Create` function is allowed to give.
            unsafe { CFRelease(self.0) };
        }
    }
}

/// Copies a Rust C string into a `+1` `CFString`.
///
/// The IOKit keys are C strings (`kIOPSCurrentCapacityKey` and friends), and
/// the header is explicit that they must be converted before use: a
/// `CFDictionaryGetValue` matches by `CFEqual`, so a raw `char *` would find
/// nothing and every reading would come back absent.
fn cf_string(text: &CStr) -> Owned {
    // SAFETY: `text` is NUL-terminated; a null allocator is `kCFAllocatorDefault`;
    // the result is a `+1` string or null, both of which `Owned` handles.
    Owned(unsafe { CFStringCreateWithCString(ptr::null(), text.as_ptr(), CF_STRING_ENCODING_UTF8) })
}

/// The dictionary keys this module reads, as `CFString`s.
///
/// Built once per [`battery`] call and released with it, so the per-second
/// cost is a handful of tiny allocations rather than a growing set of strings.
struct Keys {
    source_state: Owned,
    current_capacity: Owned,
    max_capacity: Owned,
    is_charging: Owned,
    is_present: Owned,
    time_to_empty: Owned,
    time_to_full_charge: Owned,
}

impl Keys {
    fn new() -> Self {
        // The literals are exactly the `#define`s in `IOPSKeys.h`.
        Self {
            source_state: cf_string(c"Power Source State"),
            current_capacity: cf_string(c"Current Capacity"),
            max_capacity: cf_string(c"Max Capacity"),
            is_charging: cf_string(c"Is Charging"),
            is_present: cf_string(c"Is Present"),
            time_to_empty: cf_string(c"Time to Empty"),
            time_to_full_charge: cf_string(c"Time to Full Charge"),
        }
    }
}

// -------------------------------------------------------------------------
// Dictionary access
// -------------------------------------------------------------------------

/// The number under `key`, or `None` when it is absent or not a number.
fn dict_number(dict: CFDictionaryRef, key: CFTypeRef) -> Option<i64> {
    // SAFETY: `dict` and `key` are Core Foundation objects valid for the call;
    // the returned value is borrowed from `dict`.
    let value = unsafe { CFDictionaryGetValue(dict, key) };
    if value.is_null() {
        return None;
    }
    // SAFETY: `CFGetTypeID` accepts any non-null Core Foundation object.
    if unsafe { CFGetTypeID(value) } != unsafe { CFNumberGetTypeID() } {
        return None;
    }
    let mut out = 0i64;
    // SAFETY: `value` is a `CFNumber`; the out-pointer has room for an `i64`,
    // and `CFNumberGetValue` widens an `Int32` to the requested `SInt64`.
    let widened =
        unsafe { CFNumberGetValue(value, CF_NUMBER_SINT64, ptr::from_mut(&mut out).cast()) };
    (widened != 0).then_some(out)
}

/// The boolean under `key`, or `None` when it is absent or not a boolean.
fn dict_bool(dict: CFDictionaryRef, key: CFTypeRef) -> Option<bool> {
    let value = unsafe { CFDictionaryGetValue(dict, key) };
    if value.is_null() {
        return None;
    }
    if unsafe { CFGetTypeID(value) } != unsafe { CFBooleanGetTypeID() } {
        return None;
    }
    // SAFETY: `value` is a `CFBoolean`.
    Some(unsafe { CFBooleanGetValue(value) } != 0)
}

/// Whether the string under `key` equals `expected`.
///
/// Compared after copying into a stack buffer: the values this is used on are
/// one of two short literals (`"AC Power"`, `"Battery Power"`), so no
/// allocation is warranted and `CFStringGetCString` needs none.
fn dict_string_equals(dict: CFDictionaryRef, key: CFTypeRef, expected: &CStr) -> bool {
    let value = unsafe { CFDictionaryGetValue(dict, key) };
    if value.is_null() {
        return false;
    }
    if unsafe { CFGetTypeID(value) } != unsafe { CFStringGetTypeID() } {
        return false;
    }
    let mut buffer = [c_char::default(); 32];
    // SAFETY: `value` is a `CFString` whose contents are far shorter than the
    // buffer, and `CFStringGetCString` NUL-terminates when it succeeds.
    let copied = unsafe {
        CFStringGetCString(
            value,
            buffer.as_mut_ptr(),
            buffer.len() as CFIndex,
            CF_STRING_ENCODING_UTF8,
        )
    };
    if copied == 0 {
        return false;
    }
    // SAFETY: `copied != 0` means the buffer holds a NUL-terminated string.
    let found = unsafe { CStr::from_ptr(buffer.as_ptr()) };
    found == expected
}

/// What `kIOPSPowerSourceStateKey` says about a source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum SourceState {
    /// `kIOPSACPowerValue` — connected to external power.
    Ac,
    /// `kIOPSBatteryPowerValue` — drawing from its own cells.
    Battery,
    /// Absent, off-line, or a value this module does not treat specially.
    #[default]
    Other,
}

fn dict_source_state(dict: CFDictionaryRef, key: CFTypeRef) -> SourceState {
    if dict_string_equals(dict, key, c"AC Power") {
        SourceState::Ac
    } else if dict_string_equals(dict, key, c"Battery Power") {
        SourceState::Battery
    } else {
        SourceState::Other
    }
}

/// One power source as IOKit reported it, with no Core Foundation types left.
///
/// Plain data, so the mapping to [`BatteryMetrics`] can be tested on a machine
/// with no battery.
#[derive(Debug, Clone, Copy, Default)]
struct SourceReading {
    present: bool,
    /// `kIOPSCurrentCapacityKey`, in the source's own units (percent on macOS).
    current_capacity: Option<i64>,
    /// `kIOPSMaxCapacityKey`, the same units. `100` on an Apple battery.
    max_capacity: Option<i64>,
    /// `kIOPSIsChargingKey`.
    charging: Option<bool>,
    /// `kIOPSTimeToEmptyKey`, minutes. `-1` means "still calculating".
    time_to_empty_minutes: Option<i64>,
    /// `kIOPSTimeToFullChargeKey`, minutes. `-1` means "still calculating".
    time_to_full_minutes: Option<i64>,
    /// `kIOPSPowerSourceStateKey`, as a three-way classification.
    state: SourceState,
}

/// Reads one power source's dictionary into a [`SourceReading`].
fn read_source(description: CFDictionaryRef, keys: &Keys) -> SourceReading {
    SourceReading {
        present: dict_bool(description, keys.is_present.raw()).unwrap_or(false),
        current_capacity: dict_number(description, keys.current_capacity.raw()),
        max_capacity: dict_number(description, keys.max_capacity.raw()),
        charging: dict_bool(description, keys.is_charging.raw()),
        time_to_empty_minutes: dict_number(description, keys.time_to_empty.raw()),
        time_to_full_minutes: dict_number(description, keys.time_to_full_charge.raw()),
        state: dict_source_state(description, keys.source_state.raw()),
    }
}

// -------------------------------------------------------------------------
// Mapping
// -------------------------------------------------------------------------

/// Maps one source to core battery metrics, or `None` when it is not a battery
/// with a usable reading.
///
/// The `None` cases are the point: a source that is not present, or is present
/// but will not report its capacity, produces no reading at all rather than a
/// zero — a battery the OS declines to measure is not a battery at 0 percent.
fn battery_from_reading(reading: &SourceReading) -> Option<BatteryMetrics> {
    if !reading.present {
        return None;
    }

    // Current capacity as a share of the maximum. macOS normalises both to
    // percent (max is usually 100), but the ratio is what carries meaning and
    // the unit cancels. A negative figure is a sentinel, not a real capacity.
    let (Some(current), Some(max)) = (reading.current_capacity, reading.max_capacity) else {
        return None;
    };
    if current < 0 || max <= 0 {
        return None;
    }
    let charge = Percent::ratio(current.unsigned_abs(), max.unsigned_abs());

    let charging = reading.charging.unwrap_or(false);
    // `kIOPSTimeToFullChargeKey` is only valid while charging and
    // `kIOPSTimeToEmptyKey` only while discharging, so the direction picks the
    // key. A negative value is IOKit's "still calculating": it becomes `None`,
    // never "0 minutes on a battery with hours left". When the charging flag
    // is absent, the source's own AC/battery classification decides.
    let wants_time_to_full = reading
        .charging
        .unwrap_or(matches!(reading.state, SourceState::Ac));
    let minutes = if wants_time_to_full {
        reading
            .time_to_full_minutes
            .or(reading.time_to_empty_minutes)
    } else {
        reading
            .time_to_empty_minutes
            .or(reading.time_to_full_minutes)
    }
    .filter(|minutes| *minutes > 0);

    Some(BatteryMetrics {
        charge,
        charging,
        time_remaining_secs: minutes.map(|minutes| minutes.unsigned_abs().saturating_mul(60)),
        // Draw, health, cycle count and temperature are not in this dictionary
        // in a usable form: IOKit normalises the capacities (so "max" is 100
        // and cannot be compared against a design capacity), and the live
        // amperage and pack temperature live in the raw `AppleSmartBattery`
        // gauge rather than here. Unmeasured, never zero — the same fields the
        // Windows backend also leaves `None`.
        power: None,
        health: None,
        cycle_count: None,
        temperature: None,
    })
}

/// Battery charge, charging state and time estimate for this Mac, or `None`
/// when there is no internal battery to measure.
///
/// A Mac mini or an iMac has no battery and returns `None`; so does a battery
/// that will not report its capacity. `None` renders as an em dash, which is
/// the honest answer — never a zero that would read as "flat".
#[must_use]
pub fn battery() -> Option<BatteryMetrics> {
    // SAFETY: no arguments; returns a `+1` snapshot or null.
    let info = Owned(unsafe { IOPSCopyPowerSourcesInfo() });
    if info.raw().is_null() {
        return None;
    }
    // SAFETY: `info` is a valid snapshot for as long as it is held.
    let list = Owned(unsafe { IOPSCopyPowerSourcesList(info.raw()) });
    if list.raw().is_null() {
        return None;
    }
    let keys = Keys::new();
    // SAFETY: `list` is a valid `CFArray`.
    let count = unsafe { CFArrayGetCount(list.raw()) };
    for index in 0..count {
        // SAFETY: `index` is in `0..count`; the element is borrowed from `list`.
        let source = unsafe { CFArrayGetValueAtIndex(list.raw(), index) };
        if source.is_null() {
            continue;
        }
        // SAFETY: `source` is a handle from `list` and `info` its snapshot; the
        // description is borrowed and must not be released.
        let description = unsafe { IOPSGetPowerSourceDescription(info.raw(), source) };
        if description.is_null() {
            continue;
        }
        let reading = read_source(description, &keys);
        // The AC-adapter source carries no capacity, so `battery_from_reading`
        // returns `None` for it and the loop moves on; the first present source
        // with a capacity is the internal battery.
        if let Some(metrics) = battery_from_reading(&reading) {
            return Some(metrics);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reading(
        present: bool,
        current: Option<i64>,
        max: Option<i64>,
        charging: Option<bool>,
        to_empty: Option<i64>,
        to_full: Option<i64>,
    ) -> SourceReading {
        SourceReading {
            present,
            current_capacity: current,
            max_capacity: max,
            charging,
            time_to_empty_minutes: to_empty,
            time_to_full_minutes: to_full,
            state: SourceState::Battery,
        }
    }

    #[test]
    fn charge_is_current_capacity_over_maximum() {
        let metrics =
            battery_from_reading(&reading(true, Some(40), Some(100), Some(false), None, None));
        let Some(metrics) = metrics else {
            panic!("a battery reporting capacity must produce metrics");
        };
        assert!((metrics.charge.get() - 40.0).abs() < 0.01);
    }

    #[test]
    fn a_battery_that_will_not_report_capacity_is_withheld_not_zeroed() {
        // The actual em-dash-versus-zero bug: `Some(0)` here would say "flat".
        assert!(battery_from_reading(&reading(true, None, Some(100), None, None, None)).is_none());
        assert!(battery_from_reading(&reading(true, Some(50), None, None, None, None)).is_none());
        assert!(battery_from_reading(&reading(true, None, None, None, None, None)).is_none());
    }

    #[test]
    fn an_absent_source_produces_nothing() {
        // A Mac mini and an iMac have no battery; so does a slot that is in the
        // array but reports `Is Present` false.
        assert!(
            battery_from_reading(&reading(false, Some(50), Some(100), None, None, None)).is_none()
        );
    }

    #[test]
    fn a_negative_capacity_sentinel_is_not_a_reading() {
        // IOKit uses -1 for "cannot tell"; treating it as a number would give a
        // nonsense charge, so the whole reading is withheld.
        assert!(
            battery_from_reading(&reading(true, Some(-1), Some(100), None, None, None)).is_none()
        );
    }

    #[test]
    fn a_zero_maximum_does_not_divide_by_zero() {
        assert!(battery_from_reading(&reading(true, Some(0), Some(0), None, None, None)).is_none());
    }

    #[test]
    fn a_genuine_zero_capacity_is_reported() {
        // Zero current against a real maximum is a measured zero, not absence.
        let Some(metrics) =
            battery_from_reading(&reading(true, Some(0), Some(100), Some(false), None, None))
        else {
            panic!("a measured 0 percent is a real reading");
        };
        assert!(metrics.charge.get().abs() < 0.01);
    }

    #[test]
    fn a_time_estimate_is_reported_in_seconds() {
        let Some(metrics) = battery_from_reading(&reading(
            true,
            Some(80),
            Some(100),
            Some(false),
            Some(90),
            None,
        )) else {
            panic!("metrics expected");
        };
        assert_eq!(metrics.time_remaining_secs, Some(90 * 60));
        assert!(!metrics.charging);
    }

    #[test]
    fn a_still_calculating_sentinel_is_not_a_time() {
        // -1 is "still calculating"; it must not surface as a time.
        let Some(metrics) = battery_from_reading(&reading(
            true,
            Some(80),
            Some(100),
            Some(false),
            Some(-1),
            None,
        )) else {
            panic!("metrics expected");
        };
        assert!(metrics.time_remaining_secs.is_none());
    }

    #[test]
    fn charging_uses_time_to_full() {
        let Some(metrics) = battery_from_reading(&reading(
            true,
            Some(30),
            Some(100),
            Some(true),
            Some(200),
            Some(45),
        )) else {
            panic!("metrics expected");
        };
        assert!(metrics.charging);
        assert_eq!(metrics.time_remaining_secs, Some(45 * 60));
    }

    #[test]
    fn draw_health_cycle_count_and_temperature_are_unavailable_not_invented() {
        // They need the raw gauge; `None` says "not read", a zero would say
        // "worn out" and "never charged".
        let Some(metrics) =
            battery_from_reading(&reading(true, Some(50), Some(100), Some(false), None, None))
        else {
            panic!("metrics expected");
        };
        assert!(metrics.power.is_none());
        assert!(metrics.health.is_none());
        assert!(metrics.cycle_count.is_none());
        assert!(metrics.temperature.is_none());
    }

    #[test]
    fn the_live_read_is_wired_and_returns() {
        // Whatever this machine is, the call must complete rather than trap: on
        // a Mac with a battery it is a reading, on a Mac mini it is `None`. The
        // value itself is whatever the machine is doing, so only its range is
        // asserted.
        if let Some(metrics) = battery() {
            let charge = metrics.charge.get();
            assert!(
                (0.0..=100.0).contains(&charge),
                "charge reported as {charge}"
            );
        }
    }
}
