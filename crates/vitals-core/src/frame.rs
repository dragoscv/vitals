//! Platform-neutral keyframe/delta encoding of wire [`Frame`]s.
//!
//! Every backend converts its own process records to [`Process`] and hands
//! them here, so there is exactly one copy of the delta rules. It used to live
//! in `vitals-win`, typed on Windows sample structs; a second copy for macOS
//! would have drifted the first time someone tuned a threshold in one of them.
//!
//! A keyframe with 550 processes is ~250 KB of JSON; almost none of it changes
//! between ticks. Sending only what moved cuts that by well over an order of
//! magnitude, and at 1 Hz forever the difference is the gap between a
//! background app and a noticeable one.

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::ids::{Pid, ProcessKey};
use crate::metrics::SystemMetrics;
use crate::process::Process;
use crate::sample::{Frame, FramePayload, FrameSeq};

/// How often a keyframe is sent regardless of what changed.
///
/// Deltas are only valid relative to a shared baseline. If one is ever
/// dropped — a webview reload, a missed event — the client's picture drifts
/// silently. A periodic keyframe bounds that damage to a few seconds instead
/// of leaving the UI permanently wrong.
pub const KEYFRAME_INTERVAL: u64 = 30;

/// Encodes already-converted samples as keyframes and deltas.
#[derive(Debug, Default)]
pub struct FrameEncoder {
    seq: u64,
    /// Last state sent per process, for change detection.
    previous: HashMap<ProcessKey, Process>,
}

impl FrameEncoder {
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

    /// Encodes one tick.
    pub fn encode(
        &mut self,
        system: SystemMetrics,
        processes: Vec<Process>,
        elapsed_ms: u32,
    ) -> Frame {
        self.seq = self.seq.saturating_add(1);

        // Keyframe when the client has no baseline, or on the periodic
        // refresh. `seq == 1` covers the first frame after a reset.
        let force_keyframe = self.previous.is_empty() || self.seq.is_multiple_of(KEYFRAME_INTERVAL);

        let payload = if force_keyframe {
            self.previous = processes.iter().map(|p| (p.key, p.clone())).collect();
            FramePayload::Keyframe { system, processes }
        } else {
            self.build_delta(system, processes)
        };

        Frame {
            seq: FrameSeq(self.seq),
            timestamp_ms: now_ms(),
            elapsed_ms,
            payload,
        }
    }

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
#[must_use]
pub fn is_meaningfully_different(previous: &Process, current: &Process) -> bool {
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

/// Current wall-clock time in Unix milliseconds.
#[must_use]
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::{ProcessFlags, ProcessKind, ProcessState, ProtectionLevel};
    use crate::units::{Bytes, BytesPerSec, Percent};

    fn base_process() -> Process {
        Process {
            key: ProcessKey::new(Pid(1234), 42),
            parent: None,
            name: "test".into(),
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

    fn with_pid(pid: u32) -> Process {
        let mut p = base_process();
        p.key = ProcessKey::new(Pid(pid), u64::from(pid) * 7);
        p
    }

    fn system() -> SystemMetrics {
        crate::fixtures::system()
    }

    #[test]
    fn the_first_frame_is_a_keyframe_and_the_next_a_delta() {
        let mut enc = FrameEncoder::new();
        assert!(enc.encode(system(), vec![with_pid(1)], 0).is_keyframe());
        assert!(!enc.encode(system(), vec![with_pid(1)], 0).is_keyframe());
    }

    #[test]
    fn an_unchanged_process_is_left_out_of_a_delta_and_an_exit_is_reported() {
        let mut enc = FrameEncoder::new();
        enc.encode(system(), vec![with_pid(1), with_pid(2)], 0);
        let mut moved = with_pid(1);
        moved.cpu = Percent::new(50.0);
        let frame = enc.encode(system(), vec![moved], 0);
        let FramePayload::Delta {
            changed, exited, ..
        } = frame.payload
        else {
            panic!("expected a delta");
        };
        assert_eq!(changed.len(), 1);
        assert_eq!(changed[0].key.pid, Pid(1));
        assert_eq!(exited, vec![Pid(2)]);
    }

    #[test]
    fn a_recycled_pid_is_a_new_process_not_an_update() {
        // Same pid, different start time: the old one exited and a new one
        // started. Keyed on ProcessKey, the delta reports both facts.
        let mut enc = FrameEncoder::new();
        enc.encode(system(), vec![with_pid(5)], 0);
        let mut reborn = with_pid(5);
        reborn.key = ProcessKey::new(Pid(5), 999);
        let FramePayload::Delta {
            changed, exited, ..
        } = enc.encode(system(), vec![reborn], 0).payload
        else {
            panic!("expected a delta");
        };
        assert_eq!(changed.len(), 1);
        assert_eq!(exited, vec![Pid(5)]);
    }

    #[test]
    fn a_keyframe_is_sent_periodically() {
        let mut enc = FrameEncoder::new();
        let keyframes = (0..KEYFRAME_INTERVAL + 2)
            .filter(|_| enc.encode(system(), vec![with_pid(1)], 0).is_keyframe())
            .count();
        assert!(
            keyframes >= 2,
            "expected a periodic keyframe, saw {keyframes}"
        );
    }

    #[test]
    fn requesting_a_keyframe_forces_one() {
        let mut enc = FrameEncoder::new();
        enc.encode(system(), vec![with_pid(1)], 0);
        enc.request_keyframe();
        assert!(enc.encode(system(), vec![with_pid(1)], 0).is_keyframe());
    }

    #[test]
    fn tiny_fluctuations_do_not_count_as_changes() {
        let mut a = base_process();
        let mut b = base_process();
        a.cpu = Percent::new(1.000);
        b.cpu = Percent::new(1.004);
        assert!(!is_meaningfully_different(&a, &b));
        b.memory_private = Bytes(a.memory_private.get() + 4096);
        assert!(!is_meaningfully_different(&a, &b));
        let mut c = base_process();
        c.disk_read = BytesPerSec(1024);
        assert!(!is_meaningfully_different(&a, &c));
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
        let mut d = base_process();
        d.disk_write = BytesPerSec(5 * 1024 * 1024);
        assert!(is_meaningfully_different(&a, &d));
    }

    #[test]
    fn a_status_or_flag_change_is_never_swallowed_by_a_threshold() {
        let a = base_process();
        let mut b = base_process();
        b.state = ProcessState::Suspended;
        assert!(is_meaningfully_different(&a, &b));
        let mut c = base_process();
        c.flags = ProcessFlags::ELEVATED;
        assert!(is_meaningfully_different(&a, &c));
    }

    #[test]
    fn a_gpu_reading_appearing_or_vanishing_is_always_sent() {
        let a = base_process();
        let mut b = base_process();
        b.gpu = Some(Percent::ZERO);
        assert!(is_meaningfully_different(&a, &b));
        assert!(is_meaningfully_different(&b, &a));
    }

    #[test]
    fn trivial_gpu_jitter_does_not_count_but_eight_points_do() {
        let mut a = base_process();
        a.gpu = Some(Percent::new(12.0));
        let mut b = base_process();
        b.gpu = Some(Percent::new(12.01));
        assert!(!is_meaningfully_different(&a, &b));
        let mut c = base_process();
        c.gpu = Some(Percent::new(20.0));
        assert!(is_meaningfully_different(&a, &c));
    }
}
