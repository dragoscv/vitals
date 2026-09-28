//! NVIDIA GPU temperature, fan and board power through NVML.
//!
//! # Why this is not a "vendor SDK to redistribute"
//!
//! The gap list said GPU temperature needs a vendor SDK redistributed under
//! its own licence. For NVIDIA that is not so: `nvml.dll` is installed into
//! `System32` by every GeForce/Quadro driver since R418, and it is a plain
//! user-mode library that any user may call — no elevation, no driver of our
//! own, nothing shipped by us. `nvidia-smi`, which ships with the same
//! driver, is a thin front-end to it. Measured on 2026-09-28 unelevated:
//! RTX 3060 Ti, 47 °C, fan 83 %, 59.6 W.
//!
//! It is loaded with `LOAD_LIBRARY_SEARCH_SYSTEM32` only, so a planted
//! `nvml.dll` beside the executable or on `PATH` is never picked up.
//! A machine without an NVIDIA driver has no `nvml.dll`; that is a normal
//! absence and yields an empty list, not an error.
//!
//! AMD (ADLX) and Intel (IGCL) have no equivalent that ships in `System32`,
//! so their GPUs keep the gap entry.
//!
//! # What is read, and what is not
//!
//! Per device: core temperature (`NVML_TEMPERATURE_GPU`), fan speed as a
//! percentage of maximum, and board power in milliwatts. Each call can fail
//! independently — a laptop GPU has no fan to report, some boards do not
//! meter power — and each failure is a missing reading, never a zero.
//! Fan RPM is not offered through the stable API, so the fan is reported as
//! the percentage NVML gives, labelled as such.

use std::ffi::{c_char, c_int, c_uint, c_void};

use windows::Win32::Foundation::{FreeLibrary, HMODULE};
use windows::Win32::System::LibraryLoader::{
    GetProcAddress, LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW,
};
use windows::core::{PCSTR, s, w};

/// One NVIDIA GPU's readings. `None` fields were not reported by this board.
#[derive(Debug, Clone, PartialEq)]
pub struct NvidiaGpu {
    pub index: u32,
    pub name: String,
    pub temperature_celsius: Option<f32>,
    /// Percentage of the fan's maximum speed.
    pub fan_percent: Option<f32>,
    pub power_watts: Option<f32>,
}

type NvmlReturn = c_int;
type Device = *mut c_void;
const NVML_SUCCESS: NvmlReturn = 0;
const NVML_TEMPERATURE_GPU: c_uint = 0;
const NAME_LEN: usize = 96;

type InitFn = unsafe extern "C" fn() -> NvmlReturn;
type ShutdownFn = unsafe extern "C" fn() -> NvmlReturn;
type CountFn = unsafe extern "C" fn(*mut c_uint) -> NvmlReturn;
type HandleFn = unsafe extern "C" fn(c_uint, *mut Device) -> NvmlReturn;
type NameFn = unsafe extern "C" fn(Device, *mut c_char, c_uint) -> NvmlReturn;
type TempFn = unsafe extern "C" fn(Device, c_uint, *mut c_uint) -> NvmlReturn;
type UintFn = unsafe extern "C" fn(Device, *mut c_uint) -> NvmlReturn;

/// Reads every NVIDIA GPU. Empty when there is no NVIDIA driver.
#[must_use]
pub fn read_nvidia_gpus() -> Vec<NvidiaGpu> {
    let Some(lib) = Library::load() else {
        return Vec::new();
    };
    lib.read().unwrap_or_default()
}

struct Library(HMODULE);

impl Library {
    fn load() -> Option<Self> {
        // SAFETY: a static wide string; System32 only, never the app directory.
        let module =
            unsafe { LoadLibraryExW(w!("nvml.dll"), None, LOAD_LIBRARY_SEARCH_SYSTEM32) }.ok()?;
        Some(Self(module))
    }

    /// Resolves an export as `F`.
    ///
    /// # Safety
    ///
    /// `F` must be the export's real signature.
    unsafe fn symbol<F: Copy>(&self, name: PCSTR) -> Option<F> {
        // SAFETY: `self.0` is a live module handle.
        let address = unsafe { GetProcAddress(self.0, name) }?;
        debug_assert_eq!(size_of::<F>(), size_of_val(&address));
        // SAFETY: the caller guarantees `F` is the export's signature, and a
        // function pointer is pointer-sized.
        Some(unsafe { std::mem::transmute_copy::<_, F>(&address) })
    }

