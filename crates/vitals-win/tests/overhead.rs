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
//!
//! Every run also reports what the machine was doing while the budget was
//! measured — logical processors, processes, the system-wide busy fraction and
//! this thread's own CPU time — and prints that beside the number whether the
//! gate passes or fails. The budget itself is unchanged and still fails. What
//! changes is that a red run says whether it is about the sampler or about the
//! machine: on 2026-11-20 `scripts/verify.ps1` failed here on a saturated
//! workstation, against a commit whose CI was green on a hosted runner, and
//! the red was briefly read as a regression in the commit.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use vitals_win::cpu::{CpuSampler, CpuUsage, logical_core_count};
use vitals_win::process::ProcessEnumerator;
use vitals_win::sampler::SystemSampler;
use windows_sys::Win32::Foundation::FILETIME;
use windows_sys::Win32::System::Threading::{GetCurrentThread, GetThreadTimes};

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
/// At 1 Hz the 30 ms worst case is 3% of a single core (30 ms / 1000 ms) —
/// a ceiling, not the typical cost, which sits well under the budget.
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

/// System-wide CPU busy fraction above which the machine counts as loaded.
///
/// A judgement, not a measurement: 50% of a 32-core machine is sixteen cores
/// busy, which is enough that a thread sharing one of them will wait.
const BUSY_MACHINE_PERCENT: f32 = 50.0;

/// System-wide CPU busy fraction below which the machine counts as quiet.
///
/// The gap up to [`BUSY_MACHINE_PERCENT`] is deliberate: a machine at 30% is
/// neither demonstrably busy nor demonstrably quiet, and the report says so
/// rather than rounding to whichever story is more convenient.
const QUIET_MACHINE_PERCENT: f32 = 25.0;

/// Share of one core below which this thread was demonstrably starved.
///
/// A CPU-bound thread on a quiet machine runs for most of the wall time it
/// occupies. Below half of one core it spent the majority of the span off the
/// processor, and no amount of extra sampler work explains that.
const STARVED_THREAD_PERCENT: f32 = 50.0;

/// This thread's own consumed CPU time, in 100 ns units.
///
/// The crate measures the machine ([`CpuSampler`]) and other processes
/// ([`ProcessEnumerator`]) but exposes no reading of *this* thread, and this is
/// the one figure that separates "the sampler did more work" from "the
/// scheduler gave the processor to somebody else": CPU time is what the thread
/// spent, wall time is what it waited for.
///
/// `GetThreadTimes` rather than `GetProcessTimes` or `QueryProcessCycleTime`:
/// process time would include the other tests `cargo test` runs beside this
/// one, and cycles would need a clock frequency to become a fraction of wall
/// time, which a boosting chip overstates. The kernel updates thread times on
/// its clock tick, so one reading is granular to about 15 ms — immaterial
/// summed over fifteen spans of a full sample, and not something to read into
/// a single one.
///
/// `None` when the kernel refuses the read: an unmeasured figure is absent,
/// never zero.
fn thread_cpu_100ns() -> Option<u64> {
    // `FILETIME` is `Copy` with a zeroed `Default`, so the four out-parameters
    // need no other preparation.
    let (mut creation, mut exit) = (FILETIME::default(), FILETIME::default());
    let (mut kernel, mut user) = (FILETIME::default(), FILETIME::default());

    // SAFETY: the current-thread pseudo-handle is always valid and needs no
    // closing, and every out-pointer references a live local for the duration
    // of the call.
    let ok = unsafe {
        GetThreadTimes(
            GetCurrentThread(),
            &raw mut creation,
            &raw mut exit,
            &raw mut kernel,
            &raw mut user,
        )
    };

    if ok == 0 {
        return None;
    }
    Some(to_ticks(kernel).saturating_add(to_ticks(user)))
}

/// Assembles a `FILETIME` into its 64-bit count of 100 ns units.
fn to_ticks(time: FILETIME) -> u64 {
    (u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime)
}

/// The mean utilisation of a machine's processors, or `None` for none.
///
/// An empty list is absence rather than a machine at 0%: the kernel returns no
/// processors only when the read itself failed, and reporting that as an idle
/// machine would be the exact conflation the report exists to avoid.
fn mean_busy_percent(per_core: &[CpuUsage]) -> Option<f32> {
    if per_core.is_empty() {
        return None;
    }
    let cores = per_core.len() as f32;
    Some(per_core.iter().map(|u| u.total.get()).sum::<f32>() / cores)
}

/// Converts a count of 100 ns units into a duration.
///
/// Saturating: a tick count large enough to overflow is decades of CPU time,
/// and a wrapped duration would report a thread that barely ran.
fn ticks_to_duration(ticks: u64) -> Duration {
    Duration::from_nanos(ticks.saturating_mul(100))
}

