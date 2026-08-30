//! # vitals-win
//!
//! The Windows backend.
//!
//! Implements the [`vitals_core::provider`] traits over the NT native API,
//! PDH, WMI, ETW, IPHLPAPI and D3DKMT.
//!
//! ## Attribution
//!
//! Several techniques here follow the approach taken by
//! [System Informer](https://github.com/winsiderss/systeminformer) (MIT),
//! which remains the best documentation of Windows process internals in
//! existence. Specific borrowings are marked at their call sites.
//!
//! ## Why the native API
//!
//! `NtQuerySystemInformation(SystemProcessInformation)` returns every
//! process, its threads and its IO counters in a single call. The documented
//! alternative — `CreateToolhelp32Snapshot` plus a `GetProcessTimes` and
//! `GetProcessMemoryInfo` per process — costs one handle open, three
//! syscalls and one handle close *per process, per tick*. On a machine with
//! 400 processes at 1 Hz that is 1600 syscalls a second against 1, and it is
//! the single biggest reason Task Manager itself feels heavy.

#![cfg(windows)]

pub mod cpu;
pub mod disk;
pub mod host;
pub mod memory;
pub mod network;
pub mod process;
pub mod sampler;

pub use cpu::{CpuTimes, CpuUsage};
pub use disk::{DiskCounters, DiskRates, VolumeInfo, enumerate_volumes};
pub use host::WindowsHost;
pub use memory::{MemoryPressure, MemorySampler};
pub use network::{AdapterInfo, NetworkCounters, NetworkRates, enumerate_adapters};
pub use process::{ProcessEnumerator, RawProcess};
pub use sampler::{Sample, SampledProcess, SystemSampler};
