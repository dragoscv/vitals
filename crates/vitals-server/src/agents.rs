//! The AI page over the LAN (ADR-0039, S18-07).
//!
//! Like [`crate::control`], the server knows nothing about agentq or AI
//! clients: the host hands it an [`AgentsProvider`], and the router serves
//! what that returns. The desktop's provider is the same reader thread the
//! AI page uses, so a phone and the window can never disagree.
//!
//! # Redaction
//!
//! Everything leaving this module goes through [`redact_snapshot`] or
//! [`redact_text`]. A snapshot carries command lines (`cmd`, `command`),
//! working directories and session titles, written by agents that routinely
//! put a token on a command line and a user name in every path. A paired
//! phone needs to see *that* `build:codai` waits behind a `pnpm test`, not
//! `C:\Users\<name>` or the `--token` that test was given. Redaction is
//! unconditional on this route: there is no reader for whom the raw values
//! are worth the leak, and the desktop window reads them over IPC anyway.

use serde::Serialize;
use serde_json::Value;
use vitals_core::agents::{AgentqAction, AgentsSnapshot};

/// What the host must provide for `/api/v1/agents` to answer.
///
/// Both methods block (the desktop may read agentq's directory cold, and an
/// action starts `pwsh`); the router calls them on a blocking thread.
pub trait AgentsProvider: Send + Sync {
    /// The current snapshot. Calling this counts as someone looking: the
    /// desktop keeps its reader awake for a while afterwards, and parks it
    /// again when the calls stop.
    fn snapshot(&self) -> AgentsSnapshot;

    /// Runs an action through agentq's own CLI.
    ///
    /// # Errors
    ///
    /// [`ActionError::Invalid`] when the action is refused before anything
    /// runs (a short reason, a malformed resource), [`ActionError::Unavailable`]
    /// when agentq cannot be started on this host.
    fn act(&self, action: AgentqAction) -> Result<ActionOutcome, ActionError>;
}

/// The result of an action agentq ran. `ok: false` is still a 200: agentq
/// answered, and its output says why (busy, frozen, unknown ticket).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionOutcome {
    pub ok: bool,
    pub code: Option<i32>,
    /// stdout and stderr, redacted before it is sent.
    pub output: String,
}

/// Why an action did not run at all.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ActionError {
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    Unavailable(String),
}

/// What replaces a secret.
pub const REDACTED: &str = "[redacted]";

/// Keys whose values are identifiers a client needs verbatim to act (a
/// ticket to break, a lease to release) and that never hold a path or a
/// secret. Everything else is scrubbed.
const IDENTIFIER_KEYS: &[&str] = &[
    "id", "leaseId", "runId", "session", "sha", "resource", "slot",
];

/// The snapshot as JSON with every free-text field scrubbed.
///
/// Walks the serialised value rather than the struct so a field added to
/// [`AgentsSnapshot`] later is redacted by default instead of leaking until
/// someone remembers this function.
#[must_use]
pub fn redact_snapshot(snapshot: &AgentsSnapshot) -> Value {
    let mut value = serde_json::to_value(snapshot).unwrap_or(Value::Null);
    walk(&mut value);
    value
}

/// An outcome with its output scrubbed.
#[must_use]
pub fn redact_outcome(outcome: ActionOutcome) -> ActionOutcome {
    let ActionOutcome { ok, code, output } = outcome;
    ActionOutcome {
        ok,
        code,
        output: redact_text(&output),
    }
}

fn walk(value: &mut Value) {
    match value {
        Value::String(text) => scrub(text),
        Value::Array(items) => items.iter_mut().for_each(walk),
        Value::Object(map) => {
            for (key, item) in map.iter_mut() {
                if IDENTIFIER_KEYS.contains(&key.as_str()) {
                    continue;
                }
                walk(item);
            }
        }
        _ => {}
    }
}

fn scrub(text: &mut String) {
    let clean = redact_text(text);
    if clean != *text {
        *text = clean;
    }
}

/// Replaces home directories with `~` and credentials with [`REDACTED`].
#[must_use]
pub fn redact_text(text: &str) -> String {
    redact_secrets(&redact_home(text))
}

/// `C:\Users\<name>`, `C:/Users/<name>`, `/Users/<name>` and
/// `/home/<name>` become `~`. The user name is the point: it is often a real
/// name, and it is in every path an agent writes.
fn redact_home(text: &str) -> String {
    let lower = text.to_ascii_lowercase();
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut copied = 0;
    let mut i = 0;
    while i < bytes.len() {
        let Some((start, name_at)) = home_prefix(&lower, bytes, i) else {
            i += 1;
            continue;
        };
        let name_end = text[name_at..]
            .find(['\\', '/', '"', '\'', ' ', '\t', '\n', ';', ','])
            .map_or(text.len(), |n| name_at + n);
        if name_end == name_at || start < copied {
            i += 1;
            continue;
        }
        out.push_str(&text[copied..start]);
        out.push('~');
        copied = name_end;
        i = name_end;
    }
    out.push_str(&text[copied..]);
    out
}

