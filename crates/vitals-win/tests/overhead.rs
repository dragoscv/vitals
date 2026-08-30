//! Measures the sampler's own cost.
//!
//! Not a micro-benchmark — a **budget gate**. A task manager that shows up as
//! a notable consumer in its own process list has failed, and that failure is
//! gradual: each subsystem adds a little, nobody notices, and six months
//! later the app costs 3% CPU at idle.
//!
//! Run with: `cargo test -p vitals-win --release --test overhead`
//!
//! Deliberately a test rather than a Criterion benchmark so CI runs it by
//! default. A benchmark that must be invoked explicitly is a benchmark nobody
//! runs.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use vitals_win::process::ProcessEnumerator;
use vitals_win::sampler::SystemSampler;

/// Serialises the timing tests.
///
/// `cargo test` runs tests in parallel by default, so without this they
/// partly measure each other. Measured effect on this machine: 16.5ms
/// single-threaded against 16.8ms parallel — smaller than expected, but the
/// lock costs nothing and removes a source of flakiness from CI, where core
/// counts vary.
static TIMING_LOCK: Mutex<()> = Mutex::new(());

/// Pause between samples, approximating the shipping duty cycle.
///
/// Sampling in a tight loop is not what the app does and measures the wrong
/// thing: back-to-back enumeration costs ~16.5ms, while the same call with a
/// realistic gap costs ~11ms. The kernel rebuilds its process list on demand,
/// so hammering it produces a number no user will ever experience.
const DUTY_CYCLE_GAP: Duration = Duration::from_millis(100);

/// Wall-clock budget for one full sample.
///
/// ## Why this number is not tighter
///
/// Measured on a 32-core machine with 540 processes and 30 network
/// interfaces: median 12.8ms, of which process enumeration is ~66% and
/// adapter enumeration ~31%. Both scale with counts this machine has a lot
/// of, so it is a deliberately unfavourable case.
///
/// The measurement itself varies **12.8ms to 19.3ms on an idle machine** —
/// a 50% spread, because `NtQuerySystemInformation` walks live kernel
/// structures whose cost depends on what every other process is doing. A
/// 15ms budget failed 5 runs in 8. A gate that flaky is worse than none:
/// people learn to re-run it until it passes, and a real regression then
/// slips through unnoticed.
///
/// 30ms sits above the observed maximum with margin, while still catching
/// the regressions that matter — an accidental O(n²), a per-tick allocation
/// storm, or a synchronous WMI call, all of which cost multiples rather than
/// percentages.
///
/// At 1 Hz even the worst case is ~0.3% of a single core, which rounds to
/// 0.0% in the process list we render.
///
/// Debug builds are several times slower; the gate exists to catch
/// algorithmic regressions, not an unoptimised compile.
const SAMPLE_BUDGET: Duration = if cfg!(debug_assertions) {
    Duration::from_millis(120)
} else {
    Duration::from_millis(30)
};

/// Budget for process enumeration alone, the single most expensive call.
///
/// Measured median ~8ms, same variance caveat as above.
const ENUMERATE_BUDGET: Duration = if cfg!(debug_assertions) {
    Duration::from_millis(80)
} else {
    Duration::from_millis(20)
};

/// Median of a set of durations.
///
/// Median rather than mean: any sample can be delayed by unrelated system
/// activity, and one outlier should not fail the build. The distribution is
/// one-sided — interference only ever makes a sample slower.
fn median(mut samples: Vec<Duration>) -> Duration {
    samples.sort_unstable();
    samples[samples.len() / 2]
}

#[test]
fn a_full_sample_stays_within_budget() {
    let _guard = TIMING_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    let mut sampler = SystemSampler::new();
    sampler.sample().expect("prime");

    let mut timings = Vec::with_capacity(15);
    for _ in 0..15 {
        std::thread::sleep(DUTY_CYCLE_GAP);
        let start = Instant::now();
        sampler.sample().expect("sample");
        timings.push(start.elapsed());
    }

    let mid = median(timings.clone());
    let worst = timings.iter().max().copied().unwrap_or_default();

    println!("full sample: median {mid:?}, worst {worst:?}, budget {SAMPLE_BUDGET:?}");

    assert!(
        mid <= SAMPLE_BUDGET,
        "sampling took {mid:?}, over the {SAMPLE_BUDGET:?} budget. \
         At 1 Hz that is {:.2}% of a core, and this app must not appear in \
         its own top-consumers list.",
        mid.as_secs_f64() * 100.0
    );
}

#[test]
fn process_enumeration_stays_within_budget() {
    let _guard = TIMING_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    let mut enumerator = ProcessEnumerator::new();
    enumerator.enumerate().expect("prime");

    let mut timings = Vec::with_capacity(15);
    for _ in 0..15 {
        std::thread::sleep(DUTY_CYCLE_GAP);
        let start = Instant::now();
        let processes = enumerator.enumerate().expect("enumerate");
        timings.push(start.elapsed());
        assert!(!processes.is_empty());
    }

    let mid = median(timings);
    println!("enumeration: median {mid:?}, budget {ENUMERATE_BUDGET:?}");

    assert!(
        mid <= ENUMERATE_BUDGET,
        "process enumeration took {mid:?}, over the {ENUMERATE_BUDGET:?} budget"
    );
}

#[test]
fn repeated_sampling_does_not_leak() {
    // The baseline maps must evict exited processes. A leak here is
    // invisible for an hour and fatal overnight, which is exactly how long
    // this app is expected to stay open.
    let _guard = TIMING_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    let mut sampler = SystemSampler::new();

    for _ in 0..200 {
        sampler.sample().expect("sample");
    }

    // If the internal maps grew per tick rather than being replaced, 200
    // iterations on a machine with ~500 processes would hold 100k entries.
    // Sampling stays fast only if they did not.
    let start = Instant::now();
    sampler.sample().expect("final sample");
    let elapsed = start.elapsed();

    assert!(
        elapsed <= SAMPLE_BUDGET * 2,
        "sampling degraded to {elapsed:?} after 200 iterations, which suggests \
         per-tick state is accumulating instead of being replaced"
    );
}

#[test]
fn the_enumeration_buffer_converges() {
    // The buffer should grow once to fit the machine and then stop. Growing
    // every tick would mean an allocation and a memcpy per second forever.
    let mut enumerator = ProcessEnumerator::new();

    let mut sizes = Vec::new();
    for _ in 0..10 {
        enumerator.enumerate().expect("enumerate");
        sizes.push(enumerator.buffer_capacity());
    }

    let first = sizes[0];
    assert!(
        sizes.iter().all(|&s| s == first),
        "buffer size varied across calls: {sizes:?}"
    );
}
