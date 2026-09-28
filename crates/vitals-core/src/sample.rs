//! Sampling cadence and frame envelopes.

use crate::metrics::SystemMetrics;
use crate::process::Process;

/// Monotonically increasing frame counter.
///
/// The UI uses gaps in this sequence to detect dropped frames rather than
/// guessing from timestamps, which are subject to clock adjustment.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Default,
    serde::Serialize,
    serde::Deserialize,
)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(transparent)]
pub struct FrameSeq(#[cfg_attr(feature = "ts", ts(type = "number"))] pub u64);

impl FrameSeq {
    #[inline]
    #[must_use]
    pub const fn next(self) -> Self {
        Self(self.0.wrapping_add(1))
    }
}

/// How often the sampler runs.
///
/// Adaptive by design. A task manager that burns 3% CPU while minimised — as
/// several popular ones do — is self-defeating, so the rate drops hard when
/// nothing is visible and rises only for the surface actually on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(rename_all = "camelCase")]
pub enum SampleRate {
    /// 10 Hz — only for a focused realtime graph.
    Realtime,
    /// 2 Hz — the default for a visible, focused window.
    High,
    /// 1 Hz — matches Task Manager; visible but unfocused.
    Normal,
    /// 0.25 Hz — window hidden, tray only.
    Low,
    /// 0.05 Hz — minimised or on battery saver; keeps history continuous
    /// without meaningful cost.
    Background,
    Paused,
}

impl SampleRate {
    /// Interval in milliseconds, or `None` when paused.
    #[must_use]
    pub const fn interval_ms(self) -> Option<u32> {
        Some(match self {
            Self::Realtime => 100,
            Self::High => 500,
            Self::Normal => 1_000,
            Self::Low => 4_000,
            Self::Background => 20_000,
            Self::Paused => return None,
        })
    }

    /// The rate appropriate to the current window state.
    ///
    /// Centralised so the policy is testable and cannot drift between the
    /// several places that observe window events.
    #[must_use]
    pub const fn for_window_state(visible: bool, focused: bool, power_saving: bool) -> Self {
        if !visible {
            // Nothing is on screen, so nothing needs to be current. History
            // stays continuous at a cost close to zero.
            return Self::Background;
        }
        if power_saving || !focused {
            return Self::Low;
        }
        Self::Normal
    }
}

/// What a frame carries. Most frames are deltas.
///
/// Sending 2000 full process records at 1 Hz is ~2 MB/s of serialisation for
/// data that barely changes. Deltas cut that by well over an order of
/// magnitude, and a periodic keyframe bounds the damage if one is ever lost.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum FramePayload {
    /// A complete snapshot. Sent on connect and periodically thereafter.
    Keyframe {
        system: SystemMetrics,
        processes: Vec<Process>,
    },
    /// Changes since the previous frame.
    Delta {
        system: SystemMetrics,
        /// Processes that appeared or whose values changed.
        changed: Vec<Process>,
        /// Processes that exited, by PID.
        exited: Vec<crate::ids::Pid>,
    },
}

/// One sampling tick, as delivered to the UI.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(rename_all = "camelCase")]
pub struct Frame {
    pub seq: FrameSeq,
    /// Milliseconds since the Unix epoch, for display and correlation only.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub timestamp_ms: u64,
    /// Actual elapsed time since the previous frame.
    ///
    /// Rate calculations must divide by this, not by the nominal interval.
    /// Under load the sampler runs late, and assuming a perfect 1000 ms is
    /// how monitors end up reporting throughput spikes that never happened.
    pub elapsed_ms: u32,
    pub payload: FramePayload,
}

impl Frame {
    #[must_use]
    pub const fn is_keyframe(&self) -> bool {
        matches!(self.payload, FramePayload::Keyframe { .. })
    }

    /// The machine-wide metrics, present in both payload kinds.
    #[must_use]
    pub const fn system(&self) -> &SystemMetrics {
        match &self.payload {
            FramePayload::Keyframe { system, .. } | FramePayload::Delta { system, .. } => system,
        }
    }

    /// Converts a byte count observed over this frame into a per-second rate.
    ///
    /// Guards against a zero interval, which happens when two samples land in
    /// the same millisecond and would otherwise divide by zero.
    #[must_use]
    pub fn rate_per_sec(&self, delta_bytes: u64) -> u64 {
        if self.elapsed_ms == 0 {
            return 0;
        }
        (u128::from(delta_bytes) * 1000 / u128::from(self.elapsed_ms)).min(u128::from(u64::MAX))
            as u64
    }

