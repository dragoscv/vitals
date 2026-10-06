//! A process's executable path without opening the process.
//!
//! `QueryFullProcessImageNameW` needs a handle, and the unelevated app is
//! refused one for every service, every other account's process and every
//! protected process — a third of a typical machine (213 of 655 measured on
//! 2026-10-06). Their rows then had no icon, which is the half of the list
//! Task Manager still decorates. `SystemProcessIdInformation` asks the
//! kernel for the image name by PID instead, and the kernel answers any
//! caller: it is how Process Explorer names processes it cannot open.
//!
//! Used only for icons. A PID can be recycled between the frame and this
//! call, and the worst outcome here is a wrong icon for one second; nothing
//! that acts on a process may trust a path obtained this way.

use std::ffi::c_void;

use super::raw::{NtQuerySystemInformation, UnicodeString};

/// `SystemProcessIdInformation`
const SYSTEM_PROCESS_ID_INFORMATION: i32 = 88;

#[repr(C)]
struct SystemProcessIdInformationBuffer {
    process_id: usize,
    image_name: UnicodeString,
}

/// The executable's Win32 path for `pid`, without a process handle.
///
/// `None` for the kernel's own pseudo-processes (System, Registry, Secure
/// System, Idle), which have no image file, and for a PID that has exited.
#[must_use]
pub fn image_path_by_pid(pid: u32) -> Option<String> {
    // The kernel writes up to MaximumLength bytes into our buffer; 32 KiB
    // UTF-16 covers the extended path limit.
    let mut name = vec![0_u16; 32_768];
    let mut info = SystemProcessIdInformationBuffer {
        process_id: pid as usize,
        image_name: UnicodeString {
            Length: 0,
            MaximumLength: u16::try_from(name.len() * 2).unwrap_or(u16::MAX) & !1,
            Buffer: name.as_mut_ptr(),
        },
    };
    let size = u32::try_from(size_of::<SystemProcessIdInformationBuffer>()).ok()?;
    // SAFETY: `info` is a correctly laid out SYSTEM_PROCESS_ID_INFORMATION
    // whose buffer is `name`, alive and `MaximumLength` bytes long.
    let status = unsafe {
        NtQuerySystemInformation(
            SYSTEM_PROCESS_ID_INFORMATION,
            (&raw mut info).cast::<c_void>(),
            size,
            std::ptr::null_mut(),
        )
    };
    if status < 0 {
        return None;
    }
    // SAFETY: the kernel filled `Length` bytes of `name`, still alive here.
    let nt = unsafe { info.image_name.to_string_lossy() }?;
    nt_to_win32(&nt)
}

/// `\Device\HarddiskVolume3\Windows\x.exe` → `C:\Windows\x.exe`.
///
/// The kernel names files by device; the shell needs a drive letter. Each
/// letter's device is asked for once per call — there are at most 26.
fn nt_to_win32(nt: &str) -> Option<String> {
    use windows_sys::Win32::Storage::FileSystem::QueryDosDeviceW;

    for letter in b'A'..=b'Z' {
        let drive = format!("{}:", char::from(letter));
        let wide: Vec<u16> = drive.encode_utf16().chain(Some(0)).collect();
        let mut target = vec![0_u16; 1024];
        let len = u32::try_from(target.len()).unwrap_or(u32::MAX);
        // SAFETY: both buffers are live and NUL-terminated / sized as passed.
        let written = unsafe { QueryDosDeviceW(wide.as_ptr(), target.as_mut_ptr(), len) };
        if written == 0 {
            continue;
        }
        let end = target.iter().position(|&c| c == 0).unwrap_or(target.len());
        let device = String::from_utf16_lossy(target.get(..end).unwrap_or(&[]));
        if let Some(rest) = nt.strip_prefix(device.as_str())
            && rest.starts_with('\\')
        {
            return Some(format!("{drive}{rest}"));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn our_own_path_matches_the_handle_based_answer() {
        let own = image_path_by_pid(std::process::id()).expect("own image path");
        let expected = std::env::current_exe().expect("current exe");
        assert!(
            own.eq_ignore_ascii_case(&expected.to_string_lossy()),
            "{own} vs {}",
            expected.display()
        );
    }

    #[test]
    fn a_service_host_we_cannot_open_still_has_a_path() {
        // svchost runs as SYSTEM/LocalService: an unelevated OpenProcess is
        // refused, which is exactly the case this module exists for.
        let processes = crate::process::ProcessEnumerator::new()
            .enumerate()
            .expect("enumerate");
        let Some(svchost) = processes
            .iter()
            .find(|p| p.name.as_deref() == Some("svchost.exe"))
        else {
            return; // A container image with no service host.
        };
        let path = image_path_by_pid(svchost.key.pid.get()).expect("svchost path");
        assert!(
            path.to_ascii_lowercase()
                .ends_with(r"\system32\svchost.exe"),
            "{path}"
        );
    }

    #[test]
    fn the_system_pseudo_process_has_no_image_file() {
        let has_exe = |p: &String| {
            std::path::Path::new(p)
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("exe"))
        };
        assert_eq!(image_path_by_pid(4).filter(has_exe), None);
    }
}
