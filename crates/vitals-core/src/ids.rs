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
#[serde(rename_all = "kebab-case")]
pub struct ProcessKey {
    pub pid: Pid,
    /// Process creation time, in 100ns intervals since the platform epoch.
    ///
    /// Opaque: only ever compared for equality, never interpreted here.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub start_time: u64,
}

impl ProcessKey {
    #[inline]
    #[must_use]
    pub const fn new(pid: Pid, start_time: u64) -> Self {
        Self { pid, start_time }
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
}
