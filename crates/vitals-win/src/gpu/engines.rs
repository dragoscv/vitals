//! GPU engine classification and utilisation arithmetic.
//!
//! Pure, so the delta maths is testable without a GPU.

use vitals_core::units::Percent;

/// A GPU engine, as WDDM reports it.
///
/// The kernel exposes engines by a numeric node ordinal whose meaning is
/// driver-specific, so the mapping is a heuristic refined by the driver's own
/// node metadata where available.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum EngineKind {
    /// Rasterisation and general graphics. What a game uses.
    Graphics3D,
    /// Fixed-function video decode.
    VideoDecode,
    /// Fixed-function video encode.
    VideoEncode,
    /// DMA between system and video memory.
    Copy,
    /// Compute shaders — `CUDA`, `DirectML`, `OpenCL`.
    Compute,
    /// Video processing: scaling, colour conversion, deinterlacing.
    VideoProcessing,
    /// Present and display scanout.
    Display,
    /// Reported by the driver but not recognised.
    Other(u32),
}

impl EngineKind {
    /// Whether this engine matters to a user asking "is my GPU busy?".
    ///
    /// Copy and display engines run constantly on any machine that draws a
    /// window. Including them in a headline figure means the GPU never reads
    /// as idle, which is why per-engine reporting exists.
    #[must_use]
    pub const fn is_primary_workload(self) -> bool {
        matches!(
            self,
            Self::Graphics3D | Self::Compute | Self::VideoDecode | Self::VideoEncode
        )
    }

    /// A stable identifier for the UI and for settings persistence.
    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Graphics3D => "3d",
            Self::VideoDecode => "decode",
            Self::VideoEncode => "encode",
            Self::Copy => "copy",
            Self::Compute => "compute",
            Self::VideoProcessing => "video-processing",
            Self::Display => "display",
            Self::Other(_) => "other",
        }
    }
}

/// Cumulative running time for one engine, in 100ns units.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EngineTime {
    pub node: u32,
    pub running_time: u64,
}

/// Utilisation of one engine over an interval.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EngineUsage {
    pub kind: EngineKind,
    pub node: u32,
    pub utilisation: Percent,
}

/// Computes engine utilisation between two snapshots.
///
/// Returns [`Percent::ZERO`] for an unusable interval — zero-length, or a
/// counter that ran backwards after a driver reset (a TDR, which is common
/// enough that treating it as a delta would show a 100% spike every time a
/// driver recovers).
#[must_use]
pub fn compute_utilisation(previous: u64, current: u64, elapsed_ticks: u64) -> Percent {
    if elapsed_ticks == 0 || current < previous {
        return Percent::ZERO;
    }

    let busy = current - previous;

    // An engine cannot be busy for longer than the interval, but timer skew
    // between the GPU's clock and ours can make it appear so. Clamping is
    // more honest than reporting 103%.
    Percent::ratio(busy.min(elapsed_ticks), elapsed_ticks)
}

/// Best-effort mapping from a driver node ordinal to an engine kind.
///
/// WDDM does not expose a portable engine-type enumeration, so this uses the
/// conventional ordering that NVIDIA, AMD and Intel drivers all follow for
/// their first nodes. Nodes beyond the known set are reported as
/// [`EngineKind::Other`] with their ordinal, so the UI can still show them
/// rather than silently dropping real activity.
#[must_use]
pub const fn classify_node(node: u32) -> EngineKind {
    match node {
        0 => EngineKind::Graphics3D,
        1 => EngineKind::VideoDecode,
        2 => EngineKind::VideoProcessing,
        3 => EngineKind::Copy,
        4 => EngineKind::VideoEncode,
        5 => EngineKind::Compute,
        6 => EngineKind::Display,
        other => EngineKind::Other(other),
    }
}

