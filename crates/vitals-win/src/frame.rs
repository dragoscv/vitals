//! Converts platform samples into the wire [`Frame`] the UI consumes.
//!
//! Two jobs, both about bandwidth rather than correctness:
//!
//! 1. Map platform types onto the shared protocol, so the frontend never sees
//!    a Windows-shaped struct.
//! 2. Encode deltas. A keyframe with 550 processes is ~250 KB of JSON; almost
//!    none of it changes between ticks. Sending only what moved cuts that by
//!    well over an order of magnitude, and at 1 Hz forever the difference is
//!    the gap between a background app and a noticeable one.

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use vitals_core::ids::{Pid, ProcessKey};
use vitals_core::metrics::SystemMetrics;
use vitals_core::process::{Process, ProcessFlags, ProcessKind, ProcessState, ProtectionLevel};
use vitals_core::sample::{Frame, FramePayload, FrameSeq};
use vitals_core::units::Bytes;

use crate::sampler::{Sample, SampledProcess};

/// How often a keyframe is sent regardless of what changed.
///
/// Deltas are only valid relative to a shared baseline. If one is ever
/// dropped — a webview reload, a missed event — the client's picture drifts
/// silently. A periodic keyframe bounds that damage to a few seconds instead
/// of leaving the UI permanently wrong.
const KEYFRAME_INTERVAL: u64 = 30;

/// Builds wire frames from platform samples.
#[derive(Debug, Default)]
pub struct FrameBuilder {
    seq: u64,
    /// Last state sent per process, for change detection.
    previous: HashMap<ProcessKey, Process>,
}

impl FrameBuilder {
    #[must_use]
    pub fn new() -> Self {
        Self {
            seq: 0,
            previous: HashMap::with_capacity(512),
        }
    }

    /// Forces the next frame to be a keyframe.
    ///
    /// Called when a client connects or reloads: a delta against a baseline
    /// the client does not have is worse than useless.
    pub fn request_keyframe(&mut self) {
        self.previous.clear();
    }

    /// Converts a sample into a frame.
    pub fn build(&mut self, sample: Sample) -> Frame {
        self.seq = self.seq.saturating_add(1);

        // The System Idle Process is dropped here, once, for every consumer.
        // PID 0 accumulates the CPU time of every idle cycle on every core,
        // so it ranks first in any list sorted by CPU and makes the machine
        // look permanently pegged. `is_idle_process` existed, but nothing on
        // the production path called it — the phone app was the first client
        // to show the list unfiltered, with "PID 0 · 43 %" at the top.
        let processes: Vec<Process> = sample
            .processes
            .into_iter()
            .filter(|p| !p.raw.is_idle_process())
            .map(convert)
            .collect();

        // Keyframe when the client has no baseline, or on the periodic
        // refresh. `seq == 1` covers the first frame after a reset.
        let force_keyframe = self.previous.is_empty() || self.seq.is_multiple_of(KEYFRAME_INTERVAL);

        let payload = if force_keyframe {
            self.previous = processes.iter().map(|p| (p.key, p.clone())).collect();
            FramePayload::Keyframe {
                system: sample.system,
                processes,
            }
        } else {
            self.build_delta(sample.system, processes)
        };

        Frame {
            seq: FrameSeq(self.seq),
            timestamp_ms: now_ms(),
            elapsed_ms: sample.elapsed_ms,
            payload,
        }
    }

    /// Builds a delta against the previous frame.
    fn build_delta(&mut self, system: SystemMetrics, processes: Vec<Process>) -> FramePayload {
        let mut changed = Vec::new();
        let mut next = HashMap::with_capacity(processes.len());

        for process in processes {
            match self.previous.get(&process.key) {
                Some(previous) if !is_meaningfully_different(previous, &process) => {}
                _ => changed.push(process.clone()),
            }
            next.insert(process.key, process);
        }

        // Anything in the old map and not the new one has exited.
        let exited: Vec<Pid> = self
            .previous
            .keys()
            .filter(|key| !next.contains_key(key))
            .map(|key| key.pid)
            .collect();

        self.previous = next;

        FramePayload::Delta {
            system,
            changed,
            exited,
        }
    }
}

