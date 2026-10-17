//! AI client sessions on this machine, from the files each client writes.
//!
//! | Client | Source | Format |
//! |---|---|---|
//! | Copilot CLI, VS Code agent host | `~/.copilot/session-state/<id>/{workspace.yaml,events.jsonl}` | JSONL `{type,data,timestamp}` |
//! | VS Code Copilot Chat | `%APPDATA%/Code*/User/workspaceStorage/*/GitHub.copilot-chat/transcripts/<id>.jsonl` | same event schema |
//! | Claude Code | `~/.claude/projects/*/<id>.jsonl` | JSONL `{type,timestamp,sessionId,cwd,gitBranch,message}` |
//! | Codex | `~/.codex/session_index.jsonl` | `{id,thread_name,updated_at}` (rollouts are zstd; not read) |
//!
//! # Privacy
//!
//! Message content is never read into the model. The only text taken is the
//! session name the client itself stores in its metadata (`workspace.yaml`
//! `name`, Codex `thread_name`, VS Code's `chat.ChatSessionStore.index` title
//! in the workspace's `state.vscdb`), cut to 80 characters, plus tool *names*.
//!
//! # Cost
//!
//! Event files are parsed incrementally from a remembered offset; a busy
//! day writes ~100 MB of transcripts, which must never be re-read per tick.
//! Discovery (listing directories) runs at most every [`DISCOVER_EVERY_MS`].

use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::Value;
use vitals_core::agents::{AgentClient, AgentSession, SessionState, ToolCount};

use crate::agentq::ProcStart;
use crate::time::{mtime_ms, parse_iso_ms};

/// Sessions untouched for longer than this are not listed.
pub const SESSION_WINDOW_MS: u64 = 24 * 3_600_000;
/// Open turn with activity this recent = working.
const WORKING_MS: u64 = 2 * 60_000;
/// No activity for this long = ended, even without a shutdown event.
const ENDED_MS: u64 = 6 * 3_600_000;
/// Gap above which time between two events does not count as active
/// (sources without explicit turn boundaries).
const ACTIVE_GAP_MS: u64 = 5 * 60_000;
const DISCOVER_EVERY_MS: u64 = 30_000;
/// First read of a large file starts this far from the end.
const MAX_INITIAL_BYTES: u64 = 8 * 1024 * 1024;

