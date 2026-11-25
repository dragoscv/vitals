//! agentq's coordination state, read from `%USERPROFILE%\.codai\coord`.
//!
//! Read-only by construction: nothing here opens a file for writing. agentq
//! writes every file to `<name>.tmp` and renames it under a named mutex, so a
//! reader sees either the old or the new file, never half of one — which is
//! why no lock is taken here. Stray `*.json.tmp` files from an interrupted
//! write exist on the real machine and are ignored by extension.
//!
//! Liveness mirrors agentq's own rules (`agentq.ps1` `Test-Alive`,
//! `Read-Leases`) so the page and agentq never disagree about who is dead:
//! a pid is the recorded process only when its start time matches within 3 s;
//! a ticket is stale when its heartbeat is older than 10 minutes; a lease is
//! stale when its owner is dead and idle > 120 min, or idle > 1440 min.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;
use vitals_core::agents::{
    AgentqCaller, AgentqEstimateBasis, AgentqJob, AgentqLease, AgentqMark, AgentqRelease,
    AgentqResource, AgentqTicket, AgentqWorkEstimate, AgentqWorkKind, CloudDown, Liveness,
    MachineCap, ResourceKind,
};

use crate::journal::WorkProfiles;
use crate::time::{mtime_ms, parse_iso_ms};

/// Heartbeat age after which agentq calls a ticket STALE.
pub const TICKET_STALE_MS: u64 = 10 * 60_000;
/// Start-time tolerance agentq uses to accept a pid as the same process.
pub const PROC_START_TOLERANCE_MS: u64 = 3_000;
const LEASE_DEAD_IDLE_MS: u64 = 120 * 60_000;
const LEASE_MAX_IDLE_MS: u64 = 1440 * 60_000;
/// Jobs older than this that have ended are not shown.
const JOB_WINDOW_MS: u64 = 24 * 3_600_000;

/// Answers "when did this pid start?" — `None` when it is not running.
///
/// A trait object rather than a direct Windows call so this crate builds and
/// tests on any OS; the desktop passes `vitals_win::actions::process_start_ms`.
pub type ProcStart<'a> = &'a dyn Fn(u32) -> Option<u64>;

/// The default coord directory, honouring agentq's own `AGENTQ_HOME`.
#[must_use]
pub fn default_root() -> Option<PathBuf> {
    if let Some(h) = std::env::var_os("AGENTQ_HOME") {
        return Some(PathBuf::from(h));
    }
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))?;
    Some(PathBuf::from(home).join(".codai").join("coord"))
}

pub(crate) fn read_json(path: &Path) -> Option<Value> {
    let text = fs::read_to_string(path).ok()?;
    // PowerShell 5 writes a BOM; serde_json refuses it.
    serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()
}

thread_local! {
    /// Parsed job files keyed by path, valid while the mtime is unchanged.
    /// ~900 job files exist on a busy machine and almost all have ended;
    /// re-parsing them every refresh was the largest share of a read
    /// (154 ms of ~300 ms, `prove_agents` 2026-10-17).
    static JOB_CACHE: std::cell::RefCell<std::collections::HashMap<PathBuf, (u64, Value)>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

fn read_json_cached(path: &Path, mtime: u64) -> Option<Value> {
    if let Some(v) = JOB_CACHE.with(|c| {
        c.borrow()
            .get(path)
            .filter(|(m, _)| *m == mtime)
            .map(|(_, v)| v.clone())
    }) {
        return Some(v);
    }
    let v = read_json(path)?;
    JOB_CACHE.with(|c| {
        c.borrow_mut()
            .insert(path.to_path_buf(), (mtime, v.clone()))
    });
    Some(v)
}

pub(crate) fn json_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(rd) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut v: Vec<PathBuf> = rd
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    v.sort();
    v
}

