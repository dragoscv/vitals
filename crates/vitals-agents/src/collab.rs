//! agentq's cross-agent channel — who is present, what they asked each other,
//! and what work is in flight (ADR-0039, ADR-0049).
//!
//! Read-only by construction: nothing here opens a file for writing. The
//! shapes are agentq's own, written by `agentq-msgs.ps1` and its registry and
//! job writers, and the field names are taken from those writers rather than
//! guessed — a reader that disagrees with them shows an empty page while the
//! CLI shows a full one.
//!
//! # Unknown is `None`
//!
//! Every field is optional. A registry record written by an older agentq lacks
//! fields a newer one has, a `firstSeen` written in the US date format does not
//! parse as ISO, and a job that has not started has no start time — none of
//! these is an error, and none of them becomes a default.

use std::path::Path;

use serde_json::Value;
use vitals_core::agents::{
    CollabAgent, CollabAnswer, CollabJob, CollabSender, CollabSnapshot, CollabThread,
};

use crate::agentq::{json_files, pid32, read_json, s, t, u};

/// How long after its last call an agent still counts as present, in minutes.
///
/// agentq's own window (`Get-LiveAgents` in `agentq-msgs.ps1`): an address
/// reaches the agents whose `lastSeen` is inside it, and "asked" is a
/// different outcome from "asked nobody". The page shows the same number so
/// the CLI and the screen agree.
pub const LIVE_MIN: u32 = 30;
/// A registry record last seen within a day is still worth a row, as on the AI
/// page's own registry read.
const AGENT_WINDOW_MS: u64 = 24 * 3_600_000;
/// A finished job older than this is history; a running one is always shown.
const JOB_WINDOW_MS: u64 = 24 * 3_600_000;

/// Reads the collaboration view from agentq's coordination directory.
///
/// `root` is `%USERPROFILE%\.codai\coord` (or agentq's own `AGENTQ_HOME`). A
/// directory that does not exist is not an error: the snapshot comes back
/// `installed == false` with empty lists, so a machine without agentq renders
/// an explained empty page rather than a failure.
#[must_use]
pub fn read(root: &Path, now: u64) -> CollabSnapshot {
    CollabSnapshot {
        taken_at_ms: now,
        installed: root.is_dir(),
        agents: read_agents(root, now),
        threads: read_threads(root),
        jobs: read_jobs(root, now),
        liveness_min: LIVE_MIN,
    }
}

/// The registry (`coord/agents/<session>.json`): everyone who has called
/// agentq, marked present or not by the same window the writer uses.
fn read_agents(root: &Path, now: u64) -> Vec<CollabAgent> {
    let live_window = u64::from(LIVE_MIN) * 60_000;
    let mut out: Vec<CollabAgent> = json_files(&root.join("agents"))
        .iter()
        .filter_map(|p| read_json(p))
        .filter_map(|v| {
            Some(CollabAgent {
                session: s(&v, "session")?,
                client: s(&v, "client"),
                model: s(&v, "model"),
                repo: s(&v, "repo"),
                cwd: s(&v, "cwd"),
                host: s(&v, "host"),
                user: s(&v, "user"),
                calls: u(&v, "calls").and_then(|c| u32::try_from(c).ok()),
                last_verb: s(&v, "lastVerb"),
                last_purpose: s(&v, "lastPurpose"),
                first_seen_ms: t(&v, "firstSeen"),
                last_seen_ms: t(&v, "lastSeen"),
                live: false,
            })
        })
        .filter(|a| {
            a.last_seen_ms
                .is_some_and(|x| now.saturating_sub(x) < AGENT_WINDOW_MS)
        })
        .collect();
    for a in &mut out {
        a.live = a
            .last_seen_ms
            .is_some_and(|x| now.saturating_sub(x) < live_window);
    }
    out.sort_by_key(|a| std::cmp::Reverse(a.last_seen_ms));
    out
}

/// `coord/msgs/<8hex>.json`: one directed message per file. The file *is* the
/// thread — the ask plus the answers agentq appended to it — so grouping is
/// reading the `answers` array rather than joining anything.
fn read_threads(root: &Path) -> Vec<CollabThread> {
    let mut out: Vec<CollabThread> = json_files(&root.join("msgs"))
        .iter()
        .filter_map(|p| read_json(p))
        .filter_map(|v| {
            let answers = v
                .get("answers")
                .and_then(Value::as_array)
                .map(|list| list.iter().filter_map(answer).collect::<Vec<_>>())
                .unwrap_or_default();
            let asked_at_ms = t(&v, "askedAt");
            // Newest activity: the latest answer, or the ask when there are
            // none. `None` throughout sorts last rather than first, so a file
            // whose timestamps did not parse cannot jump to the top.
            let last_ms = answers
                .iter()
                .filter_map(|a| a.at_ms)
                .fold(asked_at_ms, |acc, at| Some(acc.map_or(at, |x| x.max(at))));
            Some(CollabThread {
                id: s(&v, "id")?,
                to: s(&v, "to").unwrap_or_default(),
                resource: s(&v, "resource"),
                repo: s(&v, "repo"),
                message: s(&v, "message").unwrap_or_default(),
                state: s(&v, "state"),
                from: v.get("from").map(sender).unwrap_or_default(),
                answers,
                asked_at_ms,
                last_ms,
            })
        })
        .collect();
    out.sort_by_key(|t| std::cmp::Reverse(t.last_ms));
    out
}