/// Only the fields the tracker reads. Everything else in a line — message
/// content, tool arguments, reasoning — is skipped by serde without being
/// allocated, which is most of a multi-megabyte transcript (cold read
/// 1.2 s with `Value`, measured 2026-10-17).
#[derive(Deserialize)]
struct Event<'a> {
    #[serde(rename = "type", borrow)]
    kind: std::borrow::Cow<'a, str>,
    #[serde(borrow, default)]
    timestamp: Option<std::borrow::Cow<'a, str>>,
    #[serde(default)]
    data: EventData,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct EventData {
    session_id: Option<String>,
    selected_model: Option<String>,
    start_time: Option<String>,
    new_model: Option<String>,
    model: Option<String>,
    tool_name: Option<String>,
    tool_call_id: Option<String>,
    success: Option<bool>,
    context: Option<EventContext>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct EventContext {
    cwd: Option<String>,
    branch: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Format {
    /// Copilot event schema (session-state and VS Code transcripts).
    Copilot,
    Claude,
}

#[derive(Debug, Clone)]
struct Source {
    path: PathBuf,
    format: Format,
    client: AgentClient,
    /// session-state dir (workspace.yaml, inuse locks), when there is one.
    meta_dir: Option<PathBuf>,
    workspace: Option<String>,
    /// `workspace.yaml`, read at discovery (every 30 s) rather than per poll.
    yaml: HashMap<String, String>,
}

#[derive(Debug, Default)]
struct Acc {
    offset: u64,
    carry: Vec<u8>,
    id: Option<String>,
    model: Option<String>,
    started: Option<u64>,
    last: Option<u64>,
    cwd: Option<String>,
    branch: Option<String>,
    turn_open_at: Option<u64>,
    active_ms: u64,
    turns: u32,
    user_messages: u32,
    tool_calls: u32,
    tool_failures: u32,
    open_tools: HashMap<String, String>,
    tools: HashMap<String, u32>,
    shutdown: bool,
    tokens_in: Option<u64>,
    tokens_out: Option<u64>,
}

impl Acc {
    fn touch(&mut self, ts: Option<u64>) {
        let Some(ts) = ts else { return };
        if self.started.is_none() {
            self.started = Some(ts);
        }
        if let Some(prev) = self.last {
            // explicit turns are summed on turn_end; this is the fallback
            if self.turn_open_at.is_none() && self.turns == 0 && ts > prev {
                let gap = ts - prev;
                if gap <= ACTIVE_GAP_MS {
                    self.active_ms += gap;
                }
            }
        }
        self.last = Some(self.last.map_or(ts, |l| l.max(ts)));
    }

    fn copilot(&mut self, e: Event<'_>) {
        let ts = e.timestamp.as_deref().and_then(parse_iso_ms);
        let d = e.data;
        let nonempty = |s: Option<String>| s.filter(|x| !x.is_empty());
        match e.kind.as_ref() {
            "session.start" => {
                self.id = nonempty(d.session_id).or(self.id.take());
                self.model = nonempty(d.selected_model).or(self.model.take());
                if let Some(st) = d.start_time.as_deref().and_then(parse_iso_ms) {
                    self.started = Some(st);
                }
                if let Some(ctx) = d.context {
                    self.cwd = ctx.cwd.or(self.cwd.take());
                    self.branch = ctx.branch.or(self.branch.take());
                }
            }
            "session.model_change" => self.model = nonempty(d.new_model).or(self.model.take()),
            "assistant.message" => {
                if let Some(m) = nonempty(d.model) {
                    self.model = Some(m);
                }
            }
            "user.message" => self.user_messages += 1,
            "assistant.turn_start" => {
                if self.turn_open_at.is_none() {
                    self.turn_open_at = ts;
                }
            }
            "assistant.turn_end" => {
                if let (Some(open), Some(end)) = (self.turn_open_at.take(), ts) {
                    self.active_ms += end.saturating_sub(open);
                }
                self.turns += 1;
            }
            "tool.execution_start" => {
                self.tool_calls += 1;
                let name = nonempty(d.tool_name).unwrap_or_else(|| "tool".into());
                *self.tools.entry(name.clone()).or_default() += 1;
                if let Some(id) = d.tool_call_id {
                    self.open_tools.insert(id, name);
                }
            }
            "tool.execution_complete" => {
                if let Some(id) = d.tool_call_id {
                    self.open_tools.remove(&id);
                }
                if d.success == Some(false) {
                    self.tool_failures += 1;
                }
            }
            "session.shutdown" => {
                self.shutdown = true;
                self.turn_open_at = None;
                self.open_tools.clear();
            }
            _ => {}
        }
        // a turn_start/turn_end pair already counts; keep the gap fallback out
        self.touch(ts);
    }

    fn claude(&mut self, v: &Value) {
        let ts = v
            .get("timestamp")
            .and_then(Value::as_str)
            .and_then(parse_iso_ms);
        let gs = |k: &str| {
            v.get(k)
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
        };
        if self.id.is_none() {
            self.id = gs("sessionId");
        }
        if let Some(c) = gs("cwd") {
            self.cwd = Some(c);
        }
        if let Some(bch) = gs("gitBranch") {
            self.branch = Some(bch);
        }
        let msg = v.get("message").unwrap_or(&Value::Null);
        match v.get("type").and_then(Value::as_str).unwrap_or_default() {
            "user" => {
                // tool results come back as user lines; only real prompts count
                let is_tool_result =
                    msg.get("content")
                        .and_then(Value::as_array)
                        .is_some_and(|a| {
                            a.iter().any(|c| {
                                c.get("type").and_then(Value::as_str) == Some("tool_result")
                            })
                        });
                if !is_tool_result {
                    self.user_messages += 1;
                }
            }
            "assistant" => {
                if let Some(m) = msg.get("model").and_then(Value::as_str) {
                    self.model = Some(m.to_owned());
                }
                if let Some(usage) = msg.get("usage") {
                    let n = |k: &str| usage.get(k).and_then(Value::as_u64);
                    if let Some(i) = n("input_tokens") {
                        let cached = n("cache_read_input_tokens").unwrap_or(0)
                            + n("cache_creation_input_tokens").unwrap_or(0);
                        self.tokens_in = Some(self.tokens_in.unwrap_or(0) + i + cached);
                    }
                    if let Some(o) = n("output_tokens") {
                        self.tokens_out = Some(self.tokens_out.unwrap_or(0) + o);
                    }
                }
                if let Some(arr) = msg.get("content").and_then(Value::as_array) {
                    for c in arr
                        .iter()
                        .filter(|c| c.get("type").and_then(Value::as_str) == Some("tool_use"))
                    {
                        self.tool_calls += 1;
                        let name = c
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or("tool")
                            .to_owned();
                        *self.tools.entry(name).or_default() += 1;
                    }
                }
            }
            _ => {}
        }
        self.touch(ts);
    }

    fn feed(&mut self, path: &Path, format: Format) {
        let Ok(mut file) = File::open(path) else {
            return;
        };
        let Ok(len) = file.metadata().map(|m| m.len()) else {
            return;
        };
        if len < self.offset {
            *self = Self::default();
        }
        let mut skip_first = false;
        if self.offset == 0 && len > MAX_INITIAL_BYTES {
            self.offset = len - MAX_INITIAL_BYTES;
            skip_first = true;
        }
        if len == self.offset || file.seek(SeekFrom::Start(self.offset)).is_err() {
            return;
        }
        let mut buf = std::mem::take(&mut self.carry);
        if file.take(len - self.offset).read_to_end(&mut buf).is_err() {
            return;
        }
        self.offset = len;
        let mut parts = buf.split(|c| *c == b'\n').peekable();
        let mut first = true;
        while let Some(line) = parts.next() {
            if parts.peek().is_none() {
                self.carry = line.to_vec();
                break;
            }
            if std::mem::take(&mut first) && skip_first {
                continue;
            }
            let Ok(text) = std::str::from_utf8(line) else {
                continue;
            };
            let text = text.trim().trim_start_matches('\u{feff}');
            if text.is_empty() {
                continue;
            }
            match format {
                Format::Copilot => {
                    if let Ok(e) = serde_json::from_str::<Event<'_>>(text) {
                        self.copilot(e);
                    }
                }
                Format::Claude => {
                    if let Ok(v) = serde_json::from_str::<Value>(text) {
                        self.claude(&v);
                    }
                }
            }
        }
    }
}

/// Tracks every session file across calls.
#[derive(Debug, Default)]
pub struct SessionTracker {
    sources: Vec<Source>,
    accs: HashMap<PathBuf, Acc>,
    last_discover: u64,
    roots: Roots,
}

/// Where each client keeps its files. `None` disables a client.
#[derive(Debug, Clone, Default)]
pub struct Roots {
    pub copilot_state: Option<PathBuf>,
    /// `.../User/workspaceStorage` for each VS Code edition.
    pub vscode_storage: Vec<PathBuf>,
    pub claude_projects: Option<PathBuf>,
    pub codex_home: Option<PathBuf>,
}

impl Roots {
    /// The real locations for the current user.
    #[must_use]
    pub fn detect() -> Self {
        let home = std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .map(PathBuf::from);
        let appdata = std::env::var_os("APPDATA").map(PathBuf::from);
        let vscode_storage = appdata
            .map(|a| {
                [
                    "Code - Insiders",
                    "Code",
                    "VSCodium",
                    "VSCodium - Insiders",
                    "Cursor",
                ]
                .iter()
                .map(|e| a.join(e).join("User").join("workspaceStorage"))
                .filter(|p| p.is_dir())
                .collect()
            })
            .unwrap_or_default();
        let codex_home = std::env::var_os("CODEX_HOME")
            .map(PathBuf::from)
            .or_else(|| home.as_ref().map(|h| h.join(".codex")));
        Self {
            copilot_state: home
                .as_ref()
                .map(|h| h.join(".copilot").join("session-state")),
            vscode_storage,
            claude_projects: home.as_ref().map(|h| h.join(".claude").join("projects")),
            codex_home,
        }
    }
}

impl SessionTracker {
    #[must_use]
    pub fn new(roots: Roots) -> Self {
        Self {
            roots,
            ..Self::default()
        }
    }

    fn discover(&mut self, now: u64) {
        let fresh = |p: &Path| {
            fs::metadata(p)
                .ok()
                .as_ref()
                .and_then(mtime_ms)
                .is_some_and(|m| now.saturating_sub(m) < SESSION_WINDOW_MS)
        };
        let mut found = Vec::new();
        if let Some(root) = &self.roots.copilot_state {
            for dir in dirs(root) {
                let ev = dir.join("events.jsonl");
                if !fresh(&ev) {
                    continue;
                }
                let yaml = read_yaml(&dir.join("workspace.yaml"));
                let client = match yaml.get("client_name").map(String::as_str) {
                    Some("cli") | None => AgentClient::CopilotCli,
                    Some(c) if c.starts_with("vscode") => AgentClient::VscodeCopilot,
                    Some(c) if c.contains("codai") => AgentClient::Codai,
                    Some(_) => AgentClient::Other,
                };
                found.push(Source {
                    path: ev,
                    format: Format::Copilot,
                    client,
                    meta_dir: Some(dir),
                    workspace: yaml.get("cwd").cloned(),
                    yaml,
                });
            }
        }
        for storage in &self.roots.vscode_storage {
            for ws in dirs(storage) {
                let tdir = ws.join("GitHub.copilot-chat").join("transcripts");
                if !tdir.is_dir() || !fresh(&tdir) {
                    continue;
                }
                let folder = workspace_folder(&ws);
                // The transcript has no metadata file; VS Code keeps the
                // title it shows in the chat list in its workspace database.
                let titles = vscode_titles(&ws.join("state.vscdb"));
                for f in files(&tdir, "jsonl").into_iter().filter(|f| fresh(f)) {
                    let mut yaml = HashMap::new();
                    if let Some(t) = f
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .and_then(|id| titles.get(id))
                    {
                        yaml.insert("name".to_owned(), t.clone());
                    }
                    found.push(Source {
                        path: f,
                        format: Format::Copilot,
                        client: AgentClient::VscodeCopilot,
                        meta_dir: None,
                        workspace: folder.clone(),
                        yaml,
                    });
                }
            }
        }
        if let Some(root) = &self.roots.claude_projects {
            for proj in dirs(root) {
                if !fresh(&proj) {
                    continue;
                }
                for f in files(&proj, "jsonl").into_iter().filter(|f| fresh(f)) {
                    found.push(Source {
                        path: f,
                        format: Format::Claude,
                        client: AgentClient::ClaudeCode,
                        meta_dir: None,
                        workspace: None,
                        yaml: HashMap::new(),
                    });
                }
            }
        }
        let keep: HashSet<&PathBuf> = found.iter().map(|s| &s.path).collect();
        self.accs.retain(|p, _| keep.contains(p));
        self.sources = found;
        self.last_discover = now;
    }

    /// Reads new events and returns every session in the window.
    pub fn poll(&mut self, probe: ProcStart<'_>, now: u64) -> Vec<AgentSession> {
        if self.last_discover == 0 || now.saturating_sub(self.last_discover) >= DISCOVER_EVERY_MS {
            self.discover(now);
        }
        let mut out = Vec::with_capacity(self.sources.len());
        for src in &self.sources {
            let acc = self.accs.entry(src.path.clone()).or_default();
            acc.feed(&src.path, src.format);
            out.push(build(src, acc, probe, now));
        }
        if let Some(home) = &self.roots.codex_home {
            out.extend(codex_sessions(home, now));
        }
        out.retain(|s| {
            s.last_activity_ms
                .is_some_and(|l| now.saturating_sub(l) < SESSION_WINDOW_MS)
        });
        out.sort_by(|a, b| {
            state_rank(a.state)
                .cmp(&state_rank(b.state))
                .then(b.last_activity_ms.cmp(&a.last_activity_ms))
        });
        out
    }
}

const fn state_rank(s: SessionState) -> u8 {
    match s {
        SessionState::Working => 0,
        SessionState::Waiting => 1,
        SessionState::Idle => 2,
        SessionState::Ended => 3,
    }
}

fn build(src: &Source, acc: &Acc, probe: ProcStart<'_>, now: u64) -> AgentSession {
    let yaml = &src.yaml;
    let pid = src.meta_dir.as_ref().and_then(|d| live_lock_pid(d, probe));
    let last = acc.last;
    let idle_for = last.map_or(u64::MAX, |l| now.saturating_sub(l));
    let open_turn = acc.turn_open_at.is_some();
    let state = if acc.shutdown || (idle_for > ENDED_MS && pid.is_none()) {
        SessionState::Ended
    } else if open_turn && idle_for <= WORKING_MS {
        SessionState::Working
    } else if open_turn && idle_for <= ENDED_MS {
        SessionState::Waiting
    } else if src.format == Format::Claude && idle_for <= WORKING_MS {
        SessionState::Working
    } else {
        SessionState::Idle
    };
    // an open turn is still being worked: count it up to now
    let live_turn = acc
        .turn_open_at
        .filter(|_| state == SessionState::Working || state == SessionState::Waiting)
        .map_or(0, |o| now.saturating_sub(o));
    let mut top: Vec<ToolCount> = acc
        .tools
        .iter()
        .map(|(n, c)| ToolCount {
            name: n.clone(),
            count: *c,
        })
        .collect();
    top.sort_by(|a, b| b.count.cmp(&a.count).then(a.name.cmp(&b.name)));
    top.truncate(6);
    let id = yaml
        .get("id")
        .cloned()
        .or_else(|| acc.id.clone())
        .or_else(|| {
            src.path
                .file_stem()
                .and_then(|s| s.to_str())
                .map(str::to_owned)
        })
        .unwrap_or_default();
    let workspace = yaml
        .get("cwd")
        .cloned()
        .or_else(|| src.workspace.clone())
        .or_else(|| acc.cwd.clone());
    AgentSession {
        id,
        client: src.client,
        host: "local".into(),
        title: yaml.get("name").map(|n| cut(n, 80)),
        repo: yaml
            .get("repository")
            .cloned()
            .or_else(|| workspace.as_deref().map(leaf)),
        workspace,
        branch: yaml.get("branch").cloned().or_else(|| acc.branch.clone()),
        model: acc.model.clone(),
        state,
        started_ms: acc
            .started
            .or_else(|| yaml.get("created_at").and_then(|c| parse_iso_ms(c))),
        last_activity_ms: last,
        active_ms: acc.active_ms + live_turn,
        turns: acc.turns,
        user_messages: acc.user_messages,
        tool_calls: acc.tool_calls,
        tool_failures: acc.tool_failures,
        current_tool: if state == SessionState::Ended {
            None
        } else {
            acc.open_tools.values().next().cloned()
        },
        top_tools: top,
        tokens_in: acc.tokens_in,
        tokens_out: acc.tokens_out,
        pid,
    }
}

/// Copilot writes `inuse.<pid>.lock` while a process holds the session.
fn live_lock_pid(dir: &Path, probe: ProcStart<'_>) -> Option<u32> {
    fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .find_map(|e| {
            let name = e.file_name().into_string().ok()?;
            let pid: u32 = name
                .strip_prefix("inuse.")?
                .strip_suffix(".lock")?
                .parse()
                .ok()?;
            probe(pid).map(|_| pid)
        })
}

fn codex_sessions(home: &Path, now: u64) -> Vec<AgentSession> {
    let Ok(text) = fs::read_to_string(home.join("session_index.jsonl")) else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter_map(|v| {
            let updated = v
                .get("updated_at")
                .and_then(Value::as_str)
                .and_then(parse_iso_ms)?;
            if now.saturating_sub(updated) >= SESSION_WINDOW_MS {
                return None;
            }
            Some(AgentSession {
                id: v.get("id")?.as_str()?.to_owned(),
                client: AgentClient::Codex,
                host: "local".into(),
                title: v
                    .get("thread_name")
                    .and_then(Value::as_str)
                    .map(|t| cut(t, 80)),
                workspace: None,
                repo: None,
                branch: None,
                model: None,
                state: if now - updated <= WORKING_MS {
                    SessionState::Working
                } else {
                    SessionState::Idle
                },
                started_ms: None,
                last_activity_ms: Some(updated),
                active_ms: 0,
                turns: 0,
                user_messages: 0,
                tool_calls: 0,
                tool_failures: 0,
                current_tool: None,
                top_tools: Vec::new(),
                tokens_in: None,
                tokens_out: None,
                pid: None,
            })
        })
        .collect()
}

fn dirs(root: &Path) -> Vec<PathBuf> {
    fs::read_dir(root)
        .map(|rd| {
            rd.filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .collect()
        })
        .unwrap_or_default()
}
fn files(dir: &Path, ext: &str) -> Vec<PathBuf> {
    fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == ext))
                .collect()
        })
        .unwrap_or_default()
}

