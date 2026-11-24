//! What agentq and each AI client are using right now: which processes are
//! theirs, and the cpu / memory / disk / network / gpu those processes burn.
//!
//! # Why the process table is filtered rather than guessed
//!
//! The machine runs hundreds of processes and only a handful belong to an AI
//! tool. Two rules find them, and both are evidence rather than a heuristic:
//!
//! 1. **agentq's own state.** The process holding a queue ticket, the one
//!    running a release, a running job in `jobs/*.json`, and the
//!    `appPid`/`shellPid` in `agents/*.json` are pids agentq recorded itself,
//!    together with the start time that makes the pid an identity, so a
//!    recorded process is attributed even when its executable name is generic
//!    — and the shell a client runs in is a plain `pwsh.exe`, which no name
//!    rule could ever claim.
//! 2. **An exact executable name**, for clients that never talk to agentq
//!    (VS Code with Copilot Chat, a `claude` or `codex` run started by hand).
//!
//! A process that is not claimed by either is not counted, including its
//! descendants: attributing a whole subtree to a tool because one process in
//! it looked plausible is how "node.exe is using 3 GB" ends up in a report.
//!
//! # agentq's own work is a row per thing it is executing
//!
//! The build tree under a held ticket belongs to that build, not to the
//! editor that asked for it: agentq's row is folded from its own records and
//! then split by what each record is doing, so `build:vitals` and
//! `release vitals:any` are separate numbers whose sum is the agentq total.
//! That is the only way "what is agentq costing me, and on what" can be read
//! off the page instead of guessed from a chat window's total.
//!
//! The rows are folded from the same process table the Processes screen shows,
//! so the two agree to the tick; see `get_agent_usage` for why the webview
//! supplies it instead of the backend sampling again.

use std::collections::HashMap;
use std::path::Path;

use vitals_core::agents::{
    AgentUsage, AgentUsageKind, AgentUsageSnapshot, AgentqWorkKind, AgentqWorkUsage,
};
use vitals_core::process::Process;
use vitals_core::units::{Bytes, BytesPerSec, Percent};

use crate::agentq::{self, PROC_START_TOLERANCE_MS, ProcStart};

/// How far up a process tree the search for a claimed ancestor walks.
///
/// The table arrives from the webview, so by the time it is folded it is
/// foreign input: a pid can be its own parent, or two processes can form a
/// cycle, and either would spin here forever. Real trees are far shallower.
const MAX_ANCESTOR_DEPTH: usize = 64;

/// What a recorded pid is evidence of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Claim {
    /// A tool's own process, from the caller registry or an executable name.
    Tool(AgentUsageKind),
    /// One piece of agentq's work: an index into [`Ownership::work`].
    Work(usize),
}

/// A process agentq's own state says belongs to a tool or to a piece of work.
///
/// The start time is what makes it an identity, and it is required: Windows
/// reuses pids, so a recorded pid that is now an unrelated process must not
/// have that process's memory charged to the agent. A record that carries no
/// start time is therefore not an identity at all and is dropped by the
/// readers below — which is why a stale `shellPid` no longer claims a
/// stranger's tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Owned {
    pub pid: u32,
    pub start_ms: u64,
    pub claim: Claim,
}

/// One piece of agentq's work, before it has been priced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Work {
    pub label: String,
    pub kind: AgentqWorkKind,
    pub resource: Option<String>,
    pub repo: Option<String>,
    pub purpose: Option<String>,
    pub command: Option<String>,
    pub session: Option<String>,
    pub pid: u32,
    pub started_ms: Option<u64>,
}

/// Everything agentq's own state attributes, and the work it is executing.
///
/// Read-only, and never a guess: every entry here comes from a file agentq
/// wrote about a process it started or observed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Ownership {
    /// Claims in precedence order — later wins on a shared pid.
    pub entries: Vec<Owned>,
    /// The work those claims belong to, indexed by [`Claim::Work`].
    pub work: Vec<Work>,
}

/// The processes agentq's own state attributes to agentq and to its clients.
///
/// Clients are collected first and agentq's work last, because later claims
/// win: a shell that is holding a ticket is running that ticket, so its tree
/// is agentq's work and not the chat's. The app a client runs in is a separate
/// record, so the client keeps its own row.
#[must_use]
pub fn read_owned(root: &Path, probe: ProcStart<'_>, now: u64) -> Ownership {
    let mut owned = Ownership {
        entries: read_client_owners(&root.join("agents")),
        work: Vec::new(),
    };
    for worker in agentq::read_workers(root, probe, now) {
        let index = owned.work.len();
        owned.work.push(Work {
            label: worker.label,
            kind: worker.kind,
            resource: worker.resource,
            repo: worker.repo,
            purpose: worker.purpose,
            command: worker.command,
            session: worker.session,
            pid: worker.pid,
            started_ms: worker.started_ms,
        });
        owned.entries.push(Owned {
            pid: worker.pid,
            start_ms: worker.start_ms,
            claim: Claim::Work(index),
        });
    }
    owned
}

