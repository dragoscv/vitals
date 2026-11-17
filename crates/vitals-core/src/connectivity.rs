//! "Is it my PC, my Wi-Fi, or my internet?" — the model and the verdict.
//!
//! Pure on purpose. The round trips are taken by platform code, which is a
//! sequence of sockets and syscalls that a unit test cannot run against a
//! controlled network; the *decision* — which of the three is at fault — is a
//! function of the numbers, so it lives here with tests instead of being
//! tangled into the probing.
//!
//! The question has exactly three answers a person can act on, and the whole
//! point is to give one of them rather than a list of latencies:
//!
//! - the default gateway answers but nothing beyond it does → **the internet**;
//! - the gateway itself does not answer → **this machine's link** (Wi-Fi or
//!   cable, or the router in front of it);
//! - everything answers → nothing is wrong, and saying so is the answer.
//!
//! Anything less than that — a probe that never ran, a machine with no default
//! route — is [`Connectivity::Unknown`], never a guess.

use serde::{Deserialize, Serialize};

/// The public resolvers each probe tries, in order.
///
/// Two of them, on different networks, so one being blocked (some ISPs
/// blackhole one or the other) does not read as "the internet is down".
pub const INTERNET_TARGETS: [&str; 2] = ["1.1.1.1", "8.8.8.8"];

/// Round trips sent per target. Four is enough for the last one to be
/// believable and small enough that a probe is over in a second.
pub const PROBES_PER_TARGET: u32 = 4;

/// Above this share of lost probes, a target counts as not answering.
///
/// A quarter rather than zero: a single dropped packet on Wi-Fi is normal, and
/// a probe that called the internet "down" because one of four round trips
/// timed out would be worse than useless.
pub const LOSS_LIMIT_PERCENT: f32 = 25.0;

/// One target's four round trips.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(rename_all = "camelCase")]
pub struct TargetProbe {
    /// The address as the user would recognise it, e.g. `192.168.1.1`.
    pub target: String,
    pub sent: u32,
    pub received: u32,
    /// Mean of the round trips that came back. `None` when none did — never a
    /// zero, which would read as an impossibly fast reply.
    pub rtt_ms: Option<f64>,
}

impl TargetProbe {
    /// The share of probes that did not come back, as a percentage.
    #[must_use]
    pub fn loss_percent(&self) -> f32 {
        if self.sent == 0 {
            return 0.0;
        }
        let lost = self.sent.saturating_sub(self.received);
        #[allow(clippy::cast_precision_loss)] // four probes; exact in f32
        {
            lost as f32 / self.sent as f32 * 100.0
        }
    }

    /// Whether enough came back to call the target reachable.
    #[must_use]
    pub fn answered(&self) -> bool {
        self.received > 0 && self.loss_percent() <= LOSS_LIMIT_PERCENT
    }
}

/// The three answers the user can act on, plus "cannot tell".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(rename_all = "camelCase")]
pub enum Connectivity {
    /// Every probe answered: the connection is fine.
    Healthy,
    /// The gateway answers, the far side does not.
    Internet,
    /// The gateway itself does not answer.
    LocalLink,
    /// No default route, or the probe did not run. Not a diagnosis.
    Unknown,
}

/// What one run of the probe measured.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(rename_all = "camelCase")]
pub struct ConnectivityReport {
    /// The default gateway, when the machine has one.
    pub gateway: Option<TargetProbe>,
    /// One entry per target in [`INTERNET_TARGETS`].
    pub internet: Vec<TargetProbe>,
    /// How long a name lookup took. `None` when it was not measured.
    pub dns_ms: Option<f64>,
    pub verdict: Connectivity,
}

impl ConnectivityReport {
    /// Builds the report from the measurements the platform took.
    #[must_use]
    pub fn from_parts(
        gateway: Option<TargetProbe>,
        internet: Vec<TargetProbe>,
        dns_ms: Option<f64>,
    ) -> Self {
        let verdict = verdict(gateway.as_ref(), &internet);
        Self {
            gateway,
            internet,
            dns_ms,
            verdict,
        }
    }
}

