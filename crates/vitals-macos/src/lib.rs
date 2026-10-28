//! # vitals-macos
//!
//! macOS backend. Fills the platform-neutral model in `vitals-core` from
//! `sysinfo` (CPU, memory, swap, volumes, interfaces, processes and the
//! SMC/HID temperature sensors) plus a few direct Mach and `libproc` calls
//! where sysinfo is silent (kernel CPU share, per-process footprint and the
//! start time that makes a process key race-free).
//!
//! Two builds share this code:
//!
//! - **Developer ID** (direct download): unsandboxed, so it sees every
//!   process of the signed-in user, can signal them and can read the sensors.
//! - **Mac App Store** (`app-store` feature): sandboxed. Other processes,
//!   signals and the sensor clients are off limits, and the capability report
//!   says so rather than letting those readings come back empty and look
//!   like an idle machine.
//!
//! Every reading this platform has no source for is `None` — commit charge,
//! paged pools, per-process GPU, disk latency — never a fabricated zero.

#![cfg(target_os = "macos")]

mod actions;
mod capabilities;
mod frame;
mod hostinfo;
mod mach;
mod sampler;

pub use actions::{resume, set_priority, suspend, terminate};
pub use capabilities::capabilities;
pub use frame::FrameBuilder;
pub use hostinfo::read as host_info;
pub use sampler::{Sample, SystemSampler};

/// When a process started, in Unix milliseconds — what the AI-agents reader
/// uses to tell a live agent from a recycled PID.
#[must_use]
pub fn process_start_ms(pid: u32) -> Option<u64> {
    mach::start_time_us(pid).map(|us| us / 1000)
}

/// Whether this binary is the sandboxed Mac App Store build.
pub const APP_STORE: bool = cfg!(feature = "app-store");
