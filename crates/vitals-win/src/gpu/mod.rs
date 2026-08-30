//! GPU enumeration and sampling.
//!
//! ## Why D3DKMT rather than a vendor SDK
//!
//! NVML, ADL and Level Zero each cover one vendor, need a runtime DLL that
//! may not be installed, and disagree about what "utilisation" means. D3DKMT
//! is the kernel interface every WDDM driver implements, so one code path
//! covers NVIDIA, AMD, Intel, Qualcomm and the virtual adapters that appear
//! in VMs and remote sessions.
//!
//! The trade-off is that D3DKMT gives engine utilisation and memory but not
//! temperature, fan speed or power. Those need a vendor SDK, so they are
//! loaded opportunistically and reported as absent when unavailable — rather
//! than fabricated, or worse, silently dropped so the user cannot tell the
//! difference between a cool GPU and an unmeasured one.
//!
//! ## Why per-engine and not one number
//!
//! A GPU is several independent engines: 3D, video decode, video encode,
//! copy, compute. Task Manager shows the **maximum** across engines as "GPU
//! utilisation", so a machine transcoding video at full tilt on the encode
//! engine reads 100% while the 3D engine is idle and a game would run fine.
//! Reporting per-engine makes that legible.

pub mod adapters;
pub mod engines;

pub use adapters::{GpuAdapter, enumerate_adapters};
pub use engines::{EngineKind, EngineUsage};
