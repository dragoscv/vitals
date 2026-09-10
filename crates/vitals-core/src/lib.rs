//! # vitals-core
//!
//! The OS-agnostic heart of Vitals: the domain model and the provider traits
//! that every platform backend implements.
//!
//! This crate must never depend on a platform crate. The dependency arrow
//! always points *inward*: `vitals-win` depends on `vitals-core`, never the
//! reverse. That is what makes the macOS and Linux backends an implementation
//! detail rather than a rewrite.
//!
//! ## Design rules
//!
//! - **No allocation in hot paths.** Sampling runs up to 10x/second across
//!   thousands of processes. Types here are `Copy` where possible and use
//!   [`smallvec`] for small collections.
//! - **Everything is optional.** A metric that a platform cannot provide is
//!   `None`, never a fabricated zero. Zero and "unknown" are different facts
//!   and the UI renders them differently.
//! - **Capabilities are declared, not discovered by failure.** A backend
//!   reports what it can do via [`Capabilities`] so the UI can disable
//!   affordances up front instead of showing an error after a click.

pub mod alerts;
pub mod capability;
pub mod diagnosis;
pub mod error;
#[cfg(feature = "fixtures")]
pub mod fixtures;
pub mod history;
pub mod ids;
#[macro_use]
mod macros;
pub mod metrics;
pub mod process;
pub mod provider;
pub mod sample;
pub mod sensor;
pub mod units;

pub use capability::{Capabilities, Capability};
pub use error::{Error, Result};
pub use history::MachineSample;
pub use ids::{DiskId, GpuId, NicId, Pid, SensorId, Tid};
pub use process::{
    HandleInfo, IntegrityLevel, ModuleInfo, Process, ProcessDetail, ProcessFlags, ProcessKind,
    ProcessState, ProtectionLevel,
};
pub use provider::{
    HostProvider, NetworkProvider, PowerProvider, ProcessProvider, SensorProvider, StorageProvider,
    SystemProvider,
};
pub use sample::{Frame, FrameSeq, SampleRate};
pub use sensor::{Sensor, SensorKind, SensorReading};
pub use units::{Bytes, BytesPerSec, Celsius, Hertz, Percent, Volts, Watts};

/// The wire/schema version of the domain model.
///
/// Bumped on any breaking change to a type crossing the IPC boundary. The
/// helper service and the UI both assert on this at handshake so a stale
/// helper fails loudly instead of misreporting data.
pub const MODEL_VERSION: u32 = 1;
