//! Probe for whether this process can open a volume for a direct read.
//!
//! # Why this exists, and what it does not claim
//!
//! Reading a volume's file index directly means opening `\\.\C:` as a device,
//! which requires administrator rights. Whether that open succeeds is the
//! only question this module answers: [`is_available`] attempts the real open
//! and reports the result, so a caller can tell the user honestly whether the
//! process is privileged enough to do it.
//!
//! It claims nothing more. Vitals sizes and walks folders with [`super::scan`]
//! and never reads a raw index, so an available open is not a faster scan, and
//! nothing here should be presented as one.
//!
//! # Why there is no enumeration here any more
//!
//! An earlier version of this module enumerated the NTFS master file table
//! through `FSCTL_ENUM_USN_DATA`. That path was removed. A USN record carries
//! no file size, so it can never size a folder, and sizing the folders is the
//! expensive half of a storage scan — which makes the enumeration a cost with
//! no payoff. It was also unreachable: nothing in the application ever called
//! it. Keeping it would have advertised a capability (fast discovery) that no
//! code provided.
//!
//! # Elevation
//!
//! Opening `\\.\C:` requires administrator rights. Unelevated the open fails
//! with `ERROR_ACCESS_DENIED` and [`is_available`] returns `false`; the caller
//! falls back to [`super::scan`] rather than degrading silently.

use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_ACCESS_DENIED, GetLastError, HANDLE, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Storage::FileSystem::{FILE_SHARE_READ, FILE_SHARE_WRITE};

use vitals_core::error::{Error, Result};

use super::ffi::create_file_read;

/// Generic read access, spelled out to avoid pulling in another feature
/// module for a single constant.
const GENERIC_READ: u32 = 0x8000_0000;

/// A volume handle that closes itself.
struct VolumeHandle(HANDLE);

impl Drop for VolumeHandle {
    fn drop(&mut self) {
        // SAFETY: `self.0` came from a successful CreateFileW and is closed
        // exactly once, at drop.
        unsafe { CloseHandle(self.0) };
    }
}

/// Opens a volume for a direct read.
///
/// # Errors
///
/// [`Error::AccessDenied`] when the process is not elevated — the expected
/// outcome for a normal user, and the signal for the caller to fall back to
/// [`super::scan`].
fn open_volume(letter: char) -> Result<VolumeHandle> {
    // The device path form. A trailing backslash turns this into a request
    // for the root *directory* rather than the volume device, and the read
    // then fails with a misleading invalid-parameter error.
    let path: Vec<u16> = format!("\\\\.\\{letter}:")
        .encode_utf16()
        .chain(Some(0))
        .collect();

    // SAFETY: `path` is a NUL-terminated UTF-16 device path that outlives the
    // call.
    let opened = unsafe {
        create_file_read(
            path.as_ptr(),
            GENERIC_READ,
            // The volume stays fully usable while we hold it open; refusing
            // to share would make the open block every other process on the
            // disk.
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            0,
        )
    };

    let handle = opened.unwrap_or(INVALID_HANDLE_VALUE);
    if handle == INVALID_HANDLE_VALUE || handle.is_null() {
        // SAFETY: no arguments, no preconditions.
        let code = unsafe { GetLastError() };
        return Err(if code == ERROR_ACCESS_DENIED {
            Error::AccessDenied {
                operation: format!("open volume {letter}: for a direct read"),
            }
        } else {
            Error::Os {
                context: format!("CreateFileW(\\\\.\\{letter}:)"),
                code: code.cast_signed(),
            }
        });
    }

    Ok(VolumeHandle(handle))
}

/// Whether this process can open a drive letter for a direct read.
///
/// Cheap and truthful: it attempts the actual open rather than inspecting a
/// token, so it answers the question that matters — "will this work?" — and
/// not a proxy for it.
#[must_use]
pub fn is_available(letter: char) -> bool {
    open_volume(letter).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_probe_answers_without_panicking_and_agrees_with_the_real_open() {
        // Whatever the elevation state of the machine running the tests, the
        // probe must give a plain answer rather than fail: true exactly when
        // the volume can in fact be opened. The storage screen renders from
        // this with no failure path, so a panic here would take it down.
        assert_eq!(is_available('C'), open_volume('C').is_ok());
    }
}
