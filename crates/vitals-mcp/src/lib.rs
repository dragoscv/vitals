//! This machine, as tools an AI agent can call — the Model Context Protocol
//! server Vitals ships.
//!
//! Split the same way [`vitals_mqtt`](https://docs.rs/vitals-mqtt) is, so
//! everything that can be wrong about the wire shapes can be asserted without
//! an agent on the other end:
//!
//! - [`jsonrpc`] is the envelope: the request shape, the response shape and
//!   the five error codes.
//! - [`policy`] holds the few numbers that define behaviour (how many
//!   processes, how much history) and the read-only guarantee.
//! - [`tools`] is one enum per tool, with its description, its input schema
//!   and the function that answers it.
//! - [`protocol`] renders the MCP messages themselves: `initialize`,
//!   `tools/list`, `resources/list`, `resources/read`.
//! - [`host`] is the seam between this crate and whatever can actually read a
//!   machine — the CLI's sampler or the LAN server's materialised frame.
//! - [`server`] dispatches one message; [`stdio`] reads and writes them one
//!   per line.
//!
//! # Read-only, by construction
//!
//! There is no tool that ends, suspends, resumes or re-prioritises anything,
//! and there never will be in v1: `policy::is_control_name` fails the build
//! if a tool is ever named like one. An agent is a program that reads a
//! sentence and decides what to do; handing it a kill switch because a user
//! pasted a prompt is not a trade this project makes.
//!
//! # A number that cannot be measured is absent
//!
//! The crate's rule is `vitals-core`'s rule: a reading the host cannot make is
//! `null` or missing, never `0`. `host::McpHost` is shaped around that —
//! `Ok(vec![])` means "nothing measurable", and `Err(why)` means "could not
//! ask", and the two never collapse into one another.

pub mod host;
pub mod jsonrpc;
pub mod policy;
pub mod protocol;
pub mod server;
pub mod stdio;
pub mod tools;

#[cfg(test)]
mod test_host;

pub use host::{McpHost, Reading};
pub use jsonrpc::{Error as RpcError, Request};
pub use policy::{MAX_TOP, is_control_name};
pub use protocol::PROTOCOL_VERSION;
pub use server::Server;
pub use stdio::{serve, serve_streams};
pub use tools::{Outcome, Tool};