pub(crate) fn s(v: &Value, k: &str) -> Option<String> {
    match v.get(k)? {
        Value::String(x) if !x.is_empty() => Some(x.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}
pub(crate) fn u(v: &Value, k: &str) -> Option<u64> {
    match v.get(k)? {
        Value::Number(n) => n.as_u64().or_else(|| n.as_f64().map(|f| f.max(0.0) as u64)),
        Value::String(x) => x.trim().parse().ok(),
        _ => None,
    }
}
fn f(v: &Value, k: &str) -> Option<f64> {
    match v.get(k)? {
        Value::Number(n) => n.as_f64(),
        Value::String(x) => x.trim().parse().ok(),
        _ => None,
    }
}
fn b(v: &Value, k: &str) -> Option<bool> {
    v.get(k)?.as_bool()
}
pub(crate) fn t(v: &Value, k: &str) -> Option<u64> {
    v.get(k)?.as_str().and_then(parse_iso_ms)
}
pub(crate) fn pid32(v: &Value, k: &str) -> Option<u32> {
    u(v, k)
        .and_then(|p| u32::try_from(p).ok())
        .filter(|p| *p != 0)
}

/// Whether `pid` is still the process that started at `recorded_ms`.
fn pid_matches(probe: ProcStart<'_>, pid: Option<u32>, recorded_ms: Option<u64>) -> Option<bool> {
    let pid = pid?;
    let actual = probe(pid);
    match (actual, recorded_ms) {
        (None, _) => Some(false),
        (Some(_), None) => Some(true),
        (Some(a), Some(r)) => Some(a.abs_diff(r) <= PROC_START_TOLERANCE_MS),
    }
}

fn liveness(alive: Option<bool>, beat_ms: Option<u64>, now: u64) -> Liveness {
    match alive {
        Some(false) => Liveness::Dead,
        Some(true) if beat_ms.is_some_and(|b| now.saturating_sub(b) > TICKET_STALE_MS) => {
            Liveness::Stale
        }
        Some(true) => Liveness::Alive,
        None => Liveness::Unknown,
    }
}

/// Every queue under `queues/`, holders first.
#[must_use]
pub fn read_resources(root: &Path, probe: ProcStart<'_>, now: u64) -> Vec<AgentqResource> {
    let Ok(rd) = fs::read_dir(root.join("queues")) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for dir in rd
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_dir())
    {
        let mut tickets = Vec::new();
        let mut name = None;
        for file in json_files(&dir) {
            let Some(v) = read_json(&file) else { continue };
            // burst.json / adaptive.json live beside tickets and are not tickets
            if v.get("id").is_none() || v.get("resource").is_none() {
                continue;
            }
            let hb = PathBuf::from(format!("{}.hb", file.display()));
            let beat = fs::metadata(&hb)
                .ok()
                .as_ref()
                .and_then(mtime_ms)
                .or_else(|| t(&v, "created"));
            let wedged = PathBuf::from(format!("{}.wedged", file.display())).exists();
            let pid = pid32(&v, "pid");
            let alive = pid_matches(probe, pid, t(&v, "procStart"));
            let resource = s(&v, "resource").unwrap_or_default();
            name.get_or_insert_with(|| resource.clone());
            tickets.push(AgentqTicket {
                id: s(&v, "id").unwrap_or_default(),
                resource,
                seq: u(&v, "seq").unwrap_or(0),
                held: b(&v, "held").unwrap_or(false),
                lane: s(&v, "lane"),
                priority: u(&v, "prio").and_then(|p| i32::try_from(p).ok()),
                pid,
                session: s(&v, "session"),
                purpose: s(&v, "purpose"),
                cmd: s(&v, "cmd"),
                repo: s(&v, "repo"),
                created_ms: t(&v, "created"),
                acquired_ms: t(&v, "acquired"),
                beat_ms: beat,
                liveness: liveness(alive, beat, now),
                yielded: b(&v, "yielded").unwrap_or(false),
                wedged,
            });
        }
        let dir_name = dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_owned();
        let name = name.unwrap_or_else(|| unsafe_name(&dir_name));
        let capacity = fs::read_to_string(dir.join("capacity"))
            .ok()
            .and_then(|c| c.trim().parse().ok());
        tickets.sort_by(|a, b| b.held.cmp(&a.held).then(a.seq.cmp(&b.seq)));
        // An empty queue directory is history, not state; only resources with
        // tickets or a non-default capacity are worth a row.
        if tickets.is_empty() && name != "build:machine" {
            continue;
        }
        out.push(AgentqResource {
            kind: ResourceKind::of(&name),
            name,
            capacity,
            tickets,
        });
    }
    out.sort_by(|a, b| {
        (b.kind == ResourceKind::Machine)
            .cmp(&(a.kind == ResourceKind::Machine))
            .then(b.tickets.len().cmp(&a.tickets.len()))
            .then(a.name.cmp(&b.name))
    });
    out
}

/// agentq's `Safe()` turns `build:codai` into `build_codai`; reverse the
/// first underscore for display when a directory has no ticket to name it.
fn unsafe_name(dir: &str) -> String {
    dir.replacen('_', ":", 1)
}

#[must_use]
pub fn read_leases(root: &Path, probe: ProcStart<'_>, now: u64) -> Vec<AgentqLease> {
    let Ok(rd) = fs::read_dir(root.join("leases")) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for repo_dir in rd
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_dir())
    {
        for file in json_files(&repo_dir) {
            let Some(v) = read_json(&file) else { continue };
            let hb = PathBuf::from(format!("{}.hb", file.display()));
            let created = t(&v, "created");
            let mut beat = fs::metadata(&hb)
                .ok()
                .as_ref()
                .and_then(mtime_ms)
                .or(created);
            // agentq also counts git activity in the slot as a beat
            if let Some(path) = s(&v, "path") {
                for g in ["index", "HEAD", "logs/HEAD"] {
                    if let Some(m) = git_file_mtime(Path::new(&path), g)
                        && created.is_none_or(|c| m > c + 120_000)
                    {
                        beat = Some(beat.map_or(m, |x| x.max(m)));
                    }
                }
            }
            let pid = pid32(&v, "pid");
            let owner_alive = pid_matches(probe, pid, t(&v, "procStart"));
            let idle = beat.map_or(u64::MAX, |x| now.saturating_sub(x));
            let stale = (owner_alive == Some(false) && idle > LEASE_DEAD_IDLE_MS)
                || idle > LEASE_MAX_IDLE_MS;
            out.push(AgentqLease {
                lease_id: s(&v, "leaseId").unwrap_or_default(),
                repo: s(&v, "repo").unwrap_or_default(),
                slot: s(&v, "slot").unwrap_or_default(),
                role: s(&v, "role"),
                path: s(&v, "path"),
                branch: s(&v, "branch"),
                purpose: s(&v, "purpose"),
                session: s(&v, "session"),
                pid,
                created_ms: created,
                beat_ms: beat,
                owner_alive,
                stale,
            });
        }
    }
    out.sort_by(|a, b| a.repo.cmp(&b.repo).then(a.slot.cmp(&b.slot)));
    out
}

