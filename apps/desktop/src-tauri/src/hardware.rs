//! Hardware inventory and the Device Manager list, for the Devices screen.
//!
//! Both are on-demand `invoke`s for the reasons `inventory` gives: slow
//! (WMI, `SetupAPI`), rarely changing, and only wanted while the screen is
//! open. Both run on a pool thread — a WMI connect is hundreds of
//! milliseconds cold.
//!
//! Desktop-only on purpose: serial numbers and the device list identify the
//! machine, so neither is served over the LAN API. Serials are cut to their
//! last four characters here, before they reach the webview at all — enough
//! to tell two identical sticks apart, not enough to identify the part.

// Both commands are Windows-only; `serial_tail` and the result alias stay
// compiled everywhere so the serial-redaction tests run on Linux CI too.
#![cfg_attr(not(windows), allow(dead_code))]

use serde::Serialize;

use crate::commands::CommandError;

type CommandResult<T> = std::result::Result<T, CommandError>;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HardwareDto {
    pub cpus: Vec<CpuDto>,
    pub memory: MemoryDto,
    pub gpus: Vec<GpuDto>,
    pub drives: Vec<DriveDto>,
    pub board: BoardDto,
    pub elapsed_ms: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CpuDto {
    pub name: String,
    pub manufacturer: Option<String>,
    pub socket: Option<String>,
    pub cores: Option<u32>,
    pub threads: Option<u32>,
    pub base_clock_mhz: Option<u32>,
    pub l2_cache_kb: Option<u32>,
    pub l3_cache_kb: Option<u32>,
    pub virtualization: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryDto {
    pub usable_bytes: Option<u64>,
    pub slots: Option<u32>,
    pub max_capacity_bytes: Option<u64>,
    pub modules: Vec<MemoryModuleDto>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryModuleDto {
    pub slot: Option<String>,
    pub capacity_bytes: Option<u64>,
    pub kind: Option<String>,
    pub form_factor: Option<String>,
    pub speed_mts: Option<u32>,
    pub configured_speed_mts: Option<u32>,
    pub manufacturer: Option<String>,
    pub part_number: Option<String>,
    pub serial_tail: Option<String>,
    pub voltage_mv: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuDto {
    pub name: String,
    pub manufacturer: Option<String>,
    pub driver_version: Option<String>,
    pub driver_date: Option<String>,
    pub video_memory_bytes: Option<u64>,
    pub resolution: Option<String>,
    pub refresh_hz: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveDto {
    pub index: u32,
    pub model: String,
    /// `ssd`, `hdd`, `unknown`.
    pub media: &'static str,
    pub bus: Option<String>,
    pub size_bytes: Option<u64>,
    pub health: Option<String>,
    pub firmware: Option<String>,
    pub serial_tail: Option<String>,
    pub spindle_rpm: Option<u32>,
    pub temperature_celsius: Option<f32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardDto {
    pub manufacturer: Option<String>,
    pub product: Option<String>,
    pub version: Option<String>,
    pub bios_vendor: Option<String>,
    pub bios_version: Option<String>,
    pub bios_date: Option<String>,
    pub system_manufacturer: Option<String>,
    pub system_model: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceTreeDto {
    pub classes: Vec<DeviceClassDto>,
    pub elapsed_ms: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceClassDto {
    pub guid: String,
    pub name: String,
    pub description: String,
    pub devices: Vec<DeviceDto>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceDto {
    pub instance_id: String,
    pub name: String,
    pub manufacturer: Option<String>,
    pub driver_provider: Option<String>,
    pub driver_version: Option<String>,
    pub driver_date: Option<String>,
    pub location: Option<String>,
    pub enumerator: Option<String>,
    /// `ok`, `disabled`, `problem`, `notPresent`.
    pub status: &'static str,
    pub problem_code: Option<u32>,
    /// Device Manager's own wording for `problem_code`.
    pub problem: Option<&'static str>,
    pub present: bool,
    pub hidden: bool,
}

/// The last four characters, or `None` for anything shorter — a serial of
/// four characters or fewer would be sent whole.
fn serial_tail(serial: Option<&str>) -> Option<String> {
    let chars: Vec<char> = serial?.trim().chars().collect();
    (chars.len() > 4).then(|| chars[chars.len() - 4..].iter().collect())
}

#[tauri::command]
#[cfg(windows)]
pub async fn get_hardware() -> CommandResult<HardwareDto> {
    tauri::async_runtime::spawn_blocking(|| hardware_dto(vitals_win::hardware::read_inventory()))
        .await
        .map_err(|err| CommandError::Internal {
            message: format!("the hardware read was abandoned: {err}"),
        })
}

#[tauri::command]
#[cfg(windows)]
pub async fn get_device_tree(include_hidden: bool) -> CommandResult<DeviceTreeDto> {
    tauri::async_runtime::spawn_blocking(move || {
        device_tree_dto(vitals_win::devices::read_devices(include_hidden))
    })
    .await
    .map_err(|err| CommandError::Internal {
        message: format!("the device list read was abandoned: {err}"),
    })
}

#[cfg(windows)]
fn hardware_dto(inv: vitals_win::hardware::HardwareInventory) -> HardwareDto {
    use vitals_win::hardware::DriveMedia;

    HardwareDto {
        cpus: inv
            .cpu
            .into_iter()
            .map(|c| CpuDto {
                name: c.name,
                manufacturer: c.manufacturer,
                socket: c.socket,
                cores: c.cores,
                threads: c.logical_processors,
                base_clock_mhz: c.base_clock_mhz,
                l2_cache_kb: c.l2_cache_kb,
                l3_cache_kb: c.l3_cache_kb,
                virtualization: c.virtualization_enabled,
            })
            .collect(),
        memory: MemoryDto {
            usable_bytes: inv.memory.total_bytes,
            slots: inv.memory.slots_total,
            max_capacity_bytes: inv.memory.max_capacity_bytes,
            modules: inv
                .memory
                .modules
                .into_iter()
                .map(|m| MemoryModuleDto {
                    serial_tail: serial_tail(m.serial.as_deref()),
                    slot: m.slot,
                    capacity_bytes: m.capacity_bytes,
                    kind: m.kind,
                    form_factor: m.form_factor,
                    speed_mts: m.speed_mts,
                    configured_speed_mts: m.configured_speed_mts,
                    manufacturer: m.manufacturer,
                    part_number: m.part_number,
                    voltage_mv: m.voltage_mv,
                })
                .collect(),
        },
        gpus: inv
            .gpus
            .into_iter()
            .map(|g| GpuDto {
                name: g.name,
                manufacturer: g.manufacturer,
                driver_version: g.driver_version,
                driver_date: g.driver_date,
                video_memory_bytes: g.video_memory_bytes,
                resolution: g.resolution,
                refresh_hz: g.refresh_hz,
            })
            .collect(),
        drives: inv
            .drives
            .into_iter()
            .map(|d| DriveDto {
                serial_tail: serial_tail(d.serial.as_deref()),
                index: d.index,
                model: d.model,
                media: match d.media {
                    DriveMedia::Ssd => "ssd",
                    DriveMedia::Hdd => "hdd",
                    DriveMedia::Unknown => "unknown",
                },
                bus: d.bus,
                size_bytes: d.size_bytes,
                health: d.health,
                firmware: d.firmware,
                spindle_rpm: d.spindle_rpm,
                temperature_celsius: d.temperature_celsius,
            })
            .collect(),
        board: BoardDto {
            manufacturer: inv.board.manufacturer,
            product: inv.board.product,
            version: inv.board.version,
            bios_vendor: inv.board.bios_vendor,
            bios_version: inv.board.bios_version,
            bios_date: inv.board.bios_date,
            system_manufacturer: inv.board.system_manufacturer,
            system_model: inv.board.system_model,
        },
        elapsed_ms: inv.elapsed.as_secs_f64() * 1000.0,
    }
}

#[cfg(windows)]
fn device_tree_dto(inv: vitals_win::devices::DeviceInventory) -> DeviceTreeDto {
    use vitals_win::devices::{DeviceStatus, problem_description};

    DeviceTreeDto {
        classes: inv
            .classes
            .into_iter()
            .map(|class| DeviceClassDto {
                guid: class.guid,
                name: class.name,
                description: class.description,
                devices: class
                    .devices
                    .into_iter()
                    .map(|d| {
                        let (status, code) = match d.status {
                            DeviceStatus::Ok => ("ok", None),
                            DeviceStatus::Disabled => ("disabled", None),
                            DeviceStatus::Problem { code } => ("problem", Some(code)),
                            DeviceStatus::NotPresent => ("notPresent", None),
                        };
                        DeviceDto {
                            instance_id: d.instance_id,
                            name: d.name,
                            manufacturer: d.manufacturer,
                            driver_provider: d.driver_provider,
                            driver_version: d.driver_version,
                            driver_date: d.driver_date,
                            location: d.location,
                            enumerator: d.enumerator,
                            status,
                            problem_code: code,
                            problem: code.map(problem_description),
                            present: d.present,
                            hidden: d.hidden,
                        }
                    })
                    .collect(),
            })
            .collect(),
        elapsed_ms: inv.elapsed.as_secs_f64() * 1000.0,
    }
}

#[cfg(test)]
mod tests {
    use super::serial_tail;

    #[test]
    fn a_serial_reaches_the_webview_only_as_its_last_four_characters() {
        assert_eq!(
            serial_tail(Some("0025_388B_91C1_6D00")).as_deref(),
            Some("6D00")
        );
        assert_eq!(serial_tail(Some("  ABCDEFGH  ")).as_deref(), Some("EFGH"));
    }

    #[test]
    fn a_short_serial_is_withheld_rather_than_sent_whole() {
        assert_eq!(serial_tail(Some("1234")), None);
        assert_eq!(serial_tail(Some("")), None);
        assert_eq!(serial_tail(None), None);
    }
}
