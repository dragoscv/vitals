//! Minimal `PawnIO` driver client: open the device, load a signed module,
//! execute one of its functions.
//!
//! `PawnIO` does not expose raw `RDMSR` to callers — it runs a signed,
//! sandboxed module that decides which registers may be read. That is the
//! property that separates it from `WinRing0` (blocklisted for handing any
//! caller arbitrary MSR and port access), and why it is acceptable here.

use windows::Win32::Foundation::{CloseHandle, GENERIC_READ, GENERIC_WRITE, HANDLE};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::System::IO::DeviceIoControl;
use windows::core::HSTRING;

pub const DEVICE_PATH: &str = r"\\?\GLOBALROOT\Device\PawnIO";
const DEVICE_TYPE: u32 = 41394 << 16;
const IOCTL_LOAD: u32 = DEVICE_TYPE | (0x821 << 2);
const IOCTL_EXECUTE: u32 = DEVICE_TYPE | (0x841 << 2);
const FN_NAME_LEN: usize = 32;

const E_ACCESS_DENIED: u32 = 0x8007_0005;
const E_FILE_NOT_FOUND: u32 = 0x8007_0002;
const E_PATH_NOT_FOUND: u32 = 0x8007_0003;

#[derive(Debug)]
pub struct PawnIo {
    handle: HANDLE,
}

// SAFETY: the handle is a kernel object handle, valid from any thread; the
// owner (`Sensor`) is only ever used behind a mutex, one call at a time.
unsafe impl Send for PawnIo {}

impl PawnIo {
    pub fn open() -> Result<Self, String> {
        // SAFETY: plain Win32 call with a valid, NUL-terminated path.
        let handle = unsafe {
            CreateFileW(
                &HSTRING::from(DEVICE_PATH),
                GENERIC_READ.0 | GENERIC_WRITE.0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                None,
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                None,
            )
        }
        .map_err(|e| {
            let hint = match e.code().0.cast_unsigned() {
                E_ACCESS_DENIED => {
                    " (needs administrator or SYSTEM: use the vitals-sensors service)"
                }
                E_FILE_NOT_FOUND | E_PATH_NOT_FOUND => {
                    " (PawnIO driver not installed or not running)"
                }
                _ => "",
            };
            format!("open {DEVICE_PATH}: {e}{hint}")
        })?;
        Ok(Self { handle })
    }

    pub fn load(&self, blob: &[u8]) -> Result<(), String> {
        let len = u32::try_from(blob.len()).map_err(|_| "module too large".to_owned())?;
        let mut returned = 0u32;
        // SAFETY: the input buffer is valid for `len` bytes; no output buffer.
        unsafe {
            DeviceIoControl(
                self.handle,
                IOCTL_LOAD,
                Some(blob.as_ptr().cast()),
                len,
                None,
                0,
                Some(&raw mut returned),
                None,
            )
        }
        .map_err(|e| format!("PawnIO load module: {e}"))
    }

    pub fn execute(&self, name: &str, input: &[u64], out_len: usize) -> Result<Vec<u64>, String> {
        if name.len() >= FN_NAME_LEN || !name.is_ascii() {
            return Err(format!("bad PawnIO function name {name:?}"));
        }
        let mut buf = vec![0u8; FN_NAME_LEN + input.len() * 8];
        buf[..name.len()].copy_from_slice(name.as_bytes());
        for (i, v) in input.iter().enumerate() {
            let at = FN_NAME_LEN + i * 8;
            buf[at..at + 8].copy_from_slice(&v.to_le_bytes());
        }
        let mut out = vec![0u64; out_len];
        let in_len = u32::try_from(buf.len()).map_err(|_| "input too large".to_owned())?;
        let out_bytes = u32::try_from(out.len() * 8).map_err(|_| "output too large".to_owned())?;
        let mut returned = 0u32;
        // SAFETY: both buffers are valid for the sizes passed.
        unsafe {
            DeviceIoControl(
                self.handle,
                IOCTL_EXECUTE,
                Some(buf.as_ptr().cast()),
                in_len,
                Some(out.as_mut_ptr().cast()),
                out_bytes,
                Some(&raw mut returned),
                None,
            )
        }
        .map_err(|e| format!("PawnIO {name}: {e}"))?;
        out.truncate(returned as usize / 8);
        Ok(out)
    }
}

impl Drop for PawnIo {
    fn drop(&mut self) {
        // SAFETY: the handle came from CreateFileW and is closed exactly once.
        let _ = unsafe { CloseHandle(self.handle) };
    }
}
