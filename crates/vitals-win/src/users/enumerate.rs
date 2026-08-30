//! Session enumeration via the Terminal Services API.
//!
//! ## Why Terminal Services rather than WMI
//!
//! `Get-CimInstance Win32_LogonSession` requires joining multiple WMI classes
//! and is slow. `WTSEnumerateSessionsW` returns all sessions in one call with
//! live state. The design mirrors services.rs: the fast, direct API wins.

use std::ffi::c_void;
use std::ptr::null_mut;
use std::time::SystemTime;

use windows::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, HANDLE};
use windows::Win32::System::RemoteDesktop::{
    WTS_INFO_CLASS, WTS_SESSION_INFOW, WTSClientName, WTSDomainName, WTSEnumerateSessionsW,
    WTSFreeMemory, WTSQuerySessionInformationW, WTSUserName,
};

use vitals_core::error::{Error, Result};

use super::session::{LogonSession, SessionState};

/// An RAII guard for memory allocated by the WTS API.
///
/// `WTSEnumerateSessionsW` and `WTSQuerySessionInformationW` allocate buffers
/// the caller must free with `WTSFreeMemory`. Leaking one means holding that
/// memory until the process exits, which on a long-running monitor accumulates.
struct WtsMemory(*mut c_void);

impl WtsMemory {
    /// Wraps a pointer allocated by a WTS function.
    ///
    /// The pointer must be null or valid output from `WTSEnumerateSessionsW` /
    /// `WTSQuerySessionInformationW`. Wrapping an arbitrary pointer and then
    /// dropping this guard invokes undefined behaviour.
    const fn new(ptr: *mut c_void) -> Self {
        Self(ptr)
    }
}

impl Drop for WtsMemory {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: `self.0` is a valid pointer from a WTS function, freed once.
            unsafe { WTSFreeMemory(self.0) };
        }
    }
}

/// Queries a session information field.
///
/// Returns `None` if the query fails — the most common cause is that the field
/// is legitimately unavailable, not that something is broken. Mapping failure
/// to an empty string or a zero would be inventing a fact.
fn query_session_string(session_id: u32, info_class: WTS_INFO_CLASS) -> Option<String> {
    let mut buffer: *mut u16 = null_mut();
    let mut bytes_returned: u32 = 0;

    // SAFETY: WTSQuerySessionInformationW validates its arguments and writes
    // to the out-pointers only on success. The buffer is freed by the guard.
    let ok = unsafe {
        WTSQuerySessionInformationW(
            Some(HANDLE::default()),
            session_id,
            info_class,
            (&raw mut buffer).cast(),
            &raw mut bytes_returned,
        )
    };

    if ok.is_err() {
        return None;
    }

    let _guard = WtsMemory::new(buffer.cast());

    if buffer.is_null() || bytes_returned == 0 {
        return None;
    }

    // The buffer is a null-terminated wide string. Calculate the length by
    // scanning for the null terminator, not by trusting `bytes_returned` —
    // the documentation states it includes the null, but empirically it
    // sometimes does not.
    //
    // The bound has to come first. An unbounded `take_while` followed by a
    // `take` reads past the end of the allocation before the `take` ever
    // sees the value — the limit would be applied to a byte that had already
    // been dereferenced. Bounding the range up front is what actually makes
    // the scan safe.
    let max = (bytes_returned as usize) / 2;
    let len = (0..max)
        .take_while(|&i| {
            // SAFETY: `buffer` is non-null and Windows allocated it to hold
            // `bytes_returned` bytes, so indices below `bytes_returned / 2`
            // are within the wide-character range of that allocation.
            (unsafe { *buffer.add(i) }) != 0
        })
        .count();

    if len == 0 {
        return None;
    }

    // SAFETY: `buffer[0..len]` is valid UTF-16 allocated by Windows.
    let slice = unsafe { std::slice::from_raw_parts(buffer, len) };
    String::from_utf16(slice).ok()
}

