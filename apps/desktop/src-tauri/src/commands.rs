//! Tauri commands exposed to the webview.
//!
//! Kept deliberately few. High-frequency data does **not** come through
//! `invoke` — it is pushed over an event channel from the sampler thread.
//! Request/response over the JS bridge at 1 Hz for thousands of rows would
//! serialise on the webview's main thread and make the UI stutter, which is
//! the exact failure this product exists to avoid.

use serde::Deserialize;
use tauri::State;

use vitals_core::capability::Capabilities;
#[cfg(windows)]
use vitals_core::ids::ProcessKey;
use vitals_core::provider::HostInfo;
use vitals_core::sample::SampleRate;

use crate::state::AppState;

/// Error type returned to the frontend.
///
/// Carries a machine-readable `kind` alongside the message so the UI can
/// branch — offering elevation for a denial, staying silent for a process
/// that simply exited — rather than showing every failure as the same toast.
#[derive(Debug, thiserror::Error, serde::Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum CommandError {
    #[error("{message}")]
    AccessDenied { message: String },
    #[error("{message}")]
    NotFound { message: String },
    #[error("{message}")]
    Unsupported { message: String },
    /// The backend declined on purpose: the user dismissed a UAC prompt, or
    /// the action would have trampled something that is not ours. Not a
    /// fault, so the UI should state it and not offer a retry-as-admin.
    #[error("{message}")]
    Refused { message: String },
    #[error("{message}")]
    Internal { message: String },
}

impl From<vitals_core::Error> for CommandError {
    fn from(err: vitals_core::Error) -> Self {
        let message = err.to_string();
        match err {
            vitals_core::Error::AccessDenied { .. }
            | vitals_core::Error::HelperUnavailable { .. } => Self::AccessDenied { message },
            vitals_core::Error::NotFound(_) => Self::NotFound { message },
            vitals_core::Error::Unsupported(_) => Self::Unsupported { message },
            vitals_core::Error::Refused(_) => Self::Refused { message },
            _ => Self::Internal { message },
        }
    }
}

type CommandResult<T> = std::result::Result<T, CommandError>;

/// Static machine facts, fetched once at startup.
///
/// Read on demand rather than cached in state: it is a few milliseconds, it
/// is asked for once when the About panel opens, and caching it would mean
/// deciding when to invalidate a value that can genuinely change under a
/// running process — a CPU cannot be hot-swapped, but a VM can be migrated.
#[tauri::command]
pub fn get_host_info() -> CommandResult<HostInfo> {
    #[cfg(windows)]
    {
        Ok(vitals_win::hostinfo::read())
    }

    #[cfg(not(windows))]
    {
        Err(CommandError::Unsupported {
            message: "host info needs a platform backend, and only Windows has one".into(),
        })
    }
}

/// What the backend can currently do, plus the runtime facts that change
/// how a reading should be *labelled* rather than whether it exists.
///
/// A DTO rather than returning [`Capabilities`] bare: the disk counter
/// source is not a capability — the Disk column works either way — but it is
/// the same kind of thing, a privilege-dependent runtime fact the UI must
/// know before it can describe what it is showing. Bolting it onto this one
/// response means the screen makes one call, not two.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityReport {
    #[serde(flatten)]
    pub capabilities: Capabilities,
    /// Which kernel counter produced the per-process disk figures.
    ///
    /// `None` before the first sample has landed. Not defaulted to either
    /// variant: labelling the column "storage I/O" when we have not yet
    /// learnt whether the kernel granted us that class would be a claim
    /// about a measurement nobody has taken.
    pub disk_counter_source: Option<vitals_core::process::DiskCounterSource>,
}

/// What the backend can currently do, given privileges and installed parts.
#[tauri::command]
// Tauri's command macro requires `State` by value; it cannot be borrowed.
#[allow(clippy::needless_pass_by_value)]
pub fn get_capabilities(state: State<'_, AppState>) -> CapabilityReport {
    CapabilityReport {
        capabilities: state.capabilities(),
        disk_counter_source: state.disk_counter_source(),
    }
}

/// Makes the sampler's next frame a keyframe.
///
/// Called by the webview when it starts listening for frames. Deltas are only
/// meaningful against a baseline, and a window that attaches between the
/// periodic keyframes would otherwise show a partial process list — measured
/// at 116 of 780 — for up to thirty seconds. Desktop-only: LAN clients get a
/// keyframe from the server on connect.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn request_keyframe(state: State<'_, AppState>) {
    state.request_keyframe();
}

