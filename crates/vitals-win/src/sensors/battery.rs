//! Battery state, from the miniport rather than from WMI.
//!
//! # Why the IOCTLs and not `Win32_Battery`
//!
//! `Win32_Battery` exposes `EstimatedChargeRemaining` as a whole percent and
//! nothing else that matters: no design capacity, no full-charge capacity, no
//! cycle count, and a `DesignCapacity` property that is `null` on most
//! machines. Battery *health* — the number a user actually wants, because it
//! decides whether to replace the pack — is unobtainable from it.
//!
//! `IOCTL_BATTERY_QUERY_INFORMATION` returns design and full-charge capacity
//! and the cycle count directly from the smart-battery gauge, and
//! `IOCTL_BATTERY_QUERY_STATUS` returns instantaneous rate and voltage. It
//! costs a device-interface enumeration plus three IOCTLs, all of which are
//! sub-millisecond, against WMI's tens of milliseconds.
//!
//! # The two-call protocol, and the tag
//!
//! Every status query must carry a *battery tag* obtained from
//! `IOCTL_BATTERY_QUERY_TAG`. The tag invalidates when the pack is swapped;
//! a stale tag makes the driver fail the query rather than return another
//! battery's data. Caching the tag across samples is therefore wrong — it is
//! re-fetched every time, which is what makes hot-swap detection free.

use std::ffi::c_void;
use std::ptr;

use vitals_core::units::{Percent, Volts, Watts};

use super::convert::{
    charge_percent, health_percent, millivolts_to_volts, milliwatts_to_watts, seconds_remaining,
};

/// `GUID_DEVICE_BATTERY` `{72631E54-78A4-11D0-BCF7-00AA00B7B32A}`, in the
/// little-endian byte order a `GUID` actually has in memory.
///
/// A `static` rather than a `const` because its address is passed to
/// `SetupAPI`: a `const` is materialised as a temporary at each use site, and
/// `&raw const` on one is rejected outright.
static GUID_DEVICE_BATTERY: [u8; 16] = [
    0x54, 0x1E, 0x63, 0x72, 0xA4, 0x78, 0xD0, 0x11, 0xBC, 0xF7, 0x00, 0xAA, 0x00, 0xB7, 0xB3, 0x2A,
];

const DIGCF_PRESENT: u32 = 0x0000_0002;
const DIGCF_DEVICEINTERFACE: u32 = 0x0000_0010;

const GENERIC_READ: u32 = 0x8000_0000;
const GENERIC_WRITE: u32 = 0x4000_0000;
const FILE_SHARE_READ: u32 = 0x0000_0001;
const FILE_SHARE_WRITE: u32 = 0x0000_0002;
const OPEN_EXISTING: u32 = 3;

const ERROR_INSUFFICIENT_BUFFER: u32 = 122;
const ERROR_NO_MORE_ITEMS: u32 = 259;

// IOCTLs, assembled from CTL_CODE(FILE_DEVICE_BATTERY=0x29, function,
// METHOD_BUFFERED=0, FILE_READ_ACCESS=1). Spelled as literals because the
// macro is not available and re-deriving it at each site invites a typo in a
// value that would fail as "invalid function" rather than anything legible.
const IOCTL_BATTERY_QUERY_TAG: u32 = 0x0029_4040;
const IOCTL_BATTERY_QUERY_INFORMATION: u32 = 0x0029_4044;
const IOCTL_BATTERY_QUERY_STATUS: u32 = 0x0029_404C;

/// `BATTERY_TAG_INVALID`
const BATTERY_TAG_INVALID: u32 = 0;

/// `BatteryInformation` in the `BATTERY_QUERY_INFORMATION_LEVEL` enum.
const BATTERY_INFORMATION_LEVEL: u32 = 0;

/// `BATTERY_SYSTEM_BATTERY` — set when the pack powers the machine rather
/// than a peripheral. A wireless mouse presents a battery device too, and
/// showing its charge on the system power page would be nonsense.
const BATTERY_SYSTEM_BATTERY: u32 = 0x8000_0000;

