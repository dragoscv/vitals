//! The list Windows Device Manager shows: every device node, grouped by setup
//! class, read unelevated through `SetupAPI`.
//!
//! Read on demand only — a screen asks for it. Enumerating a few hundred
//! nodes and their driver properties costs tens of milliseconds, which is
//! fine once per visit and wasteful once per second.
//!
//! Every value that cannot be read is `None`. A device with no driver
//! version recorded has no driver version; it does not have version "".

mod setupapi;

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Where Device Manager files nodes that have no setup class — usually the
/// ones Windows found no driver for.
const OTHER_DEVICES: &str = "Other devices";

/// `CR_SUCCESS` from cfgmgr32.
const CR_SUCCESS: u32 = 0;
/// `CM_PROB_DISABLED`: the user (or policy) turned the device off. Device
/// Manager shows it with a down arrow, not a warning, so it is its own state.
const CM_PROB_DISABLED: u32 = 22;
/// `DN_NO_SHOW_IN_DM`: the driver asked to be hidden from Device Manager.
const DN_NO_SHOW_IN_DM: u32 = 0x4000_0000;

/// One setup class ("Display adapters") and the devices in it.
#[derive(Debug, Clone, PartialEq)]
pub struct DeviceClass {
    /// `{4d36e968-e325-11ce-bfc1-08002be10318}`, lowercase with braces.
    pub guid: String,
    /// The class's registry name, e.g. `Display`.
    pub name: String,
    /// What Device Manager prints as the heading; the name when absent.
    pub description: String,
    pub devices: Vec<Device>,
}

/// One device node.
#[derive(Debug, Clone, PartialEq)]
pub struct Device {
    pub instance_id: String,
    /// Friendly name, else device description, else the instance id — the
    /// id is the device's real identity, so it is never a made-up label.
    pub name: String,
    pub manufacturer: Option<String>,
    pub driver_provider: Option<String>,
    pub driver_version: Option<String>,
    /// `yyyy-mm-dd`, the date the driver package declares (not install time).
    pub driver_date: Option<String>,
    pub location: Option<String>,
    /// The bus that enumerated it: `PCI`, `USB`, `HID`, `ACPI`, `SWD`, `ROOT`.
    pub enumerator: Option<String>,
    pub status: DeviceStatus,
    pub present: bool,
    /// Device Manager hides it unless "Show hidden devices" is on.
    pub hidden: bool,
}

/// What Device Manager's icon overlay would say about a device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceStatus {
    Ok,
    Disabled,
    /// A `CM_PROB_*` code; [`problem_description`] explains it.
    Problem {
        code: u32,
    },
    /// Known to Windows but not currently connected.
    NotPresent,
}

/// Every class with at least one device, in the order Device Manager lists
/// them.
#[derive(Debug, Clone, PartialEq)]
pub struct DeviceInventory {
    /// Sorted by description, case-insensitively; devices sorted by name.
    pub classes: Vec<DeviceClass>,
    /// How long the read took, so a screen can show that it is not free.
    pub elapsed: Duration,
}

/// Class metadata, resolved once per GUID.
#[derive(Debug, Clone, PartialEq)]
struct ClassMeta {
    guid: String,
    name: String,
    description: String,
    /// `NoDisplayClass`: every device in the class is hidden by Device Manager.
    no_display: bool,
}

/// Status, presence and visibility decided together, because all three
/// come from the one `CM_Get_DevNode_Status` call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NodeStatus {
    status: DeviceStatus,
    present: bool,
    hidden: bool,
}

/// Reads the device tree. Never fails: a list that cannot be opened is an
/// empty inventory, which the screen shows as such.
///
/// `include_not_present` adds nodes Windows remembers but which are not
/// connected — Device Manager's "Show hidden devices".
pub fn read_devices(include_not_present: bool) -> DeviceInventory {
    let started = Instant::now();
    let classes = group(setupapi::enumerate(include_not_present));
    DeviceInventory {
        classes,
        elapsed: started.elapsed(),
    }
}

/// The sentence Device Manager shows for a `CM_PROB_*` code, reworded for
/// someone who does not know what a device node is.
pub fn problem_description(code: u32) -> &'static str {
    match code {
        1 => "This device is not configured correctly",
        3 => "The driver may be corrupted, or the system is low on memory",
        10 => "This device cannot start",
        12 => "Not enough free resources for this device",
        14 => "This device needs a restart to work properly",
        18 => "The drivers for this device need to be reinstalled",
        19 => "Its configuration in the registry is incomplete or damaged",
        21 => "Windows is removing this device",
        22 => "This device is disabled",
        24 => "This device is not present, not working, or missing its drivers",
        28 => "The drivers for this device are not installed",
        29 => "The firmware has disabled this device",
        31 => "This device is not working properly: its driver could not load",
        32 => "The driver service for this device is disabled",
        37 => "The driver reported a failure while starting",
        39 => "The driver is corrupted or missing",
        43 => "Windows stopped this device because it reported problems",
        45 => "This device is not connected",
        47 => "This device has been prepared for safe removal",
        48 => "Its driver is blocked because of known problems",
        49 => "The registry has reached its size limit",
        52 => "Windows cannot verify the digital signature of its driver",
        _ => "Windows reported a problem with this device",
    }
}

