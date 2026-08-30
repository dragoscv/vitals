//! System power state: AC line, power scheme, and the overlay slider.
//!
//! Two distinct things are conflated by most tools and kept apart here:
//!
//! * A **power scheme** is the classic plan — Balanced, High performance —
//!   identified by a GUID and enumerated through `powrprof`.
//! * A **power overlay** is the Windows 10+ slider (Best power efficiency …
//!   Best performance) that sits *on top of* the active scheme. Changing the
//!   slider does not change the scheme GUID, which is why a tool that reads
//!   only the scheme reports "Balanced" on a machine the user has pinned to
//!   maximum performance.
//!
//! The CPU tab's power-mode switching needs both.

use std::ffi::c_void;
use std::ptr;

use vitals_core::units::Percent;

/// `SYSTEM_POWER_STATUS.ACLineStatus`
const AC_OFFLINE: u8 = 0;
const AC_ONLINE: u8 = 1;

/// `BATTERY_PERCENTAGE_UNKNOWN` / `BATTERY_LIFE_UNKNOWN`.
const UNKNOWN_U8: u8 = 255;
const UNKNOWN_U32: u32 = u32::MAX;

const BATTERY_FLAG_NO_BATTERY: u8 = 128;

/// `SystemBatteryState` in `POWER_INFORMATION_LEVEL`.
const SYSTEM_BATTERY_STATE: u32 = 5;

const STATUS_SUCCESS: i32 = 0;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct SystemPowerStatus {
    ac_line_status: u8,
    battery_flag: u8,
    battery_life_percent: u8,
    system_status_flag: u8,
    battery_life_time: u32,
    battery_full_life_time: u32,
}

/// `SYSTEM_BATTERY_STATE` from `CallNtPowerInformation`.
///
/// A cheaper, aggregate view than the per-device IOCTLs: it needs no device
/// enumeration and sums every pack. It is kept alongside them because it is
/// the only source for `estimated_time` that accounts for the OS's own
/// smoothing, and because it answers "does this machine have a battery at
/// all" in one call.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct SystemBatteryState {
    ac_on_line: u8,
    battery_present: u8,
    charging: u8,
    discharging: u8,
    spare1: [u8; 3],
    tag: u8,
    max_capacity: u32,
    remaining_capacity: u32,
    rate: u32,
    estimated_time: u32,
    default_alert1: u32,
    default_alert2: u32,
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetSystemPowerStatus(status: *mut SystemPowerStatus) -> i32;
}

#[link(name = "powrprof")]
unsafe extern "system" {
    fn PowerGetActiveScheme(user_root: *mut c_void, policy_guid: *mut *mut [u8; 16]) -> u32;
    fn CallNtPowerInformation(
        level: u32,
        input: *mut c_void,
        input_size: u32,
        output: *mut c_void,
        output_size: u32,
    ) -> i32;
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn LocalFree(mem: *mut c_void) -> *mut c_void;
}

/// A GUID buffer allocated by `powrprof` and freed with `LocalFree`.
///
/// `PowerGetActiveScheme` allocates with `LocalAlloc`, not `CoTaskMemAlloc`
/// and not the CRT allocator. Freeing it any other way corrupts the heap
/// rather than failing, which is why this exists instead of a bare pointer.
struct LocalGuid(*mut [u8; 16]);

impl Drop for LocalGuid {
    fn drop(&mut self) {
        // SAFETY: allocated by PowerGetActiveScheme via LocalAlloc, freed once.
        unsafe { LocalFree(self.0.cast()) };
    }
}

/// Whether the machine is on mains.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineStatus {
    Ac,
    Battery,
    /// The firmware declined to say. Genuinely happens in virtual machines.
    Unknown,
}

/// The Windows power slider, distinct from the scheme.
///
/// Identified by overlay GUID. The four documented values are stable across
/// builds; an unrecognised GUID means an OEM overlay, which is reported as
/// such rather than forced into the nearest known bucket.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerMode {
    BestPowerEfficiency,
    Balanced,
    BestPerformance,
    /// A scheme GUID this build does not recognise, usually an OEM plan.
    Custom,
}

/// The system's power situation.
#[derive(Debug, Clone)]
pub struct PowerState {
    pub line: LineStatus,
    /// Whole-percent battery charge as the OS reports it.
    ///
    /// Coarser than [`super::battery::Battery::charge`], which is computed
    /// from milliwatt-hours. Present because it is the figure the taskbar
    /// shows, and a discrepancy between the two would otherwise look like a
    /// bug in ours.
    pub battery_percent: Option<Percent>,
    /// Seconds of runtime the OS estimates, when it is willing to guess.
    ///
    /// Windows returns `BATTERY_LIFE_UNKNOWN` for the first minutes after a
    /// state change while its estimator settles. That is `None`, not zero —
    /// a zero would render as "0 minutes remaining" on a full battery.
    pub seconds_remaining: Option<u32>,
    pub has_battery: bool,
    /// GUID of the active power scheme.
    pub scheme_guid: Option<[u8; 16]>,
    pub mode: PowerMode,
    /// Whether the OS is currently in battery-saver mode.
    pub power_saver: bool,
}

