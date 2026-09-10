//! The composite sampler: everything on one tick.
//!
//! Owns every per-subsystem sampler and produces a complete
//! [`SystemMetrics`] plus a process list per call. Holding them together
//! matters because they must share one notion of "now": if the CPU sampler
//! and the process sampler each measure their own interval, per-process
//! percentages will not sum to the machine total, and the discrepancy is
//! exactly the kind of thing users notice and report.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use vitals_core::error::Result;
use vitals_core::ids::ProcessKey;
use vitals_core::metrics::{
    BatteryMetrics, CpuMetrics, DiskMetrics, GpuEngine, GpuMetrics, GpuVendor, NetworkMetrics,
    SystemMetrics,
};
use vitals_core::units::{Bytes, BytesPerSec, Percent};

use crate::cpu::{CpuSampler, logical_core_count, process_cpu_percent};
use crate::disk::enumerate_volumes;
use crate::gpu::adapters::GpuSampler;
use crate::memory::MemorySampler;
use crate::network::{NetworkCounters, compute_rates as compute_net_rates, enumerate_adapters};
use crate::process::{ProcessEnumerator, RawProcess};

/// Per-process state carried between ticks, so rates can be differenced.
#[derive(Debug, Clone, Copy)]
struct ProcessBaseline {
    cpu_time: u64,
    /// Disk bytes from whichever counter `RawProcess::disk_read_bytes`
    /// selected — storage-stack where the kernel provides it, all-I/O
    /// otherwise. The enumerator fixes the source once per run, so a
    /// baseline and its successor always come from the same counter.
    read_bytes: u64,
    write_bytes: u64,
}

/// How often volume capacity is re-read.
///
/// Free space changes on human timescales, and `GetDiskFreeSpaceExW` touches
/// every mounted volume — including network shares, where a stalled server
/// can block for seconds. Re-reading it every tick spends real time on data
/// that is almost always identical.
///
/// Network counters are deliberately NOT in this tier: they are cumulative,
/// so throughput needs them every tick. Only their enumeration is expensive,
/// and the two cannot be separated without a second API.
const VOLUME_REFRESH_INTERVAL: Duration = Duration::from_secs(2);

/// One complete sample.
#[derive(Debug)]
pub struct Sample {
    pub system: SystemMetrics,
    /// Processes with rates already computed.
    pub processes: Vec<SampledProcess>,
    /// Real time covered by this sample.
    pub elapsed_ms: u32,
}

/// A process with its per-interval rates resolved.
#[derive(Debug, Clone)]
pub struct SampledProcess {
    pub raw: RawProcess,
    pub cpu: Percent,
    pub disk_read: BytesPerSec,
    pub disk_write: BytesPerSec,
    /// Share of GPU this process was responsible for, when the WDDM counters
    /// report it.
    ///
    /// `None` rather than zero on a machine with no GPU Engine counters. Zero
    /// would claim the process used no GPU, which is a different statement
    /// from not having measured.
    pub gpu: Option<Percent>,
}

/// Samples every subsystem on a shared clock.
#[derive(Debug)]
pub struct SystemSampler {
    cpu: CpuSampler,
    memory: MemorySampler,
    processes: ProcessEnumerator,
    logical_cores: u32,

    /// Previous per-process counters, keyed by race-free identity.
    ///
    /// Keyed on [`ProcessKey`] rather than PID so a recycled PID cannot
    /// inherit the previous occupant's baseline and report a nonsensical
    /// negative-then-saturated delta on its first tick.
    process_baseline: HashMap<ProcessKey, ProcessBaseline>,
    network_baseline: HashMap<u32, NetworkCounters>,

    last_tick: Option<Instant>,

    /// Cached volume list, refreshed on [`VOLUME_REFRESH_INTERVAL`].
    volumes: Vec<DiskMetrics>,
    volumes_read_at: Option<Instant>,

