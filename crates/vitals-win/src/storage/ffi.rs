//! Win32 declarations this crate's binding features do not expose.
//!
//! Two symbols this crate's enabled binding features do not reach:
//! `windows-sys` gates `CreateFileW` behind `Win32_Security` and
//! `DeviceIoControl` behind `Win32_System_IO`, neither of which is enabled.
//!
//! They are wrapped here rather than declared at their call sites for a
//! specific reason: `sensors::battery` already declares both, with `isize`
//! handles instead of pointers. Two `extern` blocks naming one symbol with
//! different signatures trips `clashing_extern_declarations`, and the lint is
//! right to fire — a genuine ABI disagreement between two declarations of the
//! same import is undefined behaviour waiting to happen. Keeping the whole
//! surface in one place makes that easy to see and easy to reconcile.
//!
//! `CreateFileW` therefore comes from the `windows` crate, which is already a
//! dependency with the required feature, so there is no second declaration at
//! all. `DeviceIoControl` has no reachable binding and is declared under a
//! distinct Rust name bound to the real symbol via `link_name`.

use std::ffi::c_void;

use windows::Win32::Foundation::HANDLE as WinHandle;
use windows::Win32::Storage::FileSystem::{
    FILE_CREATION_DISPOSITION, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_MODE,
};
use windows_sys::Win32::Foundation::HANDLE;

#[link(name = "kernel32")]
unsafe extern "system" {
    /// Sends a control code directly to a device driver.
    #[link_name = "DeviceIoControl"]
    pub fn device_io_control(
        device: HANDLE,
        control_code: u32,
        in_buffer: *const c_void,
        in_size: u32,
        out_buffer: *mut c_void,
        out_size: u32,
        bytes_returned: *mut u32,
        overlapped: *mut c_void,
    ) -> i32;
}

/// `OPEN_EXISTING` — fail rather than create.
pub const OPEN_EXISTING: u32 = 3;

/// Opens an existing file, directory or device for reading.
///
/// Narrower than the raw `CreateFileW`: this module never creates anything
/// and never passes security attributes or a template handle, so those
/// parameters are not exposed and cannot be got wrong.
///
/// Returns the raw handle in `windows-sys` form, because the rest of the
/// module closes and passes handles through `windows-sys` entry points.
///
/// # Safety
///
/// `path` must point to a NUL-terminated UTF-16 string that remains valid for
/// the duration of the call.
pub unsafe fn create_file_read(
    path: *const u16,
    desired_access: u32,
    share_mode: u32,
    flags: u32,
) -> Option<HANDLE> {
    // SAFETY: forwarded from this function's own contract — `path` is a live
    // NUL-terminated UTF-16 string. The two `None`s are the documented way to
    // pass a null security descriptor and no template handle.
    let result = unsafe {
        windows::Win32::Storage::FileSystem::CreateFileW(
            windows::core::PCWSTR(path),
            desired_access,
            FILE_SHARE_MODE(share_mode),
            None,
            FILE_CREATION_DISPOSITION(OPEN_EXISTING),
            FILE_FLAGS_AND_ATTRIBUTES(flags),
            None,
        )
    };

    // The binding maps INVALID_HANDLE_VALUE to an error, so a failure here is
    // exactly the failure the raw API reports; the caller reads
    // `GetLastError` for the reason, which the binding preserves.
    result.ok().map(|WinHandle(raw)| raw.cast::<c_void>())
}