// The three built-in scheme GUIDs, in the byte order they occupy in memory.
//
// `Data1`, `Data2` and `Data3` are little-endian; `Data4` is a plain byte
// array and is NOT swapped. Storing them in printed order instead is a real
// mistake that was made here first: `powercfg` reported High performance
// while `classify_scheme` returned `Custom`, because the comparison was
// against bytes that never appear in memory. It fails silently — every
// scheme becomes "Custom" — which is why the probe cross-check exists.

/// `GUID_MAX_POWER_SAVINGS` — `a1841308-3541-4fab-bc81-f71556f20b4a`,
/// "Power saver".
const GUID_MAX_POWER_SAVINGS: [u8; 16] = [
    0x08, 0x13, 0x84, 0xA1, 0x41, 0x35, 0xAB, 0x4F, 0xBC, 0x81, 0xF7, 0x15, 0x56, 0xF2, 0x0B, 0x4A,
];

/// `GUID_TYPICAL_POWER_SAVINGS` — `381b4222-f694-41f0-9685-ff5bb260df2e`,
/// "Balanced".
const GUID_TYPICAL_POWER_SAVINGS: [u8; 16] = [
    0x22, 0x42, 0x1B, 0x38, 0x94, 0xF6, 0xF0, 0x41, 0x96, 0x85, 0xFF, 0x5B, 0xB2, 0x60, 0xDF, 0x2E,
];

/// `GUID_MIN_POWER_SAVINGS` — `8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c`,
/// "High performance".
const GUID_MIN_POWER_SAVINGS: [u8; 16] = [
    0xDA, 0x7F, 0x5E, 0x8C, 0xBF, 0xE8, 0x96, 0x4A, 0x9A, 0x85, 0xA6, 0xE2, 0x3A, 0x8C, 0x63, 0x5C,
];

/// Classifies a scheme GUID.
///
/// The three built-in plans are matched exactly; anything else is
/// [`PowerMode::Custom`]. Guessing from the plan *name* was considered and
/// rejected: names are localised, and an OEM "Dell Optimized" plan would be
/// silently reported as Balanced on the strength of a substring.
#[must_use]
pub fn classify_scheme(guid: [u8; 16]) -> PowerMode {
    match guid {
        GUID_MAX_POWER_SAVINGS => PowerMode::BestPowerEfficiency,
        GUID_TYPICAL_POWER_SAVINGS => PowerMode::Balanced,
        GUID_MIN_POWER_SAVINGS => PowerMode::BestPerformance,
        _ => PowerMode::Custom,
    }
}

/// Reads the system power state.
///
/// Infallible by design: a desktop with no battery is a legitimate machine,
/// and every unavailable field is `None` rather than a stand-in.
#[must_use]
pub fn read_power_state() -> PowerState {
    let mut status = SystemPowerStatus::default();

    // SAFETY: `status` is a live local of exactly the declared size.
    let ok = unsafe { GetSystemPowerStatus(&raw mut status) };

    let (line, battery_percent, seconds_remaining, has_battery, power_saver) = if ok == 0 {
        (LineStatus::Unknown, None, None, false, false)
    } else {
        let line = match status.ac_line_status {
            AC_ONLINE => LineStatus::Ac,
            AC_OFFLINE => LineStatus::Battery,
            _ => LineStatus::Unknown,
        };

        let has_battery =
            status.battery_flag & BATTERY_FLAG_NO_BATTERY == 0 && status.battery_flag != UNKNOWN_U8;

        (
            line,
            (status.battery_life_percent != UNKNOWN_U8)
                .then(|| Percent::new(f32::from(status.battery_life_percent))),
            (status.battery_life_time != UNKNOWN_U32).then_some(status.battery_life_time),
            has_battery,
            status.system_status_flag == 1,
        )
    };

    let scheme_guid = active_scheme();

    PowerState {
        line,
        battery_percent,
        seconds_remaining,
        has_battery,
        scheme_guid,
        mode: scheme_guid.map_or(PowerMode::Custom, classify_scheme),
        power_saver,
    }
}

/// The active power scheme GUID.
fn active_scheme() -> Option<[u8; 16]> {
    let mut raw: *mut [u8; 16] = ptr::null_mut();

    // SAFETY: a null user root means the current user; the out-parameter is
    // a live local.
    let status = unsafe { PowerGetActiveScheme(ptr::null_mut(), &raw mut raw) };

    if status != 0 || raw.is_null() {
        return None;
    }

    let owned = LocalGuid(raw);
    // SAFETY: on success the API wrote a 16-byte GUID at `raw`, which stays
    // valid until `owned` drops at the end of this function.
    Some(unsafe { *owned.0 })
}

