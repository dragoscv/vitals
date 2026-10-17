//! agentq's `journal.jsonl`: an append-only log of ~50k lines.
//!
//! Reading the whole file every refresh would cost tens of milliseconds and
//! grow without bound, so [`JournalTail`] remembers its byte offset and parses
//! only what was appended since. On the first read it starts from the last
//! [`INITIAL_BYTES`] so the statistics window is populated without scanning
//! months of history. A file that shrank (agentq rotates at 64 MB) resets the
//! offset.

use std::collections::{BTreeMap, VecDeque};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use serde_json::Value;
use vitals_core::agents::{AgentqStats, JournalEvent, RepoStat};

use crate::time::parse_iso_ms;

/// How far back the first read reaches. ~6 MB is a few days on this machine.
pub const INITIAL_BYTES: u64 = 6 * 1024 * 1024;
/// Events kept in memory for the statistics window.
const KEEP_HOURS: u64 = 24;
/// Events returned to the UI timeline.
pub const TIMELINE_LEN: usize = 300;

#[derive(Debug, Default)]
pub struct JournalTail {
    offset: u64,
    /// A partial last line, kept until its newline arrives.
    carry: Vec<u8>,
    events: VecDeque<JournalEvent>,
}

impl JournalTail {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Reads whatever was appended since the last call.
    pub fn poll(&mut self, path: &Path, now: u64) {
        let Ok(mut file) = File::open(path) else {
            return;
        };
        let Ok(len) = file.metadata().map(|m| m.len()) else {
            return;
        };
        if len < self.offset {
            // rotated
            self.offset = 0;
            self.carry.clear();
            self.events.clear();
        }
        if self.offset == 0 && len > INITIAL_BYTES {
            self.offset = len - INITIAL_BYTES;
            // the first (partial) line is dropped by the carry logic below
            self.carry.clear();
            self.carry.push(b'\0');
        }
        if len == self.offset {
            self.trim(now);
            return;
        }
        if file.seek(SeekFrom::Start(self.offset)).is_err() {
            return;
        }
        let mut buf = Vec::with_capacity(usize::try_from(len - self.offset).unwrap_or(0));
        if file.take(len - self.offset).read_to_end(&mut buf).is_err() {
            return;
        }
        self.offset = len;
        let mut data = std::mem::take(&mut self.carry);
        let skip_first = data.first() == Some(&b'\0');
        if skip_first {
            data.clear();
        }
        data.extend_from_slice(&buf);
        let mut lines = data.split(|c| *c == b'\n').peekable();
        let mut first = true;
        while let Some(line) = lines.next() {
            if lines.peek().is_none() {
                // no trailing newline yet
                self.carry = line.to_vec();
                break;
            }
            if first && skip_first {
                first = false;
                continue;
            }
            first = false;
            if let Some(e) = parse_line(line) {
                self.events.push_back(e);
            }
        }
        self.trim(now);
    }

    fn trim(&mut self, now: u64) {
        let cutoff = now.saturating_sub(KEEP_HOURS * 3_600_000);
        while self.events.front().is_some_and(|e| e.ts_ms < cutoff) {
            self.events.pop_front();
        }
    }

    /// Newest first, at most [`TIMELINE_LEN`].
    #[must_use]
    pub fn timeline(&self) -> Vec<JournalEvent> {
        self.events
            .iter()
            .rev()
            .take(TIMELINE_LEN)
            .cloned()
            .collect()
    }

    #[must_use]
    pub fn stats(&self, now: u64, window_hours: u32) -> AgentqStats {
        stats(self.events.iter(), now, window_hours)
    }
}

