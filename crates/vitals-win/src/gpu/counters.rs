//! GPU engine utilisation from the WDDM performance counters.
//!
//! ## Why PDH and not `D3DKMTQueryStatistics`
//!
//! [`super::adapters`] uses D3DKMT because adapter enumeration there is
//! stable and documented. Engine utilisation is not: the statistics struct
//! is undocumented, its layout differs between Windows versions, and getting
//! it wrong fails *silently* — a wrong stride produces plausible numbers
//! rather than an error, which is the worst possible failure mode for a tool
//! whose entire premise is not lying about measurements.
//!
//! `\GPU Engine(*)\Utilization Percentage` is what Task Manager itself reads.
//! It is documented, its instance-name format is specified, and it carries
//! the owning PID — so per-process GPU attribution comes for free rather than
//! requiring a second correlation pass.
//!
//! ## The cost, measured rather than assumed
//!
//! The counter set has ~1000 instances on a typical machine: one per
//! (process, adapter, engine) triple. That sounds far too expensive for a
//! 1 Hz tick with a ~30 ms budget, and a PowerShell `Get-Counter` measurement
//! agreed, reporting ~6 seconds.
//!
//! That measurement was wrong. `Get-Counter` bakes in its own one-second
//! sample interval, so it cannot distinguish a slow query from its own
//! waiting. Measured properly through PDH directly
//! (`examples/gpu_engine_probe.rs`): **1.1 ms median** for collect plus
//! format across all 1000 instances. Adding the counter costs ~300 ms, but
//! that is once at startup, not per tick.
//!
//! ## What this cannot tell you
//!
//! Utilisation only. VRAM, clocks, fan and power still need a vendor SDK, and
//! remain reported as unavailable rather than guessed.

use vitals_core::ids::Pid;
use windows::Win32::System::Performance::{
    PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_DOUBLE, PDH_HCOUNTER, PDH_HQUERY, PdhAddEnglishCounterW,
    PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterArrayW, PdhOpenQueryW,
};
use windows::core::{PCWSTR, w};

use super::engines::EngineKind;

/// One parsed row of the `GPU Engine` counter set.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EngineSample {
    /// The process the work belongs to.
    pub pid: Pid,
    /// Adapter LUID, so multi-GPU machines can be separated.
    pub luid: u64,
    /// Engine kind, from the `engtype_` suffix.
    pub kind: EngineKind,
    /// Percentage of the interval this engine was busy for this process.
    pub utilisation: f64,
}

/// Maps the counter's `engtype_` suffix onto our engine model.
///
/// The suffixes are what WDDM reports, and the set differs between drivers —
/// `ofa` (optical flow accelerator) appears on recent NVIDIA parts and not at
/// all on Intel integrated graphics. An unrecognised suffix becomes
/// [`EngineKind::Other`] rather than being dropped, because a busy engine we
/// cannot name is still a busy engine, and silently discarding it would make
/// the headline figure too low.
#[must_use]
pub fn engine_kind_from_suffix(suffix: &str) -> EngineKind {
    // Some suffixes carry a trailing ordinal (`ofa_0`), so compare on the
    // leading alphabetic part.
    let base = suffix
        .split_once('_')
        .map_or(suffix, |(head, _)| head)
        .to_ascii_lowercase();

    match base.as_str() {
        "3d" => EngineKind::Graphics3D,
        "videodecode" => EngineKind::VideoDecode,
        "videoencode" => EngineKind::VideoEncode,
        "copy" => EngineKind::Copy,
        "compute" => EngineKind::Compute,
        "videoprocessing" => EngineKind::VideoProcessing,
        "legacyoverlay" | "overlay" | "scanout" => EngineKind::Display,
        _ => EngineKind::Other(0),
    }
}