/// Where a client and the shell it runs in are, from the caller registry.
///
/// A client's *shell* is the process that actually compiles, tests and pushes,
/// so counting only the app would hide most of the work.
///
/// Both halves of an identity are required. A registry entry that records a
/// pid without its start time cannot be told apart from a recycled pid, and
/// the sessions of a long day are exactly where a recycled pid lands: agentq
/// writes `appStart`/`shellStart` on every session, so an entry missing one is
/// stale or truncated, and claiming a stranger's memory on it would be a wrong
/// number rather than a missing one.
fn read_client_owners(dir: &Path) -> Vec<Owned> {
    let mut out = Vec::new();
    for path in agentq::json_files(dir) {
        let Some(v) = agentq::read_json(&path) else {
            continue;
        };
        let Some(client) = agentq::s(&v, "client") else {
            continue;
        };
        let kind = kind_of_client(&client);
        for (pid_key, start_key) in [("appPid", "appStart"), ("shellPid", "shellStart")] {
            let (Some(pid), Some(start_ms)) =
                (agentq::pid32(&v, pid_key), agentq::t(&v, start_key))
            else {
                continue;
            };
            out.push(Owned {
                pid,
                start_ms,
                claim: Claim::Tool(kind),
            });
        }
    }
    out
}

/// agentq's `client` string → the row it belongs in.
///
/// Ordered most specific first: `vscode-copilot` contains `copilot`, so the
/// VS Code test has to come before the CLI one or every VS Code session would
/// be reported as the Copilot CLI. Anything agentq knows about but this
/// catalogue does not name is still an agent worth measuring, so it lands in
/// [`AgentUsageKind::Cli`] rather than being dropped.
fn kind_of_client(client: &str) -> AgentUsageKind {
    let c = client.to_ascii_lowercase();
    let has = |needle: &str| c.contains(needle);
    if has("vscode") || has("code-insiders") {
        AgentUsageKind::VscodeCopilot
    } else if has("copilot") {
        AgentUsageKind::CopilotCli
    } else if has("claude") {
        AgentUsageKind::Claude
    } else if has("codex") {
        AgentUsageKind::Codex
    } else if has("hermes") {
        AgentUsageKind::Hermes
    } else if has("claw") {
        AgentUsageKind::OpenClaw
    } else if has("kodwen") || has("codwen") || has("codai") {
        AgentUsageKind::Kodwen
    } else {
        AgentUsageKind::Cli
    }
}

/// The executable names (and two descriptions) that identify a tool.
///
/// # What is deliberately absent, and why
///
/// - `node.exe`, `deno.exe`, `bun.exe`: the host for most CLIs installed from
///   npm. Matching them would charge every Vite dev server, every lint run and
///   every build script on the machine to an AI agent; this machine runs
///   `node.exe` for dozens of unrelated things. Telling those apart needs the
///   process command line, which the wire model does not carry — a process
///   that *could* be an agent is not one.
/// - `pwsh.exe` / `powershell.exe`: agentq itself is one of these, and so is
///   every terminal the user opened by hand. agentq's own work is read from
///   its own state instead ([`read_owned`]).
/// - `python.exe`, `git.exe`, `cargo.exe`, `msbuild.exe`: tools an agent runs,
///   not agents. They are counted as descendants of a claimed process, where
///   the evidence that they belong to the agent is the parent chain rather
///   than the name.
fn classify(name: &str, description: Option<&str>) -> Option<AgentUsageKind> {
    let lowered = name.trim().to_ascii_lowercase();
    let exe = lowered.strip_suffix(".exe").unwrap_or(&lowered);
    let kind = match exe {
        // Copilot Chat runs inside the editor, and its extension host,
        // renderers and gpu process all carry the same executable name, so
        // the whole tree is attributed to it. The chat's own share cannot be
        // separated from ordinary editing with the data Windows exposes.
        "code" | "code - insiders" | "cursor" | "vscodium" | "vscodium - insiders" => {
            AgentUsageKind::VscodeCopilot
        }
        "claude" => AgentUsageKind::Claude,
        "copilot" => AgentUsageKind::CopilotCli,
        "codex" => AgentUsageKind::Codex,
        "hermes" => AgentUsageKind::Hermes,
        "openclaw" => AgentUsageKind::OpenClaw,
        "kodwen" | "codwen" | "codai" => AgentUsageKind::Kodwen,
        _ => return classify_by_description(description),
    };
    Some(kind)
}

/// The fallback for products whose executable name is not distinctive.
///
/// Only for names that carry the product: a repackaged VS Code reports
/// "Visual Studio Code", and Claude Desktop's description is its own name.
/// Broad descriptions ("Node.js", "Installer") are not used — they would pull
/// unrelated processes in.
fn classify_by_description(description: Option<&str>) -> Option<AgentUsageKind> {
    let d = description?.trim().to_ascii_lowercase();
    if d.starts_with("visual studio code") {
        return Some(AgentUsageKind::VscodeCopilot);
    }
    if d == "claude" {
        return Some(AgentUsageKind::Claude);
    }
    None
}

