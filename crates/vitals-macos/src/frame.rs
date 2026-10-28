//! macOS samples to wire frames, through the shared encoder.

use vitals_core::frame::FrameEncoder;
use vitals_core::sample::Frame;

use crate::sampler::Sample;

/// Builds wire frames from macOS samples. The keyframe/delta rules are the
/// same object Windows uses (`vitals_core::frame`), so a threshold tuned on
/// one platform can never drift from the other.
#[derive(Debug, Default)]
pub struct FrameBuilder {
    encoder: FrameEncoder,
}

impl FrameBuilder {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Forces the next frame to be a keyframe.
    pub fn request_keyframe(&mut self) {
        self.encoder.request_keyframe();
    }

    /// Converts a sample into a frame.
    pub fn build(&mut self, sample: Sample) -> Frame {
        self.encoder
            .encode(sample.system, sample.processes, sample.elapsed_ms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SystemSampler;

    #[test]
    fn a_real_sample_becomes_a_keyframe_then_a_much_smaller_delta() {
        let mut sampler = SystemSampler::new();
        let mut frames = FrameBuilder::new();
        sampler.sample().expect("warm-up");
        std::thread::sleep(std::time::Duration::from_millis(300));
        let key = frames.build(sampler.sample().expect("sample"));
        std::thread::sleep(std::time::Duration::from_millis(300));
        let delta = frames.build(sampler.sample().expect("sample"));
        assert!(key.is_keyframe());
        assert!(!delta.is_keyframe());
        if crate::APP_STORE {
            return; // no process list, so nothing for a delta to save
        }
        let k = serde_json::to_string(&key).expect("json").len();
        let d = serde_json::to_string(&delta).expect("json").len();
        assert!(d * 2 < k, "delta {d} vs keyframe {k}");
    }
}