/// Parses a `GPU Engine` counter instance name.
///
/// The documented shape is:
///
/// ```text
/// pid_58716_luid_0x00000000_0x00020348_phys_0_eng_0_engtype_3d
/// ```
///
/// Returns `None` rather than a partly-filled row when the name does not
/// match. Windows has changed this format before, and a parser that guesses
/// at an unfamiliar shape would attribute GPU time to the wrong process —
/// worse than reporting nothing.
#[must_use]
pub fn parse_instance(name: &str) -> Option<(Pid, u64, EngineKind)> {
    let rest = name.strip_prefix("pid_")?;
    let (pid_text, rest) = rest.split_once("_luid_")?;
    let pid: u32 = pid_text.parse().ok()?;

    // The LUID is two hex words: high part, then low part.
    let (high_text, rest) = rest.split_once('_')?;
    let (low_text, rest) = rest.split_once('_')?;

    let high = parse_hex(high_text)?;
    let low = parse_hex(low_text)?;
    let luid = (u64::from(high) << 32) | u64::from(low);

    // `engtype_` is last, and everything after it is the suffix.
    let (_, suffix) = rest.split_once("engtype_")?;

    Some((Pid(pid), luid, engine_kind_from_suffix(suffix)))
}

/// Parses a `0x`-prefixed hex word.
fn parse_hex(text: &str) -> Option<u32> {
    let digits = text
        .strip_prefix("0x")
        .or_else(|| text.strip_prefix("0X"))?;
    u32::from_str_radix(digits, 16).ok()
}

// ---------------------------------------------------------------------------
// The PDH query itself
// ---------------------------------------------------------------------------

/// Reads `\GPU Engine(*)\Utilization Percentage`.
///
/// The query and its counter handle are kept open for the sampler's lifetime.
/// Adding the counter costs ~300 ms — it has to walk the counter registry —
/// and doing that per tick would blow the budget a hundred times over, while
/// reusing the handle costs ~1.1 ms per tick.
pub struct EngineCounters {
    query: PDH_HQUERY,
    counter: PDH_HCOUNTER,
    /// Whether a baseline collection has happened.
    ///
    /// The counter is a rate, so the first collection reports nothing usable.
    /// Tracking this lets `sample` return an empty vector on the first tick
    /// rather than a list of zeroes, which would read as "measured, idle".
    primed: bool,
    /// Reused between ticks so a 1000-instance read is not a fresh
    /// allocation every second.
    ///
    /// Typed as the item rather than as bytes: PDH writes an array of
    /// `PDH_FMT_COUNTERVALUE_ITEM_W`, which needs 8-byte alignment, and a
    /// `Vec<u8>` only guarantees 1. Casting one to the other is undefined
    /// behaviour that happens to work until it does not.
    buffer: Vec<PDH_FMT_COUNTERVALUE_ITEM_W>,
}

impl EngineCounters {
    /// Opens the query, or returns `None` when the counter set is absent.
    ///
    /// Absent is a normal condition, not an error: a machine with no WDDM
    /// driver — a server, a container, a VM with basic display — has no GPU
    /// Engine counters at all. The caller reports GPU utilisation as
    /// unavailable, which is true.
    #[must_use]
    pub fn open() -> Option<Self> {
        let mut query = PDH_HQUERY::default();

        // SAFETY: writes a handle on success and nothing on failure. Closed
        // in `Drop`.
        if unsafe { PdhOpenQueryW(PCWSTR::null(), 0, &raw mut query) } != 0 {
            return None;
        }

        let mut counter = PDH_HCOUNTER::default();

        // English rather than localised counter names. On a Romanian Windows
        // the localised path does not resolve, and this app ships in both —
        // so using the localised API would mean GPU utilisation silently
        // working in one language and not the other.
        // SAFETY: `query` is a live handle from the call above.
        let status = unsafe {
            PdhAddEnglishCounterW(
                query,
                w!("\\GPU Engine(*)\\Utilization Percentage"),
                0,
                &raw mut counter,
            )
        };

        if status != 0 {
            // SAFETY: `query` is live and is not used again.
            unsafe { PdhCloseQuery(query) };
            return None;
        }

        Some(Self {
            query,
            counter,
            primed: false,
            buffer: Vec::new(),
        })
    }