/// The headline figure for a GPU.
///
/// The **maximum** across primary engines, not the mean. A GPU with one
/// engine saturated is saturated for that workload, and averaging across
/// eight mostly-idle engines would report 12% for a machine that cannot
/// render another frame.
///
/// Copy and display engines are excluded: they tick over on any machine
/// drawing a window, and including them means the GPU never reads as idle.
#[must_use]
pub fn headline_utilisation(engines: &[EngineUsage]) -> Percent {
    let peak = engines
        .iter()
        .filter(|e| e.kind.is_primary_workload())
        .map(|e| e.utilisation.get())
        .fold(0.0_f32, f32::max);

    Percent::new(peak)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECOND: u64 = 10_000_000;

    fn usage(kind: EngineKind, percent: f32) -> EngineUsage {
        EngineUsage {
            kind,
            node: 0,
            utilisation: Percent::new(percent),
        }
    }

    #[test]
    fn a_fully_busy_engine_reads_one_hundred_percent() {
        assert_eq!(compute_utilisation(0, SECOND, SECOND), Percent::FULL);
    }

    #[test]
    fn an_idle_engine_reads_zero() {
        assert_eq!(compute_utilisation(SECOND, SECOND, SECOND), Percent::ZERO);
    }

    #[test]
    fn half_busy_reads_fifty_percent() {
        let usage = compute_utilisation(0, SECOND / 2, SECOND);
        assert!((usage.get() - 50.0).abs() < 0.01, "got {usage}");
    }

    #[test]
    fn a_driver_reset_reports_zero_rather_than_a_spike() {
        // A TDR resets the counter. Treating the wrap as a delta shows a 100%
        // spike every time a driver recovers, which is common enough that
        // users would see it regularly.
        assert_eq!(
            compute_utilisation(SECOND * 100, SECOND, SECOND),
            Percent::ZERO
        );
    }

    #[test]
    fn a_zero_interval_does_not_divide_by_zero() {
        assert_eq!(compute_utilisation(0, SECOND, 0), Percent::ZERO);
    }

    #[test]
    fn timer_skew_is_clamped_rather_than_exceeding_one_hundred() {
        // The GPU's clock and ours are not the same clock.
        let usage = compute_utilisation(0, SECOND * 2, SECOND);
        assert_eq!(usage, Percent::FULL, "should clamp, got {usage}");
    }

    #[test]
    fn the_headline_is_the_maximum_not_the_mean() {
        // THE case Task Manager gets wrong in the other direction. One
        // saturated engine means the GPU is saturated for that workload;
        // averaging would report 25% for a machine at full tilt.
        let engines = [
            usage(EngineKind::Graphics3D, 100.0),
            usage(EngineKind::Compute, 0.0),
            usage(EngineKind::VideoDecode, 0.0),
            usage(EngineKind::VideoEncode, 0.0),
        ];

        assert_eq!(headline_utilisation(&engines), Percent::FULL);
    }

    #[test]
    fn housekeeping_engines_do_not_inflate_the_headline() {
        // Copy and display tick over on any machine drawing a window. If they
        // counted, the GPU would never read as idle.
        let engines = [
            usage(EngineKind::Graphics3D, 0.0),
            usage(EngineKind::Copy, 90.0),
            usage(EngineKind::Display, 80.0),
        ];

        assert_eq!(headline_utilisation(&engines), Percent::ZERO);
    }

    #[test]
    fn a_transcode_shows_on_the_encode_engine() {
        let engines = [
            usage(EngineKind::Graphics3D, 2.0),
            usage(EngineKind::VideoEncode, 95.0),
        ];

        let headline = headline_utilisation(&engines);
        assert!((headline.get() - 95.0).abs() < 0.01, "got {headline}");
    }

    #[test]
    fn an_empty_engine_list_reads_zero_rather_than_panicking() {
        // A GPU present but not yet reporting nodes, common right after
        // resume from sleep.
        assert_eq!(headline_utilisation(&[]), Percent::ZERO);
    }

    #[test]
    fn known_nodes_map_to_named_engines() {
        assert_eq!(classify_node(0), EngineKind::Graphics3D);
        assert_eq!(classify_node(1), EngineKind::VideoDecode);
        assert_eq!(classify_node(4), EngineKind::VideoEncode);
        assert_eq!(classify_node(5), EngineKind::Compute);
    }

    #[test]
    fn unknown_nodes_are_preserved_rather_than_dropped() {
        // A driver with more engines than we know about must still have its
        // activity shown, or real GPU load becomes invisible.
        assert_eq!(classify_node(42), EngineKind::Other(42));
    }

    #[test]
    fn every_engine_kind_has_a_stable_slug() {
        let kinds = [
            EngineKind::Graphics3D,
            EngineKind::VideoDecode,
            EngineKind::VideoEncode,
            EngineKind::Copy,
            EngineKind::Compute,
            EngineKind::VideoProcessing,
            EngineKind::Display,
        ];

        let mut slugs: Vec<_> = kinds.iter().map(|k| k.slug()).collect();
        let count = slugs.len();
        slugs.sort_unstable();
        slugs.dedup();

        assert_eq!(slugs.len(), count, "two engine kinds share a slug");
    }
}