    /// Runs `f` on this frame with the fields that identify a person or a
    /// device — process owners and adapter MAC addresses — taken out, then
    /// puts them back.
    ///
    /// For anything written to disk without the user asking: the flight
    /// recorder runs on every tick, and a crash-survivable file of who was
    /// signed in and which hardware addresses the machine has is more than a
    /// bug report needs. Moving the values out and back costs a few pointer
    /// swaps; cloning a 600-process keyframe to redact it would cost the
    /// sampler more than the write itself.
    pub fn with_identifiers_removed<R>(&mut self, f: impl FnOnce(&Self) -> R) -> R {
        let (system, processes) = match &mut self.payload {
            FramePayload::Keyframe { system, processes } => (system, processes),
            FramePayload::Delta {
                system, changed, ..
            } => (system, changed),
        };
        let users: Vec<Option<String>> = processes.iter_mut().map(|p| p.user.take()).collect();
        let macs: Vec<Option<String>> = system.networks.iter_mut().map(|n| n.mac.take()).collect();

        let result = f(self);

        let (system, processes) = match &mut self.payload {
            FramePayload::Keyframe { system, processes } => (system, processes),
            FramePayload::Delta {
                system, changed, ..
            } => (system, changed),
        };
        for (process, user) in processes.iter_mut().zip(users) {
            process.user = user;
        }
        for (network, mac) in system.networks.iter_mut().zip(macs) {
            network.mac = mac;
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_rate_the_frontend_can_send_deserialises() {
        // These strings are what `apps/desktop/src/lib/sampleRate.ts` puts on
        // the wire, and the generated `SampleRate.ts` union is what allows
        // them. A rename here would still typecheck on both sides — ts-rs
        // regenerates the union and TypeScript is happy — while every
        // `set_sample_rate` call started failing at runtime, in a background
        // invoke whose rejection is deliberately swallowed. Nothing else
        // catches that.
        let wire = [
            ("realtime", SampleRate::Realtime),
            ("high", SampleRate::High),
            ("normal", SampleRate::Normal),
            ("low", SampleRate::Low),
            ("background", SampleRate::Background),
            ("paused", SampleRate::Paused),
        ];

        for (name, expected) in wire {
            let parsed: SampleRate = serde_json::from_str(&format!("\"{name}\""))
                .unwrap_or_else(|error| panic!("{name} must deserialise: {error}"));

            assert_eq!(parsed, expected, "{name}");
        }
    }

    #[test]
    fn hidden_window_drops_to_background_rate() {
        assert_eq!(
            SampleRate::for_window_state(false, false, false),
            SampleRate::Background
        );
        assert_eq!(
            SampleRate::for_window_state(false, true, false),
            SampleRate::Background,
            "focus is irrelevant when the window is not visible"
        );
    }

    #[test]
    fn visible_and_focused_uses_normal_rate() {
        assert_eq!(
            SampleRate::for_window_state(true, true, false),
            SampleRate::Normal
        );
    }

    #[test]
    fn power_saving_throttles_even_when_focused() {
        assert_eq!(
            SampleRate::for_window_state(true, true, true),
            SampleRate::Low
        );
    }

    #[test]
    fn paused_has_no_interval() {
        assert_eq!(SampleRate::Paused.interval_ms(), None);
        assert_eq!(SampleRate::Normal.interval_ms(), Some(1_000));
    }

    #[test]
    fn rate_uses_actual_elapsed_time_not_nominal() {
        // 1000 bytes over a late 2000ms frame is 500 B/s, not 1000 B/s.
        let frame = Frame {
            seq: FrameSeq(1),
            timestamp_ms: 0,
            elapsed_ms: 2000,
            payload: FramePayload::Delta {
                system: SystemMetrics::default(),
                changed: vec![],
                exited: vec![],
            },
        };
        assert_eq!(frame.rate_per_sec(1000), 500);
    }

    #[test]
    fn rate_survives_zero_elapsed_interval() {
        let frame = Frame {
            seq: FrameSeq(1),
            timestamp_ms: 0,
            elapsed_ms: 0,
            payload: FramePayload::Delta {
                system: SystemMetrics::default(),
                changed: vec![],
                exited: vec![],
            },
        };
        assert_eq!(frame.rate_per_sec(1000), 0, "must not divide by zero");
    }
}
