//! What the AI page shows: agentq's coordination state and the AI client
//! sessions working on this machine (ADR-0039).
//!
//! # Why the model lives here
//!
//! The parsers are in `vitals-agents`, but the shapes cross three boundaries
//! — the webview, the LAN API and the phone — so they are generated from one
//! definition like every other wire type. A field renamed here fails
//! `pnpm protocol:check` rather than silently reading `undefined` in a client.
//!
//! # Unknown is `None`
//!
//! A ticket whose holder pid cannot be checked is not "dead" and not "alive";
//! a session with no token data has no token count, not zero. Every such
//! field is an `Option`, as in the rest of the crate.

use serde::{Deserialize, Serialize};

use crate::units::{Bytes, BytesPerSec, Percent};

/// Everything the AI page renders, read in one pass.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(rename_all = "camelCase")]
pub struct AgentsSnapshot {
    /// Unix milliseconds at which the snapshot was assembled.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub taken_at_ms: u64,
    /// Whether agentq's state directory exists on this machine at all.
    pub agentq_installed: bool,
    pub resources: Vec<AgentqResource>,
    pub leases: Vec<AgentqLease>,
    pub releases: Vec<AgentqRelease>,
    pub jobs: Vec<AgentqJob>,
    pub marks: Vec<AgentqMark>,
    pub registry: Vec<AgentqCaller>,
    pub cloud_down: Vec<CloudDown>,
    pub machine: Option<MachineCap>,
    /// Newest first, bounded.
    pub journal: Vec<JournalEvent>,
    pub stats: AgentqStats,
    pub sessions: Vec<AgentSession>,
}

/// One agentq queue: a build lock, the machine pool, a commit lock…
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(rename_all = "camelCase")]
pub struct AgentqResource {
    /// `build:codai`, `build:machine`, `commit:brivio`, `deploy:codai:gateway`.
    pub name: String,
    pub kind: ResourceKind,
    pub capacity: Option<u32>,
    /// Holders first (by grant), then waiters by priority.
    pub tickets: Vec<AgentqTicket>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export, export_to = "core/"))]
#[serde(rename_all = "camelCase")]
pub enum ResourceKind {
    Build,
    Machine,
    Install,
    Commit,
    Land,
    Deploy,
    Other,
}