/// The alerts currently raised, most serious first.
///
/// Request/response for the initial list; changes arrive as
/// `vitals://alerts` events. Splitting the two means a window that opens
/// mid-episode sees the alert immediately instead of waiting for the next
/// transition.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn get_alerts(alerts: State<'_, crate::alerts::Alerts>) -> Vec<vitals_core::alerts::Alert> {
    alerts.active()
}

/// The Notifications panel's switches, pushed whenever they change.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn set_alert_prefs(alerts: State<'_, crate::alerts::Alerts>, prefs: crate::alerts::AlertPrefs) {
    alerts.set_prefs(prefs);
}

/// "Why is my PC slow?" — the current alerts joined against the process list.
///
/// Request/response, and the webview supplies the processes it already
/// holds, because the alternative is the backend cloning six hundred
/// processes every tick for a button that is pressed once a week. The
/// system metrics come from the same frame so the two agree on the instant.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn diagnose(
    alerts: State<'_, crate::alerts::Alerts>,
    system: vitals_core::metrics::SystemMetrics,
    processes: Vec<vitals_core::process::Process>,
) -> vitals_core::diagnosis::Diagnosis {
    vitals_core::diagnosis::diagnose(&alerts.active(), &system, &processes)
}

/// Localised toast titles, pushed at start and on a language change.
///
/// Toasts render in Rust because they must fire while the window is hidden;
/// the strings live in the webview because that is where i18n is. So the
/// webview hands them over once.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn set_alert_strings(
    alerts: State<'_, crate::alerts::Alerts>,
    strings: crate::alerts::NotificationStrings,
) {
    alerts.set_strings(strings);
}

/// Whether the title-bar × hides Vitals instead of quitting it.
///
/// Pushed from the webview whenever the setting changes, and once after
/// hydration, because the decision is made in Rust — the window event fires
/// before the frontend hears about it, so asking the webview then would be
/// too late.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn set_close_to_tray(tray: State<'_, crate::tray::Tray>, enabled: bool) {
    tray.set_close_to_tray(enabled);
}

/// Localised tray menu labels and tooltip words.
///
/// Same argument as [`set_alert_strings`]: the tray renders in Rust so it
/// works with the window hidden, but i18n lives in the webview.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn set_tray_strings(tray: State<'_, crate::tray::Tray>, strings: crate::tray::TrayStrings) {
    tray.set_strings(&strings);
}

/// Exits the process.
///
/// With `closeToTray` on, the × no longer quits, so there has to be something
/// that does. Invoked by the tray's Quit item and available to the UI.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn quit_app(app: tauri::AppHandle) {
    app.exit(0);
}

/// Adjusts the sampling cadence.
///
/// Called by the frontend on window visibility and focus changes so a hidden
/// window costs almost nothing.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn set_sample_rate(state: State<'_, AppState>, rate: SampleRate) {
    state.set_sample_rate(rate);
}

/// Reveals the main window once the UI has painted.
///
/// The window is created hidden (`"visible": false`) so the user never sees
/// an empty white rectangle while the webview boots — a flash that makes a
/// native app feel like a web page. The cost of that choice is that
/// something must actually show it, and if nothing does the app runs with no
/// visible window at all.
///
/// Called from the frontend after first paint rather than from `setup`,
/// because at `setup` time the webview has not rendered anything yet.
#[tauri::command]
// Tauri injects the window by value; it cannot hand us a borrow.
#[allow(clippy::needless_pass_by_value)]
pub fn show_main_window(window: tauri::Window) -> CommandResult<()> {
    // Tauri injects the window the call came from, so no lookup is needed —
    // and using the caller's window is more correct than looking up "main"
    // by label, which would break the moment a second window exists.
    window.show().map_err(|e| vitals_core::Error::Os {
        context: format!("show window: {e}"),
        code: 0,
    })?;
    window.set_focus().map_err(|e| vitals_core::Error::Os {
        context: format!("focus window: {e}"),
        code: 0,
    })?;

    Ok(())
}

/// Whether the process was started by the autostart entry.
///
/// The Run-key command line always carries `--minimized`; whether the window
/// actually stays hidden is the frontend's decision, made from the
/// `startMinimised` setting once it has hydrated. Doing it here would need
/// the setting before the store is readable.
#[must_use]
pub fn launched_minimised() -> bool {
    std::env::args().skip(1).any(|arg| arg == "--minimized")
}

