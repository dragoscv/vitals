//! The four workloads this build can measure honestly: two CPU, two memory.
//!
//! # Why `black_box` is on every one of them
//!
//! A benchmark loop whose result nobody reads is dead code, and LLVM at
//! `opt-level = 3` with fat LTO — which is exactly what the release profile
//! in the workspace manifest asks for — will delete it. The loop then takes
//! no time, and the harness divides work by a near-zero duration and reports
//! a score in the billions. This is not a hypothetical: it is the single most
//! common way a hand-rolled benchmark ends up lying, and it fails *upwards*,
//! so it looks like good news.
//!
//! [`std::hint::black_box`] is the fix. It is an optimisation barrier: the
//! compiler must assume the value passed through it is observed, so it cannot
//! prove the computation that produced it is unused. Each call below says
//! what it is protecting.
//!
//! # Time budget
//!
//! Each workload targets roughly 200–400 ms per run and the harness takes
//! three runs, so the four together sit near 4 seconds of measurement plus
//! about a second of allocation. That leaves the whole suite comfortably
//! under the ten-second mark and far under the one-minute ceiling, which
//! matters because a benchmark long enough for the machine to heat up is
//! measuring the cooler, not the chip.

use std::hint::black_box;
use std::time::{Duration, Instant};

use crate::rng::SplitMix64;

/// One measurement: what was achieved and how long it took.
///
/// The duration is returned alongside the score rather than being recomputed
/// by the caller, because a workload that finished in zero measurable time
/// must be detectable — that is the signature of the loop having been
/// optimised away.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Measurement {
    pub score: f64,
    pub elapsed: Duration,
}

// ---------------------------------------------------------------------------
// CPU
// ---------------------------------------------------------------------------

/// Iterations per single-threaded run.
///
/// Tuned so a mid-range desktop core lands near 250 ms. A larger count would
/// measure the same thing more precisely at the cost of a suite the user
/// abandons before it finishes.
const CPU_ITERATIONS: u64 = 12_000_000;

/// The scalar kernel both CPU benchmarks run.
///
/// Deliberately mixed integer and floating point with a serial dependency
/// chain: each iteration consumes the previous accumulator, so the result
/// measures per-core latency through the ALU and FPU rather than how many
/// independent operations the scheduler can keep in flight. A loop of
/// independent operations would instead measure issue width, which varies so
/// much between microarchitectures that the number stops meaning anything.
///
/// Returns the accumulator so the caller can feed it to `black_box`; if this
/// returned nothing, the entire body would be provably dead.
fn cpu_kernel(iterations: u64, seed: u64) -> u64 {
    let mut acc: u64 = black_box(seed);
    let mut float = black_box(1.000_000_1_f64);

    for i in 0..iterations {
        // Integer mixing: multiply, rotate, xor. The rotate defeats the
        // strength reduction LLVM would otherwise apply to a pure multiply
        // chain, which would turn the loop into a closed-form expression.
        acc = acc.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(i);
        acc ^= acc.rotate_left(23);

        // Floating point on the same dependency chain. `sqrt` is a real FPU
        // instruction rather than something the optimiser can constant-fold,
        // and mixing the integer accumulator in stops it being hoisted out
        // of the loop as a value that does not vary.
        float = (float * 1.000_000_3 + (acc & 0xFF) as f64).sqrt() + 1.0;
        acc = acc.wrapping_add(float.to_bits());
    }

    acc.wrapping_add(float.to_bits())
}

/// Single-threaded CPU throughput, in kernel iterations per second.
///
/// Reported as `ops/s` rather than a dimensionless "points" figure. A point
/// score implies a comparability across machines and across versions of this
/// program that we cannot back, whereas iterations per second is simply what
/// was counted.
#[must_use]
pub fn cpu_single_thread() -> Measurement {
    cpu_single_thread_with(CPU_ITERATIONS)
}

/// [`cpu_single_thread`] with an explicit iteration count.
///
/// Exists so tests can assert the shape of the result — that it scores, that
/// it takes measurable time — without running the full production workload.
/// A debug-build test doing 12 million iterations cost seconds of every
/// `cargo test`, to check a property a thousand iterations proves just as
/// well.
#[must_use]
pub fn cpu_single_thread_with(iterations: u64) -> Measurement {
    let started = Instant::now();
    // The kernel's return value is consumed here and nowhere else. Without
    // this barrier the call has no observable effect and vanishes entirely.
    black_box(cpu_kernel(black_box(iterations), 0x5EED));
    let elapsed = started.elapsed();

    Measurement {
        score: throughput(iterations as f64, elapsed),
        elapsed,
    }
}

