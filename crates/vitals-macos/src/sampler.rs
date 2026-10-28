//! One tick of machine-wide and per-process readings on macOS.

use std::collections::HashMap;
use std::time::Instant;

use sysinfo::{
    Components, DiskKind as SysDiskKind, DiskRefreshKind, Disks, MemoryRefreshKind, Networks,
    ProcessRefreshKind, ProcessStatus, ProcessesToUpdate, RefreshKind, System, UpdateKind, Users,
};
use vitals_core::ids::{DiskId, NicId, Pid, ProcessKey};
use vitals_core::metrics::{
    CpuMetrics, DiskKind, DiskMetrics, FanMetrics, MemoryMetrics, NetworkKind, NetworkMetrics,
    SystemMetrics,
};
use vitals_core::process::{Process, ProcessFlags, ProcessKind, ProcessState, ProtectionLevel};
use vitals_core::units::{Bytes, BytesPerSec, Celsius, Percent};

use crate::mach::{self, CoreTicks};

/// What one tick produced.
#[derive(Debug, Clone)]
pub struct Sample {
    pub system: SystemMetrics,
    pub processes: Vec<Process>,
    pub elapsed_ms: u32,
}

/// Previous cumulative disk bytes of one process, for its rate.
#[derive(Debug, Clone, Copy)]
struct IoBaseline {
    start_us: u64,
    read: u64,
    written: u64,
}

/// Samples the machine. Holds the baselines every rate is computed against.
#[derive(Debug)]
pub struct SystemSampler {
    sys: System,
    disks: Disks,
    networks: Networks,
    components: Components,
    users: Users,
    ticks: Option<Vec<CoreTicks>>,
    faults: Option<u64>,
    io: HashMap<u32, IoBaseline>,
    last: Option<Instant>,
    logical_cores: f32,
    own_uid: u32,
}

impl Default for SystemSampler {
    fn default() -> Self {
        Self::new()
    }
}

impl SystemSampler {
    #[must_use]
    pub fn new() -> Self {
        let sys = System::new_with_specifics(
            RefreshKind::nothing().with_memory(MemoryRefreshKind::everything()),
        );
        let logical_cores = std::thread::available_parallelism()
            .map_or(1.0, |n| n.get() as f32)
            .max(1.0);
        Self {
            sys,
            disks: Disks::new_with_refreshed_list_specifics(disk_refresh()),
            networks: Networks::new_with_refreshed_list(),
            components: Components::new_with_refreshed_list(),
            users: Users::new_with_refreshed_list(),
            ticks: mach::core_ticks(),
            faults: mach::vm_stats().map(|v| v.faults),
            io: HashMap::new(),
            last: None,
            logical_cores,
            // SAFETY: getuid cannot fail.
            own_uid: unsafe { libc::getuid() },
        }
    }

    /// Drops every rate baseline, so the first tick after a pause does not
    /// report the whole pause as one second of activity.
    pub fn reset(&mut self) {
        self.ticks = mach::core_ticks();
        self.faults = mach::vm_stats().map(|v| v.faults);
        self.io.clear();
        self.last = None;
        self.networks.refresh(true);
        self.disks.refresh_specifics(true, disk_refresh());
    }

    /// Samples everything once.
    ///
    /// # Errors
    ///
    /// Fails only when the CPU tick counters cannot be read, the one source
    /// without which a frame would be meaningless.
    pub fn sample(&mut self) -> vitals_core::Result<Sample> {
        let now = Instant::now();
        let elapsed = self
            .last
            .map_or(0.0, |t| now.duration_since(t).as_secs_f64());
        self.last = Some(now);
        let elapsed_ms = (elapsed * 1000.0) as u32;

        let cpu = self.cpu()?;
        let memory = self.memory(elapsed);
        let processes = self.processes(elapsed);

        self.disks.refresh_specifics(true, disk_refresh());
        self.networks.refresh(true);
        self.components.refresh(true);

        let thread_count = processes.iter().map(|p| p.thread_count).sum();
        let system = SystemMetrics {
            cpu: CpuMetrics {
                process_count: u32::try_from(processes.len()).unwrap_or(u32::MAX),
                thread_count,
                temperature: self.cpu_temperature(),
                ..cpu
            },
            memory,
            disks: self.disks_metrics(elapsed),
            networks: self.network_metrics(elapsed),
            // GPU utilisation lives behind undocumented IOAccelerator keys
            // (guideline 2.5.1 risk); an empty list means "not measured".
            gpus: Vec::new(),
            power_draw: None,
            battery: None,
            fans: fans(),
        };

        Ok(Sample {
            system,
            processes,
            elapsed_ms,
        })
    }

