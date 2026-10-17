//! Per-user secrets in Windows Credential Manager (generic credentials).
//!
//! Used for the codai API key the AI page signs in with (ADR-0038). Credential
//! Manager encrypts with the user's logon secret and is what Windows itself
//! offers for this; a file under `%APPDATA%` would be readable by anything
//! that can read the profile and would end up in backups.

use windows_sys::Win32::Foundation::{ERROR_NOT_FOUND, FILETIME, GetLastError};
use windows_sys::Win32::Security::Credentials::{
    CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, CREDENTIALW, CredDeleteW, CredFree, CredReadW,
    CredWriteW,
};

use vitals_core::error::{Error, Result};

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Stores `secret` under `target`, replacing any previous value.
///
/// # Errors
/// The OS refusing the write (policy, a full vault).
pub fn write(target: &str, user: &str, secret: &str) -> Result<()> {
    let mut target_w = wide(target);
    let mut user_w = wide(user);
    let mut blob: Vec<u8> = secret.as_bytes().to_vec();
    let cred = CREDENTIALW {
        Flags: 0,
        Type: CRED_TYPE_GENERIC,
        TargetName: target_w.as_mut_ptr(),
        Comment: std::ptr::null_mut(),
        LastWritten: FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        },
        CredentialBlobSize: u32::try_from(blob.len()).map_err(|_| Error::Os {
            context: "secret too large".into(),
            code: 0,
        })?,
        CredentialBlob: blob.as_mut_ptr(),
        Persist: CRED_PERSIST_LOCAL_MACHINE,
        AttributeCount: 0,
        Attributes: std::ptr::null_mut(),
        TargetAlias: std::ptr::null_mut(),
        UserName: user_w.as_mut_ptr(),
    };
    // SAFETY: every pointer in `cred` references a live buffer for the call.
    let ok = unsafe { CredWriteW(&raw const cred, 0) };
    blob.fill(0);
    if ok == 0 {
        // SAFETY: no preconditions.
        let code = unsafe { GetLastError() };
        return Err(Error::Os {
            context: "CredWriteW".into(),
            code: code.cast_signed(),
        });
    }
    Ok(())
}

/// Reads the secret under `target`; `None` when there is none.
///
/// # Errors
/// Any failure other than "not found".
pub fn read(target: &str) -> Result<Option<String>> {
    let target_w = wide(target);
    let mut out: *mut CREDENTIALW = std::ptr::null_mut();
    // SAFETY: `target_w` is NUL-terminated; `out` receives an OS allocation.
    let ok = unsafe { CredReadW(target_w.as_ptr(), CRED_TYPE_GENERIC, 0, &raw mut out) };
    if ok == 0 {
        // SAFETY: no preconditions.
        let code = unsafe { GetLastError() };
        if code == ERROR_NOT_FOUND {
            return Ok(None);
        }
        return Err(Error::Os {
            context: "CredReadW".into(),
            code: code.cast_signed(),
        });
    }
    // SAFETY: on success `out` points at a CREDENTIALW the OS owns until CredFree.
    let secret = unsafe {
        let c = &*out;
        let bytes = std::slice::from_raw_parts(c.CredentialBlob, c.CredentialBlobSize as usize);
        let s = String::from_utf8_lossy(bytes).into_owned();
        CredFree(out.cast());
        s
    };
    Ok(Some(secret))
}

/// Removes the secret; succeeds when there was none.
///
/// # Errors
/// Any failure other than "not found".
pub fn delete(target: &str) -> Result<()> {
    let target_w = wide(target);
    // SAFETY: `target_w` is NUL-terminated.
    let ok = unsafe { CredDeleteW(target_w.as_ptr(), CRED_TYPE_GENERIC, 0) };
    if ok == 0 {
        // SAFETY: no preconditions.
        let code = unsafe { GetLastError() };
        if code != ERROR_NOT_FOUND {
            return Err(Error::Os {
                context: "CredDeleteW".into(),
                code: code.cast_signed(),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_secret_round_trips_and_deleting_twice_is_not_an_error() {
        let target = format!("vitals-test/{}", std::process::id());
        write(&target, "test", "s3cret-✓").unwrap();
        assert_eq!(read(&target).unwrap().as_deref(), Some("s3cret-✓"));
        delete(&target).unwrap();
        assert_eq!(read(&target).unwrap(), None);
        delete(&target).unwrap();
    }
}
