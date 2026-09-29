//! Webview errors, written to the same capped log file as the backend.
//!
//! A release build has no devtools, so an error thrown in the webview — a
//! render crash caught by a boundary, a rejected promise nobody awaited, a
//! failed boot — was printed to a console no user can open and then lost.
//! Routing it through `tracing` puts it in `vitals.log` beside the backend's
//! own lines, which is the one file a beta tester is asked to attach.
//!
//! Everything arriving here is untrusted text from the webview, so it is
//! capped in size, capped in rate, and escaped before it reaches the file.
//! Nothing here may panic: the release profile aborts on panic (ADR 0026), and
//! a logging call that kills the app would turn a small error into a crash.

use std::sync::{LazyLock, Mutex, PoisonError};
use std::time::{Duration, Instant};

/// Enough for any real error message; a message longer than this is almost
/// always a serialised payload that someone threw by mistake.
const MAX_MESSAGE_CHARS: usize = 2_000;
/// A deep React component stack runs to a few kilobytes; this keeps all of it
/// while stopping one event from eating a meaningful share of the 2 MB cap.
const MAX_STACK_CHARS: usize = 8_000;
/// The source is a short label chosen by our own code; anything longer is
/// not ours.
const MAX_SOURCE_CHARS: usize = 64;
/// A render loop that throws every frame would otherwise write sixty lines a
/// second and fill the capped log in minutes, pushing out the first error —
/// which is the only one that matters.
const MAX_EVENTS_PER_WINDOW: u32 = 20;
const WINDOW: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Level {
    Error,
    Warn,
    Info,
}

/// Unknown levels become `Warn` rather than being rejected: a typo in the
/// webview must not make an error vanish, and `Warn` is loud enough to be
/// seen without claiming to be an error it may not be.
fn parse_level(level: &str) -> Level {
    if level.eq_ignore_ascii_case("error") {
        Level::Error
    } else if level.eq_ignore_ascii_case("info") {
        Level::Info
    } else {
        Level::Warn
    }
}