fn leaf(p: &str) -> String {
    p.trim_end_matches(['\\', '/'])
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or(p)
        .to_owned()
}

fn cut(s: &str, max: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= max {
        return s.to_owned();
    }
    let mut o: String = s.chars().take(max).collect();
    o.push('…');
    o
}

/// The flat subset of YAML `workspace.yaml` uses: `key: value`, quoted
/// values, and `key: |-` followed by an indented block (first line kept).
fn read_yaml(path: &Path) -> HashMap<String, String> {
    let Ok(text) = fs::read_to_string(path) else {
        return HashMap::new();
    };
    parse_yaml(&text)
}

fn parse_yaml(text: &str) -> HashMap<String, String> {
    let mut m = HashMap::new();
    let mut pending: Option<String> = None;
    for line in text.lines() {
        if let Some(key) = pending.take()
            && line.starts_with(' ')
        {
            m.insert(key, line.trim().to_owned());
            continue;
        }
        if line.starts_with(' ') || line.trim().is_empty() {
            continue;
        }
        let Some((k, v)) = line.split_once(':') else {
            continue;
        };
        let v = v.trim();
        if v == "|-" || v == "|" || v == ">-" || v == ">" {
            pending = Some(k.trim().to_owned());
            continue;
        }
        let v = v
            .strip_prefix('\'')
            .and_then(|x| x.strip_suffix('\''))
            .map(|x| x.replace("''", "'"))
            .or_else(|| {
                v.strip_prefix('"')
                    .and_then(|x| x.strip_suffix('"'))
                    .map(str::to_owned)
            })
            .unwrap_or_else(|| v.to_owned());
        if !v.is_empty() {
            m.insert(k.trim().to_owned(), v);
        }
    }
    m
}

