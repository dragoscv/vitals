//! The few numbers that define what this server does, in one place so a test
//! can assert the boundary instead of re-deriving it — and the read-only
//! guarantee, which is asserted rather than reviewed.

/// The most processes any tool returns, however large a `top` a client asks
/// for.
///
/// Clamped rather than refused, the same choice `/api/v1/summary` makes: an
/// agent that asks for 500 processes wants "all of them" and will render a
/// shorter list happily, where an error would leave it with none.
pub const MAX_TOP: usize = 100;

/// What `summary` returns when the client does not say.
pub const SUMMARY_DEFAULT_TOP: usize = 10;

/// What `processes` returns when the client does not say. Larger than
/// `summary`'s: a caller asking for the process list is usually ranking it.
pub const PROCESSES_DEFAULT_TOP: usize = 20;

/// What `history` covers when the client does not say. An hour, which is what
/// the store keeps at its finest resolution.
pub const HISTORY_DEFAULT_SECONDS: u32 = 3_600;

/// The longest span `history` will read: a week, the widest tier the store
/// keeps. Beyond that the answer would be the same rows with more gaps.
pub const HISTORY_MAX_SECONDS: u32 = 7 * 24 * 3_600;

/// Word stems that mean "change something on the machine".
///
/// Nothing here is a name this server may ever expose. The tools are
/// read-only by design, and a test asserting that is cheaper and more durable
/// than remembering it in review — the failure mode this guards is a later
/// change adding `end_process` because it seemed convenient.
pub const CONTROL_STEMS: [&str; 10] = [
    "kill",
    "end",
    "terminate",
    "stop",
    "suspend",
    "resume",
    "restart",
    "priority",
    "elevate",
    "set",
];

/// Whether a tool name names an action on the machine rather than a reading.
///
/// Matched anywhere in the name, so `end_process` and `process_priority` are
/// both caught, and case-insensitively because none of this should depend on
/// how someone spelled it.
#[must_use]
pub fn is_control_name(name: &str) -> bool {
    let lowered = name.to_ascii_lowercase();
    CONTROL_STEMS.iter().any(|stem| lowered.contains(stem))
}

/// `top`, clamped into `1..=MAX_TOP`, falling back to `default` when absent.
///
/// Zero is treated as "not given" rather than "none": no tool in this server
/// has a meaningful empty list, and the alternative is a client that asked
/// for `top: 0` getting an error where it meant "the usual".
#[must_use]
pub fn top(requested: Option<usize>, default: usize) -> usize {
    match requested {
        None | Some(0) => default.clamp(1, MAX_TOP),
        Some(n) => n.clamp(1, MAX_TOP),
    }
}

/// `seconds`, clamped into `60..=HISTORY_MAX_SECONDS`.
#[must_use]
pub fn seconds(requested: Option<u32>) -> u32 {
    requested
        .unwrap_or(HISTORY_DEFAULT_SECONDS)
        .clamp(60, HISTORY_MAX_SECONDS)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::tools::Tool;

    #[test]
    fn no_tool_this_server_offers_acts_on_the_machine() {
        // The row asks for no control tools in v1. This is what makes that a
        // property of the code rather than a sentence in a document.
        for tool in Tool::ALL {
            assert!(
                !is_control_name(tool.name()),
                "`{}` reads like an action; MCP has no control tools",
                tool.name()
            );
        }
    }

    #[test]
    fn the_control_detector_catches_the_words_it_exists_for() {
        for name in [
            "kill",
            "end_process",
            "terminate_process",
            "suspend",
            "resume",
            "priority",
            "set_priority",
            "KILL_PROCESS",
            "restart_service",
            "elevate",
        ] {
            assert!(is_control_name(name), "`{name}` must be refused");
        }
        for name in [
            "summary",
            "processes",
            "sensors",
            "alerts",
            "history",
            "diagnosis",
        ] {
            assert!(!is_control_name(name), "`{name}` is a reading");
        }
    }

    #[test]
    fn top_is_clamped_so_a_greedy_agent_gets_a_list_rather_than_an_error() {
        assert_eq!(top(None, 10), 10);
        assert_eq!(top(Some(3), 10), 3);
        assert_eq!(top(Some(5_000), 10), MAX_TOP);
        assert_eq!(top(Some(0), 10), 10, "zero means the default, not nothing");
        assert_eq!(top(Some(1), 10), 1);
        assert_eq!(top(Some(usize::MAX), 10), MAX_TOP);
    }

    #[test]
    fn history_is_clamped_to_the_span_the_store_actually_keeps() {
        assert_eq!(seconds(None), HISTORY_DEFAULT_SECONDS);
        assert_eq!(seconds(Some(1)), 60, "a minute is the finest ask");
        assert_eq!(seconds(Some(3_600)), 3_600);
        assert_eq!(seconds(Some(u32::MAX)), HISTORY_MAX_SECONDS);
    }

    #[test]
    fn the_week_ceiling_is_a_week() {
        // Spelled out, because `7 * 24 * 3600` and `7 * 24 * 60` differ by a
        // factor of sixty and both look like a week in a hurry.
        assert_eq!(HISTORY_MAX_SECONDS, 604_800);
        const {
            assert!(MAX_TOP > 0);
            assert!(HISTORY_MAX_SECONDS > HISTORY_DEFAULT_SECONDS);
        }
    }
}