/// One answer in the `answers` array. A record with no message is not an
/// answer, so it is dropped rather than shown blank.
fn answer(v: &Value) -> Option<CollabAnswer> {
    Some(CollabAnswer {
        session: s(v, "session"),
        repo: s(v, "repo"),
        user: s(v, "user"),
        message: s(v, "message")?,
        at_ms: t(v, "at"),
    })
}

fn sender(v: &Value) -> CollabSender {
    CollabSender {
        session: s(v, "session"),
        repo: s(v, "repo"),
        user: s(v, "user"),
        host: s(v, "host"),
        cwd: s(v, "cwd"),
        pid: pid32(v, "pid"),
    }
}

/// `coord/jobs/<id>.json`: what agentq launched in the background. Reduced to
/// the fields that say an agent is working — id, label, repo, command, state,
/// exit code and the times.
fn read_jobs(root: &Path, now: u64) -> Vec<CollabJob> {
    let mut out: Vec<CollabJob> = json_files(&root.join("jobs"))
        .iter()
        .filter_map(|p| read_json(p))
        .filter_map(|v| {
            let state = s(&v, "state").unwrap_or_else(|| "unknown".into());
            let ended_ms = t(&v, "endedAt");
            // A finished job older than the window is history; a running one is
            // always work in progress.
            if state != "running" && ended_ms.is_some_and(|e| now.saturating_sub(e) > JOB_WINDOW_MS)
            {
                return None;
            }
            Some(CollabJob {
                id: s(&v, "id")?,
                label: s(&v, "label"),
                repo: s(&v, "repo"),
                command: s(&v, "command"),
                state,
                exit_code: v
                    .get("exitCode")
                    .and_then(Value::as_i64)
                    .and_then(|x| i32::try_from(x).ok()),
                started_ms: t(&v, "startedAt"),
                ended_ms,
            })
        })
        .collect();
    // Running first, then newest: what is in flight is the thing to see.
    out.sort_by(|a, b| {
        (b.state == "running")
            .cmp(&(a.state == "running"))
            .then(b.started_ms.cmp(&a.started_ms))
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    struct Tmp(PathBuf);
    impl Tmp {
        fn new(tag: &str) -> Self {
            let p =
                std::env::temp_dir().join(format!("vitals-collab-{tag}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&p);
            std::fs::create_dir_all(&p).unwrap();
            Self(p)
        }
        fn write(&self, rel: &str, body: &str) -> PathBuf {
            let p = self.0.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, body).unwrap();
            p
        }
    }
    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    const NOW: u64 = 1_792_207_771_170; // 2026-10-17T03:29:31.170Z

    #[test]
    fn a_missing_coordination_directory_reports_not_installed_and_no_rows() {
        let snap = read(Path::new("Z:/definitely/not/here"), NOW);
        assert!(!snap.installed);
        assert!(snap.agents.is_empty() && snap.threads.is_empty() && snap.jobs.is_empty());
        assert_eq!(snap.taken_at_ms, NOW);
    }

    #[test]
    fn an_agent_last_seen_inside_the_window_is_present_and_a_stale_one_is_not_dropped() {
        let d = Tmp::new("live");
        // 5 minutes ago: present. 3 hours ago: listed, marked not present.
        d.write(
            "agents/recent.json",
            r#"{"session":"s-recent","client":"vscode-copilot","repo":"vitals","lastSeen":"2026-10-17T03:24:31.170Z"}"#,
        );
        d.write(
            "agents/older.json",
            r#"{"session":"s-old","client":"copilot-cli","repo":"codai","lastSeen":"2026-10-17T00:29:31.170Z"}"#,
        );
        // 2 days ago: outside the daily window, so not a row at all.
        d.write(
            "agents/ancient.json",
            r#"{"session":"s-ancient","lastSeen":"2026-10-15T03:29:31.170Z"}"#,
        );
        let snap = read(&d.0, NOW);
        assert!(snap.installed);
        let sessions: Vec<&str> = snap.agents.iter().map(|a| a.session.as_str()).collect();
        assert_eq!(sessions, ["s-recent", "s-old"]);
        assert!(snap.agents[0].live && !snap.agents[1].live);
    }

    #[test]
    fn a_first_seen_in_the_us_date_format_is_none_rather_than_zero() {
        // agentq's writer has emitted `M/D/YYYY H:MM:SS` for `firstSeen`; it is
        // not ISO, so the field is unknown, and unknown is None — never 0.
        let d = Tmp::new("firstseen");
        d.write(
            "agents/a.json",
            r#"{"session":"s","firstSeen":"11/24/2026 18:02:12","lastSeen":"2026-10-17T03:24:31.170Z"}"#,
        );
        let snap = read(&d.0, NOW);
        assert_eq!(snap.agents[0].first_seen_ms, None);
        assert!(snap.agents[0].last_seen_ms.is_some());
    }

    #[test]
    fn a_answered_thread_sorts_above_a_newer_unanswered_one_and_keeps_its_replies() {
        let d = Tmp::new("threads");
        // Asked first, answered last: its newest activity (the reply) is the
        // most recent of all, so it comes first despite the later ask below.
        d.write(
            "msgs/aaaaaaaa.json",
            r#"{"id":"aaaaaaaa","to":"repo:vitals","state":"answered","message":"may I take build:vitals?","askedAt":"2026-10-17T03:00:00Z","answers":[{"session":"s-b","repo":"vitals","at":"2026-10-17T03:25:00Z","message":"yes, I am done"}]}"#,
        );
        d.write(
            "msgs/bbbbbbbb.json",
            r#"{"id":"bbbbbbbb","to":"any","state":"open","message":"anyone seen the deploy?","askedAt":"2026-10-17T03:20:00Z","answers":[]}"#,
        );
        let snap = read(&d.0, NOW);
        let ids: Vec<&str> = snap.threads.iter().map(|x| x.id.as_str()).collect();
        assert_eq!(ids, ["aaaaaaaa", "bbbbbbbb"]);
        let first = &snap.threads[0];
        assert_eq!(first.answers.len(), 1);
        assert_eq!(first.answers[0].session.as_deref(), Some("s-b"));
        assert_eq!(first.last_ms, first.answers[0].at_ms);
        // An unanswered thread's last activity is its own ask.
        assert_eq!(snap.threads[1].last_ms, snap.threads[1].asked_at_ms);
    }

    #[test]
    fn a_message_with_no_answers_carries_its_author_and_an_empty_answer_list() {
        let d = Tmp::new("sender");
        d.write(
            "msgs/cccccccc.json",
            r#"{"id":"cccccccc","to":"slot:vmui:task-1","message":"are you editing agentq.ps1?","askedAt":"2026-10-17T03:20:00Z","state":"open","from":{"session":"s-a","repo":"agentq","cwd":"E:\\gh\\agentq","host":"DRAGOS","pid":77712},"answers":[]}"#,
        );
        let snap = read(&d.0, NOW);
        let t0 = &snap.threads[0];
        assert_eq!(t0.from.session.as_deref(), Some("s-a"));
        assert_eq!(t0.from.pid, Some(77712));
        assert!(t0.answers.is_empty());
        assert_eq!(t0.to, "slot:vmui:task-1");
    }

    #[test]
    fn a_running_job_is_kept_even_when_old_and_is_listed_before_finished_work() {
        let d = Tmp::new("jobs");
        // Finished 3 days ago: outside the window, dropped.
        d.write(
            "jobs/old.json",
            r#"{"id":"old","state":"done","endedAt":"2026-10-14T03:29:31.170Z","startedAt":"2026-10-14T03:00:00.000Z"}"#,
        );
        // A long-running job with no end: work in progress, kept.
        d.write(
            "jobs/run.json",
            r#"{"id":"run","label":"land-x","repo":"codai","command":"pwsh -File land.ps1","state":"running","startedAt":"2026-10-17T02:00:00.000Z"}"#,
        );
        d.write(
            "jobs/recent.json",
            r#"{"id":"recent","state":"failed","exitCode":1,"endedAt":"2026-10-17T03:10:00.000Z","startedAt":"2026-10-17T03:00:00.000Z"}"#,
        );
        let snap = read(&d.0, NOW);
        let ids: Vec<&str> = snap.jobs.iter().map(|j| j.id.as_str()).collect();
        assert_eq!(ids, ["run", "recent"]);
        assert_eq!(snap.jobs[1].exit_code, Some(1));
        assert!(snap.jobs[0].ended_ms.is_none());
    }
}