/// Machine readings taken around the timed spans.
///
/// Nothing here sleeps, and nothing here runs inside a timed span except the
/// two `GetThreadTimes` calls that bracket it: a counter-measurement that
/// slowed the thing it measures would be worse than no context at all.
#[derive(Debug)]
struct LoadProbe {
    /// Logical processors, from the OS.
    cores: usize,
    /// Machine-wide CPU utilisation: primed by `start`, read by `finish`.
    cpu: CpuSampler,
    /// This thread's own CPU time summed across the recorded spans, or `None`
    /// once any reading along the way was refused.
    own_cpu: Option<Duration>,
    /// Wall time the recorded spans covered.
    span: Duration,
}

impl LoadProbe {
    /// Takes the priming read of the machine-wide CPU counters.
    fn start() -> Self {
        let cores = logical_core_count();
        let mut cpu = CpuSampler::new(cores);
        // Every utilisation figure on Windows is a delta between two
        // cumulative snapshots, so the first read only establishes the
        // baseline. A failure here is deliberately not fatal: it surfaces as an
        // absent fraction in the report, and a budget gate should not fail for
        // its counter-measurement's sake.
        let _primed = cpu.sample();

        Self {
            cores,
            cpu,
            own_cpu: Some(Duration::ZERO),
            span: Duration::ZERO,
        }
    }

    /// Adds one measured span: its wall time, and this thread's own CPU time
    /// across it.
    ///
    /// Both CPU readings bracket the span the caller already times, so the
    /// counter-measurement performs no work inside it beyond the two syscalls.
    fn record(&mut self, cpu_before: Option<u64>, cpu_after: Option<u64>, wall: Duration) {
        self.span += wall;
        self.own_cpu = match (self.own_cpu, cpu_before, cpu_after) {
            (Some(accrued), Some(before), Some(after)) => {
                Some(accrued + ticks_to_duration(after.saturating_sub(before)))
            }
            // One unreadable reading makes the window's figure unmeasurable. A
            // partial sum would understate what the thread spent and so read as
            // a thread that barely worked.
            _ => None,
        };
    }

    /// Closes the window and returns the readings.
    ///
    /// The busy fraction is the mean of the per-processor utilisation the
    /// crate's own `CpuSampler` differences across the window. Method: it reads
    /// `NtQuerySystemInformation(SystemProcessorPerformanceInformation)` once
    /// here and once in `start`, so the fraction is a delta between two
    /// cumulative snapshots, normalised per processor exactly as the sampler's
    /// own `cpu.total` is. That is what a reader means by "the machine was
    /// busy", and obtaining it any other way would be a second opinion about
    /// somebody else's measurement. No `Win32` call of our own is involved.
    ///
    /// `processes` is whatever the caller's own enumeration reported. This
    /// probe deliberately does not enumerate: a second enumeration would be
    /// work no shipping tick performs.
    fn finish(mut self, processes: Option<u32>) -> LoadReport {
        let busy_percent = match self.cpu.sample() {
            Ok(per_core) => mean_busy_percent(&per_core),
            // Absent rather than zero: a fraction we could not read is not a
            // machine doing nothing.
            Err(_) => None,
        };

        LoadReport {
            cores: self.cores,
            processes,
            busy_percent,
            own_cpu: self.own_cpu,
            span: self.span,
        }
    }
}

/// The machine's state across one measured window.
#[derive(Debug)]
struct LoadReport {
    /// Logical processors.
    cores: usize,
    /// Processes the caller's own enumeration saw, when it saw any.
    processes: Option<u32>,
    /// Machine-wide busy fraction over the window.
    busy_percent: Option<f32>,
    /// This thread's own CPU time across the timed spans.
    own_cpu: Option<Duration>,
    /// Wall time those spans covered.
    span: Duration,
}

impl LoadReport {
    /// This thread's CPU time as a share of one core, across the timed spans.
    ///
    /// The discriminator the report exists for. A sampler that got slower does
    /// the same work in more time, so this ratio barely moves; a thread that
    /// lost the processor waits without spending, so it falls. `None` when the
    /// thread's own time could not be read, or when the spans covered no
    /// measurable wall time.
    fn own_core_percent(&self) -> Option<f32> {
        let own = self.own_cpu?;
        let wall = self.span.as_secs_f64();
        if wall <= 0.0 {
            return None;
        }
        Some((own.as_secs_f64() / wall * 100.0) as f32)
    }

    /// One line of context, printed beside the median whether the gate passes
    /// or fails.
    ///
    /// Unconditional on purpose. A number with no load beside it is the whole
    /// problem this exists to fix: `verify.ps1` failed here on a saturated
    /// workstation on 2026-11-20, against a commit whose CI was green, and
    /// nothing on screen let a reader tell a slower sampler from a busy
    /// machine.
    fn describe(&self) -> String {
        let cores = self.cores;
        let processes = self.processes.map_or_else(
            || "process count unavailable".to_owned(),
            |count| format!("{count} processes"),
        );
        let busy = self.busy_percent.map_or_else(
            || "system busy fraction unavailable".to_owned(),
            |busy| format!("system busy {busy:.1}% over the window"),
        );
        let own = match self.own_core_percent() {
            Some(own) => format!(
                "this thread used {own:.0}% of one core across {:.0}ms of timed spans",
                self.span.as_secs_f64() * 1000.0
            ),
            None => "this thread's own CPU time unavailable".to_owned(),
        };

        format!("load: {cores} logical processors, {processes}, {busy}, {own}")
    }