/// Folds the process table into a row per tool.
///
/// A process is counted once, in the row of the nearest ancestor-or-self that
/// is claimed by agentq or by the name catalogue, so a build tree is charged
/// to the tool that started it and not to every process in it.
#[must_use]
pub fn rollup(
    processes: &[Process],
    owned: &Ownership,
    probe: ProcStart<'_>,
    taken_at_ms: u64,
) -> AgentUsageSnapshot {
    let claimed = claim(processes, &owned.entries, probe);
    let mut totals: HashMap<AgentUsageKind, Totals> = HashMap::new();
    let mut works: Vec<Totals> = owned.work.iter().map(|_| Totals::default()).collect();
    for (index, process) in processes.iter().enumerate() {
        match bucket_of(&claimed, processes, index) {
            Some(Bucket::Tool(kind)) => totals.entry(kind).or_default().add(process),
            // The agentq row and the work rows are accumulated from the same
            // process in the same pass, so the breakdown is the total by
            // construction rather than by a second calculation that could
            // drift from it.
            Some(Bucket::Work(work)) => {
                if let Some(totals_for_work) = works.get_mut(work) {
                    totals_for_work.add(process);
                    totals
                        .entry(AgentUsageKind::Agentq)
                        .or_default()
                        .add(process);
                }
            }
            None => {}
        }
    }

    let mut clients: Vec<AgentUsage> = totals
        .into_iter()
        .map(|(kind, totals)| totals.into_usage(kind))
        .collect();
    // Busiest first, then by size, then by a stable kind order: two rows with
    // identical numbers must not swap places between refreshes.
    clients.sort_by(|a, b| {
        b.cpu
            .0
            .total_cmp(&a.cpu.0)
            .then(b.memory_working_set.0.cmp(&a.memory_working_set.0))
            .then(a.kind.cmp(&b.kind))
    });
    let agentq = clients
        .iter()
        .position(|row| row.kind == AgentUsageKind::Agentq)
        .map(|index| clients.remove(index));

    let mut work: Vec<AgentqWorkUsage> = owned
        .work
        .iter()
        .zip(works)
        .filter(|(_, totals)| totals.processes > 0)
        .map(|(w, totals)| AgentqWorkUsage {
            label: w.label.clone(),
            kind: w.kind,
            resource: w.resource.clone(),
            repo: w.repo.clone(),
            purpose: w.purpose.clone(),
            command: w.command.clone(),
            session: w.session.clone(),
            pid: w.pid,
            started_ms: w.started_ms,
            usage: totals.into_usage(AgentUsageKind::Agentq),
        })
        .collect();
    // Busiest first, then by size, kind and label: the work list must not
    // reshuffle between refreshes at equal cost.
    work.sort_by(|a, b| {
        b.usage
            .cpu
            .0
            .total_cmp(&a.usage.cpu.0)
            .then(
                b.usage
                    .memory_working_set
                    .0
                    .cmp(&a.usage.memory_working_set.0),
            )
            .then(a.kind.cmp(&b.kind))
            .then(a.label.cmp(&b.label))
    });

    AgentUsageSnapshot {
        taken_at_ms,
        agentq,
        clients,
        work,
    }
}

/// Which row a claimed process belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Bucket {
    Tool(AgentUsageKind),
    Work(usize),
}

/// The claimed pids, and where each pid sits in the process table.
struct Claimed {
    by_pid: HashMap<u32, usize>,
    bucket: Vec<Option<Bucket>>,
}

/// Marks the processes agentq recorded, then those the catalogue names.
///
/// agentq's recorded identity wins over the name, because it is evidence and
/// the name is a pattern: a shell recorded for a session is that session's
/// process even if its executable happens to look like something else.
fn claim(processes: &[Process], owned: &[Owned], probe: ProcStart<'_>) -> Claimed {
    let mut by_pid = HashMap::with_capacity(processes.len());
    for (index, process) in processes.iter().enumerate() {
        // First wins on a duplicate pid: the table has one entry per running
        // process, and a duplicate would mean a malformed frame.
        by_pid.entry(process.key.pid.0).or_insert(index);
    }

    let mut bucket: Vec<Option<Bucket>> = processes
        .iter()
        .map(|p| classify(&p.name, p.description.as_deref()).map(Bucket::Tool))
        .collect();
    for entry in owned {
        if let Some(&index) = by_pid.get(&entry.pid)
            && same_process(&processes[index], entry, probe)
        {
            bucket[index] = Some(match entry.claim {
                Claim::Tool(kind) => Bucket::Tool(kind),
                Claim::Work(work) => Bucket::Work(work),
            });
        }
    }

    Claimed { by_pid, bucket }
}

/// Whether the process in the table is the one agentq recorded.
///
/// The start time is mandatory, so this is an equality rather than a guess: a
/// process that began at a different instant is a different process, whatever
/// its pid says.
fn same_process(process: &Process, entry: &Owned, probe: ProcStart<'_>) -> bool {
    if process.key.pid.0 != entry.pid {
        return false;
    }
    probe(entry.pid)
        .is_some_and(|actual| actual.abs_diff(entry.start_ms) <= PROC_START_TOLERANCE_MS)
}

/// The row `processes[start]` belongs to: its own if it is claimed, else the
/// nearest claimed ancestor's.
fn bucket_of(claimed: &Claimed, processes: &[Process], start: usize) -> Option<Bucket> {
    let mut current = Some(start);
    for _ in 0..MAX_ANCESTOR_DEPTH {
        let index = current?;
        if let Some(bucket) = claimed.bucket[index] {
            return Some(bucket);
        }
        current = processes[index]
            .parent
            .and_then(|parent| claimed.by_pid.get(&parent.0).copied());
    }
    None
}

