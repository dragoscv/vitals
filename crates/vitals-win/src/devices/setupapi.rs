//! The `SetupAPI` and configuration-manager calls behind [`super::read_devices`].
//!
//! Kept apart from the pure grouping and mapping logic so that everything
//! which can be tested without a machine is tested without one, and this file
//! holds nothing but FFI and the buffer handling it forces on us.

use std::collections::HashMap;

use windows::Win32::Devices::DeviceAndDriverInstallation::{
    CM_DEVNODE_STATUS_FLAGS, CM_Get_DevNode_Status, CM_PROB, DICLASSPROP_INSTALLER,
    DIGCF_ALLCLASSES, DIGCF_PRESENT, HDEVINFO, SETUP_DI_REGISTRY_PROPERTY, SP_DEVINFO_DATA,
    SPDRP_DEVICEDESC, SPDRP_ENUMERATOR_NAME, SPDRP_FRIENDLYNAME, SPDRP_LOCATION_INFORMATION,
    SPDRP_MFG, SetupDiClassNameFromGuidW, SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInfo,
    SetupDiGetClassDescriptionW, SetupDiGetClassDevsW, SetupDiGetClassPropertyW,
    SetupDiGetDeviceInstanceIdW, SetupDiGetDevicePropertyW, SetupDiGetDeviceRegistryPropertyW,
};
use windows::Win32::Devices::Properties::{
    DEVPKEY_Device_DriverDate, DEVPKEY_Device_DriverProvider, DEVPKEY_Device_DriverVersion,
    DEVPKEY_DeviceClass_NoDisplayClass, DEVPROP_TYPE_BOOLEAN, DEVPROP_TYPE_FILETIME,
    DEVPROP_TYPE_STRING, DEVPROPTYPE,
};
use windows::Win32::Foundation::{DEVPROPKEY, ERROR_INSUFFICIENT_BUFFER};
use windows::core::{GUID, PCWSTR};

use super::{ClassMeta, Device, OTHER_DEVICES, filetime_to_iso_date, format_guid, map_status};

/// A device-information set that frees itself, so an early return can never
/// leak the list (it holds a reference on every device node it names).
struct DevInfoSet(HDEVINFO);

impl Drop for DevInfoSet {
    fn drop(&mut self) {
        // SAFETY: the handle came from a successful SetupDiGetClassDevsW and
        // is destroyed exactly once, here.
        let _ = unsafe { SetupDiDestroyDeviceInfoList(self.0) };
    }
}

/// Every device node with its class metadata, in enumeration order.
///
/// An empty vector when the set cannot be opened: the caller promises never
/// to fail, and "no devices could be read" is the honest answer.
pub(super) fn enumerate(include_not_present: bool) -> Vec<(ClassMeta, Device)> {
    let flags = if include_not_present {
        DIGCF_ALLCLASSES
    } else {
        DIGCF_ALLCLASSES | DIGCF_PRESENT
    };
    // SAFETY: no class filter, no enumerator, no parent window; the returned
    // handle is owned by the guard below.
    let t0 = std::time::Instant::now();
    let Ok(handle) = (unsafe { SetupDiGetClassDevsW(None, PCWSTR::null(), None, flags) }) else {
        return Vec::new();
    };
    eprintln!("TIMING open {:?}", t0.elapsed());
    let set = DevInfoSet(handle);
    let mut classes: HashMap<GUID, ClassMeta> = HashMap::new();
    let mut out = Vec::new();
    for index in 0.. {
        let mut data = SP_DEVINFO_DATA {
            cbSize: size_of::<SP_DEVINFO_DATA>() as u32,
            ..Default::default()
        };
        // SAFETY: `data` is a correctly sized SP_DEVINFO_DATA; the loop ends
        // on the first failure, which is ERROR_NO_MORE_ITEMS in practice.
        if unsafe { SetupDiEnumDeviceInfo(set.0, index, &raw mut data) }.is_err() {
            break;
        }
        // A node without an instance id cannot be identified or acted on,
        // and inventing one would be exactly the placeholder we refuse.
        let Some(device) = read_device(&set, &data) else {
            continue;
        };
        let meta = classes
            .entry(data.ClassGuid)
            .or_insert_with(|| class_meta(&data.ClassGuid))
            .clone();
        out.push((meta, device));
    }
    eprintln!("TIMING total {:?} classes {}", t0.elapsed(), classes.len());
    out
}

