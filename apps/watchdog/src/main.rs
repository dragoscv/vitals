//! Vitals watchdog: notices lag and freezes, names the process behind them,
//! and offers to end it from a notification.
//!
//! Born from 2026-09-28, when a Gradle daemon four shells deep inside a VS
//! Code terminal held the machine at 95 % and the only way out was killing
//! VS Code. See `docs/adr/0032-lag-watchdog.md`.
//!
//! ```text
//! vitals-watchdog              run (what the logon entry starts)
//! vitals-watchdog --diagnose   sample 10 s, print the verdict, change nothing
//! vitals-watchdog --test-toast show a sample proposal whose buttons do nothing
//! vitals-watchdog --install    start at every logon (current user, no admin)
//! vitals-watchdog --uninstall  stop starting at logon
//! ```
//!
//! It never ends anything on its own; every action is a button press.

#![cfg_attr(all(windows, not(test)), windows_subsystem = "windows")]
// The pure modules are compiled and tested on the Linux CI leg too, but only
// the Windows loop calls them, so outside tests they read as dead there.
#![cfg_attr(not(windows), allow(dead_code))]

mod action;
mod detect;
mod forest;
mod strings;

#[cfg(windows)]
mod notify;
#[cfg(windows)]
mod run;
#[cfg(windows)]
mod win;

fn main() -> anyhow::Result<()> {
    #[cfg(windows)]
    {
        run::main()
    }
    #[cfg(not(windows))]
    {
        anyhow::bail!("the watchdog is Windows-only (ADR-0007)")
    }
}
