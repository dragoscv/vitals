//! What a startup item measurably cost at boot.
//!
//! Task Manager's "Startup impact" is a rating derived from an OS boot
//! trace. This is not that: it is the CPU time and disk traffic *our own
//! sampler* observed for the item's executable during a fixed window after
//! the machine booted. Every field is `Option` because the app frequently is
//! not running when that window is open — a Vitals launched ten minutes into
//! a session has nothing to say about boot, and must say nothing rather than
//! zero.

/// The measured cost of one startup item during the boot window.
///
/// Attached to a startup entry by the desktop's inventory command. Fields are
/// individually optional so a future measurement that knows one and not the
/// other can say so; today they are set together.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(rename_all = "camelCase")]
pub struct StartupImpact {
    /// CPU milliseconds consumed inside the window. `None` when the window
    /// was not observed, which is different from an executable that ran and
    /// used none.
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub cpu_ms: Option<u64>,
    /// Bytes read plus written inside the window.
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub disk_bytes: Option<u64>,
    /// Unix milliseconds at which the window closed — shown so the user can
    /// tell whether the figure describes this boot or the previous one.
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub measured_at_ms: Option<u64>,
}

impl StartupImpact {
    /// Whether any figure is present.
    #[must_use]
    pub const fn is_measured(&self) -> bool {
        self.cpu_ms.is_some() || self.disk_bytes.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_impact_with_no_figures_is_not_measured() {
        assert!(!StartupImpact::default().is_measured());
        assert!(
            StartupImpact {
                cpu_ms: Some(0),
                ..StartupImpact::default()
            }
            .is_measured(),
            "a measured zero is a measurement; only None is absence"
        );
    }
}