impl ResourceKind {
    #[must_use]
    pub fn of(name: &str) -> Self {
        match name.split(':').next().unwrap_or_default() {
            "build" if name == "build:machine" => Self::Machine,
            "build" => Self::Build,
            "install" => Self::Install,
            "commit" => Self::Commit,
            "land" => Self::Land,
            "deploy" => Self::Deploy,
            _ => Self::Other,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(rename_all = "camelCase")]
pub struct AgentqTicket {
    pub id: String,
    pub resource: String,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub seq: u64,
    pub held: bool,
    pub lane: Option<String>,
    pub priority: Option<i32>,
    pub pid: Option<u32>,
    pub session: Option<String>,
    pub purpose: Option<String>,
    pub cmd: Option<String>,
    pub repo: Option<String>,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub created_ms: Option<u64>,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub acquired_ms: Option<u64>,
    /// Heartbeat file mtime.
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub beat_ms: Option<u64>,
    pub liveness: Liveness,
    /// Idle-yielded: holds the slot but burns no CPU (waiting on the cloud).
    pub yielded: bool,
    /// The build log went quiet (`<ticket>.wedged`).
    pub wedged: bool,
}

/// agentq's own rule: alive = pid exists AND its start time matches the
/// ticket within 3 s; stale = heartbeat older than 10 minutes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export, export_to = "core/"))]
#[serde(rename_all = "camelCase")]
pub enum Liveness {
    Alive,
    Stale,
    Dead,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(rename_all = "camelCase")]
pub struct AgentqLease {
    pub lease_id: String,
    pub repo: String,
    pub slot: String,
    pub role: Option<String>,
    pub path: Option<String>,
    pub branch: Option<String>,
    pub purpose: Option<String>,
    pub session: Option<String>,
    pub pid: Option<u32>,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub created_ms: Option<u64>,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub beat_ms: Option<u64>,
    pub owner_alive: Option<bool>,
    /// agentq's sweep rule: owner dead and idle > 120 min, or idle > 1440 min.
    pub stale: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(rename_all = "camelCase")]
pub struct AgentqRelease {
    pub run_id: String,
    pub repo: String,
    pub target: String,
    pub sha: Option<String>,
    pub phase: Option<String>,
    pub state: Option<String>,
    pub purpose: Option<String>,
    pub preemptible: Option<bool>,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub started_ms: Option<u64>,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub phase_at_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(rename_all = "camelCase")]
pub struct AgentqJob {
    pub id: String,
    pub label: Option<String>,
    pub command: Option<String>,
    pub repo: Option<String>,
    pub pid: Option<u32>,
    /// `running`, `done`, `failed`, `killed`, plus the derived `suspect`
    /// (over twice the expected time) and `dead` (pid gone, no end record).
    pub state: String,
    pub exit_code: Option<i32>,
    pub expected_min: Option<f64>,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub started_ms: Option<u64>,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub ended_ms: Option<u64>,
    /// Log file mtime: a running job whose log has been silent is suspect.
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub last_output_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(rename_all = "camelCase")]
pub struct AgentqMark {
    pub resource: String,
    /// `frozen` or `blocked`.
    pub state: String,
    pub reason: Option<String>,
    pub session: Option<String>,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub at_ms: Option<u64>,
}

/// `coord/agents/<session>.json`: who called agentq (ADR-0039, S18-09).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(rename_all = "camelCase")]
pub struct AgentqCaller {
    pub session: String,
    pub client: Option<String>,
    pub app: Option<String>,
    pub app_pid: Option<u32>,
    pub shell_pid: Option<u32>,
    pub model: Option<String>,
    pub repo: Option<String>,
    pub cwd: Option<String>,
    pub last_verb: Option<String>,
    pub last_purpose: Option<String>,
    pub calls: Option<u32>,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub first_seen_ms: Option<u64>,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub last_seen_ms: Option<u64>,
}

/// A devcloud workspace agentq has stopped sending work to.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(rename_all = "camelCase")]
pub struct CloudDown {
    pub workspace: String,
    pub reason: Option<String>,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub until_ms: Option<u64>,
}

/// The adaptive `build:machine` decision (`queues/build_machine/adaptive.json`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(rename_all = "camelCase")]
pub struct MachineCap {
    pub cap: u32,
    pub min: Option<u32>,
    pub max: Option<u32>,
    pub cpu: Option<f64>,
    pub free_ram_gb: Option<f64>,
    pub free_commit_gb: Option<f64>,
    pub why: Option<String>,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub at_ms: Option<u64>,
}

/// One journal line, reduced to what a timeline shows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(rename_all = "camelCase")]
pub struct JournalEvent {
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub ts_ms: u64,
    pub event: String,
    pub resource: Option<String>,
    pub session: Option<String>,
    pub repo: Option<String>,
    pub lane: Option<String>,
    /// purpose, reason or message — whichever the event carries.
    pub text: Option<String>,
    pub exit: Option<i32>,
    pub wait_sec: Option<f64>,
    pub hold_sec: Option<f64>,
}

/// Rollups over the journal window (`window_hours`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(rename_all = "camelCase")]
pub struct AgentqStats {
    pub window_hours: u32,
    pub runs: u32,
    pub failures: u32,
    pub commits: u32,
    pub cloud_runs: u32,
    pub wait_p50_sec: Option<f64>,
    pub wait_p90_sec: Option<f64>,
    pub hold_p50_sec: Option<f64>,
    pub hold_p90_sec: Option<f64>,
    /// Sum of every queue wait: time agents spent doing nothing but wait.
    pub wait_total_sec: f64,
    pub hold_total_sec: f64,
    pub by_repo: Vec<RepoStat>,
    /// Runs started per hour, oldest first, `window_hours` buckets.
    pub runs_per_hour: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(rename_all = "camelCase")]
pub struct RepoStat {
    pub repo: String,
    pub runs: u32,
    pub failures: u32,
    pub wait_total_sec: f64,
    pub hold_total_sec: f64,
}

/// The AI client that owns a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export, export_to = "core/"))]
#[serde(rename_all = "camelCase")]
pub enum AgentClient {
    VscodeCopilot,
    CopilotCli,
    /// The author's AI gateway. Renamed from `Codai` on 2026-11-17 when the
    /// service was rebranded to Kodwen; the wire value is `kodwen`.
    Kodwen,
    Codex,
    ClaudeCode,
    Other,
}

/// Which tool a running process is attributed to on the resources panel.
///
/// # Why this is not `AgentClient`
///
/// Two of the rows a reader needs have no session at all: agentq itself,
/// which is not a client, and a CLI driven straight from a terminal, whose
/// session agentq does not always record. Adding them to `AgentClient` would
/// put variants into the session model that can never appear on a session.
/// The two enums therefore stay separate, and `client` is the one that keeps
/// `Other` — a session of unknown origin is worth listing, a *resource* row
/// of unknown origin is not, because nobody can act on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export, export_to = "core/"))]
#[serde(rename_all = "camelCase")]
pub enum AgentUsageKind {
    /// agentq's own work on this machine: the jobs it started and their
    /// descendants (a build, a test run, a cloud gate).
    Agentq,
    /// Claude Desktop and the `claude` CLI.
    Claude,
    /// The `copilot` CLI.
    CopilotCli,
    /// The VS Code family — VS Code, VS Code Insiders, Cursor, `VSCodium` —
    /// which is where GitHub Copilot Chat runs.
    VscodeCopilot,
    /// The `codex` CLI.
    Codex,
    /// Hermes.
    Hermes,
    /// `OpenClaw`.
    OpenClaw,
    /// The Kodwen CLI.
    Kodwen,
    /// A recognised AI tool that is none of the above: agentq knows a client
    /// is running because it recorded the session, but the executable name
    /// carries no brand to show.
    Cli,
}

/// What one tool is using right now.
///
/// # `null` is not `0`
///
/// A measurement that does not exist is `null`, never `0`. Windows reports no
/// per-process network counters without an ETW kernel session, so `netRx` and
/// `netTx` are null for every row on that platform — rendering them as `0 B/s`
/// would claim the agent moved no bytes, which is a different and false fact.
/// The same applies to `gpu`/`gpuMemory` when the machine has no GPU or the
/// driver reports nothing, and to the *sum*: if one contributor to a row was
/// unmeasured, the row's total is null rather than the partial sum, because a
/// partial total looks complete and is not.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(rename_all = "camelCase")]
pub struct AgentUsage {
    pub kind: AgentUsageKind,
    /// How many processes were attributed: the recorded ones plus every
    /// descendant. A count of 1 with a large cpu is a single busy process;
    /// a count of 40 is a build tree.
    pub processes: u32,
    /// Percent of one core-equivalent across all counted processes, summed
    /// the same way the process list sums it (a 16-core machine can exceed
    /// 100).
    pub cpu: Percent,
    /// Private bytes: the memory the tool asked for itself.
    pub memory_private: Bytes,
    /// Working set: the memory physically resident, which is what the person
    /// feels when the machine runs out.
    pub memory_working_set: Bytes,
    pub disk_read: BytesPerSec,
    pub disk_write: BytesPerSec,
    /// Null when a contributor does not report network use (always, on
    /// Windows today — see the type docs).
    pub net_rx: Option<BytesPerSec>,
    pub net_tx: Option<BytesPerSec>,
    /// Null when a contributor reports no GPU time, or the machine has none.
    pub gpu: Option<Percent>,
    /// Null when a contributor reports no video memory.
    pub gpu_memory: Option<Bytes>,
}

/// What agentq is doing with the processes attributed to it.
///
/// agentq's work is not one thing: it compiles, runs tests, lands branches,
/// commits, installs and deploys, and each costs the machine differently. The
/// kind comes from agentq's own record of the work — the resource a held
/// ticket names (`build:vitals`, `commit:codai`), or the command it stored
/// when the resource names no kind. Two kinds are never derived from text: a
/// release run is a deploy and a background job is a job, because their
/// recorded purpose is prose and their command is often an inline script
/// path, whose file name describes the file rather than the work. A process
/// name is never consulted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export, export_to = "core/"))]
#[serde(rename_all = "camelCase")]
pub enum AgentqWorkKind {
    Build,
    Test,
    Commit,
    Push,
    Land,
    Install,
    Deploy,
    /// A background job started through `job-start.ps1`.
    Job,
    /// agentq work whose resource and command name none of the above.
    Other,
}

/// One piece of agentq's work, with what it is costing right now.
///
/// The numbers are the same measurements a client row carries, folded over
/// this work's own processes and their descendants: a build's compiler
/// children are inside `build:vitals` rather than spread over the session that
/// asked for it. `null` means the same as everywhere else on this type — not
/// measured, which is not zero.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(rename_all = "camelCase")]
pub struct AgentqWorkUsage {
    /// What to show: the queue resource (`build:vitals`), `release
    /// vitals:any`, or the job's label.
    pub label: String,
    pub kind: AgentqWorkKind,
    /// The queue resource in agentq's own spelling, when there is one.
    pub resource: Option<String>,
    pub repo: Option<String>,
    /// agentq's stated purpose for the ticket, which says what the work is
    /// for rather than only what runs.
    pub purpose: Option<String>,
    /// The command line the work runs, as agentq recorded it. A purpose is
    /// written for a person; this is what actually executes.
    pub command: Option<String>,
    /// The session that queued it, so a row can be traced back to a chat.
    pub session: Option<String>,
    /// The process that holds the work, as agentq recorded it: unique per
    /// row, and what an action on this row would address.
    pub pid: u32,
    /// When the work started: the ticket was acquired, the release run began,
    /// or the job was launched.
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub started_ms: Option<u64>,
    /// What this work is using, measured exactly like a client row — and
    /// summed from the same processes, so the agentq row is always the total
    /// of these and a breakdown can never disagree with it. `usage.kind` is
    /// always [`AgentUsageKind::Agentq`]; the work's own kind is `kind` above.
    pub usage: AgentUsage,
}

/// Every tool's resource use at one instant.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(rename_all = "camelCase")]
pub struct AgentUsageSnapshot {
    /// When the process table this was folded from was sampled. A client
    /// showing a stale number must be able to say so.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub taken_at_ms: u64,
    /// agentq's own row, lifted out of `clients` because it is the one the
    /// page always shows first and may show on its own.
    pub agentq: Option<AgentUsage>,
    /// One row per tool that owns at least one process, busiest first. A tool
    /// with no process is absent, not present with zeroes.
    pub clients: Vec<AgentUsage>,
    /// What agentq itself is executing, one row per piece of work that owns a
    /// process, busiest first. Work that is queued or long finished owns
    /// nothing and is absent; so is a row whose recorded process has exited.
    pub work: Vec<AgentqWorkUsage>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export, export_to = "core/"))]