    /// Collects one tick.
    ///
    /// Returns an empty vector on the first call — a rate needs two points in
    /// time — and on any tick the query fails. Rows that do not parse are
    /// skipped rather than guessed at.
    pub fn sample(&mut self) -> Vec<EngineSample> {
        // SAFETY: `self.query` is live for as long as `self` is.
        if unsafe { PdhCollectQueryData(self.query) } != 0 {
            return Vec::new();
        }

        if !self.primed {
            self.primed = true;
            return Vec::new();
        }

        self.read()
    }

    /// Reads the formatted array into `self.buffer` and parses it.
    fn read(&mut self) -> Vec<EngineSample> {
        let mut size = 0_u32;
        let mut count = 0_u32;

        // Two-call idiom: the first reports the buffer size required.
        // SAFETY: passing a null buffer is how PDH is asked for the size.
        unsafe {
            PdhGetFormattedCounterArrayW(
                self.counter,
                PDH_FMT_DOUBLE,
                &raw mut size,
                &raw mut count,
                None,
            );
        }

        if size == 0 {
            return Vec::new();
        }

        // Round up: the byte count PDH reports includes the trailing string
        // data it packs after the array, so this over-allocates slightly
        // rather than risking a short buffer.
        let items_needed = (size as usize).div_ceil(size_of::<PDH_FMT_COUNTERVALUE_ITEM_W>()) + 1;

        self.buffer.clear();
        self.buffer
            .resize(items_needed, PDH_FMT_COUNTERVALUE_ITEM_W::default());

        // SAFETY: the buffer is at least `size` bytes and correctly aligned
        // for the item type, because that is the type it holds.
        let status = unsafe {
            PdhGetFormattedCounterArrayW(
                self.counter,
                PDH_FMT_DOUBLE,
                &raw mut size,
                &raw mut count,
                Some(self.buffer.as_mut_ptr()),
            )
        };

        if status != 0 {
            return Vec::new();
        }

        // SAFETY: PDH wrote `count` items, and the buffer holds at least
        // that many — it was sized from PDH's own byte count.
        let items = unsafe {
            std::slice::from_raw_parts(
                self.buffer.as_ptr(),
                (count as usize).min(self.buffer.len()),
            )
        };

        let mut out = Vec::new();

        for item in items {
            // SAFETY: the union holds a double because PDH_FMT_DOUBLE was
            // requested.
            let value = unsafe { item.FmtValue.Anonymous.doubleValue };

            // Roughly 99% of the thousand instances are idle processes.
            // Skipping them before the string parse is what keeps this at
            // about a millisecond.
            if value <= 0.0 || !value.is_finite() {
                continue;
            }

            // SAFETY: PDH null-terminates the instance name.
            let name = unsafe { item.szName.to_string() };
            let Ok(name) = name else { continue };

            if let Some((pid, luid, kind)) = parse_instance(&name) {
                out.push(EngineSample {
                    pid,
                    luid,
                    kind,
                    utilisation: value,
                });
            }
        }

        out
    }
}

impl Drop for EngineCounters {
    fn drop(&mut self) {
        // SAFETY: `self.query` is live and closed exactly once.
        unsafe { PdhCloseQuery(self.query) };
    }
}

// Hand-written because `PDH_FMT_COUNTERVALUE_ITEM_W` contains a union and so
// cannot derive `Debug`. The buffer's contents are not interesting anyway —
// its length is.
impl std::fmt::Debug for EngineCounters {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EngineCounters")
            .field("query", &self.query.0)
            .field("counter", &self.counter.0)
            .field("primed", &self.primed)
            .field("buffer_capacity", &self.buffer.len())
            .finish()
    }
}

