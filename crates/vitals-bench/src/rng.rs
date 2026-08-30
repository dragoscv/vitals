//! A tiny deterministic generator, because the workloads need shuffling and
//! the workspace has no random-number crate.
//!
//! Adding one for this would be a poor trade: the pointer-chase permutation
//! needs statistical *spread*, not cryptographic quality, and `SplitMix64`
//! gives that in nine lines with no dependency and no build-time cost.
//!
//! Seeded rather than entropy-derived, deliberately. A benchmark that walks
//! a different permutation on every invocation has one more source of
//! run-to-run variance than it needs, and variance is the thing the harness
//! spends its whole design budget trying to expose rather than create.

/// `SplitMix64`, the mixing function Rust's own `DefaultHasher` lineage and
/// the `xoshiro` family both use for seeding.
#[derive(Debug, Clone, Copy)]
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// Next value in the sequence.
    pub const fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform value in `0..bound`, via Lemire's multiply-shift.
    ///
    /// Modulo would bias the low indices, which in a permutation shows up as
    /// short cycles clustered at the start of the buffer — exactly the
    /// locality the pointer chase exists to destroy.
    pub const fn next_below(&mut self, bound: usize) -> usize {
        if bound == 0 {
            return 0;
        }
        // `as` rather than `u128::from`: `From` is not a const trait yet, and
        // this stays `const` so the compiler can fold the shuffle in tests.
        let product = self.next_u64() as u128 * bound as u128;
        (product >> 64) as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequence_is_deterministic_for_a_seed() {
        let mut a = SplitMix64::new(42);
        let mut b = SplitMix64::new(42);
        for _ in 0..64 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn bounded_values_stay_in_range_and_spread() {
        let mut rng = SplitMix64::new(7);
        let mut seen = [0u32; 8];
        for _ in 0..8_000 {
            let v = rng.next_below(8);
            assert!(v < 8);
            seen[v] += 1;
        }
        // Every bucket hit, and none of them dominating. A biased generator
        // would starve the top bucket, which is the failure this guards.
        assert!(
            seen.iter().all(|&c| c > 700 && c < 1_300),
            "uneven distribution: {seen:?}"
        );
    }
}