const BATTERY_CAPACITY_RELATIVE: u32 = 0x4000_0000;
const BATTERY_IS_SHORT_TERM: u32 = 0x2000_0000;

const BATTERY_POWER_ON_LINE: u32 = 0x0000_0001;
const BATTERY_DISCHARGING: u32 = 0x0000_0002;
const BATTERY_CHARGING: u32 = 0x0000_0004;
const BATTERY_CRITICAL: u32 = 0x0000_0008;

/// `HANDLE`.
///
/// Spelled as a pointer rather than the `isize` this module would otherwise
/// prefer, because `storage::ffi` declares `CreateFileW` and
/// `DeviceIoControl` the same way. Two `extern` blocks in one crate that
/// disagree about a symbol's signature trip `clashing_extern_declarations`,
/// and the sibling's spelling is the one that must be matched — it is not
/// this module's to change.
type Handle = *mut c_void;
type HDevInfo = *mut c_void;

/// `INVALID_HANDLE_VALUE`, as a pointer.
fn invalid_handle() -> Handle {
    (-1_isize) as Handle
}

#[repr(C)]
#[derive(Clone, Copy)]
struct SpDevinfoData {
    cb_size: u32,
    class_guid: [u8; 16],
    dev_inst: u32,
    reserved: usize,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct SpDeviceInterfaceData {
    cb_size: u32,
    interface_class_guid: [u8; 16],
    flags: u32,
    reserved: usize,
}

/// `SP_DEVICE_INTERFACE_DETAIL_DATA_W`, header only.
///
/// The real structure is variable-length: a `u32` size followed by an inline
/// NUL-terminated path. It is never declared with the path inline because
/// `cb_size` must be the size of the *header* — 8 on 64-bit, because the
/// `u16` path member forces alignment to 4 and the struct to 8 — not the
/// size of the buffer being passed. Passing the buffer size here is the
/// classic `SetupAPI` mistake and yields `ERROR_INVALID_USER_BUFFER`.
#[repr(C)]
struct SpDeviceInterfaceDetailHeader {
    cb_size: u32,
    path: [u16; 1],
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct BatteryQueryInformation {
    battery_tag: u32,
    information_level: u32,
    at_rate: i32,
}

/// `BATTERY_INFORMATION`.
///
/// Field order is load-bearing and the layout is pinned by test: a wrong
/// size here does not fail loudly, it makes `DeviceIoControl` write fewer
/// bytes than expected and leaves `cycle_count` reading whatever was in the
/// tail of the struct.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct BatteryInformation {
    capabilities: u32,
    technology: u8,
    reserved: [u8; 3],
    chemistry: [u8; 4],
    designed_capacity: u32,
    full_charged_capacity: u32,
    default_alert1: u32,
    default_alert2: u32,
    critical_bias: u32,
    cycle_count: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct BatteryWaitStatus {
    battery_tag: u32,
    timeout: u32,
    power_state: u32,
    low_capacity: u32,
    high_capacity: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct BatteryStatus {
    power_state: u32,
    capacity: u32,
    voltage: u32,
    rate: i32,
}

#[link(name = "setupapi")]
unsafe extern "system" {
    fn SetupDiGetClassDevsW(
        class_guid: *const [u8; 16],
        enumerator: *const u16,
        parent: *mut c_void,
        flags: u32,
    ) -> HDevInfo;
    fn SetupDiDestroyDeviceInfoList(dev_info: HDevInfo) -> i32;
    fn SetupDiEnumDeviceInterfaces(
        dev_info: HDevInfo,
        dev_info_data: *const SpDevinfoData,
        interface_class_guid: *const [u8; 16],
        member_index: u32,
        interface_data: *mut SpDeviceInterfaceData,
    ) -> i32;
    fn SetupDiGetDeviceInterfaceDetailW(
        dev_info: HDevInfo,
        interface_data: *const SpDeviceInterfaceData,
        detail: *mut SpDeviceInterfaceDetailHeader,
        detail_size: u32,
        required_size: *mut u32,
        dev_info_data: *mut SpDevinfoData,
    ) -> i32;
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn CreateFileW(
        file_name: *const u16,
        desired_access: u32,
        share_mode: u32,
        security_attributes: *const c_void,
        creation_disposition: u32,
        flags_and_attributes: u32,
        template: Handle,
    ) -> Handle;
    fn CloseHandle(handle: Handle) -> i32;
    fn DeviceIoControl(
        device: Handle,
        control_code: u32,
        in_buffer: *const c_void,
        in_size: u32,
        out_buffer: *mut c_void,
        out_size: u32,
        bytes_returned: *mut u32,
        overlapped: *mut c_void,
    ) -> i32;
    fn GetLastError() -> u32;
}

/// A `SetupDi` device-information set that frees itself.
struct DevInfoSet(HDevInfo);

impl Drop for DevInfoSet {
    fn drop(&mut self) {
        // SAFETY: `self.0` came from SetupDiGetClassDevsW, is not the
        // invalid sentinel (checked at construction) and is destroyed once.
        unsafe { SetupDiDestroyDeviceInfoList(self.0) };
    }
}

/// An open device handle that closes itself.
struct DeviceHandle(Handle);

impl Drop for DeviceHandle {
    fn drop(&mut self) {
        // SAFETY: `self.0` came from CreateFileW, is not INVALID_HANDLE_VALUE
        // (checked at construction) and is closed once.
        unsafe { CloseHandle(self.0) };
    }
}

/// What the pack is doing right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChargeState {
    Charging,
    Discharging,
    /// On mains, not drawing and not charging — the usual state of a laptop
    /// left plugged in above its charge-stop threshold.
    Idle,
    /// The driver reported neither charging nor discharging, and no line
    /// power. Rare, and not worth inventing a state for.
    Unknown,
}

/// A system battery.
///
/// Every capacity is in milliwatt-hours as the miniport reports it, except
/// where a conversion is unambiguous. Fields are `Option` wherever the gauge
/// may decline to answer, which is often: cheap packs report a cycle count of
/// zero forever, and a zero cycle count is indistinguishable from a new pack.
#[derive(Debug, Clone)]
pub struct Battery {
    /// Device interface path, stable for the life of the enumeration.
    pub device_path: String,
    /// `LION`, `LiP`, `NiMH` — four ASCII bytes from the gauge.
    pub chemistry: String,
    /// Charge as a fraction of *present* full capacity, not design capacity.
    pub charge: Option<Percent>,
    /// Present full capacity over design capacity.
    ///
    /// [`Quality::Derived`](super::reading::Quality::Derived): a ratio of two
    /// firmware constants, not a live measurement.
    pub health: Option<Percent>,
    /// Instantaneous charge or discharge magnitude. Direction is in `state`.
    pub rate: Option<Watts>,
    pub voltage: Option<Volts>,
    pub state: ChargeState,
    /// Capacity as manufactured, in milliwatt-hours.
    pub design_capacity_mwh: Option<u32>,
    /// Capacity when fully charged today, in milliwatt-hours.
    pub full_charge_capacity_mwh: Option<u32>,
    /// Charge cycles, when the gauge counts them.
    ///
    /// `None` rather than `0` when unreported, because a great many packs
    /// report zero permanently and a "0 cycles" badge on a five-year-old
    /// laptop is a lie the user would believe.
    pub cycle_count: Option<u32>,
    /// Seconds until empty at the current drain, or `None` when not
    /// discharging.
    pub seconds_to_empty: Option<u32>,
    /// The gauge reports relative capacity, so the mWh figures are unitless
    /// gauge counts rather than energy.
    ///
    /// When set, capacity values must not be rendered with a Wh suffix. The
    /// ratios (charge, health) remain valid because the unit cancels.
    pub capacity_is_relative: bool,
    /// A UPS bridging a mains dropout rather than a laptop pack.
    ///
    /// Its runtime estimate means minutes to orderly shutdown, not hours of
    /// portable use, and the two must not share a column.
    pub is_short_term: bool,
}

/// Enumerates system batteries.
///
/// Returns an empty vector on a desktop, which is the common case and not an
/// error. A UPS attached over HID also appears here, correctly — it is a
/// system battery by the driver's own classification.
///
/// Peripheral batteries (mice, styluses) are excluded: they present the same
/// device interface but without `BATTERY_SYSTEM_BATTERY`, and listing a
/// mouse on the power page would be absurd.
#[must_use]
pub fn enumerate_batteries() -> Vec<Battery> {
    // SAFETY: the GUID pointer is valid for the call; a null enumerator and
    // parent are documented as "all devices, no parent window".
    let raw = unsafe {
        SetupDiGetClassDevsW(
            &raw const GUID_DEVICE_BATTERY,
            ptr::null(),
            ptr::null_mut(),
            DIGCF_PRESENT | DIGCF_DEVICEINTERFACE,
        )
    };

    if raw.is_null() || raw == invalid_handle() {
        return Vec::new();
    }

    let dev_info = DevInfoSet(raw);
    let mut out = Vec::new();

    for index in 0.. {
        let Some(path) = interface_path(&dev_info, index) else {
            break;
        };

        if let Some(battery) = query_battery(&path) {
            out.push(battery);
        }
    }

    out
}

/// The device path of interface `index`, or `None` once the set is exhausted.
fn interface_path(dev_info: &DevInfoSet, index: u32) -> Option<String> {
    let mut interface = SpDeviceInterfaceData {
        cb_size: u32::try_from(size_of::<SpDeviceInterfaceData>()).ok()?,
        interface_class_guid: [0; 16],
        flags: 0,
        reserved: 0,
    };

    // SAFETY: `dev_info.0` is a live device-information set; `interface` has
    // its `cb_size` set as the API requires, which is how it validates the
    // caller was compiled against a compatible header.
    let ok = unsafe {
        SetupDiEnumDeviceInterfaces(
            dev_info.0,
            ptr::null(),
            &raw const GUID_DEVICE_BATTERY,
            index,
            &raw mut interface,
        )
    };

    if ok == 0 {
        // SAFETY: no arguments.
        debug_assert_eq!(unsafe { GetLastError() }, ERROR_NO_MORE_ITEMS);
        return None;
    }

    // Two-call protocol again: ask for the size, then allocate. The first
    // call is *expected* to fail with ERROR_INSUFFICIENT_BUFFER, so treating
    // its zero return as an error would abandon every battery on the system.
    let mut required: u32 = 0;

    // SAFETY: a null detail pointer with a zero size is the documented way
    // to request the required length.
    unsafe {
        SetupDiGetDeviceInterfaceDetailW(
            dev_info.0,
            &raw const interface,
            ptr::null_mut(),
            0,
            &raw mut required,
            ptr::null_mut(),
        );
    }

    // SAFETY: no arguments.
    if unsafe { GetLastError() } != ERROR_INSUFFICIENT_BUFFER || required == 0 {
        return None;
    }

    // A `Vec<u8>` is only byte-aligned and casting it to the detail struct
    // would be UB; `Vec<u64>` gives the 8-byte alignment the header needs.
    let words = (required as usize).div_ceil(size_of::<u64>());
    let mut buffer: Vec<u64> = vec![0; words];
    let detail = buffer.as_mut_ptr().cast::<SpDeviceInterfaceDetailHeader>();

    // `cb_size` is the size of the fixed header, NOT of the buffer. On
    // 64-bit that is 8: a u32 followed by a u16 array, aligned to 8 by the
    // enclosing structure. Writing `required` here is the standard SetupAPI
    // error and produces ERROR_INVALID_USER_BUFFER.
    // SAFETY: `detail` points at `required` bytes of zeroed, aligned storage.
    unsafe {
        (*detail).cb_size = u32::try_from(size_of::<SpDeviceInterfaceDetailHeader>()).ok()?;
    }

    // SAFETY: `detail` is aligned and `required` bytes long; `cb_size` is set.
    let ok = unsafe {
        SetupDiGetDeviceInterfaceDetailW(
            dev_info.0,
            &raw const interface,
            detail,
            required,
            ptr::null_mut(),
            ptr::null_mut(),
        )
    };

    if ok == 0 {
        return None;
    }

    // SAFETY: on success the API wrote a NUL-terminated UTF-16 path starting
    // at the `path` member, within the `required` bytes allocated.
    let path_ptr = unsafe { (&raw const (*detail).path).cast::<u16>() };
    // SAFETY: as above; the string is NUL-terminated by the API contract.
    Some(unsafe { wide_string(path_ptr) })
}

/// Reads a NUL-terminated UTF-16 string.
///
/// # Safety
///
/// `ptr` must point at a NUL-terminated UTF-16 sequence that stays valid for
/// the duration of the call.
unsafe fn wide_string(ptr: *const u16) -> String {
    let mut len = 0;
    // SAFETY: the caller guarantees a NUL terminator, so the walk halts.
    while unsafe { *ptr.add(len) } != 0 {
        len += 1;
    }
    // SAFETY: `len` units before the terminator are all initialised.
    let slice = unsafe { std::slice::from_raw_parts(ptr, len) };
    String::from_utf16_lossy(slice)
}

/// Opens one battery device and reads it, or `None` if it is not a system
/// battery or declines to answer.
fn query_battery(path: &str) -> Option<Battery> {
    let wide: Vec<u16> = path.encode_utf16().chain(Some(0)).collect();

    // SAFETY: `wide` is NUL-terminated and outlives the call; a null security
    // descriptor means default, and a zero template handle means none.
    let raw = unsafe {
        CreateFileW(
            wide.as_ptr(),
            GENERIC_READ | GENERIC_WRITE,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            ptr::null(),
            OPEN_EXISTING,
            0,
            ptr::null_mut(),
        )
    };

    if raw.is_null() || raw == invalid_handle() {
        return None;
    }

    let device = DeviceHandle(raw);

    // The tag is re-fetched every sample rather than cached: it invalidates
    // when the pack is swapped, and a stale tag makes every subsequent query
    // fail rather than silently return the wrong battery.
    let tag = query_tag(&device)?;
    if tag == BATTERY_TAG_INVALID {
        return None;
    }

    let info = query_information(&device, tag)?;

    // A wireless mouse presents this same interface. Without the flag it is
    // a peripheral, and belongs nowhere near the system power page.
    if info.capabilities & BATTERY_SYSTEM_BATTERY == 0 {
        return None;
    }

    let status = query_status(&device, tag)?;

    let capacity_is_relative = info.capabilities & BATTERY_CAPACITY_RELATIVE != 0;

    let design = normalise_capacity(info.designed_capacity);
    let full = normalise_capacity(info.full_charged_capacity);

    let state = if status.power_state & BATTERY_CHARGING != 0 {
        ChargeState::Charging
    } else if status.power_state & BATTERY_DISCHARGING != 0 {
        ChargeState::Discharging
    } else if status.power_state & BATTERY_POWER_ON_LINE != 0 {
        ChargeState::Idle
    } else {
        ChargeState::Unknown
    };

    Some(Battery {
        device_path: path.to_owned(),
        chemistry: chemistry_label(info.chemistry),
        charge: full.and_then(|f| charge_percent(status.capacity, f)),
        health: design.zip(full).and_then(|(d, f)| health_percent(f, d)),
        rate: milliwatts_to_watts(status.rate),
        voltage: millivolts_to_volts(status.voltage),
        state,
        design_capacity_mwh: design,
        full_charge_capacity_mwh: full,
        cycle_count: (info.cycle_count > 0).then_some(info.cycle_count),
        seconds_to_empty: seconds_remaining(status.capacity, status.rate),
        capacity_is_relative,
        // A short-term battery is a UPS bridging a mains dropout. Its
        // capacity is real, but its "time remaining" answers a different
        // question from a laptop's, so it is flagged rather than silently
        // mixed into the same column.
        is_short_term: info.capabilities & BATTERY_IS_SHORT_TERM != 0,
    })
}

/// `BATTERY_UNKNOWN_CAPACITY` mapped to `None`.
fn normalise_capacity(raw: u32) -> Option<u32> {
    (raw != u32::MAX && raw != 0).then_some(raw)
}

/// The four-byte chemistry code as text, e.g. `LION`.
fn chemistry_label(raw: [u8; 4]) -> String {
    let text: String = raw
        .iter()
        .copied()
        .take_while(|b| *b != 0)
        .map(char::from)
        .collect();

    if text.trim().is_empty() {
        "unknown".to_owned()
    } else {
        text.trim().to_owned()
    }
}

fn query_tag(device: &DeviceHandle) -> Option<u32> {
    // A zero timeout means "do not wait for a battery to appear". The
    // documented default blocks, which on a machine with an empty bay would
    // stall the sample thread for the driver's own timeout.
    let timeout: u32 = 0;
    let mut tag: u32 = 0;
    let mut returned: u32 = 0;

    // SAFETY: both buffers are live locals of exactly the declared sizes.
    let ok = unsafe {
        DeviceIoControl(
            device.0,
            IOCTL_BATTERY_QUERY_TAG,
            (&raw const timeout).cast(),
            u32::try_from(size_of::<u32>()).ok()?,
            (&raw mut tag).cast(),
            u32::try_from(size_of::<u32>()).ok()?,
            &raw mut returned,
            ptr::null_mut(),
        )
    };

    (ok != 0).then_some(tag)
}

fn query_information(device: &DeviceHandle, tag: u32) -> Option<BatteryInformation> {
    let query = BatteryQueryInformation {
        battery_tag: tag,
        information_level: BATTERY_INFORMATION_LEVEL,
        at_rate: 0,
    };
    let mut info = BatteryInformation::default();
    let mut returned: u32 = 0;

    // SAFETY: both buffers are live locals of exactly the declared sizes.
    let ok = unsafe {
        DeviceIoControl(
            device.0,
            IOCTL_BATTERY_QUERY_INFORMATION,
            (&raw const query).cast(),
            u32::try_from(size_of::<BatteryQueryInformation>()).ok()?,
            (&raw mut info).cast(),
            u32::try_from(size_of::<BatteryInformation>()).ok()?,
            &raw mut returned,
            ptr::null_mut(),
        )
    };

    // A short write means the driver answered with a struct smaller than the
    // one declared here, which would leave the tail fields reading zeroed
    // storage that is indistinguishable from a real zero cycle count.
    if ok == 0 || returned as usize != size_of::<BatteryInformation>() {
        return None;
    }

    Some(info)
}

fn query_status(device: &DeviceHandle, tag: u32) -> Option<BatteryStatus> {
    // `timeout` zero plus both capacity bounds zero means "return the
    // current state immediately". The IOCTL is otherwise a *wait*: it blocks
    // until the state crosses one of the bounds, which would hang the
    // sampler indefinitely on an idle machine.
    let wait = BatteryWaitStatus {
        battery_tag: tag,
        timeout: 0,
        power_state: 0,
        low_capacity: 0,
        high_capacity: 0,
    };
    let mut status = BatteryStatus::default();
    let mut returned: u32 = 0;

    // SAFETY: both buffers are live locals of exactly the declared sizes.
    let ok = unsafe {
        DeviceIoControl(
            device.0,
            IOCTL_BATTERY_QUERY_STATUS,
            (&raw const wait).cast(),
            u32::try_from(size_of::<BatteryWaitStatus>()).ok()?,
            (&raw mut status).cast(),
            u32::try_from(size_of::<BatteryStatus>()).ok()?,
            &raw mut returned,
            ptr::null_mut(),
        )
    };

    if ok == 0 || returned as usize != size_of::<BatteryStatus>() {
        return None;
    }

    Some(status)
}

/// Whether the machine reports itself as critically low on battery.
#[must_use]
pub fn is_critical(power_state: u32) -> bool {
    power_state & BATTERY_CRITICAL != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    // Layout pins. A struct declared at the wrong size does not fail loudly
    // here: DeviceIoControl writes the driver's own number of bytes and the
    // remainder reads as zero, so `cycle_count` would silently become 0 —
    // exactly the fabricated-plausible-value failure this crate exists to
    // avoid. These assertions are the only thing standing between that bug
    // and shipping.
    #[test]
    fn battery_information_layout_is_pinned() {
        // 36, and this assertion earned its place immediately: the first
        // guess here was 40. Ten u32-sized fields with no interior padding —
        // the u8 and its three reserved bytes fill one word exactly, and the
        // four chemistry bytes fill another — so nothing rounds up.
        assert_eq!(size_of::<BatteryInformation>(), 36);
        assert_eq!(align_of::<BatteryInformation>(), 4);
    }

    #[test]
    fn battery_status_layout_is_pinned() {
        assert_eq!(size_of::<BatteryStatus>(), 16);
    }

    #[test]
    fn battery_query_information_layout_is_pinned() {
        assert_eq!(size_of::<BatteryQueryInformation>(), 12);
    }

    #[test]
    fn battery_wait_status_layout_is_pinned() {
        assert_eq!(size_of::<BatteryWaitStatus>(), 20);
    }

    #[test]
    fn setupapi_structs_are_pinned() {
        // SP_DEVICE_INTERFACE_DATA: u32 + GUID + u32 + usize, padded to 8.
        assert_eq!(size_of::<SpDeviceInterfaceData>(), 32);
        assert_eq!(size_of::<SpDevinfoData>(), 32);
        // The value written into `cb_size`, and the single most common
        // SetupAPI mistake if it is anything else.
        assert_eq!(size_of::<SpDeviceInterfaceDetailHeader>(), 8);
    }

    #[test]
    fn guid_bytes_match_the_documented_battery_class() {
        // Data1 is little-endian in memory: 72631E54 -> 54 1E 63 72.
        assert_eq!(&GUID_DEVICE_BATTERY[..4], &[0x54, 0x1E, 0x63, 0x72]);
        // Data4 is a plain byte array, so it is NOT byte-swapped.
        assert_eq!(
            &GUID_DEVICE_BATTERY[8..],
            &[0xBC, 0xF7, 0x00, 0xAA, 0x00, 0xB7, 0xB3, 0x2A]
        );
    }

    #[test]
    fn chemistry_trims_padding_and_survives_a_blank_gauge() {
        assert_eq!(chemistry_label(*b"LION"), "LION");
        assert_eq!(chemistry_label([b'L', b'i', b'P', 0]), "LiP");
        assert_eq!(chemistry_label([0; 4]), "unknown");
        assert_eq!(chemistry_label(*b"    "), "unknown");
    }

    #[test]
    fn unknown_capacity_is_none_not_zero() {
        assert_eq!(normalise_capacity(u32::MAX), None);
        assert_eq!(normalise_capacity(0), None);
        assert_eq!(normalise_capacity(50_000), Some(50_000));
    }

    #[test]
    fn enumeration_is_infallible_on_a_desktop() {
        // The contract that matters: no panic, no error, just an empty list
        // on a machine with no pack.
        let _ = enumerate_batteries();
    }
}