    fn cpu(&mut self) -> vitals_core::Result<CpuMetrics> {
        let current = mach::core_ticks().ok_or_else(|| vitals_core::Error::Os {
            context: "host_processor_info".into(),
            code: 0,
        })?;
        let shares = self
            .ticks
            .as_deref()
            .map(|prev| mach::core_shares(prev, &current))
            .unwrap_or_default();
        self.ticks = Some(current);
        let n = shares.len().max(1) as f32;
        let per_core: Vec<Percent> = shares.iter().map(|(b, _)| Percent::new(*b)).collect();
        let total = shares.iter().map(|(b, _)| b).sum::<f32>() / n;
        let kernel = shares.iter().map(|(_, k)| k).sum::<f32>() / n;

        Ok(CpuMetrics {
            total: Percent::new(total),
            per_core,
            kernel: Percent::new(kernel),
            // Apple Silicon publishes no live clock through a public API, and
            // Intel only the nominal figure; nominal is not "effective".
            effective_clock: None,
            max_clock: mach::sysctl_u64(c"hw.cpufrequency_max")
                .filter(|&hz| hz > 0)
                .map(vitals_core::units::Hertz),
            temperature: None,
            power: None,
            throttled: None,
            process_count: 0,
            thread_count: 0,
            handle_count: None,
            uptime_secs: System::uptime(),
            context_switches: None,
            interrupts: None,
        })
    }

    fn memory(&mut self, elapsed: f64) -> MemoryMetrics {
        self.sys
            .refresh_memory_specifics(MemoryRefreshKind::nothing().with_ram());
        let total = self.sys.total_memory();
        let vm = mach::vm_stats();
        // Activity Monitor's arithmetic: "used" is app memory + wired +
        // compressed, "cached" is file-backed and purgeable pages. sysinfo's
        // own used figure counts cache as used, which on a Mac that has been
        // up for a day reads as permanently full.
        let (used, cached, available) = vm.map_or_else(
            || {
                let avail = self.sys.available_memory();
                (total.saturating_sub(avail), 0, avail)
            },
            |v| {
                let page = v.page_size;
                let app = v
                    .active
                    .saturating_add(v.inactive)
                    .saturating_add(v.speculative)
                    .saturating_sub(v.external)
                    .saturating_sub(v.purgeable);
                let used = (app + v.wired + v.compressed) * page;
                let cached = (v.external + v.purgeable) * page;
                (
                    used.min(total),
                    cached,
                    total.saturating_sub(used.min(total)),
                )
            },
        );
        let swap = mach::swap_usage();
        let faults = vm.map(|v| v.faults);
        let page_faults_per_sec = match (self.faults, faults) {
            (Some(before), Some(after)) if elapsed > 0.0 => {
                Some((after.saturating_sub(before) as f64 / elapsed) as u64)
            }
            _ => None,
        };
        self.faults = faults;

        MemoryMetrics {
            total: Bytes(total),
            used: Bytes(used),
            available: Bytes(available),
            cached: Bytes(cached),
            // No commit charge, no pool split on macOS.
            paged_pool: None,
            non_paged_pool: None,
            committed: None,
            commit_limit: None,
            swap_total: Bytes(swap.map_or(0, |s| s.0)),
            swap_used: swap.map(|s| Bytes(s.1)),
            hardware_reserved: None,
            page_faults_per_sec,
            speed: None,
            slots_used: None,
            slots_total: None,
            form_factor: None,
        }
    }

