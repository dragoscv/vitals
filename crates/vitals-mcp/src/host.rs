//! The seam between this crate and whatever can actually read a machine.
//!
//! Two things implement it: the CLI's sampler (or its attachment to a running
//! desktop app) and the LAN server's materialised frame. Neither is known
//! here, which is what keeps this crate free of a sampler, a socket and a
//! Tauri dependency.
//!
//! # `Err` is not "nothing"
//!
//! Every method that returns a list returns [`Reading`], and the difference
//! between its two arms is the whole point:
//!
//! - `Ok(Vec::new())` — the host asked and the answer is "none". No sensor is
//!   measurable on this machine; nothing has been raised; nothing has been
//!   recorded. A value, and a truthful one.
//! - `Err(why)` — the host could not ask at all: no sensor service, no store,
//!   a dead connection. A tool answers this with a sentence naming the
//!   problem, never with an empty list, because "I could not look" and "there
//!   is nothing there" are different facts and an agent will act on them
//!   differently.

use vitals_core::MachineSample;
use vitals_core::alerts::Alert;
use vitals_core::diagnosis::Diagnosis;
use vitals_core::provider::HostInfo;
use vitals_core::remote::{SensorLine, Summary};

/// What a reading came back as. See the module docs for the two arms.
pub type Reading<T> = Result<T, String>;

/// Everything the tools need to know about one machine.
///
/// Deliberately small and synchronous: the caller owns its own concurrency
/// (the CLI serves one agent on one thread; the server is inside axum), and a
/// host that had to be `async` would drag a runtime into a crate that has no
/// other use for one.
pub trait McpHost {
    /// The machine's name, used for the resource URI (`vitals://<machine>`).
    ///
    /// Never empty: a host that cannot find a name returns a placeholder
    /// rather than an empty segment, which would produce the single URI
    /// `vitals://` and read as a bug on the client's side.
    fn machine(&self) -> &str;

    /// Static facts about the machine, or `None` when the host cannot read
    /// them. Absent, never a zero-filled placeholder.
    fn host_info(&self) -> Option<HostInfo>;

    /// The machine now plus its busiest processes.
    ///
    /// `Ok(None)` is "not sampling yet" — a real state, and different from a
    /// machine with no processes: a caller that gets an empty list back would
    /// render an idle PC while the sampler has simply not taken its first
    /// reading.
    fn summary(&self, top: usize) -> Reading<Option<Summary>>;

    /// Every sensor reading the host can make now.
    fn sensors(&self) -> Reading<Vec<SensorLine>>;

    /// Alerts currently raised, most serious first. `Ok(vec![])` is the
    /// healthy answer, not an absence.
    fn alerts(&self) -> Reading<Vec<Alert>>;

    /// Machine-wide samples covering the last `seconds`.
    fn history(&self, seconds: u32) -> Reading<Vec<MachineSample>>;

    /// Why the machine is slow, with the processes that account for it.
    ///
    /// `Ok(None)` is "not sampling yet"; `Ok(Some(diagnosis))` with a `null`
    /// verdict is the honest and common answer — nothing is wrong.
    fn diagnosis(&self) -> Reading<Option<Diagnosis>>;
}
