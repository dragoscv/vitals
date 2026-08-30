//! Memory sampling.
//!
//! Windows exposes memory through several APIs that disagree with each other,
//! and picking the wrong one is why memory readings differ between tools:
//!
//! - `GlobalMemoryStatusEx` — simple, but its "available" figure excludes the
//!   standby list, so it understates what is actually reclaimable.
//! - `GetPerformanceInfo` — commit charge, pool sizes and page size, but no
//!   cache breakdown.
//! - `NtQuerySystemInformation(SystemFileCacheInformation)` — the standby and
//!   cache figures Task Manager shows.
//!
//! We read all three and reconcile, because "how much memory is free" is
//! precisely the question users compare against Task Manager.

pub mod pressure;
pub mod sampler;

pub use pressure::{MemoryPressure, classify_pressure};
pub use sampler::MemorySampler;
