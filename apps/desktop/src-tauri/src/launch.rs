//! What the command line says about why this process was started.
//!
//! Three launches are not "the user opened Vitals", and each is decided here
//! before Tauri builds anything, because two of them must never show a
//! window at all:
//!
//! - `--set-taskmgr-replacement on|off` — we are the **elevated child** that
//!   a running instance spawned under UAC to write `HKLM`. Do the write,
//!   exit with the result, start nothing else.
//! - `--launch-real-taskmgr` — start the real Task Manager, bypassing our
//!   own hook, and exit. Exists so a shortcut or script can still reach
//!   `taskmgr.exe` while the replacement is on.
//! - `--elevated-process-action <action> <pid> <start>` — we are the
//!   **elevated child** a running instance spawned under UAC to end, pause
//!   or resume one process it was refused. Do that, exit with the result.
//! - `<anything ending in taskmgr.exe>` as the first argument — Windows
//!   launched us **as the debugger for Task Manager** because the hook is
//!   on. This is the taskbar menu or Ctrl+Shift+Esc; proceed to normal
//!   startup, where the single-instance plugin focuses the window that is
//!   already running. Unless we are *elevated*: a person's shortcut never
//!   is, so that is Task Manager's own elevation re-launch being redirected
//!   to us, and the right answer is to start the real Task Manager and
//!   exit — see [`handed_off_from_task_manager`].

/// What the arguments ask for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchMode {
    /// Start the app normally.
    Normal,
    /// Windows redirected a Task Manager launch to us.
    ///
    /// Behaves like [`LaunchMode::Normal`]; distinguished so the log says
    /// why the window appeared and so `--minimized` semantics never apply
    /// to it — the user pressed the shortcut *because* they want a window.
    AsTaskManager,
    /// Perform the elevated registry write and exit.
    SetReplacement { enabled: bool },
    /// Start the real `taskmgr.exe` and exit.
    LaunchRealTaskManager,
    /// Perform one process action as administrator and exit.
    ///
    /// The arguments are kept raw and parsed by `vitals-win`, which owns
    /// their format; a malformed set is reported by the child's exit code
    /// rather than falling through to a UI launch.
    ElevatedProcessAction { args: Vec<String> },
    /// Make one startup or service change as administrator and exit.
    ///
    /// Same contract as [`LaunchMode::ElevatedProcessAction`]: raw
    /// arguments, parsed by `vitals-win`, never a window.
    ElevatedStartupAction { args: Vec<String> },
}

/// Classifies the arguments this process was started with.
///
/// Pure, so it is testable without spawning anything. Takes the arguments
/// **after** the executable name.
#[must_use]
pub fn classify<I, S>(args: I) -> LaunchMode
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut args = args.into_iter();

    let Some(first) = args.next() else {
        return LaunchMode::Normal;
    };
    let first = first.as_ref();

    if first == vitals_win_arg::SET_REPLACEMENT {
        return match args.next().as_ref().map(AsRef::as_ref) {
            Some("on") => LaunchMode::SetReplacement { enabled: true },
            Some("off") => LaunchMode::SetReplacement { enabled: false },
            // A malformed internal argument is a bug in the caller, not a
            // user request; starting the UI would hide it. Refuse the write
            // and fall through to a normal start so at least something
            // visible happens.
            _ => LaunchMode::Normal,
        };
    }

    if first == "--launch-real-taskmgr" {
        return LaunchMode::LaunchRealTaskManager;
    }

    // Never falls through to a window, even when malformed: this is only
    // ever passed by our own parent, which is blocked waiting on the exit
    // code, and a UI appearing instead would hang it.
    if first == vitals_win_arg::PROCESS_ACTION {
        return LaunchMode::ElevatedProcessAction {
            args: args.map(|arg| arg.as_ref().to_owned()).collect(),
        };
    }

    if first == vitals_win_arg::STARTUP_ACTION {
        return LaunchMode::ElevatedStartupAction {
            args: args.map(|arg| arg.as_ref().to_owned()).collect(),
        };
    }

    // The loader passes the original command line — typically
    // `C:\WINDOWS\system32\taskmgr.exe` or, for the taskbar, the same path
    // with `/4` or `/7` after it — so only the first token is inspected,
    // with any surrounding quotes stripped.
    if is_taskmgr_path(first) {
        return LaunchMode::AsTaskManager;
    }

    LaunchMode::Normal
}

