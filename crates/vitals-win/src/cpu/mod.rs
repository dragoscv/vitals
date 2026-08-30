//! CPU sampling.

pub mod delta;
pub mod sampler;

pub use delta::{CpuTimes, CpuUsage, compute_usage, process_cpu_percent};
pub use sampler::{CpuSampler, logical_core_count};
