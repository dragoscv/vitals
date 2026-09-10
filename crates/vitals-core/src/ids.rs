//! Strongly-typed identifiers.
//!
//! A raw `u32` PID is a footgun: PIDs are reused by the OS within seconds of a
//! process exiting, so a stale PID can silently address a *different* process.
//! Every kill/suspend path in Vitals therefore carries a [`ProcessKey`], which
//! pairs the PID with the process start time — the standard technique for
//! making the reference unambiguous.

use std::fmt;

/// Generates the shared impls for a numeric identifier newtype.
///
/// As in [`crate::units`], the type is declared at the call site because the
/// `specta` derive cannot resolve a field type passed through a macro
/// metavariable.
macro_rules! id_impls {
    ($name:ident, $inner:ty) => {
        impl $name {
            #[inline]
            #[must_use]
            pub const fn get(self) -> $inner {
                self.0
            }
        }

        impl From<$inner> for $name {
            #[inline]
            fn from(v: $inner) -> Self {
                Self(v)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                fmt::Display::fmt(&self.0, f)
            }
        }
    };
}

/// An OS process identifier.
///
/// Not unique over time — see [`ProcessKey`].
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(transparent)]
pub struct Pid(pub u32);
id_impls!(Pid, u32);

/// An OS thread identifier.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(transparent)]
pub struct Tid(pub u32);
id_impls!(Tid, u32);

/// A physical or logical disk, stable for the lifetime of a boot.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(transparent)]
pub struct DiskId(pub u32);
id_impls!(DiskId, u32);

/// A network interface.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(transparent)]
pub struct NicId(pub u32);
id_impls!(NicId, u32);

/// A graphics adapter.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(transparent)]
pub struct GpuId(pub u32);
id_impls!(GpuId, u32);

/// A hardware sensor, unique within a [`crate::sensor::SensorKind`] and source.
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(transparent)]
pub struct SensorId(pub String);

impl SensorId {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SensorId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// An unambiguous reference to a running process.
///
/// PID alone is unsafe to act on. Windows reuses PIDs aggressively — a
/// `TerminateProcess` against a PID captured a second ago can kill an
/// unrelated process that just inherited the number. Pairing the PID with the
/// creation timestamp closes that race, and every mutating operation in the
/// provider traits requires this type rather than a bare [`Pid`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(rename_all = "camelCase")]
pub struct ProcessKey {
    pub pid: Pid,
    /// Process creation time, in 100ns intervals since the platform epoch,
    /// **rounded to the nearest value a JavaScript number can hold**.
    ///
    /// Opaque: only ever compared for equality, never interpreted here.
    ///
    /// The rounding is load-bearing. A Windows `FILETIME` is ~1.3 × 10¹⁷,
    /// above 2⁵³, so the exact integer survives JSON but not the webview's
    /// `number`. The value the UI sent back differed from the one the
    /// sampler held by a few hundred nanoseconds, the identity check
    /// rejected it, and every action on every process failed with "PID was
    /// reused" — found the day a client actually round-tripped a key.
    /// Normalising in [`Self::new`] means the sampler, the frame, the
    /// webview and the identity check all hold the same value. The
    /// granularity lost is under 2 µs, and no PID is recycled that fast.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub start_time: u64,
}

impl ProcessKey {
    /// Builds a key, normalising `start_time` so it round-trips through a
    /// JavaScript `number` unchanged. See the field docs for why.
    #[inline]
    #[must_use]
    pub fn new(pid: Pid, start_time: u64) -> Self {
        Self {
            pid,
            start_time: Self::normalise_start_time(start_time),
        }
    }

    /// The nearest `u64` a JavaScript `number` can represent exactly.
    ///
    /// Idempotent: an already-representable value maps to itself, so a key
    /// that has been through the webview compares equal to one freshly read
    /// from the kernel and normalised the same way.
    #[inline]
    #[must_use]
    // The whole point is the lossy round trip through f64.
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    pub fn normalise_start_time(start_time: u64) -> u64 {
        (start_time as f64) as u64
    }
}

impl fmt::Display for ProcessKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}@{}", self.pid, self.start_time)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_key_distinguishes_reused_pids() {
        let original = ProcessKey::new(Pid(4242), 1_000);
        let recycled = ProcessKey::new(Pid(4242), 2_000);
        assert_ne!(
            original, recycled,
            "same PID with a different start time must not compare equal"
        );
    }

    #[test]
    fn ids_serialise_transparently() {
        assert_eq!(serde_json::to_string(&Pid(7)).expect("serialise"), "7");
    }

    #[test]
    fn process_key_display_is_unambiguous() {
        assert_eq!(ProcessKey::new(Pid(12), 34).to_string(), "12@34");
    }

    #[test]
    fn a_start_time_survives_a_round_trip_through_a_javascript_number() {
        // A real FILETIME from this machine: above 2^53, so the exact integer
        // is not representable as a JS number. What the webview sends back is
        // whatever `Number(x)` produced; the key must already be that value.
        let filetime: u64 = 134_335_449_035_919_847;
        let key = ProcessKey::new(Pid(1), filetime);

        // What the webview does: parse to f64, serialise back.
        #[allow(
            clippy::cast_precision_loss,
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss
        )]
        let round_tripped = (key.start_time as f64) as u64;

        assert_eq!(key.start_time, round_tripped);
        assert_ne!(
            key.start_time, filetime,
            "the test value must actually be unrepresentable, or it proves nothing"
        );
    }

    #[test]
    fn normalisation_is_idempotent_so_a_key_from_the_webview_matches_a_fresh_one() {
        let raw: u64 = 134_335_449_035_919_847;
        let once = ProcessKey::normalise_start_time(raw);
        let twice = ProcessKey::normalise_start_time(once);
        assert_eq!(once, twice);
        assert_eq!(ProcessKey::new(Pid(1), raw), ProcessKey::new(Pid(1), once));
    }

    #[test]
    fn normalisation_keeps_distinct_processes_distinct() {
        // Two processes started within the same 2 µs would collide, which is
        // acceptable because PIDs are not recycled that fast — but processes
        // a full millisecond apart must remain distinct or the key is useless.
        let a = ProcessKey::new(Pid(1), 134_335_449_035_919_847);
        let b = ProcessKey::new(Pid(1), 134_335_449_035_919_847 + 10_000);
        assert_ne!(a, b);
    }
}