/// mtime of a file inside a worktree's git dir (`.git` is a file pointing at it).
fn git_file_mtime(worktree: &Path, rel: &str) -> Option<u64> {
    let dotgit = worktree.join(".git");
    let gitdir = if dotgit.is_file() {
        let text = fs::read_to_string(&dotgit).ok()?;
        PathBuf::from(text.trim().strip_prefix("gitdir:")?.trim())
    } else {
        dotgit
    };
    fs::metadata(gitdir.join(rel))
        .ok()
        .as_ref()
        .and_then(mtime_ms)
}

#[must_use]
pub fn read_releases(root: &Path, now: u64) -> Vec<AgentqRelease> {
    let mut out: Vec<AgentqRelease> = json_files(&root.join("releases"))
        .iter()
        .filter_map(|p| read_json(p))
        .map(|v| AgentqRelease {
            run_id: s(&v, "runId").unwrap_or_default(),
            repo: s(&v, "repo").unwrap_or_default(),
            target: s(&v, "target").unwrap_or_default(),
            sha: s(&v, "sha"),
            phase: s(&v, "phase"),
            state: s(&v, "state"),
            purpose: s(&v, "purpose"),
            preemptible: b(&v, "preemptible"),
            started_ms: t(&v, "started"),
            phase_at_ms: t(&v, "phaseAt"),
        })
        // agentq-sync shows two days; the page does the same
        .filter(|r| r.state.as_deref() == Some("running") || r.phase_at_ms.or(r.started_ms).is_some_and(|x| now.saturating_sub(x) < 48 * 3_600_000))
        .collect();
    out.sort_by_key(|r| std::cmp::Reverse(r.started_ms));
    out
}

#[must_use]
pub fn read_jobs(root: &Path, probe: ProcStart<'_>, now: u64) -> Vec<AgentqJob> {
    job_records(root, probe, now)
        .into_iter()
        .map(|r| r.job)
        .collect()
}

/// A job and the start time agentq recorded for its process.
///
/// `read_jobs` drops the start time because a row does not show it, but
/// attribution needs it: without it a pid is a number Windows may have
/// recycled, and the work it now runs would be charged to a job that ended.
struct JobRecord {
    job: AgentqJob,
    /// `pidStart` (`.NET` ticks) as Unix milliseconds.
    pid_start_ms: Option<u64>,
}