/// Multi-threaded CPU throughput, in kernel iterations per second summed
/// across every logical processor.
///
/// Thread count comes from [`std::thread::available_parallelism`], which
/// respects CPU affinity masks and container limits. `num_cpus`-style
/// "how many processors exist" would over-subscribe a pinned or containerised
/// run and report a score the machine cannot actually sustain.
#[must_use]
pub fn cpu_multi_thread() -> Measurement {
    // Each thread runs the full single-thread count, so the multi-thread
    // score is directly comparable to the single-thread one: perfect scaling
    // would be exactly `threads` times larger. Dividing a fixed total across
    // threads instead would shrink each thread's slice until it measured
    // spawn overhead on high-core machines.
    cpu_multi_thread_with(CPU_ITERATIONS)
}

/// [`cpu_multi_thread`] with an explicit per-thread iteration count.
#[must_use]
pub fn cpu_multi_thread_with(per_thread: u64) -> Measurement {
    let threads = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);

    let started = Instant::now();
    std::thread::scope(|scope| {
        let handles: Vec<_> = (0..threads)
            .map(|t| {
                scope.spawn(move || {
                    // A distinct seed per thread so the results differ; if
                    // every thread computed the same value the optimiser
                    // could in principle share the work.
                    black_box(cpu_kernel(black_box(per_thread), 0x5EED ^ t as u64))
                })
            })
            .collect();

        for handle in handles {
            // Joining before the timer stops is what makes this a measurement
            // of the whole machine rather than of thread creation.
            black_box(handle.join().unwrap_or(0));
        }
    });
    let elapsed = started.elapsed();

    Measurement {
        score: throughput(per_thread as f64 * threads as f64, elapsed),
        elapsed,
    }
}

// ---------------------------------------------------------------------------
// Memory
// ---------------------------------------------------------------------------

/// Working-set size for both memory benchmarks, in bytes.
///
/// 512 MB. The point is to exceed last-level cache by a wide enough margin
/// that cache hits are statistical noise: consumer L3 currently tops out
/// around 128 MB on the largest X3D parts, and server parts go higher still,
/// so anything in the tens of megabytes would be measuring SRAM and calling
/// it DRAM. Reporting cache bandwidth as memory bandwidth inflates the figure
/// by roughly an order of magnitude, which again fails upwards.
///
/// Allocated once and reused across every run and both benchmarks. Half a
/// gigabyte of first-touch page faults costs more than the measurement does,
/// and charging that to the first run only would make run one look slow for
/// a reason that has nothing to do with the memory subsystem.
pub const WORKING_SET_BYTES: usize = 512 * 1024 * 1024;

/// The buffer both memory benchmarks stream over.
///
/// Owned by the caller so the allocation and its page faults happen once,
/// outside any timed region.
#[derive(Debug)]
pub struct MemoryBuffer {
    values: Vec<u64>,
    /// Permutation of indices forming a single cycle, for the pointer chase.
    chain: Vec<usize>,
}

impl MemoryBuffer {
    /// Allocates and fully touches the working set.
    ///
    /// Touching every page here rather than lazily is the whole reason this
    /// is a separate step: Windows commits pages on first write, so an
    /// untouched `vec![0; n]` would charge hundreds of thousands of soft page
    /// faults to whichever benchmark ran first.
    #[must_use]
    pub fn allocate() -> Self {
        Self::with_bytes(WORKING_SET_BYTES)
    }

