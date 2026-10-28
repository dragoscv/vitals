//! Converts Windows samples into the wire [`Frame`] the UI consumes.
//!
//! The keyframe/delta encoding itself lives in
//! [`vitals_core::frame::FrameEncoder`] so every platform shares one copy of
//! the rules; this module only maps Windows-shaped records onto [`Process`]
//! and drops what no client should ever see.

use vitals_core::frame::{FrameEncoder, now_ms};
use vitals_core::process::{Process, ProcessFlags, ProcessKind, ProcessState, ProtectionLevel};
use vitals_core::sample::Frame;
use vitals_core::units::Bytes;

use crate::sampler::{Sample, SampledProcess};

/// Builds wire frames from Windows samples.
#[derive(Debug, Default)]
pub struct FrameBuilder {
    encoder: FrameEncoder,
}

impl FrameBuilder {
    #[must_use]
    pub fn new() -> Self {
        Self {
            encoder: FrameEncoder::new(),
        }
    }

    /// Forces the next frame to be a keyframe.
    pub fn request_keyframe(&mut self) {
        self.encoder.request_keyframe();
    }

    /// Converts a sample into a frame.
    pub fn build(&mut self, sample: Sample) -> Frame {
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
        self.encoder
            .encode(sample.system, processes, sample.elapsed_ms)
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sampler::SystemSampler;
    use vitals_core::frame::KEYFRAME_INTERVAL;
    use vitals_core::sample::FramePayload;

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
}