#[serde(rename_all = "camelCase")]
pub enum SessionState {
    /// A turn is open and something happened in the last two minutes.
    Working,
    /// A turn is open but nothing has been written for a while: a long
    /// tool, a build, or an agent waiting on a queue.
    Waiting,
    /// No open turn: the agent answered and waits for the person.
    Idle,
    /// Shut down, or untouched for the session window.
    Ended,
}

/// One conversation with an AI coding agent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(rename_all = "camelCase")]
pub struct AgentSession {
    pub id: String,
    pub client: AgentClient,
    /// Where it runs: `local`, or a cloud host name.
    pub host: String,
    pub title: Option<String>,
    pub workspace: Option<String>,
    pub repo: Option<String>,
    pub branch: Option<String>,
    pub model: Option<String>,
    pub state: SessionState,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub started_ms: Option<u64>,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub last_activity_ms: Option<u64>,
    /// Sum of turn durations: time the agent was actually working.
    ///
    /// The counters below are `None` where the source does not report them
    /// (a Kodwen cloud session lists only its title and timestamps), never `0`:
    /// "0 turns" next to a session that has run for hours is a false fact.
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub active_ms: Option<u64>,
    pub turns: Option<u32>,
    pub user_messages: Option<u32>,
    pub tool_calls: Option<u32>,
    pub tool_failures: Option<u32>,
    /// The tool running right now, when a call has started and not completed.
    pub current_tool: Option<String>,
    /// Most-used tools, descending.
    pub top_tools: Vec<ToolCount>,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub tokens_in: Option<u64>,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub tokens_out: Option<u64>,
    /// Prompt tokens served from the provider cache (cheaper, and the reason
    /// `tokens_in` alone is not what the bill is computed from).
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub tokens_cached: Option<u64>,
    /// Reasoning tokens the model produced but did not return as text.
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub tokens_reasoning: Option<u64>,
    /// What the source says the session cost, in US dollars, when it reports a
    /// price. `None` for a subscriber plan that bills flat or does not price
    /// per request — never `0` for "unknown".
    pub cost_usd: Option<f64>,
    /// Requests the source recorded; `None` where it records none.
    pub requests: Option<u32>,
    /// Sum of the requests' durations in milliseconds: time the model was
    /// generating, which is the denominator of `tokens_per_sec`.
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub output_ms: Option<u64>,
    /// Average time to the first token of a request, milliseconds.
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub ttft_ms: Option<u64>,
    /// Output tokens per second over `output_ms`.
    ///
    /// Defined once, here, so the desktop, the phone and the watch print the
    /// same number instead of each inventing a formula.
    pub tokens_per_sec: Option<f64>,
    /// The process that hosts the agent, when it can be attributed.
    pub pid: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export, export_to = "core/"))]