    /// Accumulated per-application usage.
    ///
    /// Lives here rather than beside the UI because it must see every tick,
    /// including the ones nobody is watching — history that only accrued
    /// while its own screen was open would be worthless.
    ///
    /// Shared rather than owned: the UI thread reads it when the App history
    /// screen asks, and this thread writes it every second. One `Arc` keeps
    /// there being exactly one history rather than two that disagree.
    history: crate::history::SharedHistory,

    /// GPU adapters and their engine utilisation.
    ///
    /// Holds an open PDH query, so it is constructed once and reused. Opening
    /// it costs ~285 ms; a tick costs ~1.1 ms.
    gpu: GpuSampler,
}

impl Default for SystemSampler {
    fn default() -> Self {
        Self::new()
    }
}

impl SystemSampler {
    #[must_use]
    pub fn new() -> Self {
        let cores = logical_core_count();
        Self {
            cpu: CpuSampler::new(cores),
            memory: MemorySampler::new(),
            processes: ProcessEnumerator::new(),
            logical_cores: u32::try_from(cores).unwrap_or(1),
            process_baseline: HashMap::with_capacity(512),
            network_baseline: HashMap::with_capacity(16),
            last_tick: None,
            volumes: Vec::new(),
            volumes_read_at: None,
            history: crate::history::SharedHistory::default(),
            gpu: GpuSampler::new(),
        }
    }

    /// A handle to the accumulated app history.
    #[must_use]
    pub fn history(&self) -> crate::history::SharedHistory {
        self.history.clone()
    }

    /// Discards every baseline.
    ///
    /// Call after a pause. Without it the first sample after resuming covers
    /// the whole paused interval and reports a spike that never happened.
    pub fn reset(&mut self) {
        self.cpu.reset();
        self.memory.reset();
        self.process_baseline.clear();
        self.network_baseline.clear();
        self.last_tick = None;
        // Drop the cache too: a pause is exactly when a disk gets plugged in
        // or a share disconnects.
        self.volumes.clear();
        self.volumes_read_at = None;
        // PDH holds its own baseline, so this reopens the query rather than
        // clearing a map. Without it the first sample after a resume covers
        // the whole paused interval.
        self.gpu.reset();
    }

    /// Takes one complete sample.
    ///
    /// The first call primes baselines and reports zero for every rate,
    /// which is honest: a rate needs two points in time.
    ///
    /// # Errors
    ///
    /// Propagates the first underlying sampler failure. Subsystems that fail
    /// individually — a disconnected disk, a vanishing adapter — degrade to
    /// empty rather than failing the tick.
    pub fn sample(&mut self) -> Result<Sample> {
        let now = Instant::now();
        // One clock for the whole tick. Measured before any sampling so
        // every subsystem is differenced over the same interval.
        let elapsed = self.last_tick.map_or(0, |prev| {
            u32::try_from(now.duration_since(prev).as_millis()).unwrap_or(u32::MAX)
        });
        self.last_tick = Some(now);

        let cpu_usage = self.cpu.sample()?;
        let memory = self.memory.sample(elapsed)?;
        let raw_processes = self.processes.enumerate()?;

        // CPU metrics need the aggregate thread and handle counts, so they
        // are computed before the process list is consumed below.
        let cpu = build_cpu_metrics(&cpu_usage, &raw_processes);

        // App history folds in from the same enumeration rather than running
        // its own, which would double the most expensive part of the tick for
        // data already in hand. It borrows the list before `resolve_process_
        // rates` consumes it, so nothing is cloned.
        self.history.record(&raw_processes);

        // GPU before processes: one PDH read produces both the per-adapter
        // engine breakdown and the per-process attribution, and reading it
        // twice would double the cost for data already in hand.
        let (gpus, gpu_by_process) = self.build_gpu_metrics(elapsed);

        let processes = self.resolve_process_rates(raw_processes, elapsed, &gpu_by_process);
        let disks = self.volumes(now);
        let networks = self.build_network_metrics(elapsed);

        Ok(Sample {
            system: SystemMetrics {
                cpu,
                memory,
                disks,
                networks,
                gpus,
                // System-wide power draw needs either a vendor SDK or an
                // EC/ACPI read behind a driver; neither exists here, so it
                // stays unavailable rather than becoming a plausible zero.
                power_draw: None,
                battery: build_battery_metrics(),
            },
            processes,
            elapsed_ms: elapsed,
        })
    }

