//! The signed-in user's profile folder, and who owns a file.
//!
//! The AI page runs agentq's PowerShell scripts (ADR-0039). Which script runs
//! must not be decided by the environment — `USERPROFILE` and `AGENTQ_BIN` are
//! inherited from whoever started Vitals — and the script must have been
//! written by this account or by an administrator, never by another ordinary
//! account that happened to be able to plant it (ENG2, S21-04).

use std::path::{Path, PathBuf};

use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::UI::Shell::{FOLDERID_Profile, KF_FLAG_DEFAULT, SHGetKnownFolderPath};
use windows_sys::Win32::Foundation::{CloseHandle, ERROR_SUCCESS, HANDLE, LocalFree};
use windows_sys::Win32::Security::Authorization::{GetNamedSecurityInfoW, SE_FILE_OBJECT};
use windows_sys::Win32::Security::{
    CreateWellKnownSid, EqualSid, GetTokenInformation, OWNER_SECURITY_INFORMATION,
    PSECURITY_DESCRIPTOR, SECURITY_MAX_SID_SIZE, TOKEN_QUERY, TOKEN_USER, TokenUser,
    WinBuiltinAdministratorsSid, WinLocalSystemSid,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

/// The profile folder of the account this process runs as, from the shell
/// (`FOLDERID_Profile`), not from `%USERPROFILE%`.
#[must_use]
pub fn user_profile() -> Option<PathBuf> {
    // SAFETY: the GUID is a static; the returned buffer is freed below.
    let raw = unsafe { SHGetKnownFolderPath(&FOLDERID_Profile, KF_FLAG_DEFAULT, None) }.ok()?;
    // SAFETY: on success `raw` is a NUL-terminated string from the shell.
    let text = unsafe { raw.to_string() }.ok();
    // SAFETY: allocated by the shell with CoTaskMemAlloc, freed exactly once.
    unsafe { CoTaskMemFree(Some(raw.0.cast_const().cast())) };
    text.map(PathBuf::from)
}

/// Whether `path` is owned by this process's account, by the Administrators
/// group (what an elevated installer writes) or by `SYSTEM`.
///
/// `false` on any failure: refusing to run a script is the safe way to be
/// wrong.
#[must_use]
pub fn owned_by_user_or_admins(path: &Path) -> bool {
    let Some(text) = path.to_str() else {
        return false;
    };
    let path_w: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let mut owner: *mut core::ffi::c_void = std::ptr::null_mut();
    let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
    // SAFETY: `path_w` is NUL-terminated; the out-pointers are live; `owner`
    // points into `descriptor`, which is freed after the last use below.
    let status = unsafe {
        GetNamedSecurityInfoW(
            path_w.as_ptr(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION,
            &raw mut owner,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &raw mut descriptor,
        )
    };
    if status != ERROR_SUCCESS {
        return false;
    }
    let _descriptor = FreeLocal(descriptor);
    if owner.is_null() {
        return false;
    }
    let mut trusted = Vec::with_capacity(3);
    if let Some(user) = current_user_sid() {
        trusted.push(user);
    }
    for kind in [WinBuiltinAdministratorsSid, WinLocalSystemSid] {
        if let Some(sid) = well_known_sid(kind) {
            trusted.push(sid);
        }
    }
    trusted.iter_mut().any(|sid| {
        // SAFETY: both are valid SIDs: `owner` from the live descriptor,
        // `sid` from a buffer the OS filled.
        unsafe { EqualSid(owner, sid.as_mut_ptr().cast()) != 0 }
    })
}

/// The SID of the account this process runs as, in a buffer it owns.
fn current_user_sid() -> Option<Vec<u8>> {
    let mut token: HANDLE = std::ptr::null_mut();
    // SAFETY: `GetCurrentProcess` is a pseudo-handle; `token` is written only
    // on success and closed below.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &raw mut token) } == 0 {
        return None;
    }
    // u64 so the buffer is aligned for TOKEN_USER, which holds a pointer
    let mut buffer = [0_u64; 64];
    let mut returned = 0_u32;
    // SAFETY: the buffer is as large as declared; `token` is live.
    let ok = unsafe {
        GetTokenInformation(
            token,
            TokenUser,
            buffer.as_mut_ptr().cast(),
            u32::try_from(size_of_val(&buffer)).unwrap_or(0),
            &raw mut returned,
        )
    };
    // SAFETY: `token` came from OpenProcessToken and is closed exactly once.
    let _ = unsafe { CloseHandle(token) };
    if ok == 0 {
        return None;
    }
    // SAFETY: on success the buffer starts with a TOKEN_USER whose SID
    // points inside the same buffer, which is alive here.
    let sid = unsafe { (*buffer.as_ptr().cast::<TOKEN_USER>()).User.Sid };
    copy_sid(sid)
}

fn well_known_sid(kind: windows_sys::Win32::Security::WELL_KNOWN_SID_TYPE) -> Option<Vec<u8>> {
    let mut sid = vec![0_u8; SECURITY_MAX_SID_SIZE as usize];
    let mut size = SECURITY_MAX_SID_SIZE;
    // SAFETY: `sid` is a buffer of `size` bytes, the documented maximum.
    let made = unsafe {
        CreateWellKnownSid(
            kind,
            std::ptr::null_mut(),
            sid.as_mut_ptr().cast(),
            &raw mut size,
        )
    };
    (made != 0).then_some(sid)
}

/// Copies a SID out of a buffer that is about to go away.
fn copy_sid(sid: *mut core::ffi::c_void) -> Option<Vec<u8>> {
    use windows_sys::Win32::Security::{CopySid, GetLengthSid};
    if sid.is_null() {
        return None;
    }
    // SAFETY: `sid` is a valid SID from the token buffer.
    let len = unsafe { GetLengthSid(sid) };
    let mut out = vec![0_u8; len as usize];
    // SAFETY: `out` holds exactly `len` bytes.
    let ok = unsafe { CopySid(len, out.as_mut_ptr().cast(), sid) };
    (ok != 0).then_some(out)
}

/// Frees a `LocalAlloc`'d security descriptor on drop.
struct FreeLocal(PSECURITY_DESCRIPTOR);

impl Drop for FreeLocal {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: allocated by GetNamedSecurityInfoW with LocalAlloc,
            // freed exactly once.
            unsafe { LocalFree(self.0) };
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn the_profile_comes_from_the_shell_and_is_a_real_folder() {
        let profile = user_profile().unwrap();
        assert!(profile.is_dir(), "{}", profile.display());
    }

    #[test]
    fn a_file_this_account_just_wrote_is_trusted_and_a_missing_one_is_not() {
        let dir = std::env::temp_dir().join(format!("vitals-own-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("mine.ps1");
        std::fs::write(&file, "# mine").unwrap();
        assert!(owned_by_user_or_admins(&file));
        assert!(!owned_by_user_or_admins(&dir.join("absent.ps1")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_file_owned_by_another_ordinary_account_is_not_trusted() {
        // `TrustedInstaller` owns System32's own files: neither this user,
        // nor Administrators, nor SYSTEM. Proves the comparison can say no.
        let system = crate::actions::taskmgr::system_directory().unwrap();
        let notepad = system.join("notepad.exe");
        if notepad.is_file() {
            assert!(!owned_by_user_or_admins(&notepad));
        }
    }
}