/// Runtime facts about how this instance was started.
#[derive(Debug, Clone, Copy, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchOptions {
    /// Started by the autostart entry rather than by the user.
    pub autostarted: bool,
}

/// Lets the frontend decide whether to reveal the window at all.
#[tauri::command]
pub fn get_launch_options() -> LaunchOptions {
    LaunchOptions {
        autostarted: launched_minimised(),
    }
}

/// How dangerous an action is, as the frontend sees it.
///
/// Mirrors [`vitals_win::Risk`] rather than re-exporting it so the wire
/// contract does not change shape when a platform backend does.
#[derive(Debug, Clone, Copy, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ActionRisk {
    Safe,
    Disruptive,
    Critical,
    Forbidden,
}

/// What will happen if an action proceeds.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionPlan {
    pub risk: ActionRisk,
    /// Translation key for the consequence, so the UI stays localised.
    pub consequence: String,
    pub needs_confirmation: bool,
    /// Whether "retry as administrator" is worth offering.
    ///
    /// False for protected processes, where elevation cannot help and the
    /// prompt is a dead end.
    pub elevation_might_help: bool,
}

/// Assesses terminating a process, without doing it.
///
/// Separate from [`terminate_process`] so the UI can state a specific
/// consequence before committing. A single generic "are you sure?" is what
/// trains people to click through warnings.
#[tauri::command]
#[cfg(windows)]
// Tauri deserialises command arguments into owned values; it cannot hand us a
// borrow.
#[allow(clippy::needless_pass_by_value)]
pub fn plan_terminate_process(pid: u32, name: Option<String>, protected: bool) -> ActionPlan {
    use vitals_core::ids::Pid;

    let plan = vitals_win::actions::plan_terminate(Pid(pid), name.as_deref(), protected);

    ActionPlan {
        risk: map_risk(plan.risk),
        consequence: plan.consequence.to_owned(),
        needs_confirmation: plan.risk.needs_confirmation(),
        elevation_might_help: plan.risk.elevation_might_help(),
    }
}

#[cfg(windows)]
const fn map_risk(risk: vitals_win::Risk) -> ActionRisk {
    match risk {
        vitals_win::Risk::Safe => ActionRisk::Safe,
        vitals_win::Risk::Disruptive => ActionRisk::Disruptive,
        vitals_win::Risk::Critical => ActionRisk::Critical,
        vitals_win::Risk::Forbidden => ActionRisk::Forbidden,
    }
}

/// Assesses suspending a process, without doing it.
#[tauri::command]
#[cfg(windows)]
#[allow(clippy::needless_pass_by_value)]
pub fn plan_suspend_process(pid: u32, name: Option<String>, protected: bool) -> ActionPlan {
    use vitals_core::ids::Pid;

    let plan = vitals_win::actions::plan_suspend(Pid(pid), name.as_deref(), protected);

    ActionPlan {
        risk: map_risk(plan.risk),
        consequence: plan.consequence.to_owned(),
        needs_confirmation: plan.risk.needs_confirmation(),
        elevation_might_help: plan.risk.elevation_might_help(),
    }
}

/// Terminates a process.
///
/// Takes the start time as well as the PID: between the frame that listed
/// the process and this call, it can exit and its PID be reused. Acting on
/// the PID alone would kill whichever process inherited the number.
///
/// `confirmed` is true only when the user confirmed the risk dialog for
/// this process. The backend re-assesses the live process and refuses a
/// critical one without it; the flag cannot make a protected one possible.
#[tauri::command]
#[cfg(windows)]
pub fn terminate_process(pid: u32, start_time: u64, confirmed: Option<bool>) -> CommandResult<()> {
    use vitals_core::ids::Pid;

    vitals_win::actions::terminate(ProcessKey::new(Pid(pid), start_time), 1, consent(confirmed))?;
    Ok(())
}

/// Suspends every thread in a process.
#[tauri::command]
#[cfg(windows)]
pub fn suspend_process(pid: u32, start_time: u64, confirmed: Option<bool>) -> CommandResult<()> {
    use vitals_core::ids::Pid;

    vitals_win::actions::suspend(ProcessKey::new(Pid(pid), start_time), consent(confirmed))?;
    Ok(())
}