    /// Returns the volume list, re-reading it only when the cache is stale.
    ///
    /// Clones rather than borrowing so the caller is not holding a borrow of
    /// `self` while the rest of the sample runs. A handful of volumes makes
    /// this immaterial next to the syscalls it avoids.
    fn volumes(&mut self, now: Instant) -> Vec<DiskMetrics> {
        let stale = self
            .volumes_read_at
            .is_none_or(|read_at| now.duration_since(read_at) >= VOLUME_REFRESH_INTERVAL);

        if stale {
            self.volumes = build_disk_metrics();
            self.volumes_read_at = Some(now);
        }

        self.volumes.clone()
    }

    /// Differences per-process counters against the previous tick.
    fn resolve_process_rates(
        &mut self,
        raw: Vec<RawProcess>,
        elapsed_ms: u32,
        gpu_by_process: &HashMap<u32, Percent>,
    ) -> Vec<SampledProcess> {
        // 100ns units, matching the kernel's CPU accounting.
        let elapsed_ticks = u64::from(elapsed_ms) * 10_000;

        // Whether the machine can report GPU attribution at all, which is a
        // different question from whether anything used the GPU this tick.
        let gpu_available = self.gpu.is_available();

        let mut out = Vec::with_capacity(raw.len());

        // Mark-and-sweep over the existing map rather than building a new
        // one. Reallocating a ~550-entry HashMap every second churned several
        // hundred KB/s for no benefit; `seen` lets exited processes still be
        // evicted without the allocation.
        let mut seen = Vec::with_capacity(raw.len());

        for process in raw {
            let previous = self.process_baseline.get(&process.key).copied();

            let cpu = match previous {
                // No baseline means the process is new, or this is the first
                // tick. Either way its lifetime CPU total is not a rate, and
                // reporting it would show every freshly launched process at
                // an absurd percentage.
                None => Percent::ZERO,
                Some(prev) => process_cpu_percent(
                    process.cpu_time().saturating_sub(prev.cpu_time),
                    elapsed_ticks,
                    self.logical_cores,
                ),
            };

            let (disk_read, disk_write) = match previous {
                None => (BytesPerSec::ZERO, BytesPerSec::ZERO),
                Some(prev) => (
                    BytesPerSec(per_second(
                        process.disk_read_bytes().0.saturating_sub(prev.read_bytes),
                        elapsed_ms,
                    )),
                    BytesPerSec(per_second(
                        process
                            .disk_write_bytes()
                            .0
                            .saturating_sub(prev.write_bytes),
                        elapsed_ms,
                    )),
                ),
            };

            self.process_baseline.insert(
                process.key,
                ProcessBaseline {
                    cpu_time: process.cpu_time(),
                    read_bytes: process.disk_read_bytes().0,
                    write_bytes: process.disk_write_bytes().0,
                },
            );
            seen.push(process.key);

            // Moved, not cloned. Each RawProcess owns a String, so cloning
            // the list meant ~550 heap allocations per tick purely to hand
            // the same data onwards.
            out.push(SampledProcess {
                // Looked up before the move, since `process` is consumed
                // below. `None` means no GPU counters exist on this machine;
                // `Some(0)` means they do and this process did no GPU work.
                // An empty map cannot distinguish the two — an idle GPU also
                // produces no rows — so availability is asked of the sampler
                // rather than inferred from the data.
                gpu: gpu_available.then(|| {
                    gpu_by_process
                        .get(&process.key.pid.get())
                        .copied()
                        .unwrap_or(Percent::ZERO)
                }),
                raw: process,
                cpu,
                disk_read,
                disk_write,
            });
        }

        // Evict processes that have exited, or the map grows without bound
        // on a machine that churns processes (any build server).
        if self.process_baseline.len() != seen.len() {
            seen.sort_unstable_by_key(|k| (k.pid.get(), k.start_time));
            self.process_baseline.retain(|key, _| {
                seen.binary_search_by_key(&(key.pid.get(), key.start_time), |k| {
                    (k.pid.get(), k.start_time)
                })
                .is_ok()
            });
        }

        out
    }

