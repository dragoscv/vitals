//! Disk enumeration and sampling.
//!
//! Two distinct things people mean by "disk", kept separate because
//! conflating them is why disk figures in monitoring tools rarely add up:
//!
//! - **Volumes** (`C:`, `D:`) — what has free space. A single physical disk
//!   can host several, and one volume can span several disks.
//! - **Physical disks** — what has throughput, queue depth, temperature and
//!   SMART health.
//!
//! Capacity comes from the volume; activity comes from the physical disk.

pub mod device;
pub mod rate;
pub mod volumes;

pub use device::{physical_drive, refine_kind, volume_counters};
pub use rate::{DiskCounters, DiskRates};
pub use volumes::{VolumeInfo, enumerate_volumes};