#[cfg(windows)]
fn consent(confirmed: Option<bool>) -> vitals_win::actions::Consent {
    if confirmed == Some(true) {
        vitals_win::actions::Consent::Confirmed
    } else {
        vitals_win::actions::Consent::Unconfirmed
    }
}

/// Resumes a suspended process.
#[tauri::command]
#[cfg(windows)]
pub fn resume_process(pid: u32, start_time: u64) -> CommandResult<()> {
    use vitals_core::ids::Pid;

    vitals_win::actions::resume(ProcessKey::new(Pid(pid), start_time))?;
    Ok(())
}

/// What the webview may ask to do as administrator.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ElevatedActionDto {
    Terminate,
    Suspend,
    Resume,
}

/// Retries one denied process action as administrator, behind a UAC prompt.
///
/// The risk dialog's "Retry as administrator" had no handler: a process
/// owned by SYSTEM or another account could be neither ended nor paused.
/// This re-launches Vitals elevated for exactly one action (see
/// `vitals_win::actions::elevated`); the UI itself never runs elevated.
///
/// `async` + `spawn_blocking`: the call waits for as long as the prompt is
/// on screen, which must not hold the IPC thread.
///
/// Desktop-only, deliberately not in LAN control: a phone must never be able
/// to raise a UAC prompt on the machine it is watching.
#[tauri::command]
#[cfg(windows)]
pub async fn process_action_as_admin(
    action: ElevatedActionDto,
    pid: u32,
    start_time: u64,
    confirmed: Option<bool>,
) -> CommandResult<()> {
    use vitals_core::ids::Pid;
    use vitals_win::actions::ElevatedAction;

    let action = match action {
        ElevatedActionDto::Terminate => ElevatedAction::Terminate,
        ElevatedActionDto::Suspend => ElevatedAction::Suspend,
        ElevatedActionDto::Resume => ElevatedAction::Resume,
    };
    let key = ProcessKey::new(Pid(pid), start_time);
    let consent = consent(confirmed);

    tauri::async_runtime::spawn_blocking(move || {
        vitals_win::actions::run_as_admin(action, key, consent)
    })
    .await
    .map_err(|err| CommandError::Internal {
        message: format!("the elevated action was abandoned: {err}"),
    })??;
    Ok(())
}

/// Scheduling priority, as the webview names it.
///
/// A DTO rather than deriving `Serialize` onto `vitals_win::Priority`: the
/// platform enum maps onto Windows priority classes and is free to grow a
/// variant that has no meaning to the UI. This is the wire contract, and it
/// is meant to be boring.
#[derive(Debug, Clone, Copy, Deserialize)]
#[cfg(windows)]
#[serde(rename_all = "kebab-case")]
pub enum PriorityDto {
    Idle,
    BelowNormal,
    Normal,
    AboveNormal,
    High,
    Realtime,
}

#[cfg(windows)]
impl From<PriorityDto> for vitals_win::Priority {
    fn from(value: PriorityDto) -> Self {
        match value {
            PriorityDto::Idle => Self::Idle,
            PriorityDto::BelowNormal => Self::BelowNormal,
            PriorityDto::Normal => Self::Normal,
            PriorityDto::AboveNormal => Self::AboveNormal,
            PriorityDto::High => Self::High,
            PriorityDto::Realtime => Self::Realtime,
        }
    }
}

/// Changes a process's scheduling priority.
///
/// `vitals-win` has implemented this since the actions module landed, and the
/// capability report advertised it — but no command exposed it, so the UI
/// listed it as unimplemented. The backend was claiming a capability the
/// frontend had no way to reach.
///
/// Realtime is deliberately reachable. A CPU-bound process there can starve
/// input and make the machine look frozen, which is why the UI warns; hiding
/// the option outright would just send people to Task Manager to do the same
/// thing with less warning.
#[tauri::command]
#[cfg(windows)]
#[allow(clippy::needless_pass_by_value)]
pub fn set_process_priority(pid: u32, start_time: u64, priority: PriorityDto) -> CommandResult<()> {
    use vitals_core::ids::Pid;

    vitals_win::actions::set_priority(ProcessKey::new(Pid(pid), start_time), priority.into())?;
    Ok(())
}