    /// Enumerates GPU adapters and their engine utilisation.
    ///
    /// Memory, clocks, fan and power are left `None` rather than zero: they
    /// need a vendor SDK, and a zero here would be indistinguishable from a
    /// GPU genuinely sitting idle at 0 MHz.
    ///
    /// Also returns per-process utilisation, keyed by PID, because the same
    /// PDH read produces both and the process list needs it.
    fn build_gpu_metrics(&mut self, elapsed_ms: u32) -> (Vec<GpuMetrics>, HashMap<u32, Percent>) {
        // 100ns units, matching the kernel's accounting elsewhere. Unused by
        // the PDH source, which does its own rate arithmetic, but part of the
        // sampler contract.
        let elapsed_ticks = u64::from(elapsed_ms) * 10_000;

        let (adapters, samples) = self.gpu.sample_with_processes(elapsed_ticks);

        let by_process = crate::gpu::counters::total_by_process(&samples)
            .into_iter()
            .map(|(pid, total)| (pid.get(), crate::gpu::adapters::percent_clamped(total)))
            .collect();

        let metrics = adapters
            .into_iter()
            .map(|adapter| {
                // The headline is the busiest primary engine, not a blend.
                // A machine transcoding video is genuinely 100% busy on the
                // encode engine while 3D is idle, and averaging them would
                // report 50% — a number describing neither.
                //
                // `None`, not zero, when there are no engines to take the
                // max of: an adapter with no counters is unmeasured, and the
                // machine's phantom display adapters were reporting 0 %
                // beside a real GPU at 16 %.
                let utilization = adapter
                    .engines
                    .iter()
                    .filter(|engine| engine.kind.is_primary_workload())
                    .map(|engine| engine.utilisation)
                    .max_by(|a, b| a.get().total_cmp(&b.get()));

                GpuMetrics {
                    id: adapter.id,
                    name: adapter.name.clone(),
                    vendor: vendor_from_name(&adapter.name),
                    engines: adapter
                        .engines
                        .iter()
                        .map(|engine| GpuEngine {
                            name: engine.kind.slug().to_owned(),
                            utilization: engine.utilisation,
                        })
                        .collect(),
                    utilization,
                    memory_used: None,
                    memory_total: adapter.dedicated_memory,
                    shared_memory_used: None,
                    core_clock: None,
                    memory_clock: None,
                    temperature: None,
                    hotspot_temperature: None,
                    power: None,
                    power_limit: None,
                    fan_percent: None,
                    fan_rpm: None,
                    throttled: None,
                    driver_version: None,
                }
            })
            .collect();

        (metrics, by_process)
    }

    /// Differences network counters and builds per-interface metrics.
    fn build_network_metrics(&mut self, elapsed_ms: u32) -> Vec<NetworkMetrics> {
        let adapters = enumerate_adapters();
        let mut next_baseline = HashMap::with_capacity(adapters.len());
        let mut out = Vec::with_capacity(adapters.len());

        for adapter in adapters {
            let previous = self.network_baseline.get(&adapter.id.get()).copied();
            next_baseline.insert(adapter.id.get(), adapter.counters);

            let rates = previous.map_or(crate::network::NetworkRates::ZERO, |prev| {
                compute_net_rates(prev, adapter.counters, u64::from(elapsed_ms))
            });

            out.push(NetworkMetrics {
                id: adapter.id,
                name: adapter.alias,
                adapter: Some(adapter.description),
                kind: adapter.kind,
                rx: rates.rx,
                tx: rates.tx,
                rx_total: Bytes(adapter.counters.bytes_received),
                tx_total: Bytes(adapter.counters.bytes_sent),
                link_speed: adapter.link_speed,
                // Addresses need a separate GetAdaptersAddresses call, which
                // is slow. Sampled on a coarser cadence, not per tick.
                ipv4: None,
                ipv6: None,
                mac: adapter.mac,
                connected: adapter.connected,
                signal: None,
                ssid: None,
                errors_per_sec: Some(rates.errors_per_sec),
            });
        }

        self.network_baseline = next_baseline;
        out
    }
}

