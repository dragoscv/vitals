//! Newtype units.
//!
//! Every numeric metric in Vitals is wrapped. This is not ceremony: the bug
//! class it eliminates — passing MB where bytes were expected, or a 0..1
//! fraction where 0..100 was expected — is the single most common source of
//! wrong numbers in system monitors, and wrong numbers are the one thing this
//! product cannot ship.
//!
//! Each type is declared explicitly rather than generated wholesale by a
//! macro. `specta`'s derive cannot resolve a type supplied through a macro
//! metavariable, so the struct and its derives must be written out; only the
//! shared trait impls are generated.

use std::fmt;

/// Generates the boilerplate impls shared by every scalar unit.
///
/// Only the impls — the type itself is declared at the call site so the
/// `specta` and `serde` derives see a concrete field type.
macro_rules! unit_impls {
    ($name:ident, $inner:ty, $suffix:literal) => {
        impl $name {
            pub const ZERO: Self = Self(0 as $inner);

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
                write!(f, "{}{}", self.0, $suffix)
            }
        }
    };
}

/// A count of bytes. Always bytes — never KB, MB or "units of 1024".
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
pub struct Bytes(#[cfg_attr(feature = "ts", ts(type = "number"))] pub u64);
unit_impls!(Bytes, u64, " B");

/// Throughput in bytes per second.
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
pub struct BytesPerSec(#[cfg_attr(feature = "ts", ts(type = "number"))] pub u64);
unit_impls!(BytesPerSec, u64, " B/s");

/// Frequency in hertz. Store hertz, format as MHz/GHz at the edge.
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
pub struct Hertz(#[cfg_attr(feature = "ts", ts(type = "number"))] pub u64);
unit_impls!(Hertz, u64, " Hz");

/// Temperature in degrees Celsius.
#[derive(
    Debug, Clone, Copy, PartialEq, PartialOrd, Default, serde::Serialize, serde::Deserialize,
)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(transparent)]
pub struct Celsius(pub f32);
unit_impls!(Celsius, f32, " °C");

/// Power draw in watts.
#[derive(
    Debug, Clone, Copy, PartialEq, PartialOrd, Default, serde::Serialize, serde::Deserialize,
)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(transparent)]
pub struct Watts(pub f32);
unit_impls!(Watts, f32, " W");

/// Electrical potential in volts.
#[derive(
    Debug, Clone, Copy, PartialEq, PartialOrd, Default, serde::Serialize, serde::Deserialize,
)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(transparent)]
pub struct Volts(pub f32);
unit_impls!(Volts, f32, " V");

/// Rotational speed in revolutions per minute.
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
pub struct Rpm(pub u32);
unit_impls!(Rpm, u32, " RPM");

/// A percentage on the range `0.0..=100.0`.
///
/// Deliberately *not* a 0..1 fraction. Every OS API in this space reports
/// percentages, and converting at the boundary twice is how you end up with a
/// CPU meter reading 6400%.
#[derive(
    Debug, Clone, Copy, PartialEq, PartialOrd, Default, serde::Serialize, serde::Deserialize,
)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(transparent)]
pub struct Percent(pub f32);

impl Percent {
    pub const ZERO: Self = Self(0.0);
    pub const FULL: Self = Self(100.0);

    /// Construct from a 0..=100 value, clamping out-of-range input.
    ///
    /// Clamping rather than asserting is deliberate: kernel counters do
    /// occasionally produce a value slightly over 100 due to timer skew
    /// between two samples, and crashing a task manager over a rounding
    /// artefact would be absurd.
    #[inline]
    #[must_use]
    pub fn new(v: f32) -> Self {
        Self(v.clamp(0.0, 100.0))
    }

    /// Construct from a 0..=1 fraction.
    #[inline]
    #[must_use]
    pub fn from_fraction(v: f32) -> Self {
        Self::new(v * 100.0)
    }

    /// Ratio of `part` to `whole`, yielding [`Percent::ZERO`] when `whole` is 0.
    #[inline]
    #[must_use]
    pub fn ratio(part: u64, whole: u64) -> Self {
        if whole == 0 {
            Self::ZERO
        } else {
            Self::new((part as f64 / whole as f64 * 100.0) as f32)
        }
    }

    #[inline]
    #[must_use]
    pub const fn get(self) -> f32 {
        self.0
    }
}

impl fmt::Display for Percent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.1}%", self.0)
    }
}

impl Bytes {
    pub const KIB: u64 = 1024;
    pub const MIB: u64 = 1024 * 1024;
    pub const GIB: u64 = 1024 * 1024 * 1024;

    #[inline]
    #[must_use]
    pub const fn from_kib(v: u64) -> Self {
        Self(v.saturating_mul(Self::KIB))
    }

    #[inline]
    #[must_use]
    pub const fn from_mib(v: u64) -> Self {
        Self(v.saturating_mul(Self::MIB))
    }

    #[inline]
    #[must_use]
    pub fn as_gib(self) -> f64 {
        self.0 as f64 / Self::GIB as f64
    }
}

impl Hertz {
    #[inline]
    #[must_use]
    pub const fn from_mhz(v: u64) -> Self {
        Self(v.saturating_mul(1_000_000))
    }

    #[inline]
    #[must_use]
    pub fn as_ghz(self) -> f64 {
        self.0 as f64 / 1_000_000_000.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_clamps_out_of_range() {
        assert_eq!(Percent::new(140.0), Percent::FULL);
        assert_eq!(Percent::new(-3.0), Percent::ZERO);
    }

    #[test]
    fn percent_ratio_handles_zero_denominator() {
        assert_eq!(Percent::ratio(5, 0), Percent::ZERO);
        assert_eq!(Percent::ratio(1, 4), Percent(25.0));
    }

    #[test]
    fn percent_from_fraction_scales_correctly() {
        assert_eq!(Percent::from_fraction(0.25), Percent(25.0));
    }

    #[test]
    fn byte_conversions_are_binary_not_decimal() {
        assert_eq!(Bytes::from_mib(1).get(), 1_048_576);
        assert!((Bytes(Bytes::GIB).as_gib() - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn byte_conversion_saturates_instead_of_overflowing() {
        assert_eq!(Bytes::from_mib(u64::MAX).get(), u64::MAX);
    }

    #[test]
    fn hertz_reports_ghz() {
        assert!((Hertz::from_mhz(3600).as_ghz() - 3.6).abs() < 1e-9);
    }

    #[test]
    fn units_serialise_transparently() {
        // The frontend must see a bare number, not `{"0": 42}`.
        let json = serde_json::to_string(&Bytes(42)).expect("serialise");
        assert_eq!(json, "42");
    }
}