/// Whether a change is worth sending.
///
/// Raw inequality would mark almost every process as changed every tick:
/// CPU jitters in the third decimal and working set moves by a page. That
/// defeats the delta entirely — measured, it sent ~90% of rows as "changed"
/// while the UI rendered visually identical output.
///
/// The thresholds are set at the point the UI can actually display a
/// difference: 0.05% CPU and 64 KiB of memory are both below one pixel of
/// movement in any chart or one digit in any table.
///
/// ## Why they are not tighter
///
/// Measured on a 587-process machine, one second apart, the ~70 rows that
/// still report changed break down as: CPU 27, memory 29, disk 7, other 7.
/// No single field dominates, so there is no cheap win left — that is real
/// activity on a busy machine, not jitter.
///
/// Raising the CPU threshold to 0.25% would cut its 27 to 5, but 0.25% of a
/// 32-core machine is eight full percent of one core. A process burning that
/// much would stop updating in the table, which is precisely the complaint
/// people have about Task Manager's own coarse rounding.
fn is_meaningfully_different(previous: &Process, current: &Process) -> bool {
    /// Smallest CPU change the UI can render.
    const CPU_EPSILON: f32 = 0.05;
    /// Smallest memory change worth a row update: 16 pages.
    const MEMORY_EPSILON: u64 = 64 * 1024;
    /// Smallest IO rate change worth a row update.
    ///
    /// Disk rates are recomputed every tick and essentially never repeat
    /// exactly, so comparing them for equality marks every process that
    /// touches a file as changed. 4 KiB/s is one page per second — below
    /// what any sane unit formatter will render differently.
    const IO_EPSILON: u64 = 4 * 1024;

    if (previous.cpu.get() - current.cpu.get()).abs() >= CPU_EPSILON {
        return true;
    }

    if previous
        .memory_private
        .get()
        .abs_diff(current.memory_private.get())
        >= MEMORY_EPSILON
    {
        return true;
    }

    if previous.disk_read.get().abs_diff(current.disk_read.get()) >= IO_EPSILON
        || previous.disk_write.get().abs_diff(current.disk_write.get()) >= IO_EPSILON
    {
        return true;
    }

    // GPU, on the same epsilon as CPU — it is the same kind of quantity and
    // rendered in the same column width.
    //
    // The `None` cases are deliberately exact. A reading appearing or
    // disappearing is the difference between "not measured" and "measured,
    // and idle", which the UI renders as an em-dash versus 0% — a threshold
    // must never swallow that.
    match (previous.gpu, current.gpu) {
        (Some(before), Some(after)) => {
            if (before.get() - after.get()).abs() >= CPU_EPSILON {
                return true;
            }
        }
        (None, None) => {}
        _ => return true,
    }

    // Anything that changes rarely but matters when it does is compared
    // exactly — a state flip, an elevation change or a thread-count change
    // must never be swallowed by a threshold.
    previous.state != current.state
        || previous.flags != current.flags
        || previous.thread_count != current.thread_count
        || previous.name != current.name
        || previous.description != current.description
}

/// Maps a sampled process onto the wire type.
fn convert(sampled: SampledProcess) -> Process {
    let raw = sampled.raw;

    Process {
        key: raw.key,
        parent: raw.parent,
        name: raw
            .name
            .clone()
            .unwrap_or_else(|| format!("PID {}", raw.key.pid.get())),
        description: sampled.description,
        // Session 0 is where services live; anything else is a user session.
        // A cheap first approximation — window ownership and the service
        // database refine it, and both cost a per-process query we do not
        // want on the hot path.
        // A switchable window makes it an app whatever its session — that is
        // the definition Task Manager's Apps group uses.
        kind: if sampled.has_window {
            ProcessKind::App
        } else if raw.session_id == 0 {
            ProcessKind::System
        } else {
            ProcessKind::Background
        },
        // Suspended is read from the thread records. The other states need a
        // per-process query (window hung-ness, exit status) and stay Running.
        state: if raw.is_suspended() {
            ProcessState::Suspended
        } else {
            ProcessState::Running
        },
        // Signing, elevation, WOW64 and window ownership each need a handle
        // open per process. Left empty here and filled in by the detail
        // query when a row is selected.
        flags: if sampled.has_window {
            ProcessFlags::HAS_WINDOW
        } else {
            ProcessFlags::empty()
        },
        integrity: None,
        protection: ProtectionLevel::None,
        cpu: sampled.cpu,
        memory_private: Bytes(raw.private_bytes),
        memory_working_set: Bytes(raw.working_set),
        disk_read: sampled.disk_read,
        disk_write: sampled.disk_write,
        // Per-process network attribution needs an ETW kernel trace session,
        // which is a separate subsystem and needs the elevated helper. `None`
        // rather than zero: a zero renders as "0 B/s" against every process
        // on the machine, which is a claim we cannot support.
        net_rx: None,
        net_tx: None,
        gpu: sampled.gpu,
        // Per-process VRAM needs a vendor SDK; the WDDM counter set reports
        // utilisation only.
        gpu_memory: None,
        thread_count: raw.thread_count,
        handle_count: Some(raw.handle_count),
        // Resolved by the sampler's owner cache: one LSA lookup per account,
        // one token read per process lifetime.
        user: sampled.owner,
        uptime_secs: uptime_from_filetime(raw.create_time),
    }
}

