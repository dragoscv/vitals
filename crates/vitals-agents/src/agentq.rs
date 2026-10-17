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
    AgentqCaller, AgentqJob, AgentqLease, AgentqMark, AgentqRelease, AgentqResource, AgentqTicket,
    CloudDown, Liveness, MachineCap, ResourceKind,
};

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

fn read_json(path: &Path) -> Option<Value> {
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

fn json_files(dir: &Path) -> Vec<PathBuf> {
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

fn s(v: &Value, k: &str) -> Option<String> {
    match v.get(k)? {
        Value::String(x) if !x.is_empty() => Some(x.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}
fn u(v: &Value, k: &str) -> Option<u64> {
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
fn t(v: &Value, k: &str) -> Option<u64> {
    v.get(k)?.as_str().and_then(parse_iso_ms)
}
fn pid32(v: &Value, k: &str) -> Option<u32> {
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
        out.push(AgentqJob {
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
        });
        if out.len() >= 200 {
            break;
        }
    }
    out
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
}