/// Sums utilisation per engine kind across every process.
///
/// Summing rather than taking a maximum: two processes each using 40% of the
/// 3D engine leave it 80% busy, and reporting 40% would understate it. The
/// total is clamped by the caller, since timer skew can push a sum slightly
/// over 100%.
#[must_use]
pub fn total_by_kind(samples: &[EngineSample]) -> Vec<(EngineKind, f64)> {
    let mut totals: Vec<(EngineKind, f64)> = Vec::new();

    for sample in samples {
        if let Some(entry) = totals.iter_mut().find(|(kind, _)| *kind == sample.kind) {
            entry.1 += sample.utilisation;
        } else {
            totals.push((sample.kind, sample.utilisation));
        }
    }

    totals.sort_by(|a, b| b.1.total_cmp(&a.1));
    totals
}

/// Utilisation per process, summed across every engine and adapter.
///
/// This is the figure a process list wants: "how much GPU is Chrome using",
/// not "how much of the copy engine on adapter 2".
#[must_use]
pub fn total_by_process(samples: &[EngineSample]) -> Vec<(Pid, f64)> {
    let mut totals: Vec<(Pid, f64)> = Vec::new();

    for sample in samples {
        // Copy and display engines are busy on any machine drawing a window,
        // so including them would show every process with a visible window as
        // a GPU consumer. Same reasoning as `is_primary_workload`.
        if !sample.kind.is_primary_workload() {
            continue;
        }

        if let Some(entry) = totals.iter_mut().find(|(pid, _)| *pid == sample.pid) {
            entry.1 += sample.utilisation;
        } else {
            totals.push((sample.pid, sample.utilisation));
        }
    }

    totals.sort_by(|a, b| b.1.total_cmp(&a.1));
    totals
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real instance name, copied verbatim from this machine.
    const REAL: &str = "pid_58716_luid_0x00000000_0x00020348_phys_0_eng_0_engtype_3d";

    #[test]
    fn parses_a_real_instance_name() {
        let (pid, luid, kind) = parse_instance(REAL).expect("the documented format must parse");

        assert_eq!(pid, Pid(58716));
        assert_eq!(luid, 0x0002_0348);
        assert_eq!(kind, EngineKind::Graphics3D);
    }

    #[test]
    fn parses_every_engine_type_this_machine_reports() {
        // Taken from the live counter set, so this is the real vocabulary
        // rather than what the documentation claims.
        let cases = [
            ("3d", EngineKind::Graphics3D),
            ("copy", EngineKind::Copy),
            ("videodecode", EngineKind::VideoDecode),
            ("videoencode", EngineKind::VideoEncode),
            ("videoprocessing", EngineKind::VideoProcessing),
            ("legacyoverlay", EngineKind::Display),
            ("compute", EngineKind::Compute),
        ];

        for (suffix, expected) in cases {
            assert_eq!(engine_kind_from_suffix(suffix), expected, "suffix {suffix}");
        }
    }

    #[test]
    fn an_engine_with_a_trailing_ordinal_still_classifies() {
        // `ofa_0` appears on recent NVIDIA drivers. It is not one we model,
        // but it must not break the parse of the rest of the name.
        let name = "pid_4_luid_0x00000000_0x00020348_phys_0_eng_9_engtype_ofa_0";
        let (pid, _, kind) = parse_instance(name).expect("must still parse");

        assert_eq!(pid, Pid(4));
        assert_eq!(kind, EngineKind::Other(0));
    }

    #[test]
    fn an_unrecognised_suffix_is_other_not_dropped() {
        // A busy engine we cannot name is still busy. Dropping it would make
        // the headline figure quietly too low.
        assert_eq!(
            engine_kind_from_suffix("somethingnewin2027"),
            EngineKind::Other(0)
        );
    }

    #[test]
    fn a_malformed_name_is_refused_rather_than_guessed() {
        // Windows has changed this format before. Attributing GPU time to the
        // wrong process is worse than reporting none.
        for bad in [
            "",
            "pid_notanumber_luid_0x0_0x1_engtype_3d",
            "luid_0x00000000_0x00020348_engtype_3d", // no pid
            "pid_1_luid_0x00000000_0x00020348_phys_0_eng_0", // no engtype
            "pid_1_luid_zzzz_0x00020348_phys_0_eng_0_engtype_3d", // bad hex
        ] {
            assert!(parse_instance(bad).is_none(), "should refuse: {bad}");
        }
    }

    #[test]
    fn the_high_word_of_a_luid_is_not_discarded() {
        // Multi-GPU machines can have adapters differing only in the high
        // word; folding them together would merge two GPUs into one.
        let a = "pid_1_luid_0x00000001_0x00020348_phys_0_eng_0_engtype_3d";
        let b = "pid_1_luid_0x00000002_0x00020348_phys_0_eng_0_engtype_3d";

        let (_, luid_a, _) = parse_instance(a).expect("parse");
        let (_, luid_b, _) = parse_instance(b).expect("parse");

        assert_ne!(luid_a, luid_b);
    }

    fn sample(pid: u32, kind: EngineKind, utilisation: f64) -> EngineSample {
        EngineSample {
            pid: Pid(pid),
            luid: 0x0002_0348,
            kind,
            utilisation,
        }
    }

    #[test]
    fn utilisation_of_one_engine_sums_across_processes() {
        // Two processes at 40% each leave the engine 80% busy. Taking a
        // maximum would report 40% and understate it.
        let samples = [
            sample(1, EngineKind::Graphics3D, 40.0),
            sample(2, EngineKind::Graphics3D, 40.0),
        ];

        let totals = total_by_kind(&samples);
        assert_eq!(totals.len(), 1);
        assert!((totals[0].1 - 80.0).abs() < 1e-9);
    }

    #[test]
    fn engines_are_reported_separately_not_averaged() {
        // The whole reason per-engine exists: a machine transcoding video is
        // 100% busy on encode and idle on 3D, and one blended number hides
        // which.
        let samples = [
            sample(1, EngineKind::VideoEncode, 100.0),
            sample(1, EngineKind::Graphics3D, 0.0),
        ];

        let totals = total_by_kind(&samples);
        assert_eq!(totals.len(), 2);
        assert_eq!(totals[0].0, EngineKind::VideoEncode);
    }

    #[test]
    fn per_process_totals_ignore_copy_and_display() {
        // Every process with a window touches the copy engine. Counting it
        // would list the whole desktop as GPU consumers.
        let samples = [
            sample(1, EngineKind::Graphics3D, 30.0),
            sample(1, EngineKind::Copy, 50.0),
            sample(2, EngineKind::Display, 90.0),
        ];

        let totals = total_by_process(&samples);

        assert_eq!(totals.len(), 1, "only the 3D user should appear");
        assert_eq!(totals[0].0, Pid(1));
        assert!((totals[0].1 - 30.0).abs() < 1e-9);
    }

    #[test]
    fn per_process_totals_sum_across_engines_and_adapters() {
        let samples = [
            sample(7, EngineKind::Graphics3D, 20.0),
            sample(7, EngineKind::Compute, 15.0),
            EngineSample {
                luid: 0x9999,
                ..sample(7, EngineKind::VideoDecode, 5.0)
            },
        ];

        let totals = total_by_process(&samples);
        assert_eq!(totals.len(), 1);
        assert!((totals[0].1 - 40.0).abs() < 1e-9);
    }

    #[test]
    fn empty_input_produces_empty_output_not_a_zero_row() {
        // A zero row reads as "measured, and it is idle". No rows reads as
        // "nothing to report", which is the truth when there is no GPU.
        assert!(total_by_kind(&[]).is_empty());
        assert!(total_by_process(&[]).is_empty());
    }
}