/// Converts a Windows FILETIME to Unix milliseconds.
///
/// FILETIME counts 100ns intervals since 1601-01-01; Unix counts
/// milliseconds since 1970-01-01. Getting this wrong puts every process's
/// start time in the seventeenth century, which is exactly the kind of bug
/// that ships because nobody scrolls that column.
fn filetime_to_unix_ms(filetime: i64) -> Option<u64> {
    /// 100ns intervals between 1601-01-01 and 1970-01-01.
    const EPOCH_DIFFERENCE: i64 = 116_444_736_000_000_000;

    if filetime <= EPOCH_DIFFERENCE {
        // The idle and system processes report 0, and a value before the Unix
        // epoch is not a real start time. None is honest; 1601 is not.
        return None;
    }

    u64::try_from((filetime - EPOCH_DIFFERENCE) / 10_000).ok()
}

/// Seconds a process has been running, from its creation FILETIME.
///
/// Zero when the start time is unknown or in the future. A clock adjustment
/// between process start and now can make `now` earlier than the creation
/// time; saturating keeps that from wrapping into ~584 million years of
/// uptime, which is what an unchecked subtraction on `u64` produces.
fn uptime_from_filetime(filetime: i64) -> u64 {
    let Some(started_ms) = filetime_to_unix_ms(filetime) else {
        return 0;
    };
    now_ms().saturating_sub(started_ms) / 1000
}