/// The aggregate battery view from `CallNtPowerInformation`.
///
/// Returns `None` when the call fails or the machine reports no pack.
/// Cheaper than [`super::battery::enumerate_batteries`] — one call, no
/// device enumeration — but it cannot report design capacity, health or
/// cycle count, which is why both exist.
#[must_use]
pub fn aggregate_battery() -> Option<AggregateBattery> {
    let mut state = SystemBatteryState::default();

    // SAFETY: a null input with zero length is the documented form for a
    // query-only level; `state` is a live local of the declared size.
    let status = unsafe {
        CallNtPowerInformation(
            SYSTEM_BATTERY_STATE,
            ptr::null_mut(),
            0,
            (&raw mut state).cast(),
            u32::try_from(size_of::<SystemBatteryState>()).ok()?,
        )
    };

    if status != STATUS_SUCCESS || state.battery_present == 0 {
        return None;
    }

    Some(AggregateBattery {
        on_ac: state.ac_on_line != 0,
        charging: state.charging != 0,
        discharging: state.discharging != 0,
        // These are milliwatt-hours only when the packs report absolute
        // capacity. A relative-capacity gauge makes them unitless counts,
        // which the per-device path flags and this aggregate cannot.
        max_capacity: (state.max_capacity != UNKNOWN_U32).then_some(state.max_capacity),
        remaining_capacity: (state.remaining_capacity != UNKNOWN_U32)
            .then_some(state.remaining_capacity),
        estimated_seconds: (state.estimated_time != UNKNOWN_U32).then_some(state.estimated_time),
    })
}

/// The OS's summed view across every pack.
#[derive(Debug, Clone, Copy)]
pub struct AggregateBattery {
    pub on_ac: bool,
    pub charging: bool,
    pub discharging: bool,
    pub max_capacity: Option<u32>,
    pub remaining_capacity: Option<u32>,
    pub estimated_seconds: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn power_status_layout_is_pinned() {
        // Four bytes then two u32s: 12, not 16. A wrong size makes
        // GetSystemPowerStatus write past or short of the struct.
        assert_eq!(size_of::<SystemPowerStatus>(), 12);
        assert_eq!(align_of::<SystemPowerStatus>(), 4);
    }

    #[test]
    fn system_battery_state_layout_is_pinned() {
        // 5 bytes of flags, 3 spare, tag, then padding to align the u32s.
        assert_eq!(size_of::<SystemBatteryState>(), 32);
        assert_eq!(align_of::<SystemBatteryState>(), 4);
    }

    #[test]
    fn built_in_schemes_are_recognised() {
        assert_eq!(
            classify_scheme(GUID_MIN_POWER_SAVINGS),
            PowerMode::BestPerformance
        );
        assert_eq!(
            classify_scheme(GUID_TYPICAL_POWER_SAVINGS),
            PowerMode::Balanced
        );
        assert_eq!(
            classify_scheme(GUID_MAX_POWER_SAVINGS),
            PowerMode::BestPowerEfficiency
        );
    }

    #[test]
    fn an_oem_scheme_is_custom_not_the_nearest_guess() {
        assert_eq!(classify_scheme([0x11; 16]), PowerMode::Custom);
    }

    #[test]
    fn high_performance_guid_is_in_memory_order_not_printed_order() {
        // powercfg /getactivescheme prints 8c5e7fda-…, so the first four
        // bytes in memory are the reverse of that. Asserting the printed
        // order here would pass while classify_scheme returned Custom for
        // every plan — the exact bug this replaced.
        assert_eq!(&GUID_MIN_POWER_SAVINGS[..4], &[0xDA, 0x7F, 0x5E, 0x8C]);
        // Data4 is a byte array and must NOT be reversed.
        assert_eq!(&GUID_MIN_POWER_SAVINGS[8..10], &[0x9A, 0x85]);
    }

    #[test]
    fn the_three_built_in_schemes_are_distinct() {
        // A copy-paste slip between three 16-byte literals is invisible on
        // inspection and would silently merge two plans.
        assert_ne!(GUID_MIN_POWER_SAVINGS, GUID_MAX_POWER_SAVINGS);
        assert_ne!(GUID_MIN_POWER_SAVINGS, GUID_TYPICAL_POWER_SAVINGS);
        assert_ne!(GUID_MAX_POWER_SAVINGS, GUID_TYPICAL_POWER_SAVINGS);
    }

    #[test]
    fn reading_power_state_never_panics_on_a_desktop() {
        let state = read_power_state();
        // A machine with no pack must not invent a charge figure.
        if !state.has_battery {
            assert!(state.battery_percent.is_none() || state.line == LineStatus::Ac);
        }
    }

    #[test]
    fn aggregate_battery_is_none_without_a_pack() {
        // No assertion on the value: this must pass on laptops too. The
        // contract under test is that it does not panic or fabricate.
        let _ = aggregate_battery();
    }
}
