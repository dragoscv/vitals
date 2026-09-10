//! Finding the running desktop app.
//!
//! The desktop writes `<data_dir>/local-api.json` when its loopback listener
//! comes up and removes it on a clean exit. The logic for `data_dir` is
//! duplicated from `apps/desktop/src-tauri/src/state.rs` on purpose: the CLI
//! must not link the desktop crate (Tauri, `WebView2` and a 30 s build) to read
//! one path. If the desktop moves the file, `docs/api/README.md` moves with
//! it, and so must this.

use std::path::{Path, PathBuf};

use serde::Deserialize;

/// What the desktop wrote. Mirrors `LocalApiDiscovery` in the desktop crate;
/// a field added there is ignored here rather than breaking the parse.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct Discovery {
    pub port: u16,
    pub pid: u32,
    pub version: String,
}

/// The directory every local data file lives in — the same rule the desktop
/// uses, so both resolve the same file on the same machine.
#[must_use]
pub fn data_dir() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .or_else(|| std::env::var_os("XDG_DATA_HOME"))
        .map_or_else(
            || PathBuf::from("."),
            |base| Path::new(&base).join("Vitals"),
        )
}

#[must_use]
pub fn discovery_path() -> PathBuf {
    data_dir().join("local-api.json")
}

/// Reads and validates the discovery file.
///
/// Returns `None` when the file is absent, unparseable, or names a process
/// that is no longer running. The last case is the one that matters: a
/// force-killed desktop leaves the file behind, and its port may by now
/// belong to something else entirely. Trusting it would mean sending
/// `POST /control` to whatever answered.
#[must_use]
pub fn read(path: &Path) -> Option<Discovery> {
    let bytes = std::fs::read(path).ok()?;
    let found: Discovery = serde_json::from_slice(&bytes).ok()?;
    validate(found, pid_is_alive)
}

/// The liveness check is injected so a test can exercise the stale-file
/// branch without needing a real dead PID that is guaranteed dead.
fn validate(found: Discovery, alive: impl Fn(u32) -> bool) -> Option<Discovery> {
    alive(found.pid).then_some(found)
}

/// Whether a process with this PID exists right now.
///
/// PIDs are recycled, so "alive" can be a different program by now; the
/// `/health` probe that follows catches that (a non-Vitals listener does not
/// answer it). This check only rules out the common case cheaply.
#[cfg(windows)]
#[must_use]
pub fn pid_is_alive(pid: u32) -> bool {
    // The desktop, by design, is unelevated and uses the same enumerator the
    // sampler does, so a PID it can see is a PID we can see.
    let mut enumerator = vitals_win::ProcessEnumerator::new();
    match enumerator.enumerate() {
        Ok(list) => list.iter().any(|p| p.key.pid.get() == pid),
        // If the process list cannot be read at all, we cannot vouch for the
        // file; the caller falls back to direct sampling, which will report
        // the same underlying failure in words.
        Err(_) => false,
    }
}

#[cfg(not(windows))]
#[must_use]
pub fn pid_is_alive(pid: u32) -> bool {
    Path::new("/proc").join(pid.to_string()).exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(pid: u32) -> Discovery {
        Discovery {
            port: 7330,
            pid,
            version: "0.1.0".into(),
        }
    }

    #[test]
    fn a_discovery_file_naming_a_dead_pid_is_rejected() {
        assert_eq!(validate(found(4242), |_| false), None);
    }

    #[test]
    fn a_discovery_file_naming_a_live_pid_is_trusted() {
        assert_eq!(validate(found(4242), |_| true), Some(found(4242)));
    }

    #[test]
    fn our_own_pid_is_alive_and_a_maximal_pid_is_not() {
        assert!(pid_is_alive(std::process::id()));
        // PIDs on Windows are multiples of 4 and far below this; on Linux
        // `pid_max` caps at 2^22. Either way nothing owns it.
        assert!(!pid_is_alive(u32::MAX - 1));
    }

    #[test]
    fn extra_fields_in_the_file_do_not_break_the_parse() {
        let json = r#"{"port":7330,"pid":1,"version":"0.2.0","future":true}"#;
        let parsed: Discovery = serde_json::from_str(json).expect("lenient parse");
        assert_eq!(parsed.version, "0.2.0");
    }
}