/// Cuts `text` to at most `max` characters, marking the cut with an ellipsis.
///
/// Counted in `char`s and cut at a `char_indices` boundary, never at a byte
/// offset: slicing a `String` inside a multi-byte character panics, and
/// Romanian diacritics make that a two-byte character away in every message.
fn truncate_chars(mut text: String, max: usize) -> String {
    if let Some((cut, _)) = text.char_indices().nth(max) {
        text.truncate(cut);
        text.push('…');
    }
    text
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Admission {
    /// Write it. `dropped_before` is how many were refused since the last one
    /// admitted, so the log says a gap exists rather than hiding it.
    Admit {
        dropped_before: u64,
    },
    Refuse,
}

/// A fixed-window counter. Coarser than a token bucket, and sufficient: the
/// goal is to keep a runaway loop from flooding the file, not fair queuing.
#[derive(Debug)]
struct Limiter {
    window_start: Instant,
    admitted: u32,
    dropped: u64,
}

impl Limiter {
    const fn new(now: Instant) -> Self {
        Self {
            window_start: now,
            admitted: 0,
            dropped: 0,
        }
    }

    fn admit(&mut self, now: Instant) -> Admission {
        if now.saturating_duration_since(self.window_start) >= WINDOW {
            self.window_start = now;
            self.admitted = 0;
        }
        if self.admitted >= MAX_EVENTS_PER_WINDOW {
            self.dropped = self.dropped.saturating_add(1);
            return Admission::Refuse;
        }
        self.admitted += 1;
        Admission::Admit {
            dropped_before: std::mem::take(&mut self.dropped),
        }
    }
}

static LIMITER: LazyLock<Mutex<Limiter>> =
    LazyLock::new(|| Mutex::new(Limiter::new(Instant::now())));

/// Writes one webview error, warning or note to the log file.
///
/// Cross-platform and available to every window: the overlay's boot can fail
/// just as the main window's can. Desktop-only in the sense that the LAN has
/// no equivalent — a phone's errors belong to the phone's browser, and a
/// paired device must not be able to write into this machine's log.
#[tauri::command]
// Tauri deserialises command arguments into owned values; it cannot hand a
// command a borrowed `&str`, so `level` arrives as a `String` it only reads.
#[allow(clippy::needless_pass_by_value)]
pub fn log_webview(level: String, message: String, stack: Option<String>, source: String) {
    // A poisoned lock only means another thread panicked mid-update; the
    // counter is still a usable number, and refusing to log would hide the
    // very failure that poisoned it.
    let admission = LIMITER
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .admit(Instant::now());
    let Admission::Admit { dropped_before } = admission else {
        return;
    };
    if dropped_before > 0 {
        tracing::warn!(
            target: "webview",
            dropped = dropped_before,
            "webview events dropped by the rate limit"
        );
    }
    let message = truncate_chars(message, MAX_MESSAGE_CHARS);
    let stack = stack.map(|s| truncate_chars(s, MAX_STACK_CHARS));
    let source = truncate_chars(source, MAX_SOURCE_CHARS);
    emit(parse_level(&level), &message, stack.as_deref(), &source);
}

fn emit(level: Level, message: &str, stack: Option<&str>, source: &str) {
    // `escape_debug`, not the raw text: a message containing a newline would
    // otherwise write what looks like a second, forged log line. Unlike
    // `escape_default` it leaves printable non-ASCII alone, so Romanian text
    // stays readable.
    let message = message.escape_debug();
    let source = source.escape_debug();
    let stack = stack.unwrap_or("").escape_debug();
    match level {
        Level::Error => tracing::error!(target: "webview", %source, %stack, "{message}"),
        Level::Warn => tracing::warn!(target: "webview", %source, %stack, "{message}"),
        Level::Info => tracing::info!(target: "webview", %source, %stack, "{message}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncation_cuts_on_a_character_boundary_inside_multibyte_text() {
        // Two-byte Romanian letters and four-byte emoji: a byte-offset cut at
        // any of these positions would panic or produce invalid UTF-8.
        let cut = truncate_chars("ăș🙂🙂x".to_owned(), 3);
        assert_eq!(cut, "ăș🙂…");
        assert_eq!(
            cut.chars().count(),
            4,
            "three kept characters plus the marker"
        );
    }

    #[test]
    fn truncation_leaves_text_within_the_limit_untouched() {
        assert_eq!(truncate_chars("țară".to_owned(), 4), "țară");
        assert_eq!(truncate_chars(String::new(), 0), "");
    }

    #[test]
    fn the_rate_limit_drops_the_twenty_first_event_and_reports_the_gap_later() {
        let start = Instant::now();
        let mut limiter = Limiter::new(start);
        for _ in 0..MAX_EVENTS_PER_WINDOW {
            assert_eq!(limiter.admit(start), Admission::Admit { dropped_before: 0 });
        }
        assert_eq!(limiter.admit(start), Admission::Refuse);
        assert_eq!(limiter.admit(start + WINDOW / 2), Admission::Refuse);

        assert_eq!(
            limiter.admit(start + WINDOW),
            Admission::Admit { dropped_before: 2 },
            "the next window admits again and says how many were lost"
        );
    }

    #[test]
    fn an_unknown_level_is_logged_as_a_warning_rather_than_discarded() {
        assert_eq!(parse_level("fatal"), Level::Warn);
        assert_eq!(parse_level(""), Level::Warn);
        assert_eq!(parse_level("ERROR"), Level::Error);
        assert_eq!(parse_level("info"), Level::Info);
        assert_eq!(parse_level("warn"), Level::Warn);
    }

    #[test]
    fn oversized_hostile_input_is_accepted_without_panicking() {
        let huge = "ă\n".repeat(50_000);
        log_webview("error".to_owned(), huge.clone(), Some(huge.clone()), huge);
    }
}