/// A running sum that remembers whether every contributor reported a value.
///
/// This is the whole point of the panel's `null` handling: a total that is
/// missing one contributor is not a total, and printing the partial sum as if
/// it were complete is a wrong number that looks right.
#[derive(Debug)]
struct MeasuredSum<T> {
    started: bool,
    measured: bool,
    total: T,
}

impl<T: Default> Default for MeasuredSum<T> {
    fn default() -> Self {
        Self {
            started: false,
            // True until a contributor says otherwise: a bucket with no
            // contributor has no row at all, so this only ever matters after
            // `add` has run.
            measured: true,
            total: T::default(),
        }
    }
}

impl<T: Default + std::ops::AddAssign + Copy> MeasuredSum<T> {
    fn add(&mut self, value: Option<T>) {
        self.started = true;
        match value {
            Some(v) => self.total += v,
            None => self.measured = false,
        }
    }

    fn value(&self) -> Option<T> {
        (self.started && self.measured).then_some(self.total)
    }
}

/// Everything one tool used, accumulated across its processes.
#[derive(Debug, Default)]
struct Totals {
    processes: u32,
    cpu: f64,
    memory_private: u64,
    memory_working_set: u64,
    disk_read: u64,
    disk_write: u64,
    net_rx: MeasuredSum<u64>,
    net_tx: MeasuredSum<u64>,
    gpu: MeasuredSum<f32>,
    gpu_memory: MeasuredSum<u64>,
}

impl Totals {
    fn add(&mut self, process: &Process) {
        self.processes = self.processes.saturating_add(1);
        self.cpu += f64::from(process.cpu.0);
        self.memory_private = self.memory_private.saturating_add(process.memory_private.0);
        self.memory_working_set = self
            .memory_working_set
            .saturating_add(process.memory_working_set.0);
        self.disk_read = self.disk_read.saturating_add(process.disk_read.0);
        self.disk_write = self.disk_write.saturating_add(process.disk_write.0);
        self.net_rx.add(process.net_rx.map(|v| v.0));
        self.net_tx.add(process.net_tx.map(|v| v.0));
        self.gpu.add(process.gpu.map(|v| v.0));
        self.gpu_memory.add(process.gpu_memory.map(|v| v.0));
    }

    fn into_usage(self, kind: AgentUsageKind) -> AgentUsage {
        AgentUsage {
            kind,
            processes: self.processes,
            cpu: Percent(self.cpu as f32),
            memory_private: Bytes(self.memory_private),
            memory_working_set: Bytes(self.memory_working_set),
            disk_read: BytesPerSec(self.disk_read),
            disk_write: BytesPerSec(self.disk_write),
            net_rx: self.net_rx.value().map(BytesPerSec),
            net_tx: self.net_tx.value().map(BytesPerSec),
            gpu: self.gpu.value().map(Percent),
            gpu_memory: self.gpu_memory.value().map(Bytes),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AgentUsageKind, AgentUsageSnapshot, Claim, Owned, Ownership, classify, kind_of_client,
        read_owned, rollup,
    };
    use std::path::Path;
    use vitals_core::agents::AgentqWorkKind;
    use vitals_core::fixtures;
    use vitals_core::ids::Pid;
    use vitals_core::process::Process;

    /// A process from the shared fixture, so a new field on `Process` does not
    /// mean a new hand-rolled thirty-field literal here.
    fn proc(name: &str, pid: u32, cpu: f32) -> Process {
        fixtures::process(name, pid, cpu)
    }

    fn child(mut p: Process, parent: u32) -> Process {
        p.parent = Some(Pid(parent));
        p
    }

    /// A `probe` backed by a fixed table of pid → start time.
    fn starts(pairs: &[(u32, u64)]) -> impl Fn(u32) -> Option<u64> + '_ {
        move |pid| {
            pairs
                .iter()
                .find(|(candidate, _)| *candidate == pid)
                .map(|(_, start)| *start)
        }
    }

    fn owned(pid: u32, start_ms: u64, kind: AgentUsageKind) -> Owned {
        Owned {
            pid,
            start_ms,
            claim: Claim::Tool(kind),
        }
    }

    /// Ownership with no agentq work in it: the client-claiming tests.
    fn clients(entries: Vec<Owned>) -> Ownership {
        Ownership {
            entries,
            work: Vec::new(),
        }
    }

    fn row(snapshot: &AgentUsageSnapshot, kind: AgentUsageKind) -> Option<&super::AgentUsage> {
        snapshot.clients.iter().find(|r| r.kind == kind)
    }

    /// `read_owned` reads two directories agentq writes, so the fixtures carry
    /// the shapes copied from this machine: registry starts are ISO-8601
    /// strings, a job's `pidStart` is .NET ticks (100 ns since 0001-01-01).
    mod recorded {
        use super::*;
        use std::fs;
        use std::path::PathBuf;