/// Maps `CM_Get_DevNode_Status`'s result. Any failure — typically
/// `CR_NO_SUCH_DEVINST` — means the node is not live, so its flags are
/// meaningless and are not read.
fn map_status(configret: u32, flags: u32, problem: u32) -> NodeStatus {
    if configret != CR_SUCCESS {
        return NodeStatus {
            status: DeviceStatus::NotPresent,
            present: false,
            hidden: false,
        };
    }
    let status = match problem {
        0 => DeviceStatus::Ok,
        CM_PROB_DISABLED => DeviceStatus::Disabled,
        code => DeviceStatus::Problem { code },
    };
    NodeStatus {
        status,
        present: true,
        hidden: flags & DN_NO_SHOW_IN_DM != 0,
    }
}

/// Groups devices under their classes and sorts both levels the way Device
/// Manager does. A class's `NoDisplayClass` makes its devices hidden.
fn group(entries: Vec<(ClassMeta, Device)>) -> Vec<DeviceClass> {
    let mut by_guid: HashMap<String, DeviceClass> = HashMap::new();
    for (meta, mut device) in entries {
        device.hidden |= meta.no_display;
        by_guid
            .entry(meta.guid.clone())
            .or_insert_with(|| DeviceClass {
                guid: meta.guid,
                name: meta.name,
                description: meta.description,
                devices: Vec::new(),
            })
            .devices
            .push(device);
    }
    let mut classes: Vec<DeviceClass> = by_guid.into_values().collect();
    for class in &mut classes {
        class.devices.sort_by(|a, b| {
            a.name
                .to_lowercase()
                .cmp(&b.name.to_lowercase())
                .then_with(|| a.instance_id.cmp(&b.instance_id))
        });
    }
    // The GUID breaks ties so two classes sharing a description still come
    // out in a stable order from one read to the next.
    classes.sort_by(|a, b| {
        a.description
            .to_lowercase()
            .cmp(&b.description.to_lowercase())
            .then_with(|| a.guid.cmp(&b.guid))
    });
    classes
}

/// 100 ns ticks in a day.
const TICKS_PER_DAY: u64 = 864_000_000_000;
/// Days from 1601-01-01 (the FILETIME epoch) to 1970-01-01.
const FILETIME_EPOCH_DAYS: i64 = 134_774;

/// A FILETIME (100 ns ticks since 1601-01-01 UTC) as `yyyy-mm-dd`.
///
/// Zero is `None`: it is what an unset FILETIME holds, and 1601-01-01 is not
/// a date any driver was written on.
fn filetime_to_iso_date(ticks: u64) -> Option<String> {
    if ticks == 0 {
        return None;
    }
    let days = i64::try_from(ticks / TICKS_PER_DAY).ok()? - FILETIME_EPOCH_DAYS;
    let (year, month, day) = civil_from_days(days);
    Some(format!("{year:04}-{month:02}-{day:02}"))
}

/// Howard Hinnant's `civil_from_days`: days since 1970-01-01 to a proleptic
/// Gregorian date. Pure integer arithmetic, so no date crate is needed.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// The registry's form of a GUID, lowercase — what `Get-PnpDevice` prints
/// in `ClassGuid`, so the two can be compared directly.
fn format_guid(guid: &windows::core::GUID) -> String {
    format_guid_parts(guid.data1, guid.data2, guid.data3, guid.data4)
}