fn job_records(root: &Path, probe: ProcStart<'_>, now: u64) -> Vec<JobRecord> {
    let dir = root.join("jobs");
    let Ok(rd) = fs::read_dir(&dir) else {
        return Vec::new();
    };
    // ~900 files on the real machine: filter by mtime before parsing
    let mut files: Vec<(u64, PathBuf)> = rd
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| Some((mtime_ms(&e.metadata().ok()?)?, e.path())))
        .collect();
    files.sort_by_key(|f| std::cmp::Reverse(f.0));
    let mut out = Vec::new();
    for (mtime, path) in files {
        let Some(v) = read_json_cached(&path, mtime) else {
            continue;
        };
        let state = s(&v, "state").unwrap_or_else(|| "unknown".into());
        let running = state == "running";
        if !running && now.saturating_sub(mtime) > JOB_WINDOW_MS {
            continue;
        }
        let pid = pid32(&v, "pid");
        // pidStart is .NET UTC ticks
        let recorded = u(&v, "pidStart")
            .and_then(|tk| tk.checked_sub(621_355_968_000_000_000))
            .map(|x| x / 10_000);
        let started = t(&v, "startedAt");
        let expected = f(&v, "expectedMin");
        let log_mtime = s(&v, "log")
            .and_then(|l| fs::metadata(l).ok())
            .as_ref()
            .and_then(mtime_ms);
        let derived = if !running {
            state
        } else if pid_matches(probe, pid, recorded) == Some(false) {
            "dead".to_owned()
        } else {
            let over = matches!((started, expected), (Some(s0), Some(e)) if (now.saturating_sub(s0) as f64) > e * 2.0 * 60_000.0);
            let silent = log_mtime.is_some_and(|m| now.saturating_sub(m) > 5 * 60_000);
            if over || silent {
                "suspect".into()
            } else {
                state
            }
        };
        out.push(JobRecord {
            pid_start_ms: recorded,
            job: AgentqJob {
                id: s(&v, "id").unwrap_or_default(),
                label: s(&v, "label"),
                command: s(&v, "command"),
                repo: s(&v, "repo"),
                pid,
                state: derived,
                exit_code: v
                    .get("exitCode")
                    .and_then(Value::as_i64)
                    .and_then(|x| i32::try_from(x).ok()),
                expected_min: expected,
                started_ms: started,
                ended_ms: t(&v, "endedAt"),
                last_output_ms: log_mtime,
            },
        });
        if out.len() >= 200 {
            break;
        }
    }
    out
}

/// One process agentq is responsible for, and the work it belongs to.
///
/// A job is not the only thing agentq runs. The process that holds a queue
/// ticket is the one that compiles, tests, lands or deploys — its children
/// are the compiler and the packager — and a live release run is the pipeline
/// that builds the installer. Those processes were previously attributed to
/// whichever editor started the session, which is how a whole `rustc` tree
/// ended up charged to a chat window.
///
/// The identity is agentq's own record: the pid *and* the start time it wrote
/// when it acquired the ticket, started the release or launched the job. A pid
/// whose recorded start no longer matches the live process is not returned at
/// all, so Windows recycling a pid cannot charge a stranger's tree to agentq.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Worker {
    pub pid: u32,
    /// The start time agentq recorded, already matched against the live
    /// process.
    pub start_ms: u64,
    /// What to show: the queue resource, `release vitals:any`, or a label.
    pub label: String,
    pub kind: AgentqWorkKind,
    pub resource: Option<String>,
    pub repo: Option<String>,
    pub purpose: Option<String>,
    pub command: Option<String>,
    pub session: Option<String>,
    pub started_ms: Option<u64>,
    /// How long this is expected to take: what the launcher declared, or the
    /// median of the runs of the same resource agentq's journal holds.
    pub estimate: Option<AgentqWorkEstimate>,
}

/// The pid as an identity.
///
/// `Some((pid, recorded))` only when agentq recorded a start time **and** the
/// live process still started at it. A record without a start time is not an
/// identity: the pid alone is a number Windows reuses.
fn verified_start(
    probe: ProcStart<'_>,
    pid: Option<u32>,
    recorded_ms: Option<u64>,
) -> Option<(u32, u64)> {
    let pid = pid?;
    let recorded = recorded_ms?;
    let actual = probe(pid)?;
    (actual.abs_diff(recorded) <= PROC_START_TOLERANCE_MS).then_some((pid, recorded))
}