fn read_device(set: &DevInfoSet, data: &SP_DEVINFO_DATA) -> Option<Device> {
    let t = std::time::Instant::now();
    let _p = property_string(set, data, &DEVPKEY_Device_DriverProvider);
    let _v = property_string(set, data, &DEVPKEY_Device_DriverVersion);
    let _d = property_filetime(set, data, &DEVPKEY_Device_DriverDate);
    let drv = t.elapsed();
    let t = std::time::Instant::now();
    let _ = registry_string(set, data, SPDRP_MFG);
    let _ = registry_string(set, data, SPDRP_LOCATION_INFORMATION);
    let _ = registry_string(set, data, SPDRP_ENUMERATOR_NAME);
    let _ = registry_string(set, data, SPDRP_FRIENDLYNAME);
    let reg = t.elapsed();
    let t = std::time::Instant::now();
    let mut f = CM_DEVNODE_STATUS_FLAGS(0);
    let mut p = CM_PROB(0);
    let _ = unsafe { CM_Get_DevNode_Status(&raw mut f, &raw mut p, data.DevInst, 0) };
    let cm = t.elapsed();
    eprintln!(
        "TDEV {} {} {}",
        drv.as_micros(),
        reg.as_micros(),
        cm.as_micros()
    );
    let instance_id = instance_id(set, data)?;
    let name = registry_string(set, data, SPDRP_FRIENDLYNAME)
        .or_else(|| registry_string(set, data, SPDRP_DEVICEDESC))
        .unwrap_or_else(|| instance_id.clone());
    let mut flags = CM_DEVNODE_STATUS_FLAGS(0);
    let mut problem = CM_PROB(0);
    // SAFETY: both out-pointers are valid locals; DevInst came from the set.
    let result =
        unsafe { CM_Get_DevNode_Status(&raw mut flags, &raw mut problem, data.DevInst, 0) };
    let node = map_status(result.0, flags.0, problem.0);
    Some(Device {
        instance_id,
        name,
        manufacturer: registry_string(set, data, SPDRP_MFG),
        driver_provider: property_string(set, data, &DEVPKEY_Device_DriverProvider),
        driver_version: property_string(set, data, &DEVPKEY_Device_DriverVersion),
        driver_date: property_filetime(set, data, &DEVPKEY_Device_DriverDate)
            .and_then(filetime_to_iso_date),
        location: registry_string(set, data, SPDRP_LOCATION_INFORMATION),
        enumerator: registry_string(set, data, SPDRP_ENUMERATOR_NAME),
        status: node.status,
        present: node.present,
        hidden: node.hidden,
    })
}

fn instance_id(set: &DevInfoSet, data: &SP_DEVINFO_DATA) -> Option<String> {
    // MAX_DEVICE_ID_LEN is 200; twice that leaves room without a retry.
    let mut buffer = [0u16; 400];
    // SAFETY: the buffer is a valid, writable slice whose length is passed.
    unsafe { SetupDiGetDeviceInstanceIdW(set.0, data, Some(&mut buffer), None) }.ok()?;
    utf16_until_nul(&buffer)
}

fn class_meta(guid: &GUID) -> ClassMeta {
    let text = format_guid(guid);
    if *guid == GUID::zeroed() {
        // Device Manager files nodes with no class under "Other devices";
        // they are usually the ones missing a driver.
        return ClassMeta {
            guid: text,
            name: "Other".to_owned(),
            description: OTHER_DEVICES.to_owned(),
            no_display: false,
        };
    }
    // Class names are at most MAX_CLASS_NAME_LEN (32); descriptions LINE_LEN (256).
    let mut name_buf = [0u16; 64];
    let mut desc_buf = [0u16; 512];
    // SAFETY: valid GUID pointer and writable slices whose lengths are passed.
    let name = unsafe { SetupDiClassNameFromGuidW(guid, &mut name_buf, None) }
        .ok()
        .and_then(|()| utf16_until_nul(&name_buf))
        // The GUID is the class's identity, so it is a truthful name when the
        // registry has none — not a stand-in value.
        .unwrap_or_else(|| text.clone());
    // SAFETY: as above.
    let description = unsafe { SetupDiGetClassDescriptionW(guid, &mut desc_buf, None) }
        .ok()
        .and_then(|()| utf16_until_nul(&desc_buf))
        .unwrap_or_else(|| name.clone());
    ClassMeta {
        guid: text,
        name,
        description,
        no_display: class_no_display(guid),
    }
}

