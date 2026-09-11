//! Exercises the Task Manager replacement against this real machine.
//!
//! Read-only by default, because the write needs elevation and a prover
//! that silently rewrites `HKLM` is not something anybody should run twice.
//! It prints what the registry actually says and what an unelevated write
//! actually returns — the two facts no unit test can establish, since both
//! depend on the token this process is running with.
//!
//! ```text
//! cargo run -p vitals-win --example prove_taskmgr
//! cargo run -p vitals-win --example prove_taskmgr -- --launch
//! ```
//!
//! `--launch` additionally starts the real Task Manager through the
//! debuggee path, which is the only way to see that the bypass works and
//! that the window survives this process exiting.

#![allow(clippy::print_stdout, clippy::expect_used)]

use vitals_win::actions::{ReplacementStatus, launch_real_task_manager, replacement_status};

fn main() {
    println!("== Task Manager replacement (IFEO) ==\n");

    let status = replacement_status().expect("IFEO is readable without elevation");
    match &status {
        ReplacementStatus::NotReplaced => {
            println!("status      : NotReplaced (Ctrl+Shift+Esc opens Windows Task Manager)");
        }
        ReplacementStatus::ReplacedByUs { path } => {
            println!("status      : ReplacedByUs");
            println!("debugger    : {path}");
        }
        ReplacementStatus::ReplacedByOther { debugger } => {
            println!("status      : ReplacedByOther");
            println!("debugger    : {debugger}");
            println!("note        : Vitals will refuse to overwrite this.");
        }
    }
    println!("is_ours     : {}", status.is_ours());

    // The write path, without performing it: `set_replacement` would raise a
    // UAC prompt, which a prover must not do unasked. What is worth showing
    // is that the unelevated attempt is refused rather than half-succeeding.
    let exe = std::env::current_exe().expect("our own path");
    println!("\nour exe     : {}", exe.display());

    match vitals_win::actions::write_replacement(true, &exe) {
        Ok(()) => println!(
            "unelevated  : WROTE — this process is elevated, and the key now points at the prover"
        ),
        Err(err) => println!("unelevated  : refused as expected — {err}"),
    }

    if std::env::args().any(|arg| arg == "--launch") {
        println!("\nlaunching the real Task Manager as a debuggee…");
        match launch_real_task_manager() {
            Ok(()) => println!(
                "launched    : ok — it must still be on screen after this process exits, \
                 which is what DebugSetProcessKillOnExit(FALSE) buys"
            ),
            Err(err) => println!("launched    : failed — {err}"),
        }
    } else {
        println!("\n(pass --launch to also start the real Task Manager through the bypass)");
    }
}