    /// Allocates a working set of an explicit size.
    ///
    /// Only the full [`WORKING_SET_BYTES`] buffer measures DRAM; a small one
    /// measures cache and must never be used for a reported score. It exists
    /// for tests, which check that the workloads return plausible finite
    /// numbers rather than checking any particular speed — and building the
    /// 64-million-element cycle for that took thirteen seconds per test in a
    /// debug build.
    #[must_use]
    pub fn with_bytes(bytes: usize) -> Self {
        let len = (bytes / size_of::<u64>()).max(2);

        let mut values = vec![0u64; len];
        for (i, slot) in values.iter_mut().enumerate() {
            *slot = i as u64;
        }

        Self {
            chain: build_cycle(len),
            values,
        }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

/// Builds a permutation that forms exactly one cycle over `0..len`.
///
/// A plain shuffle is not good enough: a random permutation decomposes into
/// several cycles, and a chase entering a short one revisits a handful of
/// lines that then sit in cache, collapsing the measured latency. Sattolo's
/// algorithm — the Fisher-Yates variant that never swaps an element with
/// itself — produces a single cycle of full length by construction, so the
/// chase is guaranteed to touch every element before repeating any.
fn build_cycle(len: usize) -> Vec<usize> {
    let mut order: Vec<usize> = (0..len).collect();

    let mut rng = SplitMix64::new(0x0DDB_A11B_ADC0_FFEE);
    for i in (1..len).rev() {
        let j = rng.next_below(i); // Strictly below `i`: that is Sattolo.
        order.swap(i, j);
    }

    // `order` is a cyclic sequence of indices; turn it into a next-pointer
    // table so the chase is a single dependent load per step rather than two.
    let mut chain = vec![0usize; len];
    for w in 0..len {
        chain[order[w]] = order[(w + 1) % len];
    }
    chain
}

/// Streaming read bandwidth over the working set, in MB/s.
///
/// A sum rather than a copy: a copy would be half read and half write and
/// the resulting figure would depend on the write-allocate policy of the
/// cache hierarchy, which differs between vendors and makes the number
/// non-comparable. Reading is the simpler thing to state honestly.
///
/// MB here is 1 000 000 bytes, the convention every DRAM datasheet uses.
#[must_use]
pub fn memory_bandwidth(buffer: &MemoryBuffer) -> Measurement {
    let data = black_box(&buffer.values);

    let started = Instant::now();

    // Four independent accumulators. A single accumulator serialises the
    // additions and makes this a latency test of the adder rather than a
    // bandwidth test of the memory bus; with four chains the loads stay
    // ahead of the arithmetic and the bus is the bottleneck, which is what
    // we claim to be measuring.
    let mut a = 0u64;
    let mut b = 0u64;
    let mut c = 0u64;
    let mut d = 0u64;

    let chunks = data.chunks_exact(4);
    let remainder = chunks.remainder();
    for chunk in chunks {
        a = a.wrapping_add(chunk[0]);
        b = b.wrapping_add(chunk[1]);
        c = c.wrapping_add(chunk[2]);
        d = d.wrapping_add(chunk[3]);
    }
    for &value in remainder {
        a = a.wrapping_add(value);
    }

    // The sum is never used for anything. Without this barrier the loads
    // themselves are unobservable and the entire traversal is removed.
    black_box(a.wrapping_add(b).wrapping_add(c).wrapping_add(d));

    let elapsed = started.elapsed();
    let bytes = (data.len() * size_of::<u64>()) as f64;

    Measurement {
        score: throughput(bytes / 1_000_000.0, elapsed),
        elapsed,
    }
}

/// Steps taken per latency run.
///
/// Each step is a full memory round trip on a cache miss, so at roughly 80 ns
/// apiece four million steps is about 320 ms — the same order as the other
/// three workloads.
const LATENCY_STEPS: usize = 4_000_000;

/// Dependent-load latency, in nanoseconds per access.
///
/// The address of each load is the *result* of the previous load, so the
/// hardware prefetcher has nothing to predict and no two accesses can
/// overlap. Walking the buffer with a computed stride instead — even a large
/// prime one — lets the prefetcher run ahead and reports something closer to
/// bandwidth-limited throughput, which on a modern part is under 10 ns and
/// simply is not the DRAM latency it claims to be.
#[must_use]
pub fn memory_latency(buffer: &MemoryBuffer) -> Measurement {
    let chain = black_box(&buffer.chain);
    if chain.is_empty() {
        return Measurement {
            score: 0.0,
            elapsed: Duration::ZERO,
        };
    }

    let mut index = black_box(0usize);

    let started = Instant::now();
    for _ in 0..LATENCY_STEPS {
        index = chain[index];
    }
    // The final index is the only evidence the chase happened. Dropping it
    // would let the compiler delete every load in the loop.
    black_box(index);
    let elapsed = started.elapsed();

    Measurement {
        score: elapsed.as_nanos() as f64 / LATENCY_STEPS as f64,
        elapsed,
    }
}

/// Work per second, guarding the divide-by-zero that an optimised-away loop
/// produces.
///
/// Returns 0.0 rather than infinity for a zero duration. Infinity would
/// serialise to JSON `null` and silently vanish in the UI; a zero is visibly
/// wrong, and the harness's own tests assert against it.
fn throughput(work: f64, elapsed: Duration) -> f64 {
    let secs = elapsed.as_secs_f64();
    if secs <= 0.0 { 0.0 } else { work / secs }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Enough work to be measurable, little enough to be free.
    ///
    /// These tests assert the *shape* of a measurement, never a speed, so the
    /// production iteration count buys nothing here and cost seconds on every
    /// `cargo test` in a debug build.
    const TEST_ITERATIONS: u64 = 50_000;

    #[test]
    fn single_thread_run_takes_measurable_time_and_scores() {
        let m = cpu_single_thread_with(TEST_ITERATIONS);
        assert!(
            m.elapsed > Duration::ZERO,
            "a zero duration means the kernel was optimised away"
        );
        assert!(m.score > 0.0 && m.score.is_finite(), "score {}", m.score);
    }

    #[test]
    fn multi_thread_run_takes_measurable_time_and_scores() {
        let m = cpu_multi_thread_with(TEST_ITERATIONS);
        assert!(m.elapsed > Duration::ZERO);
        assert!(m.score > 0.0 && m.score.is_finite(), "score {}", m.score);
    }

    #[test]
    fn cpu_kernel_result_depends_on_its_inputs() {
        // If the compiler had folded the loop to a constant, or if the seed
        // were being ignored, these would match. This is the cheap standing
        // check that the work is real.
        assert_ne!(cpu_kernel(1_000, 1), cpu_kernel(1_000, 2));
        assert_ne!(cpu_kernel(1_000, 1), cpu_kernel(2_000, 1));
    }

    #[test]
    fn cpu_kernel_scales_with_iteration_count() {
        // Ten times the work must take meaningfully longer. A loop that has
        // been deleted takes the same near-zero time whatever it is asked
        // for, so this catches the failure the black_boxes exist to prevent
        // without depending on any absolute speed.
        let small = Instant::now();
        black_box(cpu_kernel(black_box(200_000), 1));
        let small = small.elapsed();

        let large = Instant::now();
        black_box(cpu_kernel(black_box(2_000_000), 1));
        let large = large.elapsed();

        assert!(
            large > small * 2,
            "10x the iterations took {large:?} against {small:?}; the loop is not running"
        );
    }

    #[test]
    fn working_set_is_far_larger_than_any_plausible_cache() {
        // 128 MB is the largest consumer L3 currently shipping. If someone
        // shrinks the buffer to make the suite faster, this fails and says
        // why rather than the benchmark quietly reporting cache speed.
        const { assert!(WORKING_SET_BYTES >= 256 * 1024 * 1024) }
    }

    #[test]
    fn cycle_covers_every_element_exactly_once() {
        // The whole value of Sattolo's algorithm is this property; a plain
        // shuffle would fail here by producing several short cycles.
        let len = 4_096;
        let chain = build_cycle(len);

        let mut visited = vec![false; len];
        let mut index = 0;
        for _ in 0..len {
            assert!(!visited[index], "revisited {index} before covering the set");
            visited[index] = true;
            index = chain[index];
        }
        assert_eq!(index, 0, "the chase must return to its start");
        assert!(visited.into_iter().all(|v| v));
    }

    #[test]
    fn cycle_is_not_the_identity() {
        let chain = build_cycle(1_024);
        assert!(
            chain
                .iter()
                .enumerate()
                .filter(|&(i, &n)| i + 1 == n)
                .count()
                < 64,
            "the permutation is close to sequential, so the prefetcher will hide the latency"
        );
    }

    /// Big enough to exercise the code, small enough to be free.
    ///
    /// This measures cache, not DRAM, which is fine for every assertion that
    /// only checks a score is finite and positive.
    const TEST_WORKING_SET: usize = 1024 * 1024;

    #[test]
    fn memory_workloads_report_plausible_measured_values() {
        let buffer = MemoryBuffer::with_bytes(TEST_WORKING_SET);
        assert_eq!(buffer.len(), TEST_WORKING_SET / size_of::<u64>());
        assert!(!buffer.is_empty());

        let bandwidth = memory_bandwidth(&buffer);
        assert!(bandwidth.elapsed > Duration::ZERO);
        assert!(
            bandwidth.score > 0.0 && bandwidth.score.is_finite(),
            "bandwidth {} MB/s",
            bandwidth.score
        );

        let latency = memory_latency(&buffer);
        assert!(latency.elapsed > Duration::ZERO);
        assert!(
            latency.score > 0.0 && latency.score.is_finite(),
            "latency {} ns",
            latency.score
        );
    }

    /// The one test that must pay for a real working set.
    ///
    /// A dependent load that misses cache cannot resolve in under a
    /// nanosecond on any real machine, so a faster figure means the chase
    /// stayed in cache or was elided — both of which invalidate the reported
    /// score. Proving that needs the production buffer, and no smaller one
    /// would prove anything, so this is `#[ignore]`d and run deliberately:
    ///
    /// ```text
    /// cargo test -p vitals-bench --release -- --ignored
    /// ```
    #[test]
    #[ignore = "allocates 512 MB; run explicitly with --ignored"]
    fn the_real_working_set_actually_reaches_dram() {
        let buffer = MemoryBuffer::allocate();
        assert_eq!(buffer.len(), WORKING_SET_BYTES / size_of::<u64>());

        let latency = memory_latency(&buffer);
        assert!(
            latency.score > 1.0,
            "{} ns per dependent load is not a DRAM access",
            latency.score
        );
    }
}