        struct Tmp(PathBuf);
        impl Tmp {
            fn new(tag: &str) -> Self {
                let p =
                    std::env::temp_dir().join(format!("vitals-usage-{tag}-{}", std::process::id()));
                let _ = fs::remove_dir_all(&p);
                fs::create_dir_all(&p).unwrap();
                Self(p)
            }
            fn write(&self, rel: &str, body: &str) {
                let p = self.0.join(rel);
                fs::create_dir_all(p.parent().unwrap()).unwrap();
                fs::write(&p, body).unwrap();
            }
        }
        impl Drop for Tmp {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }

        /// A .NET tick count for a Unix millisecond value.
        fn ticks(ms: u64) -> u64 {
            621_355_968_000_000_000 + ms * 10_000
        }

        /// 2026-10-17T03:00:00Z, the instant the registry strings below name.
        const T0: u64 = 1_792_206_000_000;

        #[test]
        fn identities_come_from_agentq_files_and_a_stale_pid_is_not_charged() {
            let tmp = Tmp::new("owned");
            // A VS Code session whose shell is a plain `node.exe`, which the
            // name catalogue deliberately does not claim: only the recorded
            // identity can put this process in a row.
            tmp.write(
                "agents/term-1.json",
                concat!(
                    r#"{"session":"term-1","client":"vscode-copilot","app":"code","#,
                    r#""appPid":400,"appStart":"2026-10-17T03:00:00.0000000Z","#,
                    r#""shellPid":401,"shellStart":"2026-10-17T03:00:00.0000000Z"}"#,
                ),
            );
            // A Copilot CLI session whose recorded pid now belongs to
            // something else: Windows recycled it, so the pid must not be
            // charged to the client.
            tmp.write(
                "agents/term-2.json",
                concat!(
                    r#"{"session":"term-2","client":"copilot-cli","app":null,"#,
                    r#""appPid":null,"appStart":null,"shellPid":402,"#,
                    r#""shellStart":"2026-10-17T03:00:00.0000000Z"}"#,
                ),
            );
            // A running job: its state is derived from the recorded tick
            // count against the probe, exactly as the app derives it.
            tmp.write(
                "jobs/j1.json",
                &format!(
                    r#"{{"id":"j1","label":"build","pid":300,"pidStart":{},"state":"running"}}"#,
                    ticks(T0)
                ),
            );

            let starts = starts(&[(300, T0), (401, T0), (402, T0 + 86_400_000)]);
            let owned = read_owned(tmp.0.as_path(), &starts, T0);
            assert_eq!(
                owned
                    .entries
                    .iter()
                    .map(|o| (o.pid, o.start_ms, o.claim))
                    .collect::<Vec<_>>(),
                vec![
                    (400, T0, Claim::Tool(AgentUsageKind::VscodeCopilot)),
                    (401, T0, Claim::Tool(AgentUsageKind::VscodeCopilot)),
                    (402, T0, Claim::Tool(AgentUsageKind::CopilotCli)),
                    (300, T0, Claim::Work(0)),
                ],
                "the registry entries in file order, then agentq's work last, \
                 because a later claim wins on a shared pid"
            );
            assert_eq!(owned.work.len(), 1);
            assert_eq!(owned.work.first().expect("job work").label, "build");

            // The fold is what turns those identities into rows: pid 400 is
            // not running, so it must not appear.
            let table = [
                proc("pwsh.exe", 300, 4.0),
                proc("node.exe", 401, 6.0),
                proc("node.exe", 402, 90.0),
            ];
            let snapshot = rollup(&table, &owned, &starts, T0);
            let agentq_row = snapshot.agentq.as_ref().expect("agentq row");
            assert_eq!(agentq_row.processes, 1);
            assert_eq!(agentq_row.cpu.0, 4.0);
            let vscode = row(&snapshot, AgentUsageKind::VscodeCopilot).expect("vscode row");
            assert_eq!(vscode.processes, 1);
            assert_eq!(vscode.cpu.0, 6.0);
            // Nothing claims pid 402 any more, and 90 % must not be reported
            // as the CLI's: a recycled pid is not evidence of a client.
            assert!(row(&snapshot, AgentUsageKind::CopilotCli).is_none());
        }

        /// A ticket file as agentq writes it: the holder's pid and the start
        /// time that makes the pid an identity.
        fn ticket(resource: &str, pid: u32, command: &str) -> String {
            let id = format!("{pid:x}");
            format!(
                concat!(
                    r#"{{"id":"{id}","resource":"{resource}","pid":{pid},"#,
                    r#""procStart":"2026-10-17T03:00:00.0000000Z","session":"term-1","#,
                    r#""purpose":"why it was queued","cmd":"{command}","#,
                    r#""repo":"E:\\gh\\vitals","held":true}}"#
                ),
                id = id,
                resource = resource,
                pid = pid,
                command = command,
            )
        }

