//! The few numbers that define how the publisher behaves, in one place so a
//! test can assert the boundary instead of re-deriving it.

use std::time::Duration;

/// Every publish uses this level: at least once.
///
/// `QoS` 1 rather than 0 because a dropped reading is a Home Assistant sensor
/// stuck at an old value with nothing to say so, and rather than 2 because the
/// four-way handshake buys nothing for a reading that is superseded in ten
/// seconds. Kept as a number as well as the `rumqttc` enum so a test can state
/// the requirement (`QoS 1`) rather than the library's spelling of it.
pub const QOS_LEVEL: u8 = 1;

/// Discovery payloads are retained: Home Assistant restarts and must find the
/// entities without waiting for the next publish. So is the availability
/// topic, for the same reason. Ordinary readings are **not** retained — a
/// stale CPU percentage served from the broker's store after this machine
/// went to sleep is worse than no value.
pub const RETAIN_DISCOVERY: bool = true;
/// See [`RETAIN_DISCOVERY`].
pub const RETAIN_AVAILABILITY: bool = true;
/// Readings are state, not configuration.
pub const RETAIN_STATE: bool = false;

/// The keep-alive the broker is promised, in seconds. The client pings at this
/// interval; shorter means a dead link is noticed sooner, longer means fewer
/// bytes on a metered connection.
pub const KEEP_ALIVE: Duration = Duration::from_secs(30);

/// Delay before the first reconnect attempt.
pub const RECONNECT_BASE: Duration = Duration::from_secs(1);
/// The ceiling a doubling backoff reaches. A broker that is down for
/// maintenance must not be hammered once a second for an hour.
pub const RECONNECT_CAP: Duration = Duration::from_secs(60);

/// How long to wait before reconnect attempt `attempt` (1-based).
///
/// Doubling from [`RECONNECT_BASE`], capped at [`RECONNECT_CAP`]. Deliberately
/// without jitter: this runs on one machine talking to one broker, not in a
/// fleet that could stampede, and a deterministic delay is one a log line can
/// be read against.
#[must_use]
pub fn backoff(attempt: u32) -> Duration {
    let shift = attempt.saturating_sub(1).min(6);
    let scaled = RECONNECT_BASE.saturating_mul(1u32.checked_shl(shift).unwrap_or(u32::MAX));
    scaled.min(RECONNECT_CAP)
}

/// The payload on the availability topic.
///
/// Home Assistant treats anything that is not `online` as offline, so these
/// two strings are the whole contract. `offline` is also sent explicitly on a
/// clean shutdown; a crash leaves the broker to publish the same word from the
/// Last Will.
#[must_use]
pub const fn availability_payload(online: bool) -> &'static str {
    if online { "online" } else { "offline" }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_backoff_doubles_and_then_stops_growing() {
        assert_eq!(backoff(1), Duration::from_secs(1));
        assert_eq!(backoff(2), Duration::from_secs(2));
        assert_eq!(backoff(3), Duration::from_secs(4));
        assert_eq!(backoff(7), Duration::from_secs(60), "64 s is capped");
        assert_eq!(backoff(30), Duration::from_secs(60));
        assert_eq!(backoff(0), Duration::from_secs(1), "never zero");
    }

    #[test]
    fn the_null_backoff_never_overflows() {
        assert_eq!(backoff(u32::MAX), RECONNECT_CAP);
    }

    #[test]
    fn the_availability_vocabulary_is_the_one_home_assistant_reads() {
        assert_eq!(availability_payload(true), "online");
        assert_eq!(availability_payload(false), "offline");
    }

    #[test]
    fn readings_are_never_retained_and_discovery_always_is() {
        assert_eq!(QOS_LEVEL, 1, "the row asks for QoS 1 specifically");
        // Evaluated at compile time rather than as a runtime assertion: these
        // are constants, and `clippy::assertions_on_constants` is right that a
        // runtime `assert!` on one can never fail at run time. A `const` block
        // keeps the check, and makes a wrong value a build error.
        const {
            assert!(RETAIN_DISCOVERY, "discovery must be retained");
            assert!(RETAIN_AVAILABILITY, "availability must be retained");
            assert!(!RETAIN_STATE, "a reading is state, not configuration");
        }
    }
}
