//! The plan-and-task live state `track.mjs` keeps, and the one write this
//! page is allowed to make.
//!
//! `track.mjs` (in the agentq repository) is the portable store behind
//! "tracking & resume": a chat's goal, its next steps and its claimable tasks
//! live in three files under the coordination directory agentq already owns —
//!
//! - `<root>/chats/.last.json` — the pointer to the session in hand,
//! - `<root>/chats/<session>.plan.json` — that session's plan,
//! - `<root>/tasks/<id>.json` — one claimable task per file.
//!
//! This module reads those into the page's "resume pack" and performs the
//! **one** mutation the Tracking page offers: claim a task, or mark it done.
//!
//! # Why a Rust writer, when the crate is otherwise read-only
//!
//! Two rules meet here and both hold. ADR-0051 keeps the **tracker index**
//! read-only, and it stays read-only: nothing in this module touches
//! `trackers/index.json`, and the page still offers no action on a tracker
//! row. The crate's own header says the page's only writes go through
//! agentq's CLI; that is true of the **AI page** (ADR-0039), whose actions are
//! `agentq.ps1` verbs. The task store is different: `track.mjs` is a Node CLI
//! with no installed shim (it lives in the agentq clone, not in
//! `~/.copilot/bin`), and there is no environment or config that names it, so
//! the CLI cannot be located reliably. The brief for this slice is explicit
//! that the page claims "through the agentq CLI / the same state the CLI
//! uses"; this is the second half of that, and it is deliberately narrow.
//!
//! The protocol is replicated faithfully rather than approximated, because a
//! second writer that drifts is worse than none: the same `.lock` file taken
//! with the same create-or-fail `wx` open and the same bounded take-over, the
//! same temp-then-rename atomic write, the same `CLAIMABLE` set, the same
//! field names and the same refusal strings. `track.test.mjs` and the tests
//! below pin the two sides of that contract.
//!
//! Least privilege: **claim and complete only**. Nothing here creates a task,
//! deletes one, or changes its id, title or dependencies.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::agentq::{json_files, read_json};
use crate::time::{iso_from_ms, now_ms};

/// Statuses `track.mjs` lets a claim move out of — its `CLAIMABLE` list.
/// Anything else (already `done`, `blocked`, `review`, `dropped`) is refused,
/// so a completed task cannot be quietly re-opened by a second click.
const CLAIMABLE: [&str; 3] = ["todo", "claimed", "in-progress"];

/// How long to wait for the claim lock before taking it over. Matches
/// `track.mjs`, so a lock a crashed writer left behind cannot wedge the store.
const LOCK_WAIT: Duration = Duration::from_millis(3000);
/// Poll step while waiting for the lock.
const LOCK_STEP: Duration = Duration::from_millis(40);

/// The resume pack: the current session's plan and every task in the store.
///
/// The shape the page parses tolerantly (`features/tracking/model.ts`):
/// `{ "sessionId": string|null, "plan": object|null, "tasks": [object, ...] }`.
/// An empty pack — no session, no plan, no tasks — is a normal state on a
/// machine that has never written one, never an error, so the page can say so
/// instead of showing a failure.
#[must_use]
pub fn read_resume(root: &Path) -> Value {
    let session = current_session(root);
    let plan = session
        .as_deref()
        .and_then(|id| read_json(&root.join("chats").join(format!("{id}.plan.json"))));
    let tasks: Vec<Value> = json_files(&root.join("tasks"))
        .into_iter()
        .filter_map(|p| read_json(&p))
        .collect();
    json!({
        "sessionId": session,
        "plan": plan,
        "tasks": tasks,
    })
}