#[serde(rename_all = "camelCase")]
pub struct ToolCount {
    pub name: String,
    pub count: u32,
}

/// What the AI page may ask agentq to do. Every variant carries a reason,
/// because agentq journals it and other agents read it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export, export_to = "core/"))]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AgentqAction {
    Freeze {
        resource: String,
        reason: String,
    },
    Block {
        resource: String,
        reason: String,
    },
    Clear {
        resource: String,
        reason: String,
    },
    Break {
        resource: String,
        ticket: String,
        reason: String,
    },
    Note {
        message: String,
    },
    #[serde(rename_all = "camelCase")]
    Unlease {
        /// The slot directory, e.g. `E:\gh\.wt\codai\task-3` (agentq `-Repo`).
        path: String,
        lease_id: String,
        reason: String,
    },
    KillJob {
        job: String,
    },
    #[serde(rename_all = "camelCase")]
    ReapTerminals {
        dry_run: bool,
    },
    /// Ask agentq to diagnose why *this* machine is waiting and apply its own
    /// safe remedies: break a dead or long-stale holder, raise a long-waiting
    /// ticket, release a short burst. The one action that is safe to reach for
    /// first, because agentq decides what, if anything, to do.
    Propose {
        /// Optional context; agentq journals it beside its own diagnosis.
        reason: Option<String>,
    },
    /// Reap dead/stale tickets and leases, rotate the journal and prune state.
    ///
    /// `dry_run` is what the "Preview" affordance sends: agentq reports what
    /// it would reap without touching anything.
    #[serde(rename_all = "camelCase")]
    Sweep {
        dry_run: bool,
    },
    /// Raise a waiting ticket in the scheduler order.
    ///
    /// `ticket` names one explicitly; without it agentq bumps the first ticket
    /// it can attribute to this machine. `by` is capped by agentq (`bumpMax`).
    Bump {
        ticket: Option<String>,
        by: i32,
        reason: String,
    },
    /// A temporary extra slot on a resource, so a burst of work is not
    /// serialised behind one build.
    ///
    /// agentq refuses it when the free RAM or commit charge is under twice the
    /// gate, so this can never be the reason a machine runs out of memory.
    Burst {
        /// Defaults to `build:machine`.
        resource: Option<String>,
        by: i32,
        minutes: i32,
        reason: String,
    },
    /// Land a topic branch: pre-gate the rebased result, then merge and push.
    ///
    /// The most powerful action on this page — it moves the target branch.
    /// The UI gates it behind an explicit confirmation, and `gate` runs before
    /// anything is merged so a broken branch cannot reach `main`.
    Land {
        /// The clone to land in. agentq resolves the repository from where it
        /// is run, and refuses when the clone is not the one it expects, so
        /// this is always supplied rather than guessed from the app's cwd.
        repo: String,
        branch: String,
        /// `main` or `dev`.
        onto: String,
        purpose: String,
        /// A command run on the rebased result; a failure stops the land.
        gate: Option<String>,
    },
    /// Queue a resource, hold it, run a command, release it.
    ///
    /// `command` is the shell command line; without it the resource is held
    /// and released, which is only useful for the queue itself.
    Run {
        resource: String,
        purpose: String,
        command: Option<String>,
        /// `interactive`, `quick`, `gate`, `heavy` or `batch`.
        lane: Option<String>,
        /// `local`, `cloud` or `auto` — agentq's `-Where`.
        place: Option<String>,
        /// A clone to run in; sets agentq's working directory. Needed for a
        /// cloud run, which uploads the working tree as a patch.
        repo: Option<String>,
    },
    /// Commit explicit paths in a repository, then fetch, merge and push.
    ///
    /// `repo` is the clone the commit lands in; `paths` are the only files
    /// staged, so a shared clone's other work is never swept in.
    Commit {
        repo: String,
        purpose: String,
        message: String,
        paths: Vec<String>,
    },
}

