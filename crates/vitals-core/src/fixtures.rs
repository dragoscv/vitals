//! Plausible values for tests, in one place.
//!
//! Behind the `fixtures` feature so they never reach a release binary. They
//! live in core rather than in each crate's test module because these structs
//! have thirty-odd fields: hand-rolling them per crate means every new field
//! breaks four files, and the copies drift until each one is testing a
//! slightly different machine.
//!
//! Everything here is **deliberately non-zero and non-round** where a zero
//! would be indistinguishable from "unset". A fixture full of zeroes lets a
//! bug that drops a value pass every assertion.

use crate::ids::{DiskId, NicId, Pid, ProcessKey};
use crate::metrics::{
    CpuMetrics, DiskKind, DiskMetrics, MemoryMetrics, NetworkKind, NetworkMetrics, SystemMetrics,
};
use crate::process::{Process, ProcessFlags, ProcessKind, ProcessState, ProtectionLevel};
use crate::sample::{Frame, FramePayload, FrameSeq};
use crate::units::{Bytes, BytesPerSec, Percent};

const GIB: u64 = 1024 * 1024 * 1024;

/// A CPU at a third load on two cores.
#[must_use]
pub fn cpu() -> CpuMetrics {
    CpuMetrics {
        total: Percent(33.5),
        per_core: vec![Percent(30.0), Percent(37.0)],
        kernel: Percent(4.25),
        effective_clock: None,
        max_clock: None,
        temperature: None,
        power: None,
        throttled: None,
        process_count: 287,
        thread_count: 3_914,
        handle_count: None,
        uptime_secs: 7_265,
        context_switches: None,
        interrupts: None,
    }
}

/// 16 GiB, half used.
#[must_use]
pub fn memory() -> MemoryMetrics {
    MemoryMetrics {
        total: Bytes(16 * GIB),
        used: Bytes(9 * GIB),
        available: Bytes(7 * GIB),
        cached: Bytes(3 * GIB),
        paged_pool: Bytes(512 * 1024 * 1024),
        non_paged_pool: Bytes(256 * 1024 * 1024),
        committed: Bytes(11 * GIB),
        commit_limit: Bytes(24 * GIB),
        swap_total: Bytes(8 * GIB),
        swap_used: None,
        hardware_reserved: Bytes(128 * 1024 * 1024),
        page_faults_per_sec: None,
        speed: None,
        slots_used: None,
        slots_total: None,
        form_factor: None,
    }
}

/// One `NVMe` volume, lightly busy.
#[must_use]
pub fn disk() -> DiskMetrics {
    DiskMetrics {
        id: DiskId(0),
        name: "C:".to_owned(),
        model: Some("Samsung SSD 990 PRO".to_owned()),
        mount: Some("C:".to_owned()),
        kind: DiskKind::Nvme,
        total: Bytes(2 * 1024 * GIB),
        free: Bytes(700 * GIB),
        read: BytesPerSec(1_048_576),
        write: BytesPerSec(524_288),
        active_time: Percent(12.5),
        response_ms: None,
        queue_depth: None,
        temperature: None,
        health: None,
    }
}

/// One connected Ethernet adapter.
#[must_use]
pub fn network() -> NetworkMetrics {
    NetworkMetrics {
        id: NicId(0),
        name: "Ethernet".to_owned(),
        adapter: Some("Intel I225-V".to_owned()),
        kind: NetworkKind::Ethernet,
        rx: BytesPerSec(131_072),
        tx: BytesPerSec(65_536),
        rx_total: Bytes(9_000_000),
        tx_total: Bytes(4_000_000),
        link_speed: Some(2_500_000_000),
        ipv4: Some("192.168.1.20".to_owned()),
        ipv6: None,
        mac: None,
        connected: true,
        signal: None,
        ssid: None,
        errors_per_sec: None,
    }
}

/// A whole machine: CPU, memory, one disk, one adapter, no GPU.
#[must_use]
pub fn system() -> SystemMetrics {
    SystemMetrics {
        cpu: cpu(),
        memory: memory(),
        disks: vec![disk()],
        networks: vec![network()],
        gpus: Vec::new(),
        power_draw: None,
        fans: vec![crate::metrics::FanMetrics {
            name: "Fan 1".to_owned(),
            rpm: 1467,
        }],
        battery: None,
    }
}

/// A named process using `cpu_percent` of the machine.
#[must_use]
pub fn process(name: &str, pid: u32, cpu_percent: f32) -> Process {
    Process {
        key: ProcessKey {
            pid: Pid(pid),
            start_time: 133_700_000_000_000_000,
        },
        parent: None,
        name: name.to_owned(),
        kind: ProcessKind::App,
        state: ProcessState::Running,
        flags: ProcessFlags::empty(),
        integrity: None,
        protection: ProtectionLevel::None,
        cpu: Percent(cpu_percent),
        memory_private: Bytes(256 * 1024 * 1024),
        memory_working_set: Bytes(384 * 1024 * 1024),
        disk_read: BytesPerSec(4_096),
        disk_write: BytesPerSec(2_048),
        net_rx: None,
        net_tx: None,
        gpu: None,
        gpu_memory: None,
        thread_count: 42,
        handle_count: Some(900),
        user: Some("VITALS\\dev".to_owned()),
        uptime_secs: 900,
    }
}

/// A keyframe carrying `system()` and the given processes.
#[must_use]
pub fn keyframe(seq: u64, processes: Vec<Process>) -> Frame {
    Frame {
        seq: FrameSeq(seq),
        timestamp_ms: 1_700_000_000_000,
        elapsed_ms: 1_000,
        payload: FramePayload::Keyframe {
            system: system(),
            processes,
        },
    }
}