/// Enumerates all Terminal Services sessions.
///
/// Session 0 is the Services session and will be included. Listening sessions
/// are idle connection slots and will also be included — they are classified
/// by [`SessionState::is_interactive`] rather than hidden.
pub fn enumerate_sessions() -> Result<Vec<LogonSession>> {
    let mut sessions_ptr: *mut WTS_SESSION_INFOW = null_mut();
    let mut count: u32 = 0;

    // SAFETY: WTSEnumerateSessionsW validates its arguments. The buffer is
    // freed by the guard below.
    let ok = unsafe {
        WTSEnumerateSessionsW(
            Some(HANDLE::default()),
            0,
            1,
            &raw mut sessions_ptr,
            &raw mut count,
        )
    };

    if ok.is_err() {
        // SAFETY: no preconditions.
        let code = unsafe { windows::Win32::Foundation::GetLastError() };

        return Err(match code {
            ERROR_INSUFFICIENT_BUFFER => Error::Os {
                context: "WTSEnumerateSessionsW (buffer too small)".to_owned(),
                code: code.0.cast_signed(),
            },
            other => Error::Os {
                context: "WTSEnumerateSessionsW".to_owned(),
                code: other.0.cast_signed(),
            },
        });
    }

    let _guard = WtsMemory::new(sessions_ptr.cast());

    if sessions_ptr.is_null() || count == 0 {
        return Ok(Vec::new());
    }

    // SAFETY: `sessions_ptr` points to an array of `count` sessions allocated
    // by Windows, freed by the guard. The lifetime of the slice does not
    // exceed the lifetime of the guard.
    let count = count as usize;
    let sessions_slice = unsafe { std::slice::from_raw_parts(sessions_ptr, count) };

    let mut result = Vec::with_capacity(count);

    for session_info in sessions_slice {
        let session_id = session_info.SessionId;
        // `WTS_CONNECTSTATE_CLASS` is a signed enum in the Windows crate, but
        // every documented value is small and non-negative. A negative value
        // would mean Windows returned something outside the documented set,
        // which is exactly what `Unknown` exists to carry — so it is mapped
        // rather than truncated into a plausible-looking state.
        let state = match u32::try_from(session_info.State.0) {
            Ok(raw) => SessionState::from_raw(raw),
            Err(_) => SessionState::Unknown(u32::MAX),
        };

        let user_name = query_session_string(session_id, WTSUserName);
        let domain = query_session_string(session_id, WTSDomainName);
        let client_name = query_session_string(session_id, WTSClientName);

        // Logon time is not reliably obtainable from the WTS API. The
        // `WTSSessionInfo` structure theoretically contains it, but
        // `WTSQuerySessionInformationW` with `WTSSessionInfo` returns a
        // buffer that does not match the documented `WTSINFOW` layout, and
        // parsing it would be fragile. Reporting `None` is the honest answer.
        let logon_time: Option<SystemTime> = None;

        result.push(LogonSession {
            session_id,
            user_name,
            domain,
            client_name,
            state,
            logon_time,
        });
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enumeration_returns_at_least_one_session() {
        // A Windows machine always has at least one session: either the
        // console session or session 0 (Services).
        let sessions = enumerate_sessions().expect("enumeration should succeed");
        assert!(
            !sessions.is_empty(),
            "expected at least one session, got none"
        );
    }

    #[test]
    fn current_session_appears_in_the_list() {
        // The session this test runs in should be enumerated and have a state.
        let sessions = enumerate_sessions().expect("enumeration should succeed");

        // SAFETY: no preconditions.
        let current_id =
            unsafe { windows::Win32::System::RemoteDesktop::WTSGetActiveConsoleSessionId() };

        let current = sessions
            .iter()
            .find(|s| s.session_id == current_id)
            .unwrap_or_else(|| {
                panic!(
                    "expected session {} in {:?}",
                    current_id,
                    sessions.iter().map(|s| s.session_id).collect::<Vec<_>>()
                )
            });

        // The console session should be Active or Connected, not Disconnected.
        assert!(
            current.state.is_interactive(),
            "expected console session to be interactive, got {:?}",
            current.state
        );
    }

    #[test]
    fn session_zero_is_enumerated() {
        // Session 0 is the Services session and should always exist.
        let sessions = enumerate_sessions().expect("enumeration should succeed");
        let session_zero = sessions.iter().find(|s| s.session_id == 0);

        assert!(
            session_zero.is_some(),
            "expected session 0 (Services) to be enumerated"
        );
    }
}