fn parse_line(line: &[u8]) -> Option<JournalEvent> {
    let line = std::str::from_utf8(line).ok()?.trim();
    if line.is_empty() {
        return None;
    }
    let v: Value = serde_json::from_str(line.trim_start_matches('\u{feff}')).ok()?;
    let str_of = |k: &str| {
        v.get(k)
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
    };
    let num = |k: &str| v.get(k).and_then(Value::as_f64);
    let text = str_of("purpose")
        .or_else(|| str_of("reason"))
        .or_else(|| str_of("message"));
    Some(JournalEvent {
        ts_ms: str_of("ts").as_deref().and_then(parse_iso_ms)?,
        event: str_of("event")?,
        resource: str_of("resource"),
        session: str_of("session"),
        repo: str_of("repo"),
        lane: str_of("lane"),
        text: text.map(|t| truncate(&t, 240)),
        exit: v
            .get("exit")
            .and_then(Value::as_i64)
            .and_then(|x| i32::try_from(x).ok()),
        wait_sec: num("waitSec"),
        hold_sec: num("holdSec"),
    })
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_owned();
    }
    let mut out: String = s.chars().take(max).collect();
    out.push('…');
    out
}

fn percentile(sorted: &[f64], p: f64) -> Option<f64> {
    if sorted.is_empty() {
        return None;
    }
    let idx = ((sorted.len() - 1) as f64 * p).round() as usize;
    sorted.get(idx).copied()
}

/// Rollups over the last `window_hours`.
pub fn stats<'a>(
    events: impl Iterator<Item = &'a JournalEvent>,
    now: u64,
    window_hours: u32,
) -> AgentqStats {
    let window_ms = u64::from(window_hours) * 3_600_000;
    let start = now.saturating_sub(window_ms);
    let mut st = AgentqStats {
        window_hours,
        ..AgentqStats::default()
    };
    st.runs_per_hour = vec![0; window_hours as usize];
    let (mut waits, mut holds) = (Vec::new(), Vec::new());
    let mut repos: BTreeMap<String, RepoStat> = BTreeMap::new();
    for e in events.filter(|e| e.ts_ms >= start && e.ts_ms <= now) {
        match e.event.as_str() {
            "start" => {
                st.runs += 1;
                let bucket = ((e.ts_ms - start) / 3_600_000) as usize;
                if let Some(b) = st.runs_per_hour.get_mut(bucket) {
                    *b += 1;
                }
                if let Some(w) = e.wait_sec {
                    waits.push(w);
                    st.wait_total_sec += w;
                    if let Some(r) = &e.repo {
                        repo_entry(&mut repos, r).wait_total_sec += w;
                    }
                }
                if let Some(r) = &e.repo {
                    repo_entry(&mut repos, r).runs += 1;
                }
            }
            "done" | "fail" => {
                if let Some(h) = e.hold_sec {
                    holds.push(h);
                    st.hold_total_sec += h;
                    if let Some(r) = &e.repo {
                        repo_entry(&mut repos, r).hold_total_sec += h;
                    }
                }
                if e.event == "fail" {
                    st.failures += 1;
                    if let Some(r) = &e.repo {
                        repo_entry(&mut repos, r).failures += 1;
                    }
                }
            }
            "commit" => st.commits += 1,
            "cloud-done" | "cloud-fail" => st.cloud_runs += 1,
            _ => {}
        }
    }
    waits.sort_by(f64::total_cmp);
    holds.sort_by(f64::total_cmp);
    st.wait_p50_sec = percentile(&waits, 0.5);
    st.wait_p90_sec = percentile(&waits, 0.9);
    st.hold_p50_sec = percentile(&holds, 0.5);
    st.hold_p90_sec = percentile(&holds, 0.9);
    let mut by_repo: Vec<RepoStat> = repos.into_values().collect();
    by_repo.sort_by(|a, b| {
        b.hold_total_sec
            .total_cmp(&a.hold_total_sec)
            .then(b.runs.cmp(&a.runs))
    });
    st.by_repo = by_repo;
    st
}