        #[test]
        fn a_held_ticket_charges_the_build_tree_to_agentq_and_not_to_the_client_that_started_it() {
            let tmp = Tmp::new("ticket");
            // The session that asked for the build, and the ticket its shell
            // acquired: both records name pid 500, so the two compete for it.
            tmp.write(
                "agents/term-1.json",
                concat!(
                    r#"{"session":"term-1","client":"vscode-copilot","app":"code","#,
                    r#""appPid":400,"appStart":"2026-10-17T03:00:00.0000000Z","#,
                    r#""shellPid":500,"shellStart":"2026-10-17T03:00:00.0000000Z"}"#,
                ),
            );
            tmp.write(
                "queues/build_vitals/00000001-aaaa.json",
                &ticket("build:vitals", 500, "pnpm build"),
            );
            let starts = starts(&[(400, T0), (500, T0), (501, T0), (502, T0)]);
            let tree = [
                proc("Code - Insiders.exe", 400, 3.0),
                child(proc("pwsh.exe", 500, 10.0), 400),
                child(proc("node.exe", 501, 20.0), 500),
                child(proc("rustc.exe", 502, 30.0), 501),
            ];
            let owned = read_owned(tmp.0.as_path(), &starts, T0);
            let snapshot = rollup(&tree, &owned, &starts, T0);

            let work = snapshot.work.first().expect("one work row");
            assert_eq!(work.label, "build:vitals");
            assert_eq!(work.kind, AgentqWorkKind::Build);
            assert_eq!(work.repo.as_deref(), Some("E:\\gh\\vitals"));
            assert_eq!(work.purpose.as_deref(), Some("why it was queued"));
            assert_eq!(
                work.usage.processes, 3,
                "the holder, its child and the compiler it spawned"
            );
            assert_eq!(work.usage.cpu.0, 60.0);
            // The editor keeps its own process: the work claim takes the
            // shell it holds, not the application above it.
            let vscode = row(&snapshot, AgentUsageKind::VscodeCopilot).expect("editor row");
            assert_eq!(vscode.processes, 1);
            assert_eq!(vscode.cpu.0, 3.0);
            let agentq = snapshot.agentq.as_ref().expect("agentq row");
            assert_eq!(agentq.processes, work.usage.processes);
            assert_eq!(agentq.cpu.0, work.usage.cpu.0);
        }

        #[test]
        fn a_ticket_whose_recorded_holder_is_gone_is_not_agentq_work() {
            let tmp = Tmp::new("deadholder");
            tmp.write(
                "queues/build_codai/00000002-bbbb.json",
                &ticket("build:codai", 600, "pnpm build"),
            );
            // The pid is running, but it started ten minutes later than the
            // holder agentq recorded: the holder exited and Windows handed the
            // pid to something else, which is not this work.
            let starts = starts(&[(600, T0 + 600_000)]);
            let owned = read_owned(tmp.0.as_path(), &starts, T0);
            assert!(owned.work.is_empty(), "no identity, no work");
            let snapshot = rollup(&[proc("node.exe", 600, 55.0)], &owned, &starts, T0);
            assert!(snapshot.agentq.is_none());
            assert!(snapshot.work.is_empty());
        }

        #[test]
        fn a_client_entry_without_a_recorded_start_time_claims_nothing() {
            let tmp = Tmp::new("nostart");
            // A truncated or stale entry: agentq writes the start time with
            // every session, so one without it cannot be matched to a process,
            // and the pid it names may belong to anything by now.
            tmp.write(
                "agents/term-9.json",
                r#"{"session":"term-9","client":"claude-code","shellPid":700,"shellStart":null}"#,
            );
            let starts = starts(&[(700, T0)]);
            let owned = read_owned(tmp.0.as_path(), &starts, T0);
            assert_eq!(owned, Ownership::default(), "no identity, no claim");
            let snapshot = rollup(&[proc("node.exe", 700, 88.0)], &owned, &starts, T0);
            assert!(snapshot.clients.is_empty(), "88 % is not the CLI's");
        }

        #[test]
        fn the_agentq_row_is_exactly_the_sum_of_its_work_rows() {
            let tmp = Tmp::new("sum");
            tmp.write(
                "queues/build_vitals/00000001-aaaa.json",
                &ticket("build:vitals", 500, "pnpm build"),
            );
            tmp.write(
                "queues/land_codai/00000003-cccc.json",
                &ticket("land:codai", 600, "agentq land"),
            );
            tmp.write(
                "jobs/j1.json",
                &format!(
                    concat!(
                        r#"{{"id":"j1","label":"supervise the wave","pid":700,"#,
                        r#""pidStart":{},"state":"running","#,
                        r#""command":"pwsh -File supervise.ps1"}}"#
                    ),
                    ticks(T0)
                ),
            );
            let starts = starts(&[(500, T0), (501, T0), (600, T0), (700, T0)]);
            let tree = [
                proc("pwsh.exe", 500, 10.0),
                child(proc("node.exe", 501, 5.0), 500),
                proc("pwsh.exe", 600, 2.0),
                proc("pwsh.exe", 700, 1.0),
                proc("chrome.exe", 800, 99.0),
            ];
            let owned = read_owned(tmp.0.as_path(), &starts, T0);
            let snapshot = rollup(&tree, &owned, &starts, T0);

            let agentq = snapshot.agentq.as_ref().expect("agentq row");
            let summed: u32 = snapshot.work.iter().map(|w| w.usage.processes).sum();
            let summed_cpu: f32 = snapshot.work.iter().map(|w| w.usage.cpu.0).sum();
            assert_eq!(agentq.processes, summed);
            assert_eq!(agentq.cpu.0, summed_cpu);
            assert_eq!(agentq.processes, 4, "two tickets and a job, no more");
            let kinds: Vec<AgentqWorkKind> = snapshot.work.iter().map(|w| w.kind).collect();
            assert_eq!(
                kinds,
                vec![
                    AgentqWorkKind::Build,
                    AgentqWorkKind::Land,
                    AgentqWorkKind::Job
                ],
                "busiest first"
            );
            // The process nobody claims makes no row at all.
            assert!(snapshot.clients.is_empty());
        }
    }