    fn processes(&mut self, elapsed: f64) -> Vec<Process> {
        self.sys.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing()
                .with_cpu()
                .with_memory()
                .with_user(UpdateKind::OnlyIfNotSet),
        );
        let mut next_io = HashMap::with_capacity(self.io.len());
        let mut out = Vec::with_capacity(self.sys.processes().len());

        for (pid, p) in self.sys.processes() {
            let pid = pid.as_u32();
            // kernel_task (PID 0) is the kernel itself: like the Windows idle
            // process it would top every CPU list. An unprivileged sysinfo
            // never lists it (verified 2026-10-28, macOS 26.7), so no test can
            // exercise this line without root; it guards a root-run CLI.
            if pid == 0 {
                continue;
            }
            let start_us = mach::start_time_us(pid).unwrap_or(p.start_time() * 1_000_000);
            let task = mach::task_info(pid);
            let rusage = mach::rusage(pid);

            let (disk_read, disk_write) = match rusage {
                Some(r) => {
                    let rate = match self.io.get(&pid) {
                        Some(prev) if prev.start_us == start_us && elapsed > 0.0 => (
                            per_sec(r.disk_read.saturating_sub(prev.read), elapsed),
                            per_sec(r.disk_written.saturating_sub(prev.written), elapsed),
                        ),
                        _ => (BytesPerSec::ZERO, BytesPerSec::ZERO),
                    };
                    next_io.insert(
                        pid,
                        IoBaseline {
                            start_us,
                            read: r.disk_read,
                            written: r.disk_written,
                        },
                    );
                    rate
                }
                None => (BytesPerSec::ZERO, BytesPerSec::ZERO),
            };

            let uid = p.user_id().map(|u| **u);
            let user = p
                .user_id()
                .and_then(|u| self.users.get_user_by_id(u))
                .map(|u| u.name().to_owned());
            let state = match p.status() {
                ProcessStatus::Stop => ProcessState::Suspended,
                ProcessStatus::Zombie => ProcessState::Zombie,
                ProcessStatus::Sleep | ProcessStatus::Idle => ProcessState::Waiting,
                _ => ProcessState::Running,
            };
            let mut flags = ProcessFlags::empty();
            if uid == Some(0) {
                flags = flags.union(ProcessFlags::ELEVATED);
            }
            // launchd: ending it logs everyone out.
            if pid == 1 {
                flags = flags.union(ProcessFlags::CRITICAL);
            }
            let kind = if uid.is_some_and(|u| u == 0 || u < 500) || pid == 1 {
                ProcessKind::System
            } else if uid == Some(self.own_uid) {
                ProcessKind::App
            } else {
                ProcessKind::Background
            };

            out.push(Process {
                key: ProcessKey::new(Pid(pid), start_us),
                parent: p.parent().map(|pp| Pid(pp.as_u32())),
                name: p.name().to_string_lossy().into_owned(),
                description: None,
                kind,
                state,
                flags,
                integrity: None,
                protection: ProtectionLevel::None,
                // sysinfo reports a share of one core (up to N x 100); the
                // model is a share of the whole machine, like Task Manager.
                cpu: Percent::new(p.cpu_usage() / self.logical_cores),
                memory_private: Bytes(rusage.map_or_else(|| p.memory(), |r| r.footprint)),
                memory_working_set: Bytes(task.map_or_else(|| p.memory(), |t| t.resident)),
                disk_read,
                disk_write,
                net_rx: None,
                net_tx: None,
                gpu: None,
                gpu_memory: None,
                thread_count: task.map_or(0, |t| t.threads),
                handle_count: None,
                user,
                uptime_secs: p.run_time(),
            });
        }
        self.io = next_io;
        out
    }

    fn disks_metrics(&self, elapsed: f64) -> Vec<DiskMetrics> {
        let mut seen = std::collections::HashSet::new();
        self.disks
            .list()
            .iter()
            // APFS shows the same container through several system volumes
            // (/System/Volumes/Data, VM, Preboot…). The user sees one disk.
            .filter(|d| {
                let mount = d.mount_point().to_string_lossy();
                mount == "/" || !mount.starts_with("/System/Volumes")
            })
            .filter(|d| seen.insert(d.name().to_os_string()))
            .enumerate()
            .map(|(i, d)| {
                let usage = d.usage();
                let kind = if d.is_removable() {
                    DiskKind::Removable
                } else {
                    match d.kind() {
                        SysDiskKind::SSD => DiskKind::Ssd,
                        SysDiskKind::HDD => DiskKind::Hdd,
                        SysDiskKind::Unknown(_) => DiskKind::Unknown,
                    }
                };
                DiskMetrics {
                    id: DiskId(u32::try_from(i).unwrap_or(u32::MAX)),
                    name: d.name().to_string_lossy().into_owned(),
                    model: None,
                    mount: Some(d.mount_point().to_string_lossy().into_owned()),
                    kind,
                    total: Bytes(d.total_space()),
                    free: Bytes(d.available_space()),
                    read: per_sec(usage.read_bytes, elapsed),
                    write: per_sec(usage.written_bytes, elapsed),
                    // No public busy-time counter on macOS.
                    active_time: Percent::ZERO,
                    response_ms: None,
                    queue_depth: None,
                    temperature: None,
                    health: None,
                }
            })
            .collect()
    }

    fn network_metrics(&self, elapsed: f64) -> Vec<NetworkMetrics> {
        let mut out: Vec<NetworkMetrics> = self
            .networks
            .iter()
            .filter(|(name, _)| !name.starts_with("lo"))
            .enumerate()
            .map(|(i, (name, data))| {
                let mac = data.mac_address().to_string();
                let ipv4 = data
                    .ip_networks()
                    .iter()
                    .find(|n| n.addr.is_ipv4())
                    .map(|n| n.addr.to_string());
                let ipv6 = data
                    .ip_networks()
                    .iter()
                    .find(|n| n.addr.is_ipv6())
                    .map(|n| n.addr.to_string());
                NetworkMetrics {
                    id: NicId(u32::try_from(i).unwrap_or(u32::MAX)),
                    name: name.clone(),
                    adapter: None,
                    kind: interface_kind(name),
                    rx: per_sec(data.received(), elapsed),
                    tx: per_sec(data.transmitted(), elapsed),
                    rx_total: Bytes(data.total_received()),
                    tx_total: Bytes(data.total_transmitted()),
                    link_speed: None,
                    connected: ipv4.is_some() || ipv6.is_some(),
                    ipv4,
                    ipv6,
                    mac: (mac != "00:00:00:00:00:00").then_some(mac),
                    signal: None,
                    ssid: None,
                    errors_per_sec: None,
                }
            })
            .collect();
        out.sort_by_key(|n| std::cmp::Reverse(n.connected));
        out
    }

    fn cpu_temperature(&self) -> Option<Celsius> {
        if crate::APP_STORE {
            return None;
        }
        self.components
            .iter()
            .filter(|c| {
                let l = c.label().to_ascii_lowercase();
                l.contains("cpu") || l.contains("pmu tdie") || l.contains("soc")
            })
            .filter_map(sysinfo::Component::temperature)
            .filter(|t| t.is_finite() && *t > 0.0)
            .reduce(f32::max)
            .map(Celsius)
    }
}