/// Pins a process to a set of logical processors.
///
/// The mask is a bitfield, one bit per logical processor. An empty mask is
/// refused by the backend rather than being treated as "all": a process
/// affinitised to no processor cannot be scheduled at all, and Windows
/// returns a bare `INVALID_PARAMETER` that says nothing about why.
#[tauri::command]
#[cfg(windows)]
pub fn set_process_affinity(pid: u32, start_time: u64, mask: u64) -> CommandResult<()> {
    use vitals_core::ids::Pid;

    vitals_win::actions::set_affinity(ProcessKey::new(Pid(pid), start_time), mask)?;
    Ok(())
}

/// Whether a process is currently throttled ("efficiency mode").
///
/// `None` means the state could not be read, which is **not** the same as
/// off: a protected or higher-integrity process denies us the handle, and a
/// UI told "off" would offer to turn on a throttle it cannot set. The
/// frontend renders the difference.
#[tauri::command]
#[cfg(windows)]
pub fn get_efficiency_mode(pid: u32, start_time: u64) -> CommandResult<Option<bool>> {
    use vitals_core::ids::Pid;

    Ok(vitals_win::actions::efficiency_mode(ProcessKey::new(
        Pid(pid),
        start_time,
    ))?)
}

/// Switches a process's efficiency mode on or off.
///
/// Deliberately desktop-only: unlike terminate and priority it is not in
/// `ControlRequest`, because throttling a process from a phone is a change
/// whose effect the person holding the phone cannot see.
#[tauri::command]
#[cfg(windows)]
pub fn set_efficiency_mode(pid: u32, start_time: u64, enabled: bool) -> CommandResult<()> {
    use vitals_core::ids::Pid;

    vitals_win::actions::set_efficiency_mode(ProcessKey::new(Pid(pid), start_time), enabled)?;
    Ok(())
}

/// Lists the kernel handles a process holds.
///
/// `async` + `spawn_blocking`: the system handle table is megabytes on a
/// busy machine and naming the objects can take hundreds of milliseconds, so
/// running it inline would block every other command behind it. Called only
/// when the user expands the Handles section, never on a timer.
#[tauri::command]
#[cfg(windows)]
pub async fn get_process_handles(
    pid: u32,
    start_time: u64,
) -> CommandResult<Vec<vitals_core::process::HandleInfo>> {
    use vitals_core::ids::Pid;

    let key = ProcessKey::new(Pid(pid), start_time);
    let handle =
        tauri::async_runtime::spawn_blocking(move || vitals_win::handles::for_process(key));

    handle
        .await
        .map_err(|err| CommandError::Internal {
            message: format!("the handle enumeration thread did not finish: {err}"),
        })?
        .map_err(CommandError::from)
}

/// Lists the modules mapped into a process.
///
/// Same argument as [`get_process_handles`]: several hundred modules for a
/// browser, each needing a path resolved out of the target's address space.
#[tauri::command]
#[cfg(windows)]
pub async fn get_process_modules(
    pid: u32,
    start_time: u64,
) -> CommandResult<Vec<vitals_core::process::ModuleInfo>> {
    use vitals_core::ids::Pid;

    let key = ProcessKey::new(Pid(pid), start_time);
    let handle =
        tauri::async_runtime::spawn_blocking(move || vitals_win::modules::for_process(key));

    handle
        .await
        .map_err(|err| CommandError::Internal {
            message: format!("the module enumeration thread did not finish: {err}"),
        })?
        .map_err(CommandError::from)
}

/// The full executable path of a process, when it lets us read it.
///
/// `None` for a process that denies the handle. The two shell actions are
/// disabled in the UI on `None`, which is the right answer: an enabled button
/// that always fails for `csrss.exe` teaches the user the panel is broken.
#[tauri::command]
#[cfg(windows)]
pub fn get_executable_path(pid: u32, start_time: u64) -> CommandResult<Option<String>> {
    use vitals_core::ids::Pid;

    Ok(vitals_win::actions::executable_path(ProcessKey::new(
        Pid(pid),
        start_time,
    ))?)
}

/// Opens Explorer with a file selected.
///
/// Takes the path rather than a `ProcessKey`: the caller already holds the
/// executable path from the process detail, and re-deriving it here would
/// open a second handle for a value we were just given.
#[tauri::command]
#[cfg(windows)]
#[allow(clippy::needless_pass_by_value)]
pub fn open_file_location(path: String) -> CommandResult<()> {
    vitals_win::actions::open_file_location(std::path::Path::new(&path))?;
    Ok(())
}