/// When a home prefix starts at byte `i` of `lower`, the byte where the
/// replaced span begins and the byte where the user name begins.
fn home_prefix(lower: &str, bytes: &[u8], i: usize) -> Option<(usize, usize)> {
    let rest = lower.get(i..)?;
    let marker = ["\\users\\", "/users/", "/home/"]
        .into_iter()
        .find(|m| rest.starts_with(m))?;
    let drive = i >= 2 && bytes[i - 1] == b':' && bytes[i - 2].is_ascii_alphabetic();
    let start = if drive {
        i - 2
    } else if marker.starts_with('/') && (i == 0 || !bytes[i - 1].is_ascii_alphanumeric()) {
        i
    } else {
        return None;
    };
    Some((start, i + marker.len()))
}

/// Words that announce a secret in the next argument (`--token abc`,
/// `Bearer abc`) or before an `=`/`:` (`API_KEY=abc`).
const SECRET_NAMES: &[&str] = &[
    "token",
    "secret",
    "password",
    "passwd",
    "apikey",
    "api_key",
    "api-key",
    "bearer",
    "authorization",
];

/// Prefixes of credentials that are recognisable on their own.
const SECRET_PREFIXES: &[&str] = &[
    "sk-",
    "sk_",
    "ghp_",
    "gho_",
    "ghu_",
    "ghs_",
    "github_pat_",
    "glpat-",
    "xoxb-",
    "xoxp-",
    "akia",
    "aiza",
    "eyj",
];

fn redact_secrets(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut hide_next = false;
    let mut rest = text;
    while !rest.is_empty() {
        let word_len = rest.find(char::is_whitespace).unwrap_or(rest.len());
        let (word, tail) = rest.split_at(word_len);
        let space_len = tail
            .find(|c: char| !c.is_whitespace())
            .unwrap_or(tail.len());
        let (space, next) = tail.split_at(space_len);
        if word.is_empty() {
            out.push_str(space);
        } else if hide_next {
            out.push_str(REDACTED);
            hide_next = false;
        } else {
            let (clean, announces) = redact_word(word);
            out.push_str(&clean);
            hide_next = announces;
        }
        if !word.is_empty() {
            out.push_str(space);
        }
        rest = next;
    }
    out
}

/// One whitespace-free word: the redacted form, and whether it announces
/// that the next word is a secret.
///
/// Only a flag (`--token`) or an auth scheme (`Bearer`) announces: a purpose
/// like "rotate the token handling" is prose, and hiding "handling" would
/// make the page harder to read without protecting anything.
fn redact_word(word: &str) -> (String, bool) {
    let bare = word.trim_matches(|c: char| matches!(c, '"' | '\'' | ',' | ';'));
    let lower = bare.to_ascii_lowercase();
    let flag = bare.starts_with('-') && !bare.contains('=');
    let scheme = matches!(
        lower.trim_end_matches(':'),
        "bearer" | "authorization" | "token"
    );
    if (flag || scheme) && names_a_secret(bare.trim_start_matches('-').trim_end_matches(':')) {
        return (word.to_owned(), true);
    }
    if let Some(at) = bare.find(['=', ':']) {
        let key = bare[..at].trim_start_matches('-');
        let value = bare[at + 1..].trim_matches(|c: char| matches!(c, '"' | '\''));
        if !value.is_empty() && (names_a_secret(key) || looks_like_a_credential(value)) {
            let offset = word.find(bare).unwrap_or(0);
            return (format!("{}{REDACTED}", &word[..=offset + at]), false);
        }
    }
    if looks_like_a_credential(bare) {
        return (word.replace(bare, REDACTED), false);
    }
    (word.to_owned(), false)
}

fn names_a_secret(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    !lower.is_empty()
        && (SECRET_NAMES.iter().any(|s| lower.ends_with(s))
            || lower.ends_with("_key")
            || lower.ends_with("-key"))
}

/// A known credential prefix, or a long opaque run of letters and digits
/// that is not hex (a git sha, a UUID): those are identifiers, and hiding
/// them would make the page useless without protecting anything.
fn looks_like_a_credential(word: &str) -> bool {
    let lower = word.to_ascii_lowercase();
    if word.len() >= 16 && SECRET_PREFIXES.iter().any(|p| lower.starts_with(p)) {
        return true;
    }
    let opaque = word
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '+' | '='));
    let hex_like = word.chars().all(|c| c.is_ascii_hexdigit() || c == '-');
    word.len() >= 32
        && opaque
        && !hex_like
        && word.chars().any(|c| c.is_ascii_digit())
        && word.chars().any(|c| c.is_ascii_alphabetic())
}

