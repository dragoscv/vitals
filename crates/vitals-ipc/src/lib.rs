//! # vitals-ipc
//!
//! Transport between the sampler, the elevated helper and the UI.
//!
//! Two distinct channels, because they have opposite requirements:
//!
//! - **Frames** (sampler → UI) are high-volume, lossy-tolerant and latency
//!   sensitive. They go through a lock-free ring buffer: if the UI stalls, old
//!   frames are overwritten rather than queued. A task manager that buffers a
//!   backlog of stale frames and then replays them is worse than one that
//!   drops them.
//! - **Commands** (UI → helper) are low-volume, must not be lost, and must be
//!   authenticated. They go over a named pipe with a request/response shape.

pub mod attach;
pub mod frame_buffer;
pub mod protocol;

pub use attach::{AttachClient, AttachServer, FrameStream};
pub use frame_buffer::{FrameBuffer, FrameReader, FrameWriter};
pub use protocol::{Command, CommandResult, Handshake};