fn format_guid_parts(data1: u32, data2: u16, data3: u16, d: [u8; 8]) -> String {
    format!(
        "{{{data1:08x}-{data2:04x}-{data3:04x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}}}",
        d[0], d[1], d[2], d[3], d[4], d[5], d[6], d[7]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(guid: &str, description: &str, no_display: bool) -> ClassMeta {
        ClassMeta {
            guid: guid.to_owned(),
            name: description.to_owned(),
            description: description.to_owned(),
            no_display,
        }
    }

    fn device(name: &str) -> Device {
        Device {
            instance_id: format!("ROOT\\{name}\\0000"),
            name: name.to_owned(),
            manufacturer: None,
            driver_provider: None,
            driver_version: None,
            driver_date: None,
            location: None,
            enumerator: None,
            status: DeviceStatus::Ok,
            present: true,
            hidden: false,
        }
    }

    fn filetime_at(unix_seconds: u64) -> u64 {
        (unix_seconds + 11_644_473_600) * 10_000_000
    }

    #[test]
    fn the_inbox_driver_date_filetime_reads_as_2006_06_21() {
        // 2006-06-21T00:00:00Z: the date on every in-box Windows driver.
        let ticks = filetime_at(1_150_848_000);
        assert_eq!(filetime_to_iso_date(ticks).as_deref(), Some("2006-06-21"));
        // Any time during that day is still that day.
        let evening = ticks + 23 * 3_600 * 10_000_000;
        assert_eq!(filetime_to_iso_date(evening).as_deref(), Some("2006-06-21"));
    }

    #[test]
    fn leap_days_and_the_unix_epoch_convert_exactly() {
        assert_eq!(
            filetime_to_iso_date(filetime_at(0)).as_deref(),
            Some("1970-01-01")
        );
        // 2024-02-29T12:00:00Z
        assert_eq!(
            filetime_to_iso_date(filetime_at(1_709_208_000)).as_deref(),
            Some("2024-02-29")
        );
        assert_eq!(filetime_to_iso_date(1).as_deref(), Some("1601-01-01"));
    }

    #[test]
    fn an_unset_filetime_is_no_date_rather_than_1601() {
        assert_eq!(filetime_to_iso_date(0), None);
    }

    #[test]
    fn a_disabled_device_is_disabled_not_a_problem() {
        let node = map_status(CR_SUCCESS, 0, 22);
        assert_eq!(node.status, DeviceStatus::Disabled);
        assert!(node.present);
    }

    #[test]
    fn a_problem_code_is_kept_and_a_clean_node_is_ok() {
        assert_eq!(
            map_status(CR_SUCCESS, 0, 43).status,
            DeviceStatus::Problem { code: 43 }
        );
        assert_eq!(map_status(CR_SUCCESS, 0, 0).status, DeviceStatus::Ok);
    }

    #[test]
    fn a_node_with_no_live_devinst_is_not_present_and_its_flags_ignored() {
        // CR_NO_SUCH_DEVINST, with garbage flags that must not leak through.
        let node = map_status(13, DN_NO_SHOW_IN_DM, 22);
        assert_eq!(node.status, DeviceStatus::NotPresent);
        assert!(!node.present);
        assert!(!node.hidden);
    }

    #[test]
    fn the_no_show_flag_hides_a_device_whatever_its_status() {
        assert!(map_status(CR_SUCCESS, DN_NO_SHOW_IN_DM | 0x0A, 0).hidden);
        assert!(map_status(CR_SUCCESS, DN_NO_SHOW_IN_DM, 43).hidden);
        assert!(!map_status(CR_SUCCESS, 0x0A, 0).hidden);
    }

    #[test]
    fn code_43_is_explained_and_an_unknown_code_gets_the_generic_sentence() {
        assert!(problem_description(43).contains("reported problems"));
        assert_eq!(
            problem_description(9999),
            "Windows reported a problem with this device"
        );
        assert_ne!(problem_description(22), problem_description(9999));
    }

    #[test]
    fn devices_group_by_class_and_both_levels_sort_case_insensitively() {
        let display = meta("{b}", "display adapters", false);
        let disks = meta("{a}", "Disk drives", false);
        let classes = group(vec![
            (display.clone(), device("zeta GPU")),
            (disks, device("Samsung SSD")),
            (display, device("Alpha GPU")),
        ]);
        let headings: Vec<&str> = classes.iter().map(|c| c.description.as_str()).collect();
        assert_eq!(headings, ["Disk drives", "display adapters"]);
        let names: Vec<&str> = classes[1].devices.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, ["Alpha GPU", "zeta GPU"]);
        assert_eq!(classes[0].devices.len(), 1);
    }

    #[test]
    fn a_no_display_class_hides_its_devices_and_a_hidden_device_stays_hidden() {
        let mut already_hidden = device("Volume Manager");
        already_hidden.hidden = true;
        let classes = group(vec![
            (meta("{s}", "Software components", true), device("Codec")),
            (meta("{v}", "System devices", false), already_hidden),
            (meta("{v}", "System devices", false), device("PCI bus")),
        ]);
        assert!(classes[0].devices[0].hidden);
        let system = &classes[1].devices;
        assert!(
            !system
                .iter()
                .find(|d| d.name == "PCI bus")
                .is_some_and(|d| d.hidden)
        );
        assert!(
            system
                .iter()
                .find(|d| d.name == "Volume Manager")
                .is_some_and(|d| d.hidden)
        );
    }

    #[test]
    fn guids_format_lowercase_with_braces_like_the_registry() {
        let text = format_guid_parts(
            0x4D36_E968,
            0xE325,
            0x11CE,
            [0xBF, 0xC1, 0x08, 0x00, 0x2B, 0xE1, 0x03, 0x18],
        );
        assert_eq!(text, "{4d36e968-e325-11ce-bfc1-08002be10318}");
        assert_eq!(
            format_guid_parts(0, 0, 0, [0; 8]),
            "{00000000-0000-0000-0000-000000000000}"
        );
    }
}
