//! Seconds since the last keyboard or mouse input.
//!
//! macOS has no "last input time" syscall. `CGEventSourceSecondsSinceLastEventType`
//! is what the screen saver, `pmset` and everyone else uses for exactly this
//! question, and it is a plain C function in CoreGraphics: no Objective-C, no
//! run loop, no accessibility permission, nothing to entitle.
//!
//! This exists because the updater will only replace the app under nobody's
//! nose, and on macOS the platform half of that question was answered with
//! `None` — so a Mac never reached the idle branch and a waiting update sat
//! there until the user quit. The Windows answer has been there since S21-05;
//! this is the other half.

use std::ffi::{c_double, c_uint};

/// The two constants from `CGEventTypes.h`, whose values are stable public
/// API rather than an implementation detail.
///
/// `kCGEventSourceStateCombinedSessionState` (0) is the state that merges
/// hardware and synthetic events, which is what "did a person touch this
/// machine" means. `kCGAnyInputEventType` is `0xFFFFFFFF`.
const COMBINED_SESSION_STATE: c_uint = 0;
const ANY_INPUT_EVENT_TYPE: c_uint = 0xFFFF_FFFF;

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    /// Seconds since the last event of `event_type` from `state`.
    fn CGEventSourceSecondsSinceLastEventType(state: c_uint, event_type: c_uint) -> c_double;
}

/// Seconds since the last keyboard or mouse input, or `None` when macOS will
/// not answer.
///
/// `None` rather than `0`: the idle-install rule treats "cannot tell" as "not
/// idle", so both values would refuse to install — but they are different
/// facts, and a fabricated zero would also be wrong the other way round, where
/// a genuine zero means the user is typing this second. Every other reading in
/// Vitals keeps that distinction; this one keeps it too.
#[must_use]
pub fn idle_secs() -> Option<u64> {
    // SAFETY: a plain C function with no memory preconditions and no ownership
    // contract. It returns a double, so there is nothing to free and no
    // pointer to validate.
    let seconds = unsafe {
        CGEventSourceSecondsSinceLastEventType(COMBINED_SESSION_STATE, ANY_INPUT_EVENT_TYPE)
    };
    // A negative or non-finite answer means the API could not tell; macOS has
    // been seen to return -1 for an unknown session state rather than an error.
    if !seconds.is_finite() || seconds < 0.0 {
        return None;
    }
    // A session up for years exceeds u64 seconds never; truncation is the
    // intended rounding to whole seconds.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Some(seconds as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn something_has_been_touched_recently_or_the_answer_is_absent() {
        // The value is whatever this machine has been doing, so nothing about
        // it is asserted beyond being a real answer: the point of the test is
        // that the call is wired and returns rather than trapping.
        if let Some(seconds) = idle_secs() {
            // A session cannot report more idle time than the machine has been
            // up for in any plausible reading; a wild value would mean the
            // wrong constant was passed.
            assert!(
                seconds < 60 * 60 * 24 * 365,
                "idle reported as {seconds} seconds, which is not a session"
            );
        }
    }
}