/// What agentq looks like on this machine, so the UI can decide whether the
/// page is worth showing at all.
///
/// Absence is not an error: agentq runs on Windows only, and on another
/// platform the page still lists AI sessions, so `relevant` stays true.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, export_to = "core/", rename_all = "camelCase")
)]
#[serde(rename_all = "camelCase")]
// A wire DTO, and the four flags are four independent facts rather than one
// state: agentq's script can be installed while its state directory is empty,
// an AI client can be present with no agentq at all, and `windows` is a
// compile-time property of the build. The generated TypeScript binding mirrors
// these names one to one and the page composes them into its own verdict, so
// folding them into two-variant enums here would change the wire shape the
// client already reads without removing a case it has to handle.
#[allow(clippy::struct_excessive_bools)]
pub struct AgentqStatus {
    /// `agentq.ps1` is installed where Vitals looks for it.
    pub script: bool,
    /// The coordination state directory exists, so there is something to show.
    pub coord: bool,
    /// At least one AI client keeps its files on this machine.
    pub clients: bool,
    /// Actions are only ever run on Windows; elsewhere the page is read-only.
    pub windows: bool,
}

#[cfg(test)]
mod action_wire_tests {
    use super::AgentqAction;

    /// The webview and the phone send camelCase keys (ts-rs generates them from
    /// the enum's `rename_all`), so serde must accept exactly that. This is the
    /// contract the IPC boundary has no compiler for: if serde and ts-rs
    /// disagree about a field's name, `invoke` fails at runtime and nothing
    /// else says so.
    #[test]
    fn every_field_the_generated_binding_names_is_accepted_by_serde() {
        let cases = [
            r#"{"kind":"reapTerminals","dryRun":true}"#,
            r#"{"kind":"sweep","dryRun":true}"#,
            r#"{"kind":"propose","reason":null}"#,
            r#"{"kind":"bump","ticket":null,"by":20,"reason":"long wait"}"#,
            r#"{"kind":"burst","resource":null,"by":1,"minutes":30,"reason":"burst"}"#,
            r#"{"kind":"land","repo":"E:\\gh\\vitals","branch":"feat/x","onto":"main","purpose":"ship","gate":null}"#,
            r#"{"kind":"run","resource":"build:codai","purpose":"tsc","command":null,"lane":null,"place":null,"repo":null}"#,
            r#"{"kind":"commit","repo":"E:\\gh\\vitals","purpose":"fix","message":"fix(x): y","paths":["a.ts"]}"#,
        ];
        for json in cases {
            let parsed: AgentqAction =
                serde_json::from_str(json).unwrap_or_else(|e| panic!("serde rejected {json}: {e}"));
            let back = serde_json::to_value(&parsed).expect("re-serialise");
            assert_eq!(
                back,
                serde_json::from_str::<serde_json::Value>(json).unwrap()
            );
        }
    }
}