/// sysinfo exposes temperatures only; fan RPM needs the SMC keys directly.
/// Empty means "not measured", never "no fans".
fn fans() -> Vec<FanMetrics> {
    Vec::new()
}

fn disk_refresh() -> DiskRefreshKind {
    DiskRefreshKind::nothing()
        .with_storage()
        .with_io_usage()
        .with_kind()
}

fn per_sec(bytes: u64, elapsed: f64) -> BytesPerSec {
    if elapsed <= 0.0 {
        return BytesPerSec::ZERO;
    }
    BytesPerSec((bytes as f64 / elapsed) as u64)
}

/// Interface kind from the BSD name. macOS names are stable: `en0` is the
/// built-in Wi-Fi on laptops and Ethernet on desktops, which the name alone
/// cannot tell apart, so `en*` is `Unknown` rather than a guess.
fn interface_kind(name: &str) -> NetworkKind {
    if name.starts_with("lo") {
        NetworkKind::Loopback
    } else if name.starts_with("utun") || name.starts_with("ipsec") || name.starts_with("ppp") {
        NetworkKind::Vpn
    } else if name.starts_with("bridge")
        || name.starts_with("vmenet")
        || name.starts_with("awdl")
        || name.starts_with("llw")
        || name.starts_with("anpi")
        || name.starts_with("ap")
        || name.starts_with("gif")
        || name.starts_with("stf")
    {
        NetworkKind::Virtual
    } else {
        NetworkKind::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn two_samples() -> Sample {
        let mut s = SystemSampler::new();
        s.sample().expect("first sample");
        std::thread::sleep(std::time::Duration::from_millis(300));
        s.sample().expect("second sample")
    }

    #[test]
    fn the_machine_reports_cores_memory_and_processes() {
        let sample = two_samples();
        let sys = &sample.system;
        assert!(!sys.cpu.per_core.is_empty());
        assert!(sys.cpu.total.get() >= 0.0 && sys.cpu.total.get() <= 100.0);
        assert!(sys.memory.total.get() > 0);
        assert!(sys.memory.used.get() <= sys.memory.total.get());
        assert_eq!(sys.cpu.process_count as usize, sample.processes.len());
        if crate::APP_STORE {
            // sysinfo's apple-app-store feature never enumerates other
            // processes: the sandbox build must not invent any.
            assert!(sample.processes.is_empty(), "{}", sample.processes.len());
        } else {
            assert!(sample.processes.len() > 10, "{}", sample.processes.len());
        }
    }

    #[test]
    fn windows_only_memory_fields_are_absent_not_zero() {
        let m = two_samples().system.memory;
        assert!(m.committed.is_none());
        assert!(m.commit_limit.is_none());
        assert!(m.paged_pool.is_none());
        assert!(m.non_paged_pool.is_none());
        assert!(m.hardware_reserved.is_none());
    }

    #[test]
    fn process_cpu_is_a_share_of_the_machine_not_of_one_core() {
        if crate::APP_STORE {
            return; // no process list in the sandbox build
        }
        // One thread spinning flat out is one core: 100 / cores of the
        // machine. Reported per core (sysinfo's raw figure) it would read
        // ~100 %, which is the bug this guards against.
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = std::sync::Arc::clone(&stop);
        let spinner = std::thread::spawn(move || {
            let mut x = 0u64;
            while !flag.load(std::sync::atomic::Ordering::Relaxed) {
                x = std::hint::black_box(x.wrapping_add(1));
            }
        });
        let mut s = SystemSampler::new();
        s.sample().expect("first");
        std::thread::sleep(std::time::Duration::from_millis(800));
        let sample = s.sample().expect("second");
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        spinner.join().expect("join");

        let cores = s.logical_cores;
        let me = std::process::id();
        let mine = sample
            .processes
            .iter()
            .find(|p| p.key.pid.get() == me)
            .expect("own process")
            .cpu
            .get();
        let one_core = 100.0 / cores;
        assert!(
            mine > one_core * 0.5 && mine < one_core * 1.6,
            "one busy thread read {mine} %, expected about {one_core} % on {cores} cores"
        );
    }

    #[test]
    fn this_process_is_listed_with_its_own_start_time() {
        if crate::APP_STORE {
            return; // no process list in the sandbox build
        }
        let sample = two_samples();
        let me = std::process::id();
        let mine = sample
            .processes
            .iter()
            .find(|p| p.key.pid.get() == me)
            .expect("own process listed");
        assert!(mine.thread_count >= 1);
        assert!(mine.memory_private.get() > 0);
        assert_eq!(
            mine.key,
            ProcessKey::new(Pid(me), mach::start_time_us(me).expect("start"))
        );
    }

    #[test]
    fn the_root_volume_is_listed_once() {
        let disks = two_samples().system.disks;
        let roots = disks
            .iter()
            .filter(|d| d.mount.as_deref() == Some("/"))
            .count();
        assert_eq!(roots, 1, "{disks:?}");
    }

    #[test]
    fn loopback_is_not_an_adapter() {
        let nets = two_samples().system.networks;
        assert!(nets.iter().all(|n| !n.name.starts_with("lo")));
    }
}