/// The decision. See the module docs for the three answers.
///
/// A target that never sent a probe is treated as absent rather than as a
/// failure: a machine with no route to the internet at all must not have its
/// missing measurement read as "the gateway is dead".
#[must_use]
pub fn verdict(gateway: Option<&TargetProbe>, internet: &[TargetProbe]) -> Connectivity {
    let Some(gateway) = gateway else {
        // No default route is a network that is off, not a diagnosis we can
        // make from a round trip.
        return Connectivity::Unknown;
    };
    // At least one target has to have been tried; an empty list means the
    // probe did not reach the internet test, not that the internet is down.
    let tried: Vec<&TargetProbe> = internet.iter().filter(|p| p.sent > 0).collect();
    if tried.is_empty() {
        return Connectivity::Unknown;
    }
    let internet_ok = tried.iter().any(|p| p.answered());
    if !gateway.answered() {
        // The local link is the first hop; if it cannot answer, nothing past
        // it can be blamed.
        return Connectivity::LocalLink;
    }
    if internet_ok {
        Connectivity::Healthy
    } else {
        Connectivity::Internet
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn probe(target: &str, sent: u32, received: u32) -> TargetProbe {
        TargetProbe {
            target: target.to_owned(),
            sent,
            received,
            #[allow(clippy::cast_precision_loss)]
            rtt_ms: (received > 0).then_some(12.0 + f64::from(received)),
        }
    }

    #[test]
    fn a_clean_gateway_and_a_reachable_target_is_healthy() {
        let gateway = probe("192.168.1.1", 4, 4);
        let internet = vec![probe("1.1.1.1", 4, 4)];
        assert_eq!(verdict(Some(&gateway), &internet), Connectivity::Healthy);
    }

    #[test]
    fn a_gateway_that_answers_alone_blames_the_internet() {
        let gateway = probe("192.168.1.1", 4, 4);
        let internet = vec![probe("1.1.1.1", 4, 0), probe("8.8.8.8", 4, 0)];
        assert_eq!(verdict(Some(&gateway), &internet), Connectivity::Internet);
    }

    #[test]
    fn a_gateway_that_does_not_answer_blames_the_local_link() {
        let gateway = probe("192.168.1.1", 4, 0);
        let internet = vec![probe("1.1.1.1", 4, 0)];
        assert_eq!(verdict(Some(&gateway), &internet), Connectivity::LocalLink);
    }

    #[test]
    fn one_blocked_resolver_does_not_read_as_an_outage() {
        // Some networks blackhole one of the two public resolvers; the other
        // answering proves the internet is fine.
        let gateway = probe("192.168.1.1", 4, 4);
        let internet = vec![probe("1.1.1.1", 4, 0), probe("8.8.8.8", 4, 4)];
        assert_eq!(verdict(Some(&gateway), &internet), Connectivity::Healthy);
    }

    #[test]
    fn a_single_dropped_probe_is_not_an_outage() {
        let gateway = probe("192.168.1.1", 4, 3);
        let internet = vec![probe("1.1.1.1", 4, 3)];
        assert_eq!(verdict(Some(&gateway), &internet), Connectivity::Healthy);
        assert!(gateway.answered(), "3 of 4 is a healthy link");
    }

    #[test]
    fn half_the_probes_lost_is_an_outage() {
        let gateway = probe("192.168.1.1", 4, 2);
        let internet = vec![probe("1.1.1.1", 4, 2)];
        assert_eq!(
            verdict(Some(&gateway), &internet),
            Connectivity::LocalLink,
            "half the probes lost is the link, not a blip"
        );
    }

    #[test]
    fn no_default_route_is_unknown_not_a_blame() {
        assert_eq!(verdict(None, &[]), Connectivity::Unknown);
    }

    #[test]
    fn an_untried_internet_probe_is_unknown_not_an_outage() {
        let gateway = probe("192.168.1.1", 4, 4);
        let internet = vec![probe("1.1.1.1", 0, 0)];
        assert_eq!(verdict(Some(&gateway), &internet), Connectivity::Unknown);
    }

    #[test]
    fn loss_is_reported_as_a_share_never_as_a_zero_reply_time() {
        let dead = probe("1.1.1.1", 4, 0);
        assert_eq!(dead.loss_percent(), 100.0);
        assert_eq!(dead.rtt_ms, None, "no reply must not read as 0 ms");
        assert!(!dead.answered());
        let none_sent = probe("1.1.1.1", 0, 0);
        assert_eq!(
            none_sent.loss_percent(),
            0.0,
            "nothing sent is not 100% lost"
        );
    }
}