    #[test]
    fn a_client_is_recognised_from_the_executable_name_it_ships_under() {
        let cases = [
            ("claude.exe", AgentUsageKind::Claude),
            ("copilot.exe", AgentUsageKind::CopilotCli),
            ("Code - Insiders.exe", AgentUsageKind::VscodeCopilot),
            ("code.exe", AgentUsageKind::VscodeCopilot),
            ("Cursor.exe", AgentUsageKind::VscodeCopilot),
            ("codex.exe", AgentUsageKind::Codex),
            ("hermes.exe", AgentUsageKind::Hermes),
            ("openclaw.exe", AgentUsageKind::OpenClaw),
            ("kodwen.exe", AgentUsageKind::Kodwen),
            ("codwen", AgentUsageKind::Kodwen),
        ];
        for (name, want) in cases {
            assert_eq!(classify(name, None), Some(want), "{name}");
        }
    }

    #[test]
    fn a_process_that_merely_could_be_an_agent_is_not_one() {
        // `node.exe` is the one that matters: it is the host for most CLIs
        // installed from npm, and claiming it would charge every build script
        // on the machine to an agent.
        for name in [
            "node.exe",
            "python.exe",
            "pwsh.exe",
            "powershell.exe",
            "git.exe",
            "cargo.exe",
            "notepad.exe",
            "cod.exe",
            "copy.exe",
            "",
        ] {
            assert_eq!(classify(name, None), None, "{name} was claimed");
        }
    }

    #[test]
    fn a_client_is_known_from_agentq_registry_strings_before_its_executable_name() {
        assert_eq!(
            kind_of_client("vscode-copilot"),
            AgentUsageKind::VscodeCopilot
        );
        assert_eq!(kind_of_client("copilot-cli"), AgentUsageKind::CopilotCli);
        assert_eq!(kind_of_client("claude-code"), AgentUsageKind::Claude);
        assert_eq!(kind_of_client("codex"), AgentUsageKind::Codex);
        assert_eq!(kind_of_client("kodwen"), AgentUsageKind::Kodwen);
        assert_eq!(kind_of_client("codai-cli"), AgentUsageKind::Kodwen);
        // A client agentq knows about but this catalogue does not name is
        // still an agent and still gets a row.
        assert_eq!(kind_of_client("some-new-cli"), AgentUsageKind::Cli);
    }

    #[test]
    fn a_resource_names_the_kind_of_work_it_holds_and_a_test_run_outranks_it() {
        use crate::agentq::work_kind;
        let cases: [(Option<&str>, Option<&str>, Option<AgentqWorkKind>); 11] = [
            (Some("build:vitals"), None, Some(AgentqWorkKind::Build)),
            (Some("land:codai"), None, Some(AgentqWorkKind::Land)),
            (Some("commit:metu"), None, Some(AgentqWorkKind::Commit)),
            (Some("push:money"), None, Some(AgentqWorkKind::Push)),
            (Some("install:brivio"), None, Some(AgentqWorkKind::Install)),
            (
                Some("deploy:codai:gateway"),
                None,
                Some(AgentqWorkKind::Deploy),
            ),
            // Tests queue through `build:`, and a suite is not a build.
            (
                Some("build:codai"),
                Some("pnpm test"),
                Some(AgentqWorkKind::Test),
            ),
            (
                Some("build:codai"),
                Some("cargo nextest run"),
                Some(AgentqWorkKind::Test),
            ),
            // A release run holds no resource, so its command decides; a
            // command that names both a deploy and a build is the deploy it
            // was queued as.
            (
                None,
                Some("pnpm --filter @vitals/desktop tauri build"),
                Some(AgentqWorkKind::Build),
            ),
            (
                None,
                Some("deploy-clean.ps1 -Inner -Command pnpm build"),
                Some(AgentqWorkKind::Deploy),
            ),
            // A tunnel script names nothing: the caller decides what silence
            // means for its own records.
            (None, Some("-File C:\\Temp\\k2w8\\tunnel.ps1"), None),
        ];
        for (resource, command, want) in cases {
            assert_eq!(
                work_kind(resource, command),
                want,
                "{resource:?} {command:?}"
            );
        }
    }

    #[test]
    fn descendants_of_a_recorded_process_are_counted_with_it() {
        let tree = [
            child(proc("pwsh.exe", 100, 10.0), 1),
            child(proc("node.exe", 101, 20.0), 100),
            child(proc("cargo.exe", 102, 30.0), 101),
            proc("notepad.exe", 200, 99.0),
        ];
        let owned = [owned(100, 5_000, AgentUsageKind::Agentq)];
        let snapshot = rollup(&tree, &clients(owned.to_vec()), &starts(&[(100, 5_000)]), 7);

        // agentq's row is lifted out of the client list, so read it there.
        let row = snapshot.agentq.as_ref().expect("agentq row");
        assert_eq!(row.processes, 3, "the whole tree, and only the tree");
        assert_eq!(row.cpu.0, 60.0);
        // The unowned process is claimed by nobody, so it makes no row and
        // its 99 % appears in no total.
        assert!(snapshot.clients.is_empty());
        assert_eq!(snapshot.taken_at_ms, 7);
    }