#[cfg(test)]
mod usage_wire_tests {
    use super::{AgentUsage, AgentUsageKind, AgentUsageSnapshot};
    use crate::units::{Bytes, BytesPerSec, Percent};

    fn row_with_nothing_measured() -> AgentUsage {
        AgentUsage {
            kind: AgentUsageKind::VscodeCopilot,
            processes: 5,
            cpu: Percent(12.5),
            memory_private: Bytes(1_000),
            memory_working_set: Bytes(2_000),
            disk_read: BytesPerSec(3),
            disk_write: BytesPerSec(4),
            net_rx: None,
            net_tx: None,
            gpu: None,
            gpu_memory: None,
        }
    }

    /// The names the generated TypeScript binding uses are the names serde
    /// writes. A field renamed on one side only is invisible to the compiler
    /// and arrives in the webview as `undefined`, which the panel would render
    /// as an em dash — a wrong measurement that looks like a missing one.
    #[test]
    fn a_usage_row_serialises_to_the_camel_case_names_the_binding_declares() {
        let value = serde_json::to_value(row_with_nothing_measured()).expect("serialise");
        assert_eq!(
            value,
            serde_json::json!({
                "kind": "vscodeCopilot",
                "processes": 5,
                "cpu": 12.5,
                "memoryPrivate": 1000,
                "memoryWorkingSet": 2000,
                "diskRead": 3,
                "diskWrite": 4,
                // `null` is the assertion that matters: an unmeasured series
                // must never travel as `0`.
                "netRx": null,
                "netTx": null,
                "gpu": null,
                "gpuMemory": null,
            })
        );
    }