/// Shows the shell's Properties dialog for a file.
#[tauri::command]
#[cfg(windows)]
#[allow(clippy::needless_pass_by_value)]
pub fn show_file_properties(path: String) -> CommandResult<()> {
    vitals_win::actions::show_file_properties(std::path::Path::new(&path))?;
    Ok(())
}

/// Who Ctrl+Shift+Esc currently opens, as the Settings switch sees it.
///
/// A DTO rather than serialising `vitals_win::ReplacementStatus` directly:
/// the switch needs one boolean and, when it is disabled, the name of the
/// tool that owns the hook — not a platform enum.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskManagerReplacement {
    /// Vitals is what the taskbar menu and Ctrl+Shift+Esc launch.
    pub enabled: bool,
    /// Another program owns the hook, so the switch must be disabled and
    /// say why. `None` when the hook is ours or absent.
    pub replaced_by: Option<String>,
    /// The executable the hook points at when `enabled`, for display.
    pub path: Option<String>,
}

/// Reads the Task Manager replacement state from the registry.
///
/// Uncached: the value can be changed by another tool while Vitals is open,
/// and the Settings panel asks once when it mounts.
#[tauri::command]
#[cfg(windows)]
pub fn get_taskmgr_replacement() -> CommandResult<TaskManagerReplacement> {
    use vitals_win::actions::ReplacementStatus;

    Ok(match vitals_win::actions::replacement_status()? {
        ReplacementStatus::NotReplaced => TaskManagerReplacement {
            enabled: false,
            replaced_by: None,
            path: None,
        },
        ReplacementStatus::ReplacedByUs { path } => TaskManagerReplacement {
            enabled: true,
            replaced_by: None,
            path: Some(path),
        },
        ReplacementStatus::ReplacedByOther { debugger } => TaskManagerReplacement {
            enabled: false,
            replaced_by: Some(debugger),
            path: None,
        },
    })
}

/// Makes Vitals — or stops it being — what Ctrl+Shift+Esc opens.
///
/// Writes `HKLM`, so this triggers a UAC prompt on the first change. A
/// declined prompt comes back as [`CommandError::Refused`], and so does an
/// attempt to displace another tool's hook. `async` because the elevated
/// child is waited on, and the wait must not block the command thread that
/// the rest of the UI is invoking on.
#[tauri::command]
#[cfg(windows)]
pub async fn set_taskmgr_replacement(enabled: bool) -> CommandResult<TaskManagerReplacement> {
    let exe = std::env::current_exe().map_err(vitals_core::Error::from)?;

    tauri::async_runtime::spawn_blocking(move || {
        vitals_win::actions::set_replacement(enabled, &exe)
    })
    .await
    .map_err(|err| CommandError::Internal {
        message: format!("the registry write was abandoned: {err}"),
    })??;

    get_taskmgr_replacement()
}

