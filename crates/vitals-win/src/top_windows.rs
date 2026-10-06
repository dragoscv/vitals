//! Which processes own a window a person can see, and bringing one forward.
//!
//! "Apps" in Task Manager means "has a window on the taskbar". Without this
//! the sampler classed every user process as Background, the Apps filter was
//! always empty, and there was no way to tell which rows a "Bring to front"
//! action could act on (found 2026-10-06).
//!
//! One `EnumWindows` per tick: a few hundred windows, well under a
//! millisecond, so it runs on the hot path rather than per selected row.

use std::collections::HashSet;

use vitals_core::error::{Error, Result};
use vitals_core::ids::ProcessKey;
use windows_sys::Win32::Foundation::{HWND, LPARAM, RECT};
use windows_sys::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DwmGetWindowAttribute};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GW_OWNER, GWL_EXSTYLE, GetForegroundWindow, GetWindow, GetWindowLongW,
    GetWindowRect, GetWindowTextLengthW, GetWindowThreadProcessId, IsIconic, IsWindowVisible,
    SW_RESTORE, SetForegroundWindow, ShowWindow, WS_EX_TOOLWINDOW,
};

use crate::actions::process::{ProcessHandle, verify_identity};

/// Whether `hwnd` is a window Alt+Tab would list.
///
/// Visible, not owned by another window (dialogs and tooltips are), not a
/// tool window, not cloaked (UWP keeps hidden frames "visible" but cloaked),
/// has a title, and has a non-empty area.
fn is_app_window(hwnd: HWND) -> bool {
    // SAFETY: all of these accept any HWND and fail softly on a stale one.
    unsafe {
        if IsWindowVisible(hwnd) == 0 || !GetWindow(hwnd, GW_OWNER).is_null() {
            return false;
        }
        if (GetWindowLongW(hwnd, GWL_EXSTYLE).cast_unsigned() & WS_EX_TOOLWINDOW) != 0 {
            return false;
        }
        if GetWindowTextLengthW(hwnd) == 0 {
            return false;
        }
        let mut cloaked = 0_u32;
        let size = u32::try_from(size_of::<u32>()).unwrap_or(4);
        if DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED.cast_unsigned(),
            (&raw mut cloaked).cast(),
            size,
        ) >= 0
            && cloaked != 0
        {
            return false;
        }
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        GetWindowRect(hwnd, &raw mut rect) != 0 && rect.right > rect.left && rect.bottom > rect.top
    }
}

fn owner_pid(hwnd: HWND) -> u32 {
    let mut pid = 0_u32;
    // SAFETY: `pid` is a live out-param.
    unsafe { GetWindowThreadProcessId(hwnd, &raw mut pid) };
    pid
}

/// Every top-level app window, with its owning PID, in Z order.
fn app_windows() -> Vec<(HWND, u32)> {
    unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> windows_sys::core::BOOL {
        // SAFETY: `lparam` is the `&mut Vec` passed below, alive for the
        // whole synchronous enumeration.
        let out = unsafe { &mut *(lparam as *mut Vec<(HWND, u32)>) };
        if is_app_window(hwnd) {
            out.push((hwnd, owner_pid(hwnd)));
        }
        1
    }
    let mut out: Vec<(HWND, u32)> = Vec::with_capacity(64);
    // SAFETY: the callback only touches `out`, which outlives the call.
    unsafe { EnumWindows(Some(collect), (&raw mut out) as LPARAM) };
    out
}

/// PIDs that own at least one window a person can switch to.
#[must_use]
pub fn windowed_pids() -> HashSet<u32> {
    app_windows().into_iter().map(|(_, pid)| pid).collect()
}

/// Brings the process's topmost app window to the foreground, restoring it
/// if minimised.
///
/// Returns `Ok(false)` when the process has no such window — a service, a
/// console host, a tray-only app — so the UI can say so instead of failing.
///
/// # Errors
///
/// - [`Error::NotFound`] when the process has exited or its PID was reused.
pub fn bring_to_front(key: ProcessKey) -> Result<bool> {
    // Identity first: a recycled PID must not raise an unrelated window.
    match ProcessHandle::open(
        key.pid,
        windows_sys::Win32::System::Threading::PROCESS_QUERY_LIMITED_INFORMATION,
    ) {
        Ok(handle) => verify_identity(&handle, key)?,
        // Another account's or a protected process: its windows (if any) are
        // still on this desktop, and raising one changes nothing about it.
        Err(Error::AccessDenied { .. }) => {}
        Err(other) => return Err(other),
    }
    let pid = key.pid.get();
    let Some((hwnd, _)) = app_windows().into_iter().find(|(_, owner)| *owner == pid) else {
        return Ok(false);
    };
    // SAFETY: `hwnd` came from the enumeration just now; a window closed in
    // between makes these calls fail, which is reported as `false`.
    unsafe {
        if IsIconic(hwnd) != 0 {
            ShowWindow(hwnd, SW_RESTORE);
        }
        // Vitals is the foreground app (the user just clicked its menu), so
        // Windows lets it hand the foreground on.
        SetForegroundWindow(hwnd);
        Ok(GetForegroundWindow() == hwnd)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_desktop_session_has_explorer_or_some_window_and_no_pid_zero() {
        let pids = windowed_pids();
        assert!(!pids.contains(&0), "PID 0 owns no app window");
        // A headless CI runner may have none; on a desktop there is always
        // at least the taskbar's owner or an app.
        if std::env::var_os("CI").is_none() {
            assert!(!pids.is_empty());
        }
    }

    #[test]
    fn a_process_with_no_window_is_reported_as_such_not_as_an_error() {
        // The test runner is a console process with no window of its own.
        let own = crate::process::ProcessEnumerator::new()
            .enumerate()
            .expect("enumerate")
            .into_iter()
            .find(|p| p.key.pid.get() == std::process::id())
            .expect("own process");
        assert!(!bring_to_front(own.key).expect("no error"));
    }
}