/// Whether an argument is a path to `taskmgr.exe`, however it is cased or
/// quoted.
fn is_taskmgr_path(arg: &str) -> bool {
    let trimmed = arg.trim().trim_matches('"');
    trimmed
        .rsplit(['\\', '/'])
        .next()
        .is_some_and(|name| name.eq_ignore_ascii_case("taskmgr.exe"))
}

/// The internal argument spellings, shared with `vitals-win` on Windows.
///
/// On other platforms the constant is repeated here so the classifier still
/// compiles and is tested; the Windows build asserts the two agree.
mod vitals_win_arg {
    #[cfg(windows)]
    pub const SET_REPLACEMENT: &str = vitals_win::actions::SET_REPLACEMENT_ARG;
    #[cfg(not(windows))]
    pub const SET_REPLACEMENT: &str = "--set-taskmgr-replacement";
    #[cfg(windows)]
    pub const PROCESS_ACTION: &str = vitals_win::actions::PROCESS_ACTION_ARG;
    #[cfg(not(windows))]
    pub const PROCESS_ACTION: &str = "--elevated-process-action";
    #[cfg(windows)]
    pub const STARTUP_ACTION: &str = vitals_win::startup::STARTUP_ACTION_ARG;
    #[cfg(not(windows))]
    pub const STARTUP_ACTION: &str = "--elevated-startup-action";
}

/// Runs a non-UI launch mode to completion.
///
/// Returns the process exit code. `Normal` and `AsTaskManager` are not
/// handled here — they start the app — and asking for them is a programming
/// error surfaced as a non-zero exit rather than a panic, because
/// `panic = "abort"` in release would leave no message at all.
#[must_use]
pub fn run_headless(mode: &LaunchMode) -> i32 {
    match mode {
        LaunchMode::SetReplacement { enabled } => set_replacement(*enabled),
        LaunchMode::LaunchRealTaskManager => launch_real_taskmgr(),
        LaunchMode::ElevatedProcessAction { args } => elevated_process_action(args),
        LaunchMode::ElevatedStartupAction { args } => elevated_startup_action(args),
        LaunchMode::Normal | LaunchMode::AsTaskManager => {
            tracing::error!("run_headless called for a UI launch mode");
            2
        }
    }
}

#[cfg(windows)]
fn set_replacement(enabled: bool) -> i32 {
    // `write_replacement`, not `set_replacement`: this *is* the elevated
    // pass, and the un-elevated variant would re-prompt for UAC on a denial
    // instead of reporting it.
    let exe = match std::env::current_exe() {
        Ok(exe) => exe,
        Err(err) => {
            tracing::error!(%err, "cannot resolve our own executable path");
            return 1;
        }
    };

    match vitals_win::actions::write_replacement(enabled, &exe) {
        Ok(()) => {
            tracing::info!(enabled, "task manager replacement written");
            0
        }
        Err(err) => {
            tracing::error!(%err, "task manager replacement write failed");
            1
        }
    }
}

#[cfg(windows)]
fn launch_real_taskmgr() -> i32 {
    match vitals_win::actions::launch_real_task_manager() {
        Ok(()) => 0,
        Err(err) => {
            tracing::error!(%err, "could not start the real Task Manager");
            1
        }
    }
}

#[cfg(windows)]
fn elevated_process_action(args: &[String]) -> i32 {
    let code = vitals_win::actions::elevated::perform(args);
    tracing::info!(?args, code, "elevated process action finished");
    i32::try_from(code).unwrap_or(i32::MAX)
}

#[cfg(not(windows))]
fn elevated_process_action(_args: &[String]) -> i32 {
    tracing::error!("elevated process actions are a Windows mechanism");
    1
}

#[cfg(windows)]
fn elevated_startup_action(args: &[String]) -> i32 {
    let code = vitals_win::startup::control::perform(args);
    tracing::info!(?args, code, "elevated startup action finished");
    i32::try_from(code).unwrap_or(i32::MAX)
}

#[cfg(not(windows))]
fn elevated_startup_action(_args: &[String]) -> i32 {
    tracing::error!("elevated startup actions are a Windows mechanism");
    1
}

/// Whether this `AsTaskManager` launch is Task Manager's own elevation hop.
///
/// Not part of [`classify`] because it depends on the process token, not
/// the arguments, and `classify` is kept pure so it can be tested
/// exhaustively.
#[must_use]
pub fn handed_off_from_task_manager() -> bool {
    #[cfg(windows)]
    {
        vitals_win::actions::is_task_manager_elevation_hop()
    }
    #[cfg(not(windows))]
    {
        false
    }
}