/// Opens the real Windows Task Manager even while Vitals has replaced it.
///
/// The tray menu's "Open Windows Task Manager" needs this: with the hook
/// on, a plain `taskmgr.exe` launch would start a second Vitals instead.
#[tauri::command]
#[cfg(windows)]
pub fn launch_real_taskmgr() -> CommandResult<()> {
    vitals_win::actions::launch_real_task_manager()?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Non-Windows stubs
//
// Present rather than absent so `invoke` fails with a reason the UI can
// render, instead of Tauri's "command not found" — which reads as a bug in
// Vitals rather than a platform that has no backend yet.
// ---------------------------------------------------------------------------

/// See the Windows implementation.
///
/// # Errors
///
/// Always: no non-Windows backend exists yet.
#[tauri::command]
#[cfg(not(windows))]
pub fn get_efficiency_mode(pid: u32, start_time: u64) -> CommandResult<Option<bool>> {
    let _ = (pid, start_time);
    Err(unsupported("efficiency mode"))
}

/// See the Windows implementation.
///
/// # Errors
///
/// Always: no non-Windows backend exists yet.
#[tauri::command]
#[cfg(not(windows))]
pub fn set_efficiency_mode(pid: u32, start_time: u64, enabled: bool) -> CommandResult<()> {
    let _ = (pid, start_time, enabled);
    Err(unsupported("efficiency mode"))
}

/// See the Windows implementation.
///
/// # Errors
///
/// Always: no non-Windows backend exists yet.
#[tauri::command]
#[cfg(not(windows))]
pub fn get_process_handles(
    pid: u32,
    start_time: u64,
) -> CommandResult<Vec<vitals_core::process::HandleInfo>> {
    let _ = (pid, start_time);
    Err(unsupported("handle enumeration"))
}

/// See the Windows implementation.
///
/// # Errors
///
/// Always: no non-Windows backend exists yet.
#[tauri::command]
#[cfg(not(windows))]
pub fn get_process_modules(
    pid: u32,
    start_time: u64,
) -> CommandResult<Vec<vitals_core::process::ModuleInfo>> {
    let _ = (pid, start_time);
    Err(unsupported("module enumeration"))
}

/// See the Windows implementation.
///
/// # Errors
///
/// Always: no non-Windows backend exists yet.
#[tauri::command]
#[cfg(not(windows))]
pub fn get_executable_path(pid: u32, start_time: u64) -> CommandResult<Option<String>> {
    let _ = (pid, start_time);
    Err(unsupported("the executable path"))
}

/// See the Windows implementation.
///
/// # Errors
///
/// Always: UAC is a Windows mechanism.
#[tauri::command]
#[cfg(not(windows))]
pub fn process_action_as_admin(
    action: ElevatedActionDto,
    pid: u32,
    start_time: u64,
    confirmed: Option<bool>,
) -> CommandResult<()> {
    let _ = (action, pid, start_time, confirmed);
    Err(unsupported("retrying as administrator"))
}

/// See the Windows implementation.
///
/// # Errors
///
/// Always: no non-Windows backend exists yet.
#[tauri::command]
#[cfg(not(windows))]
#[allow(clippy::needless_pass_by_value)]
pub fn open_file_location(path: String) -> CommandResult<()> {
    let _ = path;
    Err(unsupported("revealing a file"))
}

/// See the Windows implementation.
///
/// # Errors
///
/// Always: no non-Windows backend exists yet.
#[tauri::command]
#[cfg(not(windows))]
#[allow(clippy::needless_pass_by_value)]
pub fn show_file_properties(path: String) -> CommandResult<()> {
    let _ = path;
    Err(unsupported("the file properties dialog"))
}

/// See the Windows implementation.
///
/// # Errors
///
/// Always: Image File Execution Options is a Windows mechanism.
#[tauri::command]
#[cfg(not(windows))]
pub fn get_taskmgr_replacement() -> CommandResult<()> {
    Err(unsupported("replacing Task Manager"))
}

/// See the Windows implementation.
///
/// # Errors
///
/// Always: Image File Execution Options is a Windows mechanism.
#[tauri::command]
#[cfg(not(windows))]
pub fn set_taskmgr_replacement(enabled: bool) -> CommandResult<()> {
    let _ = enabled;
    Err(unsupported("replacing Task Manager"))
}

/// See the Windows implementation.
///
/// # Errors
///
/// Always: there is no Task Manager to launch.
#[tauri::command]
#[cfg(not(windows))]
pub fn launch_real_taskmgr() -> CommandResult<()> {
    Err(unsupported("launching Task Manager"))
}

/// The one place the non-Windows refusal is worded.
#[cfg(not(windows))]
fn unsupported(what: &str) -> CommandError {
    CommandError::Unsupported {
        message: format!("{what} needs a platform backend, and only Windows has one"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn access_denied_maps_to_an_elevation_prompt() {
        let err: CommandError = vitals_core::Error::AccessDenied {
            operation: "terminate".into(),
        }
        .into();
        assert!(matches!(err, CommandError::AccessDenied { .. }));
    }

    #[test]
    fn a_vanished_process_maps_to_not_found_not_a_crash() {
        let err: CommandError = vitals_core::Error::NotFound("pid 42".into()).into();
        assert!(matches!(err, CommandError::NotFound { .. }));
    }

    #[test]
    fn missing_helper_is_treated_as_fixable_by_elevating() {
        let err: CommandError = vitals_core::Error::HelperUnavailable {
            reason: "not installed".into(),
        }
        .into();
        assert!(
            matches!(err, CommandError::AccessDenied { .. }),
            "the UI should offer to install/start the helper, not show a generic failure"
        );
    }

    #[test]
    fn errors_serialise_with_a_discriminating_tag() {
        let err = CommandError::NotFound {
            message: "gone".into(),
        };
        let json = serde_json::to_string(&err).expect("serialise");
        // kebab-case matches the generated TypeScript unions, so the frontend
        // discriminates on the same literals everywhere.
        assert!(json.contains("\"kind\":\"not-found\""), "got {json}");
    }
}
