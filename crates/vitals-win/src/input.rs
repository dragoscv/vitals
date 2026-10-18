//! Whether anyone is at the machine, and whether Windows is ending the session.
//!
//! Both exist for the updater (vitals ADR-0038 follow-up, 2026-10-18). It used
//! to install only when the app quit, but a tray monitor almost never quits:
//! on this machine the only "quits" in two weeks were Windows restarts, and
//! each one started the setup three seconds after the restart began, so the
//! session ended under it. 1.0.0, 1.0.1 and 1.0.2 were downloaded, verified
//! and lost that way while 0.9.0 kept running.

use windows_sys::Win32::System::SystemInformation::GetTickCount64;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
use windows_sys::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_SHUTTINGDOWN};

/// Seconds since the last keyboard or mouse input in this session.
///
/// `None` when Windows cannot say (a service session has no input desktop):
/// an unknown absence is not a long one, so nothing that waits for idleness
/// may treat it as such.
#[must_use]
pub fn idle_secs() -> Option<u64> {
    let mut info = LASTINPUTINFO {
        cbSize: u32::try_from(size_of::<LASTINPUTINFO>()).ok()?,
        dwTime: 0,
    };
    // SAFETY: `cbSize` is set and the struct is live for the call.
    if unsafe { GetLastInputInfo(&raw mut info) } == 0 {
        return None;
    }
    // SAFETY: no preconditions.
    let now = unsafe { GetTickCount64() };
    Some(u64::from(elapsed_ms(now, info.dwTime)) / 1000)
}

/// `dwTime` is the low 32 bits of the tick count, so the difference is taken
/// in that width: the 49.7-day wrap must not read as a very long absence.
fn elapsed_ms(now: u64, last_input: u32) -> u32 {
    let low = u32::try_from(now & u64::from(u32::MAX)).unwrap_or(0);
    low.wrapping_sub(last_input)
}

/// True while Windows is logging off, restarting or shutting down.
///
/// Anything started now is killed with the session, so work that must not be
/// cut in half (an installer replacing the app's own files) waits for the
/// next one instead.
#[must_use]
pub fn session_ending() -> bool {
    // SAFETY: no preconditions.
    unsafe { GetSystemMetrics(SM_SHUTTINGDOWN) != 0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_time_is_measured_in_the_low_32_bits_so_the_tick_wrap_is_not_an_absence() {
        // Ten seconds before the wrap, read five seconds after it.
        let last = u32::MAX - 10_000;
        let now = (1_u64 << 32) + 5_000;
        assert_eq!(elapsed_ms(now, last), 15_001);
    }

    #[test]
    fn an_input_at_this_tick_is_zero_idle() {
        assert_eq!(elapsed_ms(123_456, 123_456), 0);
    }
}