fn repo_entry<'a>(m: &'a mut BTreeMap<String, RepoStat>, repo: &str) -> &'a mut RepoStat {
    m.entry(repo.to_owned()).or_insert_with(|| RepoStat {
        repo: repo.to_owned(),
        runs: 0,
        failures: 0,
        wait_total_sec: 0.0,
        hold_total_sec: 0.0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn line(ts: &str, ev: &str, extra: &str) -> String {
        format!("{{\"ts\":\"{ts}\",\"event\":\"{ev}\",\"session\":\"s\"{extra}}}\n")
    }

    #[test]
    fn only_appended_bytes_are_parsed_and_a_half_written_line_waits_for_its_newline() {
        let p = std::env::temp_dir().join(format!("vitals-journal-{}.jsonl", std::process::id()));
        std::fs::write(
            &p,
            line(
                "2026-10-17T03:00:00Z",
                "start",
                ",\"repo\":\"codai\",\"waitSec\":4",
            ),
        )
        .unwrap();
        let now = parse_iso_ms("2026-10-17T03:30:00Z").unwrap();
        let mut j = JournalTail::new();
        j.poll(&p, now);
        assert_eq!(j.timeline().len(), 1);
        let mut f = std::fs::OpenOptions::new().append(true).open(&p).unwrap();
        f.write_all(b"{\"ts\":\"2026-10-17T03:10:00Z\",\"event\":\"do")
            .unwrap();
        j.poll(&p, now);
        assert_eq!(j.timeline().len(), 1, "partial line must not be parsed");
        f.write_all(b"ne\",\"holdSec\":60,\"repo\":\"codai\"}\n")
            .unwrap();
        j.poll(&p, now);
        let tl = j.timeline();
        assert_eq!(tl.len(), 2);
        assert_eq!(tl[0].event, "done");
        assert_eq!(tl[0].hold_sec, Some(60.0));
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn a_rotated_journal_starts_over_instead_of_reading_past_the_end() {
        let p =
            std::env::temp_dir().join(format!("vitals-journal-rot-{}.jsonl", std::process::id()));
        let now = parse_iso_ms("2026-10-17T03:30:00Z").unwrap();
        let big = line(
            "2026-10-17T03:00:00Z",
            "note",
            ",\"message\":\"xxxxxxxxxxxxxxxxxxxxxxxxxxxx\"",
        )
        .repeat(5);
        std::fs::write(&p, &big).unwrap();
        let mut j = JournalTail::new();
        j.poll(&p, now);
        std::fs::write(
            &p,
            line(
                "2026-10-17T03:20:00Z",
                "mark",
                ",\"reason\":\"after rotation\"",
            ),
        )
        .unwrap();
        j.poll(&p, now);
        let tl = j.timeline();
        assert_eq!(tl.len(), 1);
        assert_eq!(tl[0].text.as_deref(), Some("after rotation"));
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn stats_count_runs_failures_and_percentiles_inside_the_window_only() {
        let at = |s: &str| parse_iso_ms(s).unwrap();
        let ev = |ts: &str, e: &str, w: Option<f64>, h: Option<f64>| JournalEvent {
            ts_ms: at(ts),
            event: e.into(),
            resource: None,
            session: None,
            repo: Some("codai".into()),
            lane: None,
            text: None,
            exit: None,
            wait_sec: w,
            hold_sec: h,
        };
        let evs = [
            ev("2026-10-16T01:00:00Z", "start", Some(999.0), None), // outside 24 h
            ev("2026-10-17T02:00:00Z", "start", Some(10.0), None),
            ev("2026-10-17T02:01:00Z", "done", None, Some(30.0)),
            ev("2026-10-17T02:30:00Z", "start", Some(20.0), None),
            ev("2026-10-17T02:31:00Z", "fail", None, Some(90.0)),
        ];
        let s = stats(evs.iter(), at("2026-10-17T03:00:00Z"), 24);
        assert_eq!((s.runs, s.failures), (2, 1));
        assert!((s.wait_total_sec - 30.0).abs() < 1e-9);
        assert_eq!(s.hold_p90_sec, Some(90.0));
        assert_eq!(s.runs_per_hour.iter().sum::<u32>(), 2);
        assert_eq!(s.by_repo[0].failures, 1);
    }

    #[test]
    fn an_empty_window_has_no_percentiles_rather_than_zero() {
        let s = stats(std::iter::empty(), 1_000_000_000_000, 6);
        assert_eq!(s.wait_p50_sec, None);
        assert_eq!(s.runs_per_hour.len(), 6);
    }
}