    /// The caveat that belongs beside a reading over budget.
    ///
    /// Only a machine that was measurably busy while starving this thread is
    /// called busy; only one that was measurably quiet while this thread ran is
    /// called quiet; anything between the two says it is ambiguous rather than
    /// picking a side. When a figure is missing the sentence says what could
    /// not be read, because silence about an unmeasured machine reads as a
    /// quiet one.
    fn attribution(&self) -> String {
        let cores = self.cores;
        match (self.busy_percent, self.own_core_percent()) {
            (Some(busy), Some(own))
                if busy >= BUSY_MACHINE_PERCENT && own < STARVED_THREAD_PERCENT =>
            {
                format!(
                    "The machine was demonstrably busy (system {busy:.0}% busy across {cores} \
                     logical processors) and this thread was starved of the CPU while sampling \
                     ({own:.0}% of one core), so the scheduler spent part of this time, not the \
                     sampler."
                )
            }
            (Some(busy), Some(own))
                if busy <= QUIET_MACHINE_PERCENT && own >= STARVED_THREAD_PERCENT =>
            {
                format!(
                    "The machine was demonstrably quiet (system {busy:.0}% busy across {cores} \
                     logical processors) and this thread had the CPU while sampling ({own:.0}% of \
                     one core), so this time is the sampler's own and the reading is likely a \
                     real regression."
                )
            }
            (Some(busy), Some(own)) => format!(
                "The evidence is ambiguous: the system was {busy:.0}% busy across {cores} logical \
                 processors while this thread used {own:.0}% of one core, which fits neither a \
                 starved thread on a busy machine nor a quiet machine, so this reading could be \
                 either."
            ),
            _ => "The machine's load could not be read, so nothing here distinguishes a slower \
                  sampler from a busy machine; re-run on a quiet machine before reading this as a \
                  regression."
                .to_owned(),
        }
    }
}

#[test]
fn a_full_sample_stays_within_budget() {
    let _guard = TIMING_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    let mut sampler = SystemSampler::new();
    sampler.sample().expect("prime");

    // Primed before the loop, so the busy fraction covers the same window the
    // timings do — gaps included.
    let mut load = LoadProbe::start();

    let mut timings = Vec::with_capacity(15);
    let mut processes = None;
    for _ in 0..15 {
        std::thread::sleep(DUTY_CYCLE_GAP);
        let cpu_before = thread_cpu_100ns();
        let start = Instant::now();
        let sample = sampler.sample().expect("sample");
        let elapsed = start.elapsed();
        load.record(cpu_before, thread_cpu_100ns(), elapsed);
        processes = sample.system.cpu.process_count;
        timings.push(elapsed);
    }

    let mid = median(timings.clone());
    let worst = timings.iter().max().copied().unwrap_or_default();
    let load = load.finish(processes);

    println!(
        "full sample: median {mid:?}, worst {worst:?}, budget {SAMPLE_BUDGET:?}; {}",
        load.describe()
    );

    assert!(
        mid <= SAMPLE_BUDGET,
        "sampling took {mid:?}, over the {SAMPLE_BUDGET:?} budget. \
         At 1 Hz that is {:.2}% of a core, and this app must not appear in \
         its own top-consumers list. {} {}",
        mid.as_secs_f64() * 100.0,
        load.attribution(),
        load.describe()
    );
}

#[test]
fn process_enumeration_stays_within_budget() {
    let _guard = TIMING_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    let mut enumerator = ProcessEnumerator::new();
    enumerator.enumerate().expect("prime");

    // Primed before the loop, so the busy fraction covers the same window the
    // timings do — gaps included.
    let mut load = LoadProbe::start();

    let mut timings = Vec::with_capacity(15);
    let mut processes = None;
    for _ in 0..15 {
        std::thread::sleep(DUTY_CYCLE_GAP);
        let cpu_before = thread_cpu_100ns();
        let start = Instant::now();
        let listed = enumerator.enumerate().expect("enumerate");
        let elapsed = start.elapsed();
        load.record(cpu_before, thread_cpu_100ns(), elapsed);
        processes = u32::try_from(listed.len()).ok();
        timings.push(elapsed);
        assert!(!listed.is_empty());
    }

    let mid = median(timings);
    let load = load.finish(processes);

    println!(
        "enumeration: median {mid:?}, budget {ENUMERATE_BUDGET:?}; {}",
        load.describe()
    );

    assert!(
        mid <= ENUMERATE_BUDGET,
        "process enumeration took {mid:?}, over the {ENUMERATE_BUDGET:?} budget. {} {}",
        load.attribution(),
        load.describe()
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