/// Current wall-clock time in Unix milliseconds.
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sampler::SystemSampler;
    use vitals_core::units::{BytesPerSec, Percent};

    fn sample() -> Sample {
        let mut sampler = SystemSampler::new();
        sampler.sample().expect("sample")
    }

    #[test]
    fn the_first_frame_is_a_keyframe() {
        // A delta against a baseline the client does not have is useless.
        let mut builder = FrameBuilder::new();
        let frame = builder.build(sample());
        assert!(frame.is_keyframe());
    }

    #[test]
    fn subsequent_frames_are_deltas() {
        let mut builder = FrameBuilder::new();
        builder.build(sample());
        std::thread::sleep(std::time::Duration::from_millis(120));
        let frame = builder.build(sample());
        assert!(!frame.is_keyframe(), "the second frame should be a delta");
    }

    #[test]
    fn the_idle_pseudo_process_never_reaches_a_frame() {
        // The enumerator sees PID 0 (asserted in enumerate.rs); the frame
        // must not. It is the single largest CPU "consumer" on any machine
        // and every process list sorted by CPU would put it first.
        let raw = sample();
        assert!(
            raw.processes.iter().any(|p| p.raw.is_idle_process()),
            "precondition: the sampler itself still reports PID 0"
        );

        let mut builder = FrameBuilder::new();
        let frame = builder.build(raw);
        let FramePayload::Keyframe { processes, .. } = &frame.payload else {
            unreachable!("first frame is a keyframe")
        };
        assert!(
            processes.iter().all(|p| p.key.pid.get() != 0),
            "PID 0 leaked into the wire frame"
        );
    }

    #[test]
    fn a_delta_is_far_smaller_than_a_keyframe() {
        // The entire reason the delta path exists. If it is not dramatically
        // smaller it is pure complexity for nothing.
        let mut builder = FrameBuilder::new();
        let keyframe = builder.build(sample());
        std::thread::sleep(std::time::Duration::from_millis(150));
        let delta = builder.build(sample());

        let key_size = serde_json::to_string(&keyframe).expect("serialise").len();
        let delta_size = serde_json::to_string(&delta).expect("serialise").len();

        println!("keyframe {key_size} bytes, delta {delta_size} bytes");
        assert!(
            delta_size * 2 < key_size,
            "delta ({delta_size}) was not meaningfully smaller than the keyframe ({key_size})"
        );
    }

    #[test]
    fn a_keyframe_is_sent_periodically() {
        // Bounds the damage if a delta is ever dropped.
        let mut builder = FrameBuilder::new();
        let mut keyframes = 0;

        for _ in 0..KEYFRAME_INTERVAL + 2 {
            // Reuse one sample; only the sequence number matters here.
            let frame = builder.build(sample());
            if frame.is_keyframe() {
                keyframes += 1;
            }
        }

        assert!(
            keyframes >= 2,
            "expected a periodic keyframe, saw {keyframes}"
        );
    }

    #[test]
    fn requesting_a_keyframe_forces_one() {
        let mut builder = FrameBuilder::new();
        builder.build(sample());
        builder.request_keyframe();
        assert!(builder.build(sample()).is_keyframe());
    }

    #[test]
    fn sequence_numbers_increase() {
        let mut builder = FrameBuilder::new();
        let a = builder.build(sample());
        let b = builder.build(sample());
        assert!(b.seq.0 > a.seq.0);
    }

    #[test]
    fn tiny_fluctuations_do_not_count_as_changes() {
        // Without this the delta is worthless: CPU jitters in the third
        // decimal on every process, every tick.
        let mut a = base_process();
        let mut b = base_process();
        a.cpu = Percent::new(1.000);
        b.cpu = Percent::new(1.004);
        assert!(!is_meaningfully_different(&a, &b));

        b.memory_private = Bytes(a.memory_private.get() + 4096);
        assert!(!is_meaningfully_different(&a, &b));
    }

    #[test]
    fn visible_changes_do_count() {
        let a = base_process();
        let mut b = base_process();
        b.cpu = Percent::new(a.cpu.get() + 1.0);
        assert!(is_meaningfully_different(&a, &b));

        let mut c = base_process();
        c.memory_private = Bytes(a.memory_private.get() + 10 * 1024 * 1024);
        assert!(is_meaningfully_different(&a, &c));
    }

    #[test]
    fn a_status_change_is_never_swallowed_by_a_threshold() {
        let a = base_process();
        let mut b = base_process();
        b.state = ProcessState::Suspended;
        assert!(
            is_meaningfully_different(&a, &b),
            "a suspended process must reach the UI regardless of its numbers"
        );
    }

    #[test]
    fn an_elevation_change_is_never_swallowed_either() {
        // Flags carry security-relevant facts; a threshold must not hide one.
        let a = base_process();
        let mut b = base_process();
        b.flags = ProcessFlags::ELEVATED;
        assert!(is_meaningfully_different(&a, &b));
    }

    #[test]
    fn a_gpu_reading_appearing_or_vanishing_is_always_sent() {
        // `None` and `Some(0)` mean different things — "not measured" versus
        // "measured, and idle" — and the UI renders them differently, as an
        // em-dash versus 0%. A numeric threshold would treat the transition
        // as a zero-sized change and the row would keep showing the stale
        // one indefinitely.
        let a = base_process();

        let mut b = base_process();
        b.gpu = Some(Percent::ZERO);

        assert!(
            is_meaningfully_different(&a, &b),
            "unmeasured -> measured-and-idle must reach the UI"
        );
        assert!(
            is_meaningfully_different(&b, &a),
            "measured-and-idle -> unmeasured must reach the UI too"
        );
    }

    #[test]
    fn trivial_gpu_jitter_does_not_count_as_a_change() {
        // Same reasoning as CPU: below what the column can render, so sending
        // the row costs payload for a number that looks identical.
        let mut a = base_process();
        a.gpu = Some(Percent::new(12.0));

        let mut b = base_process();
        b.gpu = Some(Percent::new(12.01));

        assert!(!is_meaningfully_different(&a, &b));

        let mut c = base_process();
        c.gpu = Some(Percent::new(20.0));

        assert!(is_meaningfully_different(&a, &c), "8% is not jitter");
    }

    #[test]
    fn trivial_io_jitter_does_not_count_as_a_change() {
        // Disk rates are recomputed every tick and essentially never repeat
        // exactly. Comparing them for equality marked every process that
        // touches a file as changed — measured at 95-138 of 599 processes
        // per frame, which is most of the delta's payload.
        let a = base_process();
        let mut b = base_process();
        b.disk_read = BytesPerSec(1024);
        assert!(!is_meaningfully_different(&a, &b));
    }

    #[test]
    fn real_io_activity_does_count() {
        let a = base_process();
        let mut b = base_process();
        b.disk_write = BytesPerSec(5 * 1024 * 1024);
        assert!(is_meaningfully_different(&a, &b));
    }

    #[test]
    fn filetime_converts_to_a_plausible_unix_time() {
        // 2024-01-01T00:00:00Z as a FILETIME.
        //
        // Taken from Windows itself rather than computed by hand:
        //   [datetime]::new(2024,1,1,0,0,0,'Utc').ToFileTimeUtc()
        // An earlier hand-derived value was 5h20m out, and the test caught
        // the constant rather than the code.
        let filetime = 133_485_408_000_000_000_i64;
        let ms = filetime_to_unix_ms(filetime).expect("valid");
        // 2024-01-01 is 1_704_067_200_000 ms after the Unix epoch.
        assert_eq!(ms, 1_704_067_200_000);
    }

    #[test]
    fn a_zero_filetime_is_none_not_the_year_1601() {
        // The idle and system processes report zero. Rendering that as a date
        // in 1601 is the classic version of this bug.
        assert!(filetime_to_unix_ms(0).is_none());
        assert!(filetime_to_unix_ms(-1).is_none());
    }

    #[test]
    fn real_processes_get_plausible_uptimes() {
        // Ten years. Anything beyond that means the epoch conversion is
        // wrong — the classic symptom being a process apparently started in
        // 1601, which reads as ~13 billion seconds of uptime.
        const TEN_YEARS: u64 = 10 * 365 * 24 * 3600;

        let mut builder = FrameBuilder::new();
        let frame = builder.build(sample());

        let FramePayload::Keyframe { processes, .. } = frame.payload else {
            panic!("expected a keyframe");
        };

        for p in &processes {
            assert!(
                p.uptime_secs < TEN_YEARS,
                "{} reported {}s of uptime; the FILETIME epoch conversion is wrong",
                p.name,
                p.uptime_secs
            );
        }

        assert!(
            processes.iter().any(|p| p.uptime_secs > 0),
            "no process reported any uptime at all"
        );
    }

    #[test]
    fn every_process_has_a_display_name() {
        // A blank cell reads as a bug. Protected processes report no image
        // name, so the PID is the fallback.
        let mut builder = FrameBuilder::new();
        let frame = builder.build(sample());

        let FramePayload::Keyframe { processes, .. } = frame.payload else {
            panic!("expected a keyframe");
        };

        for p in &processes {
            assert!(!p.name.is_empty(), "pid {} had no name", p.key.pid.get());
        }
    }

    fn base_process() -> Process {
        Process {
            key: ProcessKey::new(Pid(1234), 42),
            parent: None,
            name: "test.exe".into(),
            description: None,
            kind: ProcessKind::Background,
            state: ProcessState::Running,
            flags: ProcessFlags::empty(),
            integrity: None,
            protection: ProtectionLevel::None,
            cpu: Percent::new(1.0),
            memory_private: Bytes(100 * 1024 * 1024),
            memory_working_set: Bytes(120 * 1024 * 1024),
            disk_read: BytesPerSec::ZERO,
            disk_write: BytesPerSec::ZERO,
            net_rx: None,
            net_tx: None,
            gpu: None,
            gpu_memory: None,
            thread_count: 4,
            handle_count: Some(100),
            user: None,
            uptime_secs: 3600,
        }
    }
}
