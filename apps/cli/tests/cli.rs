//! The built `vitals` binary, run as a user runs it (QA-14).
//!
//! The unit tests in `main.rs` parse argument vectors; none of them ran the
//! executable, so a panic in `main`, output on the wrong stream or a JSON
//! shape the parser in `ps` never sees would pass every test. These spawn
//! the real binary with `--source local`, so the result never depends on
//! whether a desktop app happens to be running on the test machine.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::process::{Command, Output};

fn vitals(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_vitals"))
        .args(args)
        .output()
        .expect("spawn the vitals binary")
}

#[test]
fn help_lists_every_subcommand_the_cli_defines() {
    let out = vitals(&["--help"]);
    assert!(out.status.success(), "{out:?}");
    let help = String::from_utf8(out.stdout).expect("utf-8 help");
    // Mirrors `enum Commands` in main.rs. A subcommand missing from help is
    // a feature nobody will find.
    for name in [
        "ps", "top", "info", "eco", "kill", "suspend", "resume", "priority", "report", "serve",
    ] {
        assert!(
            help.lines()
                .any(|line| line.trim_start().starts_with(&format!("{name} "))),
            "`vitals --help` does not list `{name}`:\n{help}"
        );
    }
}

#[test]
fn an_unknown_subcommand_fails_with_a_usage_error_not_a_panic() {
    let out = vitals(&["definitely-not-a-command"]);
    assert_eq!(out.status.code(), Some(2), "{out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!stderr.contains("panicked"), "{stderr}");
}

#[cfg(windows)]
#[test]
fn ps_json_is_an_array_of_processes_each_carrying_its_pid() {
    let out = vitals(&["--source", "local", "ps", "--json"]);
    assert!(out.status.success(), "{out:?}");
    // stdout is the pipe a script reads: it must be JSON and nothing else.
    let list: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("ps --json prints only JSON on stdout");
    let list = list.as_array().expect("a JSON array");
    assert!(!list.is_empty(), "a running machine has processes");
    let own = std::process::id();
    let mut saw_self = false;
    for process in list {
        let pid = process["key"]["pid"]
            .as_u64()
            .unwrap_or_else(|| panic!("a process without a numeric key.pid: {process}"));
        if pid == u64::from(own) {
            saw_self = true;
        }
    }
    assert!(
        saw_self,
        "the test process {own} is running and must appear in the list"
    );
}

#[cfg(windows)]
#[test]
fn info_succeeds_and_json_info_is_an_object() {
    let out = vitals(&["--source", "local", "info"]);
    assert!(out.status.success(), "{out:?}");
    assert!(!out.stdout.is_empty(), "info printed nothing");

    let out = vitals(&["--source", "local", "info", "--json"]);
    assert!(out.status.success(), "{out:?}");
    let value: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("info --json prints only JSON on stdout");
    assert!(value.is_object(), "{value}");
}

/// Spawns a harmless process the test owns, so acting on it touches
/// nothing else on the machine.
#[cfg(windows)]
fn sleeper() -> std::process::Child {
    Command::new("ping")
        .args(["-n", "120", "127.0.0.1"])
        .stdout(std::process::Stdio::null())
        .spawn()
        .expect("spawn ping")
}

#[cfg(windows)]
#[test]
fn priority_suspend_resume_and_kill_act_on_a_real_process_by_pid() {
    let mut child = sleeper();
    let pid = child.id().to_string();

    for args in [
        vec!["priority", pid.as_str(), "below-normal"],
        vec!["suspend", pid.as_str()],
        vec!["resume", pid.as_str()],
    ] {
        let mut full = vec!["--source", "local", "--json"];
        full.extend(&args);
        let out = vitals(&full);
        assert!(out.status.success(), "{args:?}: {out:?}");
        let reply: serde_json::Value =
            serde_json::from_slice(&out.stdout).expect("--json prints only JSON");
        assert_eq!(
            reply["pid"].as_u64(),
            Some(u64::from(child.id())),
            "{reply}"
        );
    }
    assert!(
        child.try_wait().expect("poll child").is_none(),
        "suspend + resume must leave the process running"
    );

    let out = vitals(&["--source", "local", "kill", &pid]);
    assert!(out.status.success(), "{out:?}");
    let status = child.wait().expect("wait for the killed child");
    assert!(!status.success(), "kill must end the process: {status:?}");
}

#[cfg(windows)]
#[test]
fn kill_refuses_a_pid_that_is_not_running_with_a_non_zero_exit() {
    let mut child = sleeper();
    let pid = child.id();
    child.kill().expect("kill child");
    child.wait().expect("reap child");

    let out = vitals(&["--source", "local", "kill", &pid.to_string()]);
    assert!(!out.status.success(), "{out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains(&format!("PID {pid}")), "{stderr}");
    assert!(!stderr.contains("panicked"), "{stderr}");
}
