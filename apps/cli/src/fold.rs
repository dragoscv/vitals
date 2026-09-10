//! Folds a stream of frames into the current state of the machine.
//!
//! Deltas describe changes against the previous frame, so a consumer that
//! wants "what is running now" must maintain a materialised view. This is the
//! same rule `crates/vitals-server/src/state.rs` and
//! `packages/protocol/src/guards.ts` apply; a fourth copy would be wrong the
//! moment one of them changed, so the rule is stated once here in words:
//!
//! **Exits are applied before changes.** A PID that exited and was reused
//! within one tick appears in both lists, and removing afterwards would delete
//! the new owner.

use std::collections::HashMap;

use vitals_core::ids::Pid;
use vitals_core::metrics::SystemMetrics;
use vitals_core::process::Process;
use vitals_core::sample::{Frame, FramePayload};

/// The reconstructed present.
#[derive(Debug, Default, Clone)]
pub struct View {
    pub timestamp_ms: u64,
    pub system: SystemMetrics,
    processes: HashMap<Pid, Process>,
    /// False until a keyframe has been seen. A delta before that describes
    /// changes to a machine we have never seen and is dropped.
    primed: bool,
}

impl View {
    /// Applies one frame. Returns `false` when the frame was ignored.
    pub fn apply(&mut self, frame: &Frame) -> bool {
        match &frame.payload {
            FramePayload::Keyframe { system, processes } => {
                self.timestamp_ms = frame.timestamp_ms;
                self.system = system.clone();
                self.processes = processes.iter().map(|p| (p.key.pid, p.clone())).collect();
                self.primed = true;
                true
            }
            FramePayload::Delta {
                system,
                changed,
                exited,
            } => {
                if !self.primed {
                    return false;
                }
                self.timestamp_ms = frame.timestamp_ms;
                self.system = system.clone();
                for pid in exited {
                    self.processes.remove(pid);
                }
                for process in changed {
                    self.processes.insert(process.key.pid, process.clone());
                }
                true
            }
        }
    }

    #[must_use]
    pub const fn is_primed(&self) -> bool {
        self.primed
    }

    /// Every process, in PID order so successive renders do not shuffle.
    #[must_use]
    pub fn processes(&self) -> Vec<Process> {
        let mut out: Vec<Process> = self.processes.values().cloned().collect();
        out.sort_by_key(|p| p.key.pid.get());
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vitals_core::fixtures;
    use vitals_core::sample::FrameSeq;

    fn delta(changed: Vec<Process>, exited: Vec<Pid>) -> Frame {
        Frame {
            seq: FrameSeq(2),
            timestamp_ms: 1,
            elapsed_ms: 1_000,
            payload: FramePayload::Delta {
                system: fixtures::system(),
                changed,
                exited,
            },
        }
    }

    #[test]
    fn exits_apply_before_changes_so_a_recycled_pid_survives() {
        let mut view = View::default();
        view.apply(&fixtures::keyframe(
            1,
            vec![fixtures::process("old.exe", 100, 1.0)],
        ));

        // PID 100 exited and was reused by new.exe within the same tick.
        let recycled = fixtures::process("new.exe", 100, 2.0);
        view.apply(&delta(vec![recycled], vec![Pid(100)]));

        let names: Vec<_> = view.processes().into_iter().map(|p| p.name).collect();
        assert_eq!(
            names,
            vec!["new.exe"],
            "the new owner of PID 100 must survive"
        );
    }

    #[test]
    fn a_delta_before_any_keyframe_is_dropped_rather_than_inventing_a_machine() {
        let mut view = View::default();
        let applied = view.apply(&delta(vec![fixtures::process("x.exe", 1, 0.0)], vec![]));
        assert!(!applied);
        assert!(!view.is_primed());
        assert!(view.processes().is_empty());
    }

    #[test]
    fn a_keyframe_replaces_the_whole_process_table() {
        let mut view = View::default();
        view.apply(&fixtures::keyframe(
            1,
            vec![fixtures::process("a.exe", 1, 0.0)],
        ));
        view.apply(&fixtures::keyframe(
            2,
            vec![fixtures::process("b.exe", 2, 0.0)],
        ));
        let names: Vec<_> = view.processes().into_iter().map(|p| p.name).collect();
        assert_eq!(names, vec!["b.exe"]);
    }
}