/// What a piece of agentq work is, from the resource it holds or its command.
///
/// `None` when neither names a kind; each source knows what silence means for
/// its own records. A test run is recognised first, before the resource is
/// consulted: tests queue through `build:` and a suite that compiles and a
/// build that compiles cost the machine very differently.
///
/// This is asked only about a **held ticket**, where both strings are agentq's
/// own record of what it took and what it ran. A release run and a background
/// job do not ask it: agentq recorded a release and a job, and their `purpose`
/// and command are prose or an inline script path, so matching them names the
/// file rather than the work — a job waiting for a freeze to clear reads as a
/// land because its script is called `land-when-unfrozen.ps1`.
#[must_use]
pub fn work_kind(resource: Option<&str>, command: Option<&str>) -> Option<AgentqWorkKind> {
    let cmd = command.unwrap_or_default().to_ascii_lowercase();
    let names_test = [
        "cargo test",
        "cargo nextest",
        "nextest run",
        "vitest",
        "pytest",
        "pnpm test",
        "npm test",
        "yarn test",
        "gradlew test",
        "gate-run",
    ]
    .iter()
    .any(|m| cmd.contains(m));
    if names_test {
        return Some(AgentqWorkKind::Test);
    }
    let res = resource.unwrap_or_default().to_ascii_lowercase();
    for (prefix, kind) in [
        ("build:", AgentqWorkKind::Build),
        ("land:", AgentqWorkKind::Land),
        ("commit:", AgentqWorkKind::Commit),
        ("push:", AgentqWorkKind::Push),
        ("install:", AgentqWorkKind::Install),
        ("deploy:", AgentqWorkKind::Deploy),
    ] {
        if res.starts_with(prefix) {
            return Some(kind);
        }
    }
    // No queue resource: a release run or a job, where the command is the only
    // evidence there is.
    for (marker, kind) in [
        ("land", AgentqWorkKind::Land),
        ("deploy", AgentqWorkKind::Deploy),
        ("build", AgentqWorkKind::Build),
        ("commit", AgentqWorkKind::Commit),
        ("push", AgentqWorkKind::Push),
        ("install", AgentqWorkKind::Install),
    ] {
        if cmd.contains(marker) {
            return Some(kind);
        }
    }
    None
}

/// Every process agentq is responsible for right now, one per pid.
///
/// Queue tickets win over release runs, which win over background jobs, so the
/// most specific label survives when one process holds two records — the
/// holder of `build:vitals` is that build, not "some job".
#[must_use]
pub fn read_workers(root: &Path, probe: ProcStart<'_>, now: u64) -> Vec<Worker> {
    let profiles = crate::journal::shared_profiles(root, now);
    let mut out: Vec<Worker> = Vec::new();
    let mut seen: std::collections::HashSet<u32> = std::collections::HashSet::new();
    let mut keep = |w: Worker| seen.insert(w.pid).then_some(w);
    for w in ticket_workers(root, probe, &profiles) {
        if let Some(w) = keep(w) {
            out.push(w);
        }
    }
    for w in release_workers(root, probe, &profiles) {
        if let Some(w) = keep(w) {
            out.push(w);
        }
    }
    for w in job_workers(root, probe, now) {
        if let Some(w) = keep(w) {
            out.push(w);
        }
    }
    out
}

/// The median of what this resource cost the last few times, when the journal
/// holds any run of it.
///
/// The figure is agentq's own: every `done` event carries the seconds the work
/// took, so the estimate is a measurement rather than a guess at one. A
/// resource nobody has finished gets no estimate, and `None` is drawn as no
/// bar — a made-up ETA reads exactly like a known one.
fn measured(profiles: &WorkProfiles, resources: &str) -> Option<AgentqWorkEstimate> {
    let (sec, samples) = profiles.median_sec(resources)?;
    let expected_ms = (sec * 1000.0).round();
    if !expected_ms.is_finite() || expected_ms <= 0.0 {
        return None;
    }
    Some(AgentqWorkEstimate {
        expected_ms: expected_ms as u64,
        basis: AgentqEstimateBasis::Measured,
        samples,
    })
}

/// What a launcher said its own job would take: `job-start.ps1 -ExpectMin`.
///
/// agentq already calls a job `suspect` at twice this figure, so the page and
/// agentq read the same number the same way.
fn declared(expected_min: Option<f64>) -> Option<AgentqWorkEstimate> {
    let minutes = expected_min?;
    if !minutes.is_finite() || minutes <= 0.0 {
        return None;
    }
    let expected_ms = (minutes * 60_000.0).round();
    if !expected_ms.is_finite() || expected_ms <= 0.0 {
        return None;
    }
    Some(AgentqWorkEstimate {
        expected_ms: expected_ms as u64,
        basis: AgentqEstimateBasis::Declared,
        samples: 0,
    })
}

/// The processes holding a queue ticket: the resource's actual work.
fn ticket_workers(root: &Path, probe: ProcStart<'_>, profiles: &WorkProfiles) -> Vec<Worker> {
    let Ok(rd) = fs::read_dir(root.join("queues")) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for dir in rd
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_dir())
    {
        for file in json_files(&dir) {
            let Some(v) = read_json(&file) else { continue };
            // A queued-but-waiting ticket owns no process, so it owns no
            // resource use either.
            if !b(&v, "held").unwrap_or(false) {
                continue;
            }
            let Some((pid, start_ms)) = verified_start(probe, pid32(&v, "pid"), t(&v, "procStart"))
            else {
                continue;
            };
            let resource = s(&v, "resource")
                .or_else(|| dir.file_name().and_then(|n| n.to_str()).map(unsafe_name));
            let command = s(&v, "cmd");
            let estimate = resource.as_deref().and_then(|r| measured(profiles, r));
            out.push(Worker {
                label: resource.clone().unwrap_or_else(|| "agentq".to_owned()),
                kind: work_kind(resource.as_deref(), command.as_deref())
                    .unwrap_or(AgentqWorkKind::Other),
                pid,
                start_ms,
                resource,
                repo: s(&v, "repo"),
                purpose: s(&v, "purpose"),
                command,
                session: s(&v, "session"),
                started_ms: t(&v, "acquired").or_else(|| t(&v, "created")),
                estimate,
            });
        }
    }
    out
}