#[cfg(not(windows))]
fn set_replacement(_enabled: bool) -> i32 {
    tracing::error!("Task Manager replacement is a Windows mechanism");
    1
}

#[cfg(not(windows))]
fn launch_real_taskmgr() -> i32 {
    tracing::error!("there is no Task Manager to launch on this platform");
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_arguments_is_a_normal_launch() {
        assert_eq!(classify(Vec::<&str>::new()), LaunchMode::Normal);
    }

    #[test]
    fn an_elevated_process_action_never_becomes_a_window_even_when_malformed() {
        // The parent is blocked on this child's exit code; a UI launch here
        // would hang it behind a UAC prompt the user already accepted.
        assert_eq!(
            classify([vitals_win_arg::PROCESS_ACTION, "terminate", "12", "34"]),
            LaunchMode::ElevatedProcessAction {
                args: vec!["terminate".into(), "12".into(), "34".into()]
            }
        );
        assert_eq!(
            classify([vitals_win_arg::PROCESS_ACTION]),
            LaunchMode::ElevatedProcessAction { args: vec![] }
        );
    }

    #[test]
    fn an_elevated_startup_action_never_becomes_a_window_even_when_malformed() {
        assert_eq!(
            classify([
                vitals_win_arg::STARTUP_ACTION,
                "service",
                "53",
                "stop",
                "confirmed"
            ]),
            LaunchMode::ElevatedStartupAction {
                args: vec![
                    "service".into(),
                    "53".into(),
                    "stop".into(),
                    "confirmed".into()
                ]
            }
        );
        assert_eq!(
            classify([vitals_win_arg::STARTUP_ACTION]),
            LaunchMode::ElevatedStartupAction { args: vec![] }
        );
    }

    #[test]
    fn the_taskbar_launch_is_recognised_by_its_first_argument_alone() {
        // This is the exact shape the loader hands a debugger: the original
        // command line, path first, Task Manager's own switches after.
        assert_eq!(
            classify([r"C:\WINDOWS\system32\taskmgr.exe", "/4"]),
            LaunchMode::AsTaskManager
        );
        assert_eq!(
            classify(["\"C:\\Windows\\System32\\TASKMGR.EXE\""]),
            LaunchMode::AsTaskManager
        );
        assert_eq!(classify(["taskmgr.exe"]), LaunchMode::AsTaskManager);
    }

    #[test]
    fn a_lookalike_is_not_task_manager() {
        // `nottaskmgr.exe`-style names and a directory called taskmgr.exe
        // are not the redirect. Both would otherwise suppress `--minimized`.
        assert_eq!(classify(["mytaskmgr.exe"]), LaunchMode::Normal);
        assert_eq!(classify([r"C:\taskmgr.exe\other.exe"]), LaunchMode::Normal);
        assert_eq!(classify(["--minimized"]), LaunchMode::Normal);
    }

    #[test]
    fn the_elevated_child_argument_carries_its_direction() {
        assert_eq!(
            classify(["--set-taskmgr-replacement", "on"]),
            LaunchMode::SetReplacement { enabled: true }
        );
        assert_eq!(
            classify(["--set-taskmgr-replacement", "off"]),
            LaunchMode::SetReplacement { enabled: false }
        );
    }

    #[test]
    fn a_malformed_elevated_argument_does_not_write_anything() {
        // Writing HKLM on a guess is the one thing this must never do.
        assert_eq!(classify(["--set-taskmgr-replacement"]), LaunchMode::Normal);
        assert_eq!(
            classify(["--set-taskmgr-replacement", "yes"]),
            LaunchMode::Normal
        );
    }

    #[test]
    fn the_real_task_manager_escape_hatch_is_recognised() {
        assert_eq!(
            classify(["--launch-real-taskmgr"]),
            LaunchMode::LaunchRealTaskManager
        );
    }

    #[test]
    fn a_ui_mode_handed_to_the_headless_runner_is_an_error_not_a_start() {
        assert_eq!(run_headless(&LaunchMode::Normal), 2);
        assert_eq!(run_headless(&LaunchMode::AsTaskManager), 2);
    }

    #[cfg(windows)]
    #[test]
    fn the_argument_spelling_agrees_with_vitals_win() {
        // Two spellings of the internal argument is how the elevated child
        // would silently start the UI instead of writing the key.
        assert_eq!(
            vitals_win_arg::SET_REPLACEMENT,
            vitals_win::actions::SET_REPLACEMENT_ARG
        );
    }
}