/// The session in hand, from `track.mjs`'s own `chats/.last.json` pointer.
///
/// `None` when the pointer is absent or unreadable, which is the honest answer
/// before any chat has registered — the page then shows the goal as unknown
/// rather than attributing someone else's plan to this window.
fn current_session(root: &Path) -> Option<String> {
    let last = read_json(&root.join("chats").join(".last.json"))?;
    last.get("chatId")?
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

/// Claims task `id` for `by`, exactly as `track.mjs task claim` does.
///
/// # Errors
/// A task id with a path separator or traversal, a missing task, a task
/// another session already owns, a status that is not claimable, or a
/// filesystem failure. Each names the reason.
pub fn claim(root: &Path, id: &str, by: &str) -> Result<(), String> {
    apply(root, id, Op::Claim { by })
}

/// Marks task `id` done, exactly as `track.mjs task done` does.
///
/// # Errors
/// A task id with a path separator or traversal, a missing task, or a
/// filesystem failure.
pub fn complete(root: &Path, id: &str, evidence: Option<&str>) -> Result<(), String> {
    apply(root, id, Op::Complete { evidence })
}

/// The two mutations this page may perform. Copy, so it can move into the lock
/// closure without a clone.
#[derive(Clone, Copy)]
enum Op<'a> {
    Claim { by: &'a str },
    Complete { evidence: Option<&'a str> },
}

/// Runs `op` inside the lock, mirroring `track.mjs`'s read-check-write.
fn apply(root: &Path, id: &str, op: Op<'_>) -> Result<(), String> {
    let file = task_file(root, id)?;
    with_lock(&file, || {
        let mut task = read_json(&file).ok_or_else(|| format!("no task '{id}'"))?;
        let now = iso_from_ms(now_ms());
        match op {
            Op::Claim { by } => {
                if let Some(owner) = task.get("owner").and_then(Value::as_str)
                    && owner != by
                {
                    return Err(format!("already claimed by {owner}"));
                }
                let status = task.get("status").and_then(Value::as_str).unwrap_or("todo");
                if !CLAIMABLE.contains(&status) {
                    return Err(format!("status is '{status}'"));
                }
                set(&mut task, "owner", Value::String(by.to_owned()));
                set(&mut task, "status", Value::String("claimed".to_owned()));
                set(&mut task, "claimedAt", Value::String(now.clone()));
            }
            Op::Complete { evidence } => {
                set(&mut task, "status", Value::String("done".to_owned()));
                if let Some(text) = evidence.filter(|e| !e.is_empty()) {
                    set(&mut task, "evidence", Value::String(text.to_owned()));
                }
                set(&mut task, "doneAt", Value::String(now.clone()));
            }
        }
        set(&mut task, "updatedAt", Value::String(now));
        write_json_atomic(&file, &task)
    })?
}

/// `tasks/<id>.json`, refusing an id that could escape the directory.
///
/// A task id becomes a filename, so a separator or a `..` is refused here
/// rather than turned into a write somewhere else in the tree — the same rule
/// `track.mjs`'s `taskFile` enforces.
fn task_file(root: &Path, id: &str) -> Result<PathBuf, String> {
    let ok = !id.is_empty()
        && id.len() <= 200
        && !id.contains(['\\', '/'])
        && !id.contains("..")
        && !id.chars().any(char::is_control);
    if !ok {
        return Err(format!("invalid task id '{id}'"));
    }
    Ok(root.join("tasks").join(format!("{id}.json")))
}

/// Sets a top-level key, leaving a non-object (which cannot happen for a task
/// the reader accepted) untouched rather than panicking.
fn set(task: &mut Value, key: &str, value: Value) {
    if let Some(map) = task.as_object_mut() {
        map.insert(key.to_owned(), value);
    }
}

/// The exclusive lock that makes a claim atomic across processes, and the same
/// bounded take-over `track.mjs` uses: the `wx` (create-or-fail) open is the
/// OS primitive both processes see, and a lock a crashed writer left behind is
/// taken over after [`LOCK_WAIT`] rather than wedging the store for ever.
fn with_lock<T>(file: &Path, f: impl FnOnce() -> T) -> Result<T, String> {
    let lock = PathBuf::from(format!("{}.lock", file.display()));
    if let Some(parent) = lock.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("claim lock: {e}"))?;
    }
    let deadline = Instant::now() + LOCK_WAIT;
    loop {
        let mut opts = fs::OpenOptions::new();
        opts.write(true).create_new(true).truncate(false);
        match opts.open(&lock) {
            Ok(_) => break,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                if Instant::now() >= deadline {
                    // Take over: a holder that never released is either dead or
                    // past every deadline the CLI itself honours.
                    let _ = fs::remove_file(&lock);
                    let _ = opts.open(&lock);
                    break;
                }
                std::thread::sleep(LOCK_STEP);
            }
            Err(e) => return Err(format!("claim lock: {e}")),
        }
    }
    let out = f();
    let _ = fs::remove_file(&lock);
    Ok(out)
}