/// `sessionId → title` from VS Code's chat list index
/// (`ItemTable['chat.ChatSessionStore.index']` in the workspace `state.vscdb`).
///
/// Opened read-only with a short busy timeout: VS Code holds the database
/// open and writes to it, and a locked read must cost the tick nothing but
/// a missing title. Empty on any failure — the title is cosmetic.
fn vscode_titles(db: &Path) -> HashMap<String, String> {
    let mut out = HashMap::new();
    if !db.is_file() {
        return out;
    }
    let Ok(conn) = rusqlite::Connection::open_with_flags(
        db,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) else {
        return out;
    };
    let _ = conn.busy_timeout(std::time::Duration::from_millis(50));
    let Ok(raw) = conn.query_row(
        "SELECT value FROM ItemTable WHERE key = 'chat.ChatSessionStore.index'",
        [],
        |r| r.get::<_, String>(0),
    ) else {
        return out;
    };
    parse_vscode_index(&raw, &mut out);
    out
}

fn parse_vscode_index(raw: &str, out: &mut HashMap<String, String>) {
    let Ok(v) = serde_json::from_str::<Value>(raw) else {
        return;
    };
    let entries = v.get("entries").unwrap_or(&v);
    let iter: Box<dyn Iterator<Item = &Value>> = match entries {
        Value::Object(m) => Box::new(m.values()),
        Value::Array(a) => Box::new(a.iter()),
        _ => return,
    };
    for e in iter {
        if let (Some(id), Some(t)) = (
            e.get("sessionId").and_then(Value::as_str),
            e.get("title").and_then(Value::as_str),
        ) && !t.trim().is_empty()
        {
            out.insert(id.to_owned(), cut(t, 80));
        }
    }
}

