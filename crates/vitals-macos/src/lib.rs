//! # vitals-macos
//!
//! macOS backend. Implements the [`vitals_core::provider`] traits over
//! Mach, `libproc`, `IOKit` and `sysctl`.
//!
//! Unimplemented until the Windows backend is complete — see the note in
//! `vitals-linux` for why the crate exists regardless.

#![cfg(target_os = "macos")]

// Planned mapping:
//
//   ProcessProvider  -> libproc (proc_listpids, proc_pidinfo), Mach task_info
//   SystemProvider   -> host_statistics64, sysctl, IOKit
//   SensorProvider   -> IOKit SMC / AppleSMC; Apple Silicon via IOReport
//   NetworkProvider  -> libproc PROC_PIDFDSOCKETINFO, pf/Network Extension
//   PowerProvider    -> IOPMCopy*, pmset
//
// Note: several of these require entitlements or are unavailable in a
// sandboxed app. The capability model must reflect that honestly rather than
// failing at the call site.
