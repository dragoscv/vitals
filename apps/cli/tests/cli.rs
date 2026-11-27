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
        "ps",
        "top",
        "info",
        "eco",
        "kill",
        "suspend",
        "resume",
        "priority",
        "report",
        "mcp",
        "serve",
        "completions",
        "sensors",
        "history",
        "alerts",
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

#[cfg(windows)]
#[test]
fn top_on_a_pipe_prints_one_snapshot_and_exits() {
    // `vitals()` captures stdout, so it is a pipe, never a terminal.
    let out = vitals(&["--source", "local", "top"]);
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).expect("utf-8");
    assert!(
        stdout.contains("CPU ") && stdout.contains("Alerts"),
        "one summary line was expected:\n{stdout}"
    );
    assert!(
        !stdout.contains("press q"),
        "a piped run must not enter the interactive loop, which needs raw mode:\n{stdout}"
    );
}

#[test]
fn an_unreachable_attach_address_exits_three() {
    // 3 = cannot reach the app. Nothing listens on port 1; the attach fails
    // before any source is sampled, so this is deterministic on every platform.
    let out = vitals(&["--attach", "http://127.0.0.1:1", "ps"]);
    assert_eq!(out.status.code(), Some(3), "{out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!stderr.contains("panicked"), "{stderr}");
}

#[test]
fn serve_refuses_control_headless_with_a_clear_message_and_exit_four() {
    // 4 = a control action was refused. `--control` is refused before the port
    // is bound, so this neither listens nor hangs.
    let out = vitals(&["serve", "--control", "--port", "0"]);
    assert_eq!(out.status.code(), Some(4), "{out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("--control is refused"),
        "the refusal must say what was refused:\n{stderr}"
    );
}

#[cfg(windows)]
#[test]
fn history_from_a_direct_sampler_exits_five() {
    // 5 = the named data was not found. History lives in the desktop's store,
    // and a direct sampler has none, which it says rather than inventing one.
    let out = vitals(&["--source", "local", "history"]);
    assert_eq!(out.status.code(), Some(5), "{out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("history"),
        "the message must name what is missing:\n{stderr}"
    );
}

#[test]
fn completions_prints_a_script_for_the_named_shell() {
    let out = vitals(&["completions", "bash"]);
    assert!(out.status.success(), "{out:?}");
    let script = String::from_utf8(out.stdout).expect("utf-8");
    assert!(script.contains("vitals"), "the script names the program");
    assert!(!script.trim().is_empty(), "an empty completion script");
}

#[test]
fn completions_refuses_an_unknown_shell_with_a_usage_error() {
    let out = vitals(&["completions", "not-a-shell"]);
    assert_eq!(out.status.code(), Some(2), "{out:?}");
}

#[cfg(windows)]
#[test]
fn a_piped_run_never_collects_escape_bytes_with_or_without_no_color() {
    // The styled note must not reach a pipe, and NO_COLOR must not change that.
    // The three styling cases (TTY, NO_COLOR, pipe) are unit-tested where a
    // terminal can be simulated; this proves the real binary agrees on a pipe.
    for no_color in [None, Some("1")] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_vitals"));
        command.args(["--source", "local", "ps"]);
        if let Some(value) = no_color {
            command.env("NO_COLOR", value);
        }
        let out = command.output().expect("spawn vitals");
        assert!(
            !out.stdout.contains(&0x1b),
            "stdout collected an escape byte (NO_COLOR={no_color:?})"
        );
        assert!(
            !out.stderr.contains(&0x1b),
            "stderr collected an escape byte (NO_COLOR={no_color:?})"
        );
    }
}

/// Reads the first stdout line within `within`, then leaves the child alive
/// for the caller to kill. Bounded so a server that never prints fails the
/// test rather than hanging it.
#[cfg(windows)]
fn first_stdout_line(
    child: &mut std::process::Child,
    within: std::time::Duration,
) -> Option<String> {
    use std::io::BufRead;
    use std::sync::mpsc;

    let out = child.stdout.take()?;
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut reader = std::io::BufReader::new(out);
        let mut line = String::new();
        let ok = reader.read_line(&mut line).is_ok();
        let _ = tx.send((ok, line));
    });
    match rx.recv_timeout(within) {
        Ok((true, line)) => Some(line),
        _ => None,
    }
}

#[cfg(windows)]
#[test]
fn serve_json_prints_one_machine_readable_line_before_serving() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_vitals"))
        .args(["serve", "--json", "--port", "0"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn vitals serve --json");

    let line = first_stdout_line(&mut child, std::time::Duration::from_secs(20));
    let _ = child.kill();
    let _ = child.wait();
    let line = line.expect("serve --json must describe the server on stdout");

    let info: serde_json::Value =
        serde_json::from_str(line.trim()).expect("the first line must be one JSON object");
    assert!(info["port"].as_u64().is_some_and(|p| p > 0), "{info}");
    assert_eq!(info["scope"], "read", "{info}");
    assert!(
        info["token"].as_str().is_some_and(|t| t.len() >= 40),
        "a generated token must be reported so a script can use it: {info}"
    );
    assert_eq!(info["generated"], true, "{info}");
    assert!(
        info["pairingCode"].as_str().is_some_and(|c| c.len() >= 6),
        "{info}"
    );
}