#[cfg(test)]
mod tests {
    use super::*;
    use vitals_core::agents::{AgentqLease, AgentqResource, AgentqTicket, Liveness, ResourceKind};

    #[test]
    fn a_windows_home_directory_loses_the_user_name_in_either_slash() {
        assert_eq!(
            redact_text(r"cd C:\Users\dragos\proj; type c:/users/dragos/x.txt"),
            r"cd ~\proj; type ~/x.txt"
        );
    }

    #[test]
    fn unix_homes_are_redacted_but_a_word_ending_in_home_is_not() {
        assert_eq!(redact_text("/home/ana/src and /Users/bob"), "~/src and ~");
        assert_eq!(redact_text("my/home/x"), "my/home/x");
    }

    #[test]
    fn a_path_outside_home_is_kept_whole_so_a_slot_can_still_be_released() {
        assert_eq!(
            redact_text(r"E:\gh\.wt\codai\task-3"),
            r"E:\gh\.wt\codai\task-3"
        );
    }

    #[test]
    fn a_token_after_its_flag_or_in_an_assignment_is_hidden() {
        assert_eq!(
            redact_text("vitals serve --token abc123 --port 1"),
            "vitals serve --token [redacted] --port 1"
        );
        assert_eq!(
            redact_text("OPENAI_API_KEY=x1 GITHUB_TOKEN=y2 run"),
            "OPENAI_API_KEY=[redacted] GITHUB_TOKEN=[redacted] run"
        );
        assert_eq!(redact_text("Bearer abc"), "Bearer [redacted]");
    }

    #[test]
    fn a_recognisable_credential_is_hidden_without_a_flag() {
        let out = redact_text("curl -H x ghp_0123456789abcdefABCDEF0123 end");
        assert_eq!(out, "curl -H x [redacted] end");
        let out = redact_text("key=\"sk-live-0123456789abcdef\"");
        assert!(!out.contains("sk-live"), "{out}");
    }

    #[test]
    fn shas_uuids_and_ordinary_words_survive() {
        let text = "git push origin 0123456789abcdef0123456789abcdef01234567 \
                    0b058ac1-1111-2222-3333-444455556666 pnpm test";
        assert_eq!(redact_text(text), text);
    }

    #[test]
    fn whitespace_is_preserved_exactly() {
        assert_eq!(redact_text("  a\t b \n"), "  a\t b \n");
    }

    #[test]
    fn identifiers_pass_through_and_every_other_string_is_scrubbed() {
        let snapshot = AgentsSnapshot {
            agentq_installed: true,
            resources: vec![AgentqResource {
                name: "build:codai".into(),
                kind: ResourceKind::Build,
                capacity: Some(1),
                tickets: vec![AgentqTicket {
                    id: "t-0123456789abcdefghijklmnopqrstuvwxyz".into(),
                    resource: "build:codai".into(),
                    seq: 1,
                    held: true,
                    lane: None,
                    priority: None,
                    pid: Some(4),
                    session: Some("s-0123456789abcdefghijklmnopqrstuvwxyz".into()),
                    purpose: Some(r"gate in C:\Users\dragos\x".into()),
                    cmd: Some("pnpm test --token sekrit".into()),
                    repo: Some(r"C:\Users\dragos\gh\codai".into()),
                    created_ms: None,
                    acquired_ms: None,
                    beat_ms: None,
                    liveness: Liveness::Alive,
                    yielded: false,
                    wedged: false,
                }],
            }],
            leases: vec![AgentqLease {
                lease_id: "46b5f311".into(),
                repo: "codai".into(),
                slot: "task-3".into(),
                role: None,
                path: Some(r"C:\Users\dragos\.wt\codai\task-3".into()),
                branch: None,
                purpose: None,
                session: None,
                pid: None,
                created_ms: None,
                beat_ms: None,
                owner_alive: None,
                stale: false,
            }],
            ..AgentsSnapshot::default()
        };
        let json = redact_snapshot(&snapshot).to_string();
        assert!(!json.contains("dragos"), "{json}");
        assert!(!json.contains("sekrit"), "{json}");
        assert!(
            json.contains("t-0123456789abcdefghijklmnopqrstuvwxyz"),
            "{json}"
        );
        assert!(
            json.contains("s-0123456789abcdefghijklmnopqrstuvwxyz"),
            "{json}"
        );
        assert!(json.contains(r#""leaseId":"46b5f311""#), "{json}");
        assert!(json.contains(r"~\\.wt\\codai\\task-3"), "{json}");
    }

    #[test]
    fn an_outcome_is_redacted_but_keeps_its_verdict() {
        let out = redact_outcome(ActionOutcome {
            ok: false,
            code: Some(3),
            output: r"busy: C:\Users\dragos\x".into(),
        });
        assert_eq!(out.output, r"busy: ~\x");
        assert_eq!((out.ok, out.code), (false, Some(3)));
    }
}