/// Builds machine-wide CPU metrics from per-core usage.
/// Reads battery state, or `None` on a machine that has no battery.
///
/// On the tick rather than behind the 5-second sensor cache because this is
/// `GetSystemPowerStatus`, not WMI: measured at 0.01 ms warm. The sensor
/// cache exists for the thermal reads that cost tens of milliseconds, and
/// putting a free call behind it would only make the charge figure stale.
///
/// Health, cycle count, temperature and instantaneous draw are left `None`.
/// They come from the battery's own IOCTL interface, which is a separate
/// read against a device handle — worth doing, but not worth faking here.
fn build_battery_metrics() -> Option<BatteryMetrics> {
    battery_from_aggregate(crate::sensors::power::aggregate_battery()?)
}

/// The pure half of [`build_battery_metrics`], so the decisions below can be
/// tested on a machine that has no battery — such as the one this was written
/// on.
fn battery_from_aggregate(
    aggregate: crate::sensors::power::AggregateBattery,
) -> Option<BatteryMetrics> {
    // Charge as a share of full capacity. Both figures are in the same
    // driver-defined unit, which cancels — so this is a ratio even though
    // neither number means anything on its own.
    let charge = match (aggregate.remaining_capacity, aggregate.max_capacity) {
        (Some(remaining), Some(max)) if max > 0 => {
            Percent::ratio(u64::from(remaining), u64::from(max))
        }
        // A battery present but not reporting capacity is real — some
        // firmware only exposes it while discharging. Reporting zero would
        // say "flat", so the whole reading is withheld instead.
        _ => return None,
    };

    Some(BatteryMetrics {
        charge,
        charging: aggregate.charging,
        time_remaining_secs: aggregate.estimated_seconds.map(u64::from),
        power: None,
        health: None,
        cycle_count: None,
        temperature: None,
    })
}

/// Infers the vendor from the driver-reported adapter name.
///
/// A string match, because the alternative — reading the PCI vendor ID —
/// means a registry walk per adapter for a field used only to pick an icon.
/// Anything unrecognised is `Unknown` rather than guessed: a wrong vendor
/// badge is a small lie, but it is still a lie.
fn vendor_from_name(name: &str) -> GpuVendor {
    let lower = name.to_ascii_lowercase();

    if lower.contains("nvidia") || lower.contains("geforce") || lower.contains("quadro") {
        GpuVendor::Nvidia
    } else if lower.contains("amd") || lower.contains("radeon") {
        GpuVendor::Amd
    } else if lower.contains("intel") || lower.contains("arc ") {
        GpuVendor::Intel
    } else if lower.contains("qualcomm") || lower.contains("adreno") {
        GpuVendor::Qualcomm
    } else if lower.contains("apple") {
        GpuVendor::Apple
    } else {
        GpuVendor::Unknown
    }
}

fn build_cpu_metrics(per_core: &[crate::cpu::CpuUsage], processes: &[RawProcess]) -> CpuMetrics {
    let cores = per_core.len().max(1) as f32;

    let total = Percent::new(per_core.iter().map(|u| u.total.get()).sum::<f32>() / cores);
    let kernel = Percent::new(per_core.iter().map(|u| u.kernel.get()).sum::<f32>() / cores);

    let thread_count: u32 = processes.iter().map(|p| p.thread_count).sum();
    let handle_count: u32 = processes.iter().map(|p| p.handle_count).sum();

    CpuMetrics {
        total,
        per_core: per_core.iter().map(|u| u.total).collect(),
        kernel,
        // Clocks, temperature, power and throttle state need MSR or vendor
        // access. Absent rather than guessed.
        effective_clock: None,
        max_clock: None,
        temperature: None,
        power: None,
        throttled: None,
        process_count: u32::try_from(processes.len()).unwrap_or(u32::MAX),
        thread_count,
        handle_count: Some(handle_count),
        uptime_secs: uptime_secs(),
        context_switches: None,
        interrupts: None,
    }
}

