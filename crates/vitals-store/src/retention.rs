//! Retention and downsampling policy.

use serde::{Deserialize, Serialize};

/// A storage tier's time resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Resolution {
    Second,
    Minute,
    FiveMinutes,
    Hour,
    Day,
}

impl Resolution {
    #[must_use]
    pub const fn seconds(self) -> u64 {
        match self {
            Self::Second => 1,
            Self::Minute => 60,
            Self::FiveMinutes => 300,
            Self::Hour => 3_600,
            Self::Day => 86_400,
        }
    }
}

/// How long data is kept at each resolution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetentionPolicy {
    /// Whether history is recorded at all. Defaults to `false`.
    pub enabled: bool,
    /// `(resolution, retain_for_seconds)`, coarsest last.
    pub tiers: Vec<(Resolution, u64)>,
    /// Hard cap on the database file. Oldest data is evicted first.
    ///
    /// A byte budget rather than only a time budget, because a busy machine
    /// generates far more rows than an idle one and a purely time-based
    /// policy makes the on-disk size unpredictable.
    pub max_bytes: u64,
}

impl Default for RetentionPolicy {
    fn default() -> Self {
        Self {
            enabled: false,
            tiers: vec![
                (Resolution::Second, 3_600),
                (Resolution::Minute, 86_400),
                (Resolution::FiveMinutes, 30 * 86_400),
                (Resolution::Hour, 365 * 86_400),
            ],
            max_bytes: 512 * 1024 * 1024,
        }
    }
}

impl RetentionPolicy {
    /// The resolution that should serve a query reaching `age_secs` back.
    ///
    /// Picks the finest tier that still covers the requested age, so a chart
    /// never silently renders at a coarser resolution than it could.
    #[must_use]
    pub fn resolution_for_age(&self, age_secs: u64) -> Resolution {
        self.tiers
            .iter()
            .find(|(_, retain)| age_secs <= *retain)
            .map_or(Resolution::Day, |(res, _)| *res)
    }

    /// Rough number of rows retained per series, for a size estimate shown in
    /// settings before the user enables history.
    #[must_use]
    pub fn estimated_rows_per_series(&self) -> u64 {
        self.tiers
            .iter()
            .map(|(res, retain)| retain / res.seconds())
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_is_disabled_by_default() {
        assert!(
            !RetentionPolicy::default().enabled,
            "recording must be opt-in"
        );
    }

    #[test]
    fn recent_queries_use_the_finest_resolution() {
        let p = RetentionPolicy::default();
        assert_eq!(p.resolution_for_age(60), Resolution::Second);
        assert_eq!(p.resolution_for_age(3_600), Resolution::Second);
    }

    #[test]
    fn older_queries_step_down_through_tiers() {
        let p = RetentionPolicy::default();
        assert_eq!(p.resolution_for_age(3_601), Resolution::Minute);
        assert_eq!(p.resolution_for_age(200_000), Resolution::FiveMinutes);
        assert_eq!(p.resolution_for_age(40 * 86_400), Resolution::Hour);
    }

    #[test]
    fn queries_beyond_all_tiers_fall_back_to_daily() {
        let p = RetentionPolicy::default();
        assert_eq!(p.resolution_for_age(10 * 365 * 86_400), Resolution::Day);
    }

    #[test]
    fn row_estimate_is_bounded() {
        // Sanity: the default policy must not imply an absurd row count per
        // series, or the settings screen would be quoting a misleading size.
        let rows = RetentionPolicy::default().estimated_rows_per_series();
        assert!(rows > 0 && rows < 100_000, "unexpected estimate: {rows}");
    }
}