    #[test]
    fn a_pid_recycled_by_an_unrelated_process_is_not_charged_to_the_agent() {
        // Same pid, a different process: the recorded start time is 5 s of
        // something else's run away.
        let tree = [proc("pwsh.exe", 100, 42.0)];
        let owned = [owned(100, 5_000, AgentUsageKind::Agentq)];
        let snapshot = rollup(
            &tree,
            &clients(owned.to_vec()),
            &starts(&[(100, 30_000)]),
            0,
        );
        assert!(snapshot.agentq.is_none());
        assert!(snapshot.clients.is_empty());
    }

    #[test]
    fn a_series_missing_one_contributor_is_null_rather_than_a_partial_total() {
        let mut measured = child(proc("node.exe", 101, 1.0), 100);
        measured.net_rx = Some(vitals_core::units::BytesPerSec(1_000));
        measured.gpu = Some(vitals_core::units::Percent(4.0));
        // The recorded process itself reports neither series, as every
        // process on Windows does for the network.
        let tree = [child(proc("pwsh.exe", 100, 1.0), 1), measured];
        let owned = [owned(100, 5_000, AgentUsageKind::Agentq)];
        let snapshot = rollup(&tree, &clients(owned.to_vec()), &starts(&[(100, 5_000)]), 0);

        let row = snapshot.agentq.as_ref().expect("agentq row");
        assert_eq!(row.processes, 2);
        assert!(
            row.net_rx.is_none(),
            "a partial network total is not a total"
        );
        assert!(row.gpu.is_none(), "a partial gpu total is not a total");
    }

    #[test]
    fn a_series_every_contributor_reports_is_summed() {
        let mut root = child(proc("pwsh.exe", 100, 1.0), 1);
        root.net_tx = Some(vitals_core::units::BytesPerSec(200));
        root.gpu = Some(vitals_core::units::Percent(0.5));
        let mut a = child(proc("node.exe", 101, 1.0), 100);
        a.net_tx = Some(vitals_core::units::BytesPerSec(1_000));
        a.gpu = Some(vitals_core::units::Percent(4.0));
        let mut b = child(proc("node.exe", 102, 1.0), 100);
        b.net_tx = Some(vitals_core::units::BytesPerSec(500));
        b.gpu = Some(vitals_core::units::Percent(1.5));
        let tree = [root, a, b];
        let owned = [owned(100, 5_000, AgentUsageKind::Cli)];
        let snapshot = rollup(&tree, &clients(owned.to_vec()), &starts(&[(100, 5_000)]), 0);

        let row = row(&snapshot, AgentUsageKind::Cli).expect("row");
        assert_eq!(row.net_tx.map(|v| v.0), Some(1_700));
        assert_eq!(row.gpu.map(|v| v.0), Some(6.0));
        assert!(
            row.net_rx.is_none(),
            "still nobody reports the receive side"
        );
    }

    #[test]
    fn nothing_is_reported_when_no_process_belongs_to_an_agent() {
        let tree = [proc("notepad.exe", 1, 5.0), proc("chrome.exe", 2, 5.0)];
        let snapshot = rollup(&tree, &Ownership::default(), &starts(&[]), 0);
        assert!(snapshot.agentq.is_none());
        assert!(snapshot.clients.is_empty());
    }

    #[test]
    fn the_busiest_tool_comes_first_and_agentq_is_lifted_out_of_the_client_rows() {
        let tree = [
            proc("Code - Insiders.exe", 10, 5.0),
            proc("claude.exe", 20, 90.0),
            proc("pwsh.exe", 30, 1.0),
        ];
        let owned = [owned(30, 5_000, AgentUsageKind::Agentq)];
        let snapshot = rollup(&tree, &clients(owned.to_vec()), &starts(&[(30, 5_000)]), 0);

        assert_eq!(
            snapshot.agentq.as_ref().expect("agentq row").kind,
            AgentUsageKind::Agentq
        );
        let order: Vec<AgentUsageKind> = snapshot.clients.iter().map(|r| r.kind).collect();
        assert_eq!(
            order,
            vec![AgentUsageKind::Claude, AgentUsageKind::VscodeCopilot]
        );
    }

    #[test]
    fn a_cycle_in_the_parent_chain_stops_instead_of_spinning_forever() {
        // Foreign input: the webview supplies this table. Two pids that claim
        // each other must not hang the read.
        let mut a = proc("node.exe", 100, 1.0);
        a.parent = Some(Pid(101));
        let mut b = proc("node.exe", 101, 1.0);
        b.parent = Some(Pid(100));
        let snapshot = rollup(&[a, b], &Ownership::default(), &starts(&[]), 0);
        assert!(snapshot.clients.is_empty());
    }

    #[test]
    fn agentq_owns_nothing_when_its_state_directory_is_absent() {
        let absent = Path::new("Z:\\does\\not\\exist");
        assert_eq!(read_owned(absent, &starts(&[]), 0), Ownership::default());
    }
}