/// Builds volume metrics.
///
/// Throughput is absent until the physical-disk layer lands: capacity comes
/// from the volume, activity from the physical device, and the two are not
/// the same object.
fn build_disk_metrics() -> Vec<DiskMetrics> {
    enumerate_volumes()
        .into_iter()
        .map(|v| DiskMetrics {
            id: v.id,
            name: v.label.clone().unwrap_or_else(|| v.mount.clone()),
            model: None,
            mount: Some(v.mount),
            kind: v.kind,
            total: v.total,
            free: v.available,
            read: BytesPerSec::ZERO,
            write: BytesPerSec::ZERO,
            active_time: Percent::ZERO,
            response_ms: None,
            queue_depth: None,
            temperature: None,
            health: None,
        })
        .collect()
}

/// System uptime in seconds.
fn uptime_secs() -> u64 {
    // SAFETY: no arguments, no preconditions; returns milliseconds since boot.
    unsafe { windows_sys::Win32::System::SystemInformation::GetTickCount64() / 1000 }
}

/// Converts a per-interval delta into a per-second rate.
#[inline]
fn per_second(delta: u64, elapsed_ms: u32) -> u64 {
    if elapsed_ms == 0 {
        return 0;
    }
    let scaled = u128::from(delta).saturating_mul(1000);
    u64::try_from(scaled / u128::from(elapsed_ms)).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;
    use std::time::Duration;

    fn aggregate(
        remaining: Option<u32>,
        max: Option<u32>,
    ) -> crate::sensors::power::AggregateBattery {
        crate::sensors::power::AggregateBattery {
            on_ac: false,
            charging: false,
            discharging: true,
            max_capacity: max,
            remaining_capacity: remaining,
            estimated_seconds: Some(3600),
        }
    }

    #[test]
    fn battery_charge_is_a_ratio_of_the_driver_units() {
        // Neither number means anything on its own — the unit is
        // driver-defined — but it cancels in the ratio.
        let metrics = battery_from_aggregate(aggregate(Some(2500), Some(5000)))
            .expect("a battery reporting capacity must produce metrics");

        assert!((metrics.charge.get() - 50.0).abs() < 0.01);
        assert_eq!(metrics.time_remaining_secs, Some(3600));
    }

    #[test]
    fn a_battery_that_will_not_report_capacity_is_withheld_not_zeroed() {
        // Some firmware only exposes capacity while discharging. Reporting
        // zero would tell the user their battery is flat, which is a much
        // worse answer than showing nothing.
        assert!(battery_from_aggregate(aggregate(None, Some(5000))).is_none());
        assert!(battery_from_aggregate(aggregate(Some(2500), None)).is_none());
        assert!(battery_from_aggregate(aggregate(None, None)).is_none());
    }

    #[test]
    fn a_zero_capacity_battery_does_not_divide_by_zero() {
        // A dying or misreporting battery can report a full capacity of zero.
        assert!(battery_from_aggregate(aggregate(Some(0), Some(0))).is_none());
    }

    #[test]
    fn health_and_cycle_count_are_unavailable_rather_than_invented() {
        // They need the battery's own IOCTL interface. `None` says "not
        // read"; a zero would say "worn out" and "never charged".
        let metrics = battery_from_aggregate(aggregate(Some(100), Some(100))).expect("metrics");

        assert!(metrics.health.is_none());
        assert!(metrics.cycle_count.is_none());
        assert!(metrics.temperature.is_none());
        assert!(metrics.power.is_none());
    }

    #[test]
    fn the_first_sample_reports_no_rates() {
        // A rate needs two points in time. Reporting a process's lifetime
        // CPU total on the first tick would show everything at 100%.
        let mut sampler = SystemSampler::new();
        let sample = sampler.sample().expect("first sample");

        assert_eq!(sample.elapsed_ms, 0);
        assert!(sample.processes.iter().all(|p| p.cpu == Percent::ZERO));
    }

    #[test]
    fn the_second_sample_produces_rates() {
        let mut sampler = SystemSampler::new();
        sampler.sample().expect("prime");
        sleep(Duration::from_millis(300));
        let sample = sampler.sample().expect("second");

        assert!(
            sample.elapsed_ms >= 250,
            "elapsed was {}",
            sample.elapsed_ms
        );
        assert!(!sample.processes.is_empty());
    }

    #[test]
    fn every_metric_is_within_its_valid_range() {
        let mut sampler = SystemSampler::new();
        sampler.sample().expect("prime");
        sleep(Duration::from_millis(200));
        let sample = sampler.sample().expect("sample");

        assert!((0.0..=100.0).contains(&sample.system.cpu.total.get()));
        assert!((0.0..=100.0).contains(&sample.system.cpu.kernel.get()));
        for p in &sample.processes {
            assert!(
                (0.0..=100.0).contains(&p.cpu.get()),
                "{} reported {}% CPU",
                p.raw.name.as_deref().unwrap_or("?"),
                p.cpu
            );
        }
    }

    #[test]
    fn process_cpu_sums_to_roughly_the_machine_total() {
        // The consistency check that matters. If the samplers used separate
        // clocks these would diverge, and a user comparing the Processes tab
        // against the CPU graph would see two different numbers.
        let mut sampler = SystemSampler::new();
        sampler.sample().expect("prime");
        sleep(Duration::from_millis(500));
        let sample = sampler.sample().expect("sample");

        let process_sum: f32 = sample
            .processes
            .iter()
            .filter(|p| !p.raw.is_idle_process())
            .map(|p| p.cpu.get())
            .sum();
        let machine_total = sample.system.cpu.total.get();

        // Generous tolerance: short-lived processes that started and exited
        // inside the interval are counted by the kernel's per-CPU totals but
        // are absent from both snapshots, so the sum is legitimately a little
        // low. An order-of-magnitude gap would mean a real bug.
        assert!(
            (process_sum - machine_total).abs() < 25.0,
            "process sum {process_sum:.1}% vs machine total {machine_total:.1}%"
        );
    }

    #[test]
    fn reset_discards_baselines() {
        let mut sampler = SystemSampler::new();
        sampler.sample().expect("prime");
        sleep(Duration::from_millis(100));
        sampler.reset();

        let sample = sampler.sample().expect("after reset");
        assert_eq!(sample.elapsed_ms, 0, "the clock should restart");
        assert!(sample.processes.iter().all(|p| p.cpu == Percent::ZERO));
    }

    #[test]
    fn the_process_baseline_does_not_grow_without_bound() {
        // Exited processes must be evicted, or the map leaks on any machine
        // that churns processes.
        let mut sampler = SystemSampler::new();
        sampler.sample().expect("prime");
        let after_first = sampler.process_baseline.len();

        for _ in 0..5 {
            sleep(Duration::from_millis(20));
            sampler.sample().expect("sample");
        }

        let after_many = sampler.process_baseline.len();
        assert!(
            after_many < after_first * 2,
            "baseline grew from {after_first} to {after_many}"
        );
    }

    #[test]
    fn volumes_and_adapters_are_present() {
        let mut sampler = SystemSampler::new();
        let sample = sampler.sample().expect("sample");

        assert!(!sample.system.disks.is_empty(), "no volumes");
        assert!(!sample.system.networks.is_empty(), "no adapters");
    }

    #[test]
    fn uptime_is_plausible() {
        let up = uptime_secs();
        assert!(up > 0);
        // 10 years. A machine reporting more has a broken tick count.
        assert!(up < 10 * 365 * 24 * 3600, "uptime was {up}s");
    }

    #[test]
    fn per_second_handles_a_zero_interval() {
        assert_eq!(per_second(1_000, 0), 0);
        assert_eq!(per_second(1_000, 1_000), 1_000);
        assert_eq!(per_second(1_000, 500), 2_000);
    }
}