/// Writes `value` as pretty JSON with a trailing newline, via a temp file and a
/// rename — a reader sees either the old file or the whole new one, never half
/// a record. The temp name carries pid and time so two writers cannot collide.
fn write_json_atomic(file: &Path, value: &Value) -> Result<(), String> {
    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let tmp = PathBuf::from(format!(
        "{}.{}.{}.tmp",
        file.display(),
        std::process::id(),
        now_ms()
    ));
    let text = format!(
        "{}\n",
        serde_json::to_string_pretty(value).map_err(|e| e.to_string())?
    );
    fs::write(&tmp, text).map_err(|e| e.to_string())?;
    fs::rename(&tmp, file).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    /// Hands every test in this binary its own fixture. The pid and the clock
    /// are not enough: `cargo test` runs these in parallel threads of ONE
    /// process, so two tests that start in the same millisecond would be handed
    /// the same directory and would read each other's tasks — which is how
    /// `the_resume_pack_carries_...` saw two tasks instead of one.
    static NEXT_ROOT: AtomicU64 = AtomicU64::new(0);

    /// A throwaway coordination directory, removed on drop.
    struct Root(PathBuf);

    impl Root {
        fn new() -> Self {
            let seq = NEXT_ROOT.fetch_add(1, Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!(
                "vitals-tasks-{}-{}-{}",
                std::process::id(),
                now_ms(),
                seq
            ));
            fs::create_dir_all(dir.join("tasks")).unwrap();
            Self(dir)
        }

        fn path(&self) -> &Path {
            &self.0
        }

        fn task(&self, id: &str, body: &str) -> PathBuf {
            let file = self.0.join("tasks").join(format!("{id}.json"));
            fs::write(&file, body).unwrap();
            file
        }
    }

    impl Drop for Root {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn read(path: &Path) -> Value {
        read_json(path).unwrap()
    }

    #[test]
    fn a_claim_writes_the_owner_the_status_and_the_times_the_cli_writes() {
        let root = Root::new();
        let file = root.task(
            "T1",
            r#"{"version":1,"id":"T1","title":"x","status":"todo"}"#,
        );

        claim(root.path(), "T1", "chat-7").unwrap();

        let task = read(&file);
        assert_eq!(task["owner"], "chat-7");
        assert_eq!(task["status"], "claimed");
        assert!(task["claimedAt"].as_str().is_some_and(|s| s.ends_with('Z')));
        assert_eq!(task["claimedAt"], task["updatedAt"]);
        // The title the file already held is preserved, not rewritten.
        assert_eq!(task["title"], "x");
    }

    #[test]
    fn a_second_session_cannot_claim_a_task_another_already_holds() {
        let root = Root::new();
        root.task(
            "T2",
            r#"{"version":1,"id":"T2","title":"x","status":"claimed","owner":"chat-1"}"#,
        );

        let err = claim(root.path(), "T2", "chat-2").unwrap_err();

        assert!(err.contains("already claimed by chat-1"), "{err}");
        // The refused claim left the file exactly as it was.
        assert_eq!(read(&root.path().join("tasks/T2.json"))["owner"], "chat-1");
    }

    #[test]
    fn a_re_claim_by_the_same_owner_is_allowed_and_a_closed_task_is_not() {
        let root = Root::new();
        root.task(
            "T3",
            r#"{"version":1,"id":"T3","title":"x","status":"claimed","owner":"chat-1"}"#,
        );
        // Same owner: an idempotent re-claim, which the CLI allows.
        claim(root.path(), "T3", "chat-1").unwrap();

        root.task(
            "T4",
            r#"{"version":1,"id":"T4","title":"x","status":"done"}"#,
        );
        let err = claim(root.path(), "T4", "chat-1").unwrap_err();
        assert!(err.contains("status is 'done'"), "{err}");
    }

    #[test]
    fn completing_a_task_records_the_evidence_and_leaves_no_lock_behind() {
        let root = Root::new();
        let file = root.task(
            "T5",
            r#"{"version":1,"id":"T5","title":"x","status":"claimed","owner":"chat-1"}"#,
        );

        complete(root.path(), "T5", Some("pnpm test green")).unwrap();

        let task = read(&file);
        assert_eq!(task["status"], "done");
        assert_eq!(task["evidence"], "pnpm test green");
        assert!(task["doneAt"].as_str().is_some_and(|s| s.ends_with('Z')));
        // A lock left behind would wedge every later claim.
        assert!(!file.with_extension("json.lock").exists());
    }

    #[test]
    fn a_missing_task_is_named_rather_than_created() {
        let root = Root::new();
        let err = claim(root.path(), "nope", "chat-1").unwrap_err();
        assert!(err.contains("no task 'nope'"), "{err}");
        // Least privilege: a refused claim must not have created the file.
        assert!(!root.path().join("tasks/nope.json").exists());
    }

    #[test]
    fn an_id_that_could_escape_the_tasks_directory_is_refused() {
        let root = Root::new();
        for bad in ["../T", "a/b", r"a\b"] {
            let err = claim(root.path(), bad, "chat-1").unwrap_err();
            assert!(err.contains("invalid task id"), "{bad}: {err}");
        }
    }

    #[test]
    fn the_resume_pack_carries_the_session_the_plan_and_every_task() {
        let root = Root::new();
        fs::create_dir_all(root.path().join("chats")).unwrap();
        fs::write(
            root.path().join("chats/.last.json"),
            r#"{"chatId":"chat-9"}"#,
        )
        .unwrap();
        fs::write(
            root.path().join("chats/chat-9.plan.json"),
            r#"{"version":1,"goal":"ship it","next":[{"title":"t"}]}"#,
        )
        .unwrap();
        root.task(
            "T6",
            r#"{"version":1,"id":"T6","title":"x","status":"todo"}"#,
        );

        let pack = read_resume(root.path());

        assert_eq!(pack["sessionId"], "chat-9");
        assert_eq!(pack["plan"]["goal"], "ship it");
        assert_eq!(pack["tasks"].as_array().map(Vec::len), Some(1));
    }

    #[test]
    fn a_machine_that_has_written_nothing_yields_an_empty_pack_not_an_error() {
        let root = Root::new();
        // No `.last.json`, no plan, no tasks at all.
        let pack = read_resume(root.path());
        assert!(pack["sessionId"].is_null());
        assert!(pack["plan"].is_null());
        assert_eq!(pack["tasks"].as_array().map(Vec::len), Some(0));
    }
}