/// The process running a release: agentq's own pipeline, whose children are
/// the compiler and the packager of an installer.
fn release_workers(root: &Path, probe: ProcStart<'_>, profiles: &WorkProfiles) -> Vec<Worker> {
    let mut out = Vec::new();
    for file in json_files(&root.join("releases")) {
        let Some(v) = read_json(&file) else { continue };
        if s(&v, "state").as_deref() != Some("running") {
            continue;
        }
        let Some((pid, start_ms)) = verified_start(probe, pid32(&v, "pid"), t(&v, "procStart"))
        else {
            continue;
        };
        let repo = s(&v, "repo").unwrap_or_default();
        let target = s(&v, "target").unwrap_or_default();
        // agentq records the command a release runs in `purpose`. The kind is
        // not derived from it: a release run is a deploy whatever its command
        // and prose happen to say.
        let command = s(&v, "purpose");
        // The resource the run holds, in agentq's own spelling: its journal
        // names a release run `deploy:<repo>:<target>`, holds a ticket for it
        // and records the run's duration against that name.
        let resource = format!("deploy:{repo}:{target}");
        let estimate = measured(profiles, &resource);
        out.push(Worker {
            label: format!("release {repo}:{target}"),
            kind: AgentqWorkKind::Deploy,
            pid,
            start_ms,
            resource: Some(resource),
            repo: Some(repo),
            purpose: None,
            command,
            session: s(&v, "session"),
            started_ms: t(&v, "started"),
            estimate,
        });
    }
    out
}

/// The processes of the background jobs still running.
fn job_workers(root: &Path, probe: ProcStart<'_>, now: u64) -> Vec<Worker> {
    job_records(root, probe, now)
        .into_iter()
        .filter(|r| r.job.state == "running" || r.job.state == "suspect")
        .filter_map(|r| {
            let job = r.job;
            Some(Worker {
                label: job.label.clone().unwrap_or_else(|| job.id.clone()),
                // A background job is a job. Its command is often an inline
                // `-File <path>`, and matching that text would name the script.
                kind: AgentqWorkKind::Job,
                pid: job.pid?,
                start_ms: r.pid_start_ms?,
                resource: None,
                repo: job.repo,
                purpose: None,
                command: job.command,
                session: None,
                started_ms: job.started_ms,
                estimate: declared(job.expected_min),
            })
        })
        .collect()
}

#[must_use]
pub fn read_marks(root: &Path) -> Vec<AgentqMark> {
    json_files(&root.join("marks"))
        .iter()
        .filter_map(|p| read_json(p))
        .filter_map(|v| {
            Some(AgentqMark {
                resource: s(&v, "resource")?,
                state: s(&v, "state")?,
                reason: s(&v, "reason"),
                session: s(&v, "session"),
                at_ms: t(&v, "at"),
            })
        })
        .collect()
}

/// `agents/<session>.json` written by agentq on every call (S18-09).
#[must_use]
pub fn read_registry(root: &Path, now: u64) -> Vec<AgentqCaller> {
    let mut out: Vec<AgentqCaller> = json_files(&root.join("agents"))
        .iter()
        .filter_map(|p| read_json(p))
        .filter_map(|v| {
            Some(AgentqCaller {
                session: s(&v, "session")?,
                client: s(&v, "client"),
                app: s(&v, "app"),
                app_pid: pid32(&v, "appPid"),
                shell_pid: pid32(&v, "shellPid"),
                model: s(&v, "model"),
                repo: s(&v, "repo"),
                cwd: s(&v, "cwd"),
                last_verb: s(&v, "lastVerb"),
                last_purpose: s(&v, "lastPurpose"),
                calls: u(&v, "calls").and_then(|c| u32::try_from(c).ok()),
                first_seen_ms: t(&v, "firstSeen"),
                last_seen_ms: t(&v, "lastSeen"),
            })
        })
        .filter(|c| {
            c.last_seen_ms
                .is_some_and(|x| now.saturating_sub(x) < 24 * 3_600_000)
        })
        .collect();
    out.sort_by_key(|c| std::cmp::Reverse(c.last_seen_ms));
    out
}

