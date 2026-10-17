//! What the AI page shows: agentq's coordination state and the AI client
//! sessions working on this machine (ADR-0038).
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

/// `coord/agents/<session>.json`: who called agentq (ADR-0038, S18-09).
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
    Codai,
    Codex,
    ClaudeCode,
    Other,
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
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub active_ms: u64,
    pub turns: u32,
    pub user_messages: u32,
    pub tool_calls: u32,
    pub tool_failures: u32,
    /// The tool running right now, when a call has started and not completed.
    pub current_tool: Option<String>,
    /// Most-used tools, descending.
    pub top_tools: Vec<ToolCount>,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub tokens_in: Option<u64>,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub tokens_out: Option<u64>,
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
}