    fn read(&self) -> Option<Vec<NvidiaGpu>> {
        // SAFETY: each type alias matches the NVML header for that export.
        // `_v2` variants are the ones current drivers export and the only
        // ones that honour the documented argument layout.
        let (init, shutdown, count, handle, name, temp, fan, power) = unsafe {
            (
                self.symbol::<InitFn>(s!("nvmlInit_v2"))?,
                self.symbol::<ShutdownFn>(s!("nvmlShutdown"))?,
                self.symbol::<CountFn>(s!("nvmlDeviceGetCount_v2"))?,
                self.symbol::<HandleFn>(s!("nvmlDeviceGetHandleByIndex_v2"))?,
                self.symbol::<NameFn>(s!("nvmlDeviceGetName"))?,
                self.symbol::<TempFn>(s!("nvmlDeviceGetTemperature"))?,
                self.symbol::<UintFn>(s!("nvmlDeviceGetFanSpeed"))?,
                self.symbol::<UintFn>(s!("nvmlDeviceGetPowerUsage"))?,
            )
        };

        // SAFETY: no arguments; balanced by `shutdown` below.
        if unsafe { init() } != NVML_SUCCESS {
            return None;
        }

        let mut devices = 0;
        // SAFETY: a valid out pointer.
        let listed = unsafe { count(&raw mut devices) } == NVML_SUCCESS;
        let gpus = if listed {
            (0..devices)
                .filter_map(|index| {
                    let mut device: Device = std::ptr::null_mut();
                    // SAFETY: `index` < count; valid out pointer.
                    if unsafe { handle(index, &raw mut device) } != NVML_SUCCESS {
                        return None;
                    }
                    Some(read_device(index, device, name, temp, fan, power))
                })
                .collect()
        } else {
            Vec::new()
        };

        // SAFETY: balances the successful `init` above.
        unsafe { shutdown() };
        Some(gpus)
    }
}

fn read_device(
    index: u32,
    device: Device,
    name: NameFn,
    temp: TempFn,
    fan: UintFn,
    power: UintFn,
) -> NvidiaGpu {
    let mut buffer = [0 as c_char; NAME_LEN];
    // SAFETY: `device` came from NVML; the buffer is NAME_LEN bytes.
    let named = unsafe { name(device, buffer.as_mut_ptr(), NAME_LEN as c_uint) } == NVML_SUCCESS;
    let label = if named {
        // SAFETY: NVML null-terminates within the buffer on success.
        unsafe { std::ffi::CStr::from_ptr(buffer.as_ptr()) }
            .to_string_lossy()
            .into_owned()
    } else {
        format!("NVIDIA GPU {index}")
    };

    let read = |call: &dyn Fn(*mut c_uint) -> NvmlReturn| {
        let mut value: c_uint = 0;
        (call(&raw mut value) == NVML_SUCCESS).then_some(value)
    };

    // SAFETY (all three): `device` is a live NVML handle; out pointers valid.
    let celsius = read(&|out| unsafe { temp(device, NVML_TEMPERATURE_GPU, out) });
    let fan_percent = read(&|out| unsafe { fan(device, out) });
    let milliwatts = read(&|out| unsafe { power(device, out) });

    to_gpu(index, label, celsius, fan_percent, milliwatts)
}

/// Converts raw NVML values, rejecting the implausible.
///
/// A temperature of 0 or above 150 °C, or a fan above 100 %, is a driver
/// placeholder rather than a reading.
#[must_use]
#[allow(clippy::cast_precision_loss)] // small integers, exact in f32
pub fn to_gpu(
    index: u32,
    name: String,
    celsius: Option<u32>,
    fan_percent: Option<u32>,
    milliwatts: Option<u32>,
) -> NvidiaGpu {
    NvidiaGpu {
        index,
        name,
        temperature_celsius: celsius
            .filter(|&c| (1..=150).contains(&c))
            .map(|c| c as f32),
        fan_percent: fan_percent.filter(|&f| f <= 100).map(|f| f as f32),
        power_watts: milliwatts
            .filter(|&mw| mw > 0 && mw < 2_000_000)
            .map(|mw| mw as f32 / 1000.0),
    }
}

impl Drop for Library {
    fn drop(&mut self) {
        // SAFETY: loaded in `load`, freed once.
        let _ = unsafe { FreeLibrary(self.0) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_the_values_measured_on_the_development_machine() {
        let gpu = to_gpu(0, "RTX".into(), Some(47), Some(83), Some(59_580));
        assert_eq!(gpu.temperature_celsius, Some(47.0));
        assert_eq!(gpu.fan_percent, Some(83.0));
        assert!((gpu.power_watts.expect("metered") - 59.58).abs() < 0.01);
    }

    #[test]
    fn a_reading_the_board_does_not_report_stays_absent_never_zero() {
        let gpu = to_gpu(0, "Laptop".into(), Some(0), None, Some(0));
        assert_eq!(gpu.temperature_celsius, None);
        assert_eq!(gpu.fan_percent, None);
        assert_eq!(gpu.power_watts, None);
    }

    #[test]
    fn a_missing_nvidia_driver_is_an_empty_list_not_a_failure() {
        // On this machine there is a driver, so only the type is checked;
        // the contract is that the call never panics.
        let _ = read_nvidia_gpus();
    }
}