/// `workspace.json` → `{"folder":"file:///e%3A/gh/vitals"}` → `e:\gh\vitals`.
fn workspace_folder(ws: &Path) -> Option<String> {
    let v: Value =
        serde_json::from_str(&fs::read_to_string(ws.join("workspace.json")).ok()?).ok()?;
    let uri = v.get("folder").or_else(|| v.get("workspace"))?.as_str()?;
    let path = uri.strip_prefix("file:///").unwrap_or(uri);
    Some(percent_decode(path).replace('/', "\\"))
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && i + 2 < b.len()
            && let Ok(x) = u8::from_str_radix(&s[i + 1..i + 3], 16)
        {
            out.push(x);
            i += 3;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(ts: &str, ty: &str, data: &str) -> String {
        format!("{{\"type\":\"{ty}\",\"timestamp\":\"{ts}\",\"data\":{data}}}\n")
    }

    struct Tmp(PathBuf);
    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn tmp(tag: &str) -> Tmp {
        let p = std::env::temp_dir().join(format!("vitals-sessions-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        Tmp(p)
    }

    #[test]
    fn a_vscode_session_takes_its_title_from_the_chat_list_index() {
        let t = tmp("vscdb");
        let db = t.0.join("state.vscdb");
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute_batch(
            "CREATE TABLE ItemTable (key TEXT UNIQUE ON CONFLICT REPLACE, value BLOB)",
        )
        .unwrap();
        let idx = r#"{"version":1,"entries":{"a1":{"sessionId":"a1","title":"Fix the sampler","lastMessageDate":1},"b2":{"sessionId":"b2","title":"  "}}}"#;
        conn.execute(
            "INSERT INTO ItemTable (key, value) VALUES ('chat.ChatSessionStore.index', ?1)",
            [idx],
        )
        .unwrap();
        drop(conn);
        let titles = vscode_titles(&db);
        assert_eq!(
            titles.get("a1").map(String::as_str),
            Some("Fix the sampler")
        );
        // a blank title stays absent so the UI says "Untitled", not ""
        assert!(!titles.contains_key("b2"));
        assert!(vscode_titles(&t.0.join("missing.vscdb")).is_empty());
    }

    #[test]
    fn active_time_is_the_sum_of_turns_not_the_session_age() {
        let mut a = Acc::default();
        for line in [
            ev(
                "2026-10-17T01:00:00Z",
                "session.start",
                r#"{"sessionId":"s","selectedModel":"codai","startTime":"2026-10-17T01:00:00Z"}"#,
            ),
            ev(
                "2026-10-17T01:00:01Z",
                "assistant.turn_start",
                r#"{"turnId":"0"}"#,
            ),
            ev(
                "2026-10-17T01:00:31Z",
                "assistant.turn_end",
                r#"{"turnId":"0"}"#,
            ),
            // an hour of the person thinking
            ev(
                "2026-10-17T02:00:00Z",
                "assistant.turn_start",
                r#"{"turnId":"1"}"#,
            ),
            ev(
                "2026-10-17T02:01:00Z",
                "assistant.turn_end",
                r#"{"turnId":"1"}"#,
            ),
        ] {
            a.copilot(serde_json::from_str(line.trim()).unwrap());
        }
        assert_eq!(a.active_ms, 90_000);
        assert_eq!(a.turns, 2);
        assert_eq!(a.model.as_deref(), Some("codai"));
    }

    #[test]
    fn an_open_tool_call_is_the_current_tool_and_a_failed_one_is_counted() {
        let d = tmp("tool");
        let f = d.0.join("t.jsonl");
        let now = parse_iso_ms("2026-10-17T03:00:30Z").unwrap();
        fs::write(
            &f,
            [
                ev(
                    "2026-10-17T03:00:00Z",
                    "assistant.turn_start",
                    r#"{"turnId":"0"}"#,
                ),
                ev(
                    "2026-10-17T03:00:01Z",
                    "tool.execution_start",
                    r#"{"toolCallId":"a","toolName":"read_file"}"#,
                ),
                ev(
                    "2026-10-17T03:00:02Z",
                    "tool.execution_complete",
                    r#"{"toolCallId":"a","success":false}"#,
                ),
                ev(
                    "2026-10-17T03:00:03Z",
                    "tool.execution_start",
                    r#"{"toolCallId":"b","toolName":"run_in_terminal"}"#,
                ),
            ]
            .concat(),
        )
        .unwrap();
        let src = Source {
            path: f.clone(),
            format: Format::Copilot,
            client: AgentClient::VscodeCopilot,
            meta_dir: None,
            workspace: Some("e:\\gh\\vitals".into()),
            yaml: HashMap::new(),
        };
        let mut acc = Acc::default();
        acc.feed(&f, Format::Copilot);
        let s = build(&src, &acc, &|_| None, now);
        assert_eq!(s.state, SessionState::Working);
        assert_eq!(s.current_tool.as_deref(), Some("run_in_terminal"));
        assert_eq!((s.tool_calls, s.tool_failures), (2, 1));
        assert_eq!(s.active_ms, 30_000, "open turn counts up to now");
        assert_eq!(s.repo.as_deref(), Some("vitals"));
    }

    #[test]
    fn an_open_turn_that_went_quiet_is_waiting_and_a_closed_one_is_idle() {
        let d = tmp("state");
        let f = d.0.join("t.jsonl");
        fs::write(
            &f,
            ev(
                "2026-10-17T03:00:00Z",
                "assistant.turn_start",
                r#"{"turnId":"0"}"#,
            ),
        )
        .unwrap();
        let src = Source {
            path: f.clone(),
            format: Format::Copilot,
            client: AgentClient::CopilotCli,
            meta_dir: None,
            workspace: None,
            yaml: HashMap::new(),
        };
        let mut acc = Acc::default();
        acc.feed(&f, Format::Copilot);
        let ten_min = parse_iso_ms("2026-10-17T03:10:00Z").unwrap();
        assert_eq!(
            build(&src, &acc, &|_| None, ten_min).state,
            SessionState::Waiting
        );
        let mut f2 = fs::OpenOptions::new().append(true).open(&f).unwrap();
        std::io::Write::write_all(
            &mut f2,
            ev(
                "2026-10-17T03:05:00Z",
                "assistant.turn_end",
                r#"{"turnId":"0"}"#,
            )
            .as_bytes(),
        )
        .unwrap();
        acc.feed(&f, Format::Copilot);
        assert_eq!(
            build(&src, &acc, &|_| None, ten_min).state,
            SessionState::Idle
        );
        let day = parse_iso_ms("2026-10-17T12:00:00Z").unwrap();
        assert_eq!(build(&src, &acc, &|_| None, day).state, SessionState::Ended);
    }

    #[test]
    fn claude_lines_give_tokens_and_tool_results_are_not_user_messages() {
        let mut a = Acc::default();
        for l in [
            r#"{"type":"user","timestamp":"2026-10-17T03:00:00Z","sessionId":"c1","cwd":"E:\\gh\\x","gitBranch":"main","message":{"content":"hi"}}"#,
            r#"{"type":"assistant","timestamp":"2026-10-17T03:00:05Z","message":{"model":"claude-x","usage":{"input_tokens":10,"cache_read_input_tokens":90,"output_tokens":7},"content":[{"type":"tool_use","name":"Bash"}]}}"#,
            r#"{"type":"user","timestamp":"2026-10-17T03:00:09Z","message":{"content":[{"type":"tool_result"}]}}"#,
        ] {
            a.claude(&serde_json::from_str(l).unwrap());
        }
        assert_eq!(a.user_messages, 1);
        assert_eq!((a.tokens_in, a.tokens_out), (Some(100), Some(7)));
        assert_eq!(a.tool_calls, 1);
        assert_eq!(a.active_ms, 9_000);
        assert_eq!(a.branch.as_deref(), Some("main"));
    }

    #[test]
    fn workspace_yaml_block_scalars_and_quotes_are_read() {
        let m = parse_yaml(
            "id: abc\ncwd: e:\\gh\\vitals\nname: |-\n  First line\n  second\nbranch: 'it''s'\nclient_name: vscode-agent-host\n",
        );
        assert_eq!(m.get("name").map(String::as_str), Some("First line"));
        assert_eq!(m.get("branch").map(String::as_str), Some("it's"));
        assert_eq!(m.get("cwd").map(String::as_str), Some("e:\\gh\\vitals"));
    }

    #[test]
    fn a_vscode_workspace_uri_becomes_a_windows_path() {
        let d = tmp("wsjson");
        fs::write(
            d.0.join("workspace.json"),
            r#"{"folder":"file:///e%3A/gh/vitals"}"#,
        )
        .unwrap();
        assert_eq!(workspace_folder(&d.0).as_deref(), Some("e:\\gh\\vitals"));
    }

    #[test]
    fn discovery_reads_workspace_yaml_for_the_client_and_repo() {
        let d = tmp("disc");
        let state = d.0.join("state").join("sid1");
        fs::create_dir_all(&state).unwrap();
        fs::write(state.join("workspace.yaml"), "id: sid1\nclient_name: vscode-agent-host\ncwd: E:\\gh\\agentq\nrepository: dragoscv/agentq\n").unwrap();
        // event timestamped "now" so it is inside the window whatever the clock says
        let now = crate::time::now_ms();
        fs::write(
            state.join("events.jsonl"),
            "{\"type\":\"user.message\",\"data\":{}}\n",
        )
        .unwrap();
        let roots = Roots {
            copilot_state: Some(d.0.join("state")),
            ..Roots::default()
        };
        let mut t = SessionTracker::new(roots);
        t.discover(now);
        assert_eq!(t.sources.len(), 1);
        assert_eq!(t.sources[0].client, AgentClient::VscodeCopilot);
        let mut acc = Acc::default();
        acc.feed(&t.sources[0].path, Format::Copilot);
        let s = build(&t.sources[0], &acc, &|_| None, now);
        assert_eq!(s.id, "sid1");
        assert_eq!(s.repo.as_deref(), Some("dragoscv/agentq"));
        assert_eq!(s.user_messages, 1);
    }
}