    /// Every variant's wire name, because the UI switches on the string it
    /// receives and a renamed variant would fall through to the default arm.
    #[test]
    fn every_usage_kind_has_the_wire_name_the_binding_declares() {
        let cases = [
            (AgentUsageKind::Agentq, "\"agentq\""),
            (AgentUsageKind::Claude, "\"claude\""),
            (AgentUsageKind::CopilotCli, "\"copilotCli\""),
            (AgentUsageKind::VscodeCopilot, "\"vscodeCopilot\""),
            (AgentUsageKind::Codex, "\"codex\""),
            (AgentUsageKind::Hermes, "\"hermes\""),
            (AgentUsageKind::OpenClaw, "\"openClaw\""),
            (AgentUsageKind::Kodwen, "\"kodwen\""),
            (AgentUsageKind::Cli, "\"cli\""),
        ];
        for (kind, wire) in cases {
            assert_eq!(serde_json::to_string(&kind).expect("serialise"), wire);
            assert_eq!(
                serde_json::from_str::<AgentUsageKind>(wire).expect("deserialise"),
                kind
            );
        }
    }

    /// Absence survives a round trip: a snapshot with no agentq row and no
    /// clients must come back as absent, not as an empty row of zeroes.
    #[test]
    fn a_snapshot_with_nothing_to_show_round_trips_as_absent() {
        let empty = AgentUsageSnapshot {
            taken_at_ms: 1_700_000_000_000,
            agentq: None,
            clients: Vec::new(),
            work: Vec::new(),
        };
        let json = serde_json::to_string(&empty).expect("serialise");
        assert!(json.contains(r#""agentq":null"#), "{json}");
        assert_eq!(
            serde_json::from_str::<AgentUsageSnapshot>(&json).expect("deserialise"),
            empty
        );
    }
}