#[must_use]
pub fn read_cloud_down(root: &Path, now: u64) -> Vec<CloudDown> {
    json_files(&root.join("cloud-down"))
        .iter()
        .filter_map(|p| read_json(p))
        .filter_map(|v| {
            let until = t(&v, "until");
            if until.is_some_and(|x| x < now) {
                return None;
            }
            Some(CloudDown {
                workspace: s(&v, "ws")?,
                reason: s(&v, "reason"),
                until_ms: until,
            })
        })
        .collect()
}

#[must_use]
pub fn read_machine_cap(root: &Path) -> Option<MachineCap> {
    let v = read_json(
        &root
            .join("queues")
            .join("build_machine")
            .join("adaptive.json"),
    )?;
    Some(MachineCap {
        cap: u32::try_from(u(&v, "cap")?).ok()?,
        min: u(&v, "min").and_then(|x| u32::try_from(x).ok()),
        max: u(&v, "max").and_then(|x| u32::try_from(x).ok()),
        cpu: f(&v, "cpu"),
        free_ram_gb: f(&v, "freeRamGB"),
        free_commit_gb: f(&v, "freeCommitGB"),
        why: s(&v, "why"),
        at_ms: t(&v, "at"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Tmp(PathBuf);
    impl Tmp {
        fn new(tag: &str) -> Self {
            let p =
                std::env::temp_dir().join(format!("vitals-agents-{tag}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&p);
            fs::create_dir_all(&p).unwrap();
            Self(p)
        }
        fn write(&self, rel: &str, body: &str) -> PathBuf {
            let p = self.0.join(rel);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(&p, body).unwrap();
            p
        }
    }
    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    const NOW: u64 = 1_792_207_771_170; // 2026-10-17T03:29:31.170Z

    #[test]
    fn a_recycled_pid_is_dead_even_though_a_process_with_that_number_runs() {
        let d = Tmp::new("recycle");
        d.write(
            "queues/build_codai/00000001-aaaa.json",
            r#"{"id":"aaaa","seq":1,"resource":"build:codai","pid":42,"procStart":"2026-10-17T03:00:00Z","held":true}"#,
        );
        // pid 42 exists, but started an hour later than the ticket says
        let probe = |_: u32| Some(NOW);
        let r = read_resources(&d.0, &probe, NOW);
        assert_eq!(r[0].tickets[0].liveness, Liveness::Dead);
        let probe_same = |_: u32| parse_iso_ms("2026-10-17T03:00:01.5Z");
        let r = read_resources(&d.0, &probe_same, NOW);
        assert_ne!(
            r[0].tickets[0].liveness,
            Liveness::Dead,
            "1.5 s apart is within agentq's 3 s"
        );
    }

    #[test]
    fn an_old_heartbeat_makes_a_live_holder_stale_and_a_missing_pid_unknown() {
        let d = Tmp::new("stale");
        d.write(
            "queues/build_x/00000001-a.json",
            r#"{"id":"a","seq":1,"resource":"build:x","pid":7,"created":"2026-10-17T02:00:00Z"}"#,
        );
        d.write(
            "queues/build_x/00000002-b.json",
            r#"{"id":"b","seq":2,"resource":"build:x"}"#,
        );
        let probe = |_: u32| Some(0);
        let r = read_resources(&d.0, &probe, NOW);
        // no .hb -> created (89 min ago) is the beat -> stale
        assert_eq!(r[0].tickets[0].liveness, Liveness::Stale);
        assert_eq!(r[0].tickets[1].liveness, Liveness::Unknown);
    }

    #[test]
    fn holders_sort_before_waiters_and_sidecar_files_are_not_tickets() {
        let d = Tmp::new("order");
        d.write(
            "queues/build_machine/00000005-w.json",
            r#"{"id":"w","seq":5,"resource":"build:machine"}"#,
        );
        d.write(
            "queues/build_machine/00000009-h.json",
            r#"{"id":"h","seq":9,"resource":"build:machine","held":true}"#,
        );
        d.write(
            "queues/build_machine/adaptive.json",
            r#"{"cap":3,"cpu":50.0,"at":"2026-10-17T03:29:00Z"}"#,
        );
        d.write(
            "queues/build_machine/burst.json",
            r#"{"resource":"build:machine","until":"x"}"#,
        );
        d.write("queues/build_machine/capacity", "3");
        d.write("queues/build_machine/00000010-x.json.tmp", "{half");
        let probe = |_: u32| None;
        let r = read_resources(&d.0, &probe, NOW);
        assert_eq!(r[0].kind, ResourceKind::Machine);
        assert_eq!(r[0].capacity, Some(3));
        let ids: Vec<_> = r[0].tickets.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, ["h", "w"]);
        assert_eq!(read_machine_cap(&d.0).map(|m| m.cap), Some(3));
    }

    #[test]
    fn a_lease_whose_owner_died_two_hours_ago_is_stale_but_a_fresh_one_is_not() {
        let d = Tmp::new("lease");
        d.write(
            "leases/codai/task-1.json",
            r#"{"leaseId":"l1","repo":"codai","slot":"task-1","pid":5,"created":"2026-10-17T01:00:00Z"}"#,
        );
        d.write(
            "leases/codai/task-2.json",
            r#"{"leaseId":"l2","repo":"codai","slot":"task-2","pid":5,"created":"2026-10-17T03:20:00Z"}"#,
        );
        let dead = |_: u32| None;
        let l = read_leases(&d.0, &dead, NOW);
        assert!(l[0].stale && l[0].owner_alive == Some(false));
        assert!(!l[1].stale, "owner dead but idle only 9 min");
    }

    #[test]
    fn a_running_job_whose_process_is_gone_is_dead_and_an_overdue_one_is_suspect() {
        let d = Tmp::new("jobs");
        d.write(
            "jobs/j1.json",
            r#"{"id":"j1","state":"running","pid":9,"startedAt":"2026-10-17T03:25:00Z","expectedMin":10}"#,
        );
        d.write(
            "jobs/j2.json",
            r#"{"id":"j2","state":"running","pid":9,"startedAt":"2026-10-17T02:00:00Z","expectedMin":10}"#,
        );
        let gone = |_: u32| None;
        let j = read_jobs(&d.0, &gone, NOW);
        assert!(j.iter().all(|x| x.state == "dead"));
        let alive = |_: u32| Some(0);
        let j = read_jobs(&d.0, &alive, NOW);
        let by = |id: &str| j.iter().find(|x| x.id == id).map(|x| x.state.clone());
        assert_eq!(by("j1").as_deref(), Some("running"));
        assert_eq!(by("j2").as_deref(), Some("suspect"));
    }

    #[test]
    fn an_expired_cloud_down_record_is_hidden() {
        let d = Tmp::new("cloud");
        d.write(
            "cloud-down/codai.json",
            r#"{"ws":"codai","until":"2026-10-17T04:00:00Z","reason":"404"}"#,
        );
        d.write(
            "cloud-down/old.json",
            r#"{"ws":"old","until":"2026-10-17T01:00:00Z"}"#,
        );
        let c = read_cloud_down(&d.0, NOW);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].workspace, "codai");
    }

    #[test]
    fn a_bom_written_by_windows_powershell_does_not_hide_a_file() {
        let d = Tmp::new("bom");
        d.write(
            "marks/build_x.json",
            "\u{feff}{\"resource\":\"build:x\",\"state\":\"frozen\",\"reason\":\"incident\"}",
        );
        assert_eq!(read_marks(&d.0)[0].state, "frozen");
    }

    #[test]
    fn a_worker_whose_text_merely_mentions_a_kind_is_not_reported_as_one() {
        let d = Tmp::new("workers");
        // On the real machine this job is a waiter whose script is called
        // `land-when-unfrozen.ps1`; matching the command named the file and
        // reported a background job as a land. A job is a job.
        d.write(
            "jobs/j1.json",
            &format!(
                r#"{{"id":"j1","label":"wave8 land when the freeze clears","state":"running","pid":9,"command":"-File \"E:\\gh\\.copilot-tmp\\land-when-unfrozen.ps1\" -MaxMin 150","startedAt":"2026-10-17T03:28:00Z","expectedMin":10,"pidStart":{ticks}}}"#,
                ticks = (NOW * 10_000) + 621_355_968_000_000_000
            ),
        );
        // And this release run compiles an installer, so its purpose mentions
        // a build: the run is still a deploy.
        d.write(
            "releases/vitals__any.json",
            r#"{"runId":"r1","repo":"vitals","target":"any","sha":"abc","ref":"main","follow":true,"phase":"build","state":"running","session":"term-1","pid":11,"procStart":"2026-10-17T03:29:31Z","started":"2026-10-17T03:00:00Z","purpose":"pnpm build && node scripts/release.mjs run","preemptible":false,"committed":true}"#,
        );
        let probe = |_: u32| Some(NOW);
        let workers = read_workers(&d.0, &probe, NOW);
        let job = workers.iter().find(|w| w.pid == 9).expect("the job");
        assert_eq!(job.kind, AgentqWorkKind::Job, "prose must not name it");
        let release = workers.iter().find(|w| w.pid == 11).expect("the release");
        assert_eq!(release.kind, AgentqWorkKind::Deploy);
        assert_eq!(release.label, "release vitals:any");
    }
}