fn class_no_display(guid: &GUID) -> bool {
    let mut ty = DEVPROPTYPE(0);
    let mut value = [0u8; 1];
    // SAFETY: valid key, type and one-byte buffer (DEVPROP_BOOLEAN is a byte).
    let read = unsafe {
        SetupDiGetClassPropertyW(
            guid,
            &DEVPKEY_DeviceClass_NoDisplayClass,
            &raw mut ty,
            Some(&mut value),
            None,
            DICLASSPROP_INSTALLER,
        )
    };
    // DEVPROP_TRUE is 0xFF; anything non-zero is treated as set.
    read.is_ok() && ty == DEVPROP_TYPE_BOOLEAN && value[0] != 0
}

fn registry_string(
    set: &DevInfoSet,
    data: &SP_DEVINFO_DATA,
    property: SETUP_DI_REGISTRY_PROPERTY,
) -> Option<String> {
    let bytes = read_buffer(|buffer, required| {
        // SAFETY: the buffer is writable and its length is passed; `required`
        // points at a live local in `read_buffer`.
        unsafe {
            SetupDiGetDeviceRegistryPropertyW(
                set.0,
                data,
                property,
                None,
                Some(buffer),
                Some(required),
            )
        }
    })?;
    utf16_bytes(&bytes)
}

fn device_property(
    set: &DevInfoSet,
    data: &SP_DEVINFO_DATA,
    key: &DEVPROPKEY,
) -> Option<(DEVPROPTYPE, Vec<u8>)> {
    let mut ty = DEVPROPTYPE(0);
    let bytes = read_buffer(|buffer, required| {
        // SAFETY: as in `registry_string`; `ty` outlives the call.
        unsafe {
            SetupDiGetDevicePropertyW(
                set.0,
                data,
                key,
                &raw mut ty,
                Some(buffer),
                Some(required),
                0,
            )
        }
    })?;
    Some((ty, bytes))
}

fn property_string(set: &DevInfoSet, data: &SP_DEVINFO_DATA, key: &DEVPROPKEY) -> Option<String> {
    let (ty, bytes) = device_property(set, data, key)?;
    (ty == DEVPROP_TYPE_STRING)
        .then(|| utf16_bytes(&bytes))
        .flatten()
}

fn property_filetime(set: &DevInfoSet, data: &SP_DEVINFO_DATA, key: &DEVPROPKEY) -> Option<u64> {
    let (ty, bytes) = device_property(set, data, key)?;
    if ty != DEVPROP_TYPE_FILETIME {
        return None;
    }
    let raw: [u8; 8] = bytes.get(..8)?.try_into().ok()?;
    // FILETIME is { low: u32, high: u32 } in little-endian memory order,
    // which is exactly a little-endian u64.
    Some(u64::from_le_bytes(raw))
}

/// Runs a `SetupAPI` getter against a 1 KiB buffer, growing it once if the
/// call reports it too small. Almost every property fits the first time, so
/// this costs one call per property rather than the two of a size probe.
fn read_buffer(
    mut call: impl FnMut(&mut [u8], *mut u32) -> windows::core::Result<()>,
) -> Option<Vec<u8>> {
    let mut buffer = vec![0u8; 1024];
    for _ in 0..2 {
        let mut required = 0u32;
        match call(&mut buffer, &raw mut required) {
            Ok(()) => {
                let used = required as usize;
                if used > 0 && used < buffer.len() {
                    buffer.truncate(used);
                }
                return Some(buffer);
            }
            Err(error)
                if error.code() == ERROR_INSUFFICIENT_BUFFER.to_hresult()
                    && required as usize > buffer.len() =>
            {
                buffer.resize(required as usize, 0);
            }
            Err(_) => return None,
        }
    }
    None
}

fn utf16_bytes(bytes: &[u8]) -> Option<String> {
    let units: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u16::from_le_bytes(*pair))
        .collect();
    utf16_until_nul(&units)
}

/// Decodes up to the first NUL. Blank text is `None`: an empty friendly name
/// means "not set", and must fall through to the next source.
fn utf16_until_nul(units: &[u16]) -> Option<String> {
    let end = units.iter().position(|&u| u == 0).unwrap_or(units.len());
    let text = String::from_utf16_lossy(&units[..end]);
    let trimmed = text.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_utf16_text_is_absent_so_the_name_falls_through() {
        assert_eq!(utf16_until_nul(&[0x20, 0x20, 0]), None);
        assert_eq!(utf16_until_nul(&[]), None);
    }

    #[test]
    fn utf16_decoding_stops_at_the_first_nul() {
        let bytes = [b'P', 0, b'C', 0, b'I', 0, 0, 0, b'X', 0];
        assert_eq!(utf16_bytes(&bytes).as_deref(), Some("PCI"));
    }
}
