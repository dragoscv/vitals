//! The Windows side: measuring, acting, notifying.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use vitals_core::error::Error;
use vitals_core::ids::{Pid, ProcessKey};
use vitals_win::actions::{
    Consent, ElevatedAction, Priority, ProcessFacts, Risk, assess_termination, run_as_admin,
    set_priority, terminate,
};
use vitals_win::process::{ProcessEnumerator, RawProcess};
use windows_sys::Win32::Foundation::CloseHandle;
use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcessId, GetCurrentThread, GetProcessInformation, OpenProcess,
    PROCESS_PROTECTION_LEVEL_INFORMATION, PROCESS_QUERY_LIMITED_INFORMATION, PROTECTION_LEVEL_NONE,
    ProcessProtectionLevelInfo, SetThreadPriority, THREAD_PRIORITY_NORMAL,
    THREAD_PRIORITY_TIME_CRITICAL,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    FindWindowW, GetForegroundWindow, GetMessageW, GetWindowThreadProcessId, IsHungAppWindow, MSG,
    SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_HOTKEY,
};

use crate::action::{Action, Shell, appeared};
use crate::forest::{Forest, Proc};

/// Raises the calling thread above every ordinary thread on the machine.
///
/// `TIME_CRITICAL` is 15, the ceiling of the non-realtime range, so the
/// watchdog keeps being scheduled while a Normal- or even High-class build
/// saturates every core — which is precisely when it must notice, decide and
/// end something. The process class is deliberately left Normal: a
/// `REALTIME` class would outrank the input stack itself and could produce
/// the freeze it exists to cure, and the thread sleeps almost all the time,
/// so the priority costs nothing when the machine is calm.
pub fn run_at_top_priority() -> bool {
    // SAFETY: pseudo-handle of the calling thread; no other preconditions.
    unsafe { SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_TIME_CRITICAL) != 0 }
}

/// Wake-up delay of an ordinary-priority thread, in microseconds.
///
/// The probe sleeps 10 ms at a time at `THREAD_PRIORITY_NORMAL` in a Normal
/// process — the same scheduling class as the threads that draw the cursor
/// and handle input in ordinary apps — and records how late it woke. CPU
/// percent cannot tell "busy but yielding" (a below-normal build) from "busy
/// and in the way"; this can.
#[derive(Debug, Clone)]
pub struct LagProbe {
    worst_us: Arc<AtomicU64>,
}

impl LagProbe {
    /// Starts the probe thread. It runs for the life of the process.
    pub fn start() -> std::io::Result<Self> {
        let worst_us = Arc::new(AtomicU64::new(0));
        let shared = Arc::clone(&worst_us);
        std::thread::Builder::new()
            .name("lag-probe".into())
            .spawn(move || {
                // The default timer resolution is 15.6 ms, so a 10 ms sleep
                // normally takes up to ~16 ms. Anything past that is delay.
                const ASKED: Duration = Duration::from_millis(10);
                const TICK: Duration = Duration::from_micros(15_625);
                // SAFETY: pseudo-handle of the calling thread.
                unsafe { SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_NORMAL) };
                loop {
                    let before = Instant::now();
                    std::thread::sleep(ASKED);
                    let late = before.elapsed().saturating_sub(TICK);
                    let us = u64::try_from(late.as_micros()).unwrap_or(u64::MAX);
                    shared.fetch_max(us, Ordering::Relaxed);
                }
            })?;
        Ok(Self { worst_us })
    }

    /// The worst delay since the last call, in ms.
    pub fn take_ms(&self) -> f64 {
        self.worst_us.swap(0, Ordering::Relaxed) as f64 / 1000.0
    }
}

/// Physical memory: (load percent, total bytes).
pub fn memory() -> Option<(u32, u64)> {
    status().map(|s| (s.dwMemoryLoad, s.ullTotalPhys))
}

/// Commit charge, percent of the commit limit.
///
/// The limit is RAM plus every page file. Running out of it is what took
/// `dwm.exe` down on 2026-10-05 with RAM to spare: WSL's `vmmemWSL` had
/// committed 84–109 GB of a 208 GB limit, and the rest went to everything
/// else.
pub fn commit_load() -> Option<u32> {
    status().and_then(|s| crate::detect::commit_load(s.ullTotalPageFile, s.ullAvailPageFile))
}

fn status() -> Option<MEMORYSTATUSEX> {
    let mut status = MEMORYSTATUSEX {
        dwLength: u32::try_from(size_of::<MEMORYSTATUSEX>()).unwrap_or(0),
        ..empty_status()
    };
    // SAFETY: `dwLength` is set; the struct is live.
    (unsafe { GlobalMemoryStatusEx(&raw mut status) } != 0).then_some(status)
}

const fn empty_status() -> MEMORYSTATUSEX {
    MEMORYSTATUSEX {
        dwLength: 0,
        dwMemoryLoad: 0,
        ullTotalPhys: 0,
        ullAvailPhys: 0,
        ullTotalPageFile: 0,
        ullAvailPageFile: 0,
        ullTotalVirtual: 0,
        ullAvailVirtual: 0,
        ullAvailExtendedVirtual: 0,
    }
}

/// The PID behind the foreground window, when Windows says it is hung.
///
/// `IsHungAppWindow` is the same test the shell uses to show "(Not
/// Responding)": no message pumped for five seconds.
pub fn hung_foreground() -> Option<Pid> {
    // SAFETY: no preconditions; a null result is handled.
    let hwnd = unsafe { GetForegroundWindow() };
    hung_owner(hwnd)
}

/// The PID behind the taskbar, when Windows says it is hung.
///
/// The foreground check alone missed the 2026-10-05 freezes: the taskbar
/// stopped answering while the window in front — a browser, a terminal —
/// was fine, so nothing was ever "hung" from the foreground's point of view
/// and the user could not reach Start, the clock or the notifications.
pub fn hung_taskbar() -> Option<Pid> {
    hung_owner(taskbar())
}

fn taskbar() -> windows_sys::Win32::Foundation::HWND {
    let class: Vec<u16> = "Shell_TrayWnd"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    // SAFETY: a valid wide class name and no title; a null result is handled
    // by every caller.
    unsafe { FindWindowW(class.as_ptr(), std::ptr::null()) }
}

fn window_pid(hwnd: windows_sys::Win32::Foundation::HWND) -> Option<Pid> {
    if hwnd.is_null() {
        return None;
    }
    let mut pid = 0_u32;
    // SAFETY: `hwnd` came from Windows; `pid` is a live out-pointer.
    unsafe { GetWindowThreadProcessId(hwnd, &raw mut pid) };
    (pid != 0).then_some(Pid(pid))
}

fn hung_owner(hwnd: windows_sys::Win32::Foundation::HWND) -> Option<Pid> {
    if hwnd.is_null() {
        return None;
    }
    // SAFETY: `hwnd` is a window handle Windows just returned.
    if unsafe { IsHungAppWindow(hwnd) } == 0 {
        return None;
    }
    window_pid(hwnd)
}

/// How long the foreground window takes to answer a message, in ms.
///
/// `WM_NULL` does nothing, so the time is pure queue latency: how long the
/// window's UI thread took to get round to its message loop. That is exactly
/// what a person feels as "it does not react". Measured on 2026-09-28 at
/// 97–100 % CPU from builds: 0.1–8 ms, so a busy machine alone never trips
/// it. `SMTO_ABORTIFHUNG` returns at once for a window Windows already
/// considers hung, which `hung_foreground` reports separately; the cap keeps
/// one frozen app from stalling this loop.
pub fn window_response_ms(cap_ms: u32) -> Option<f64> {
    // SAFETY: no preconditions; a null result is handled.
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.is_null() {
        return None;
    }
    let mut result = 0_usize;
    let started = Instant::now();
    // SAFETY: `hwnd` came from Windows; `result` is a live out-pointer.
    // WM_NULL (0) carries no parameters.
    let ok =
        unsafe { SendMessageTimeoutW(hwnd, 0, 0, 0, SMTO_ABORTIFHUNG, cap_ms, &raw mut result) };
    let ms = started.elapsed().as_secs_f64() * 1000.0;
    // A timeout is "at least the cap": report the cap, not the error.
    Some(if ok == 0 {
        ms.max(f64::from(cap_ms))
    } else {
        ms
    })
}

/// Seconds since the last keyboard or mouse input in this session.
pub fn idle_secs() -> u64 {
    use windows_sys::Win32::System::SystemInformation::GetTickCount64;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
    let mut info = LASTINPUTINFO {
        cbSize: u32::try_from(size_of::<LASTINPUTINFO>()).unwrap_or(0),
        dwTime: 0,
    };
    // SAFETY: `cbSize` is set and the struct is live.
    if unsafe { GetLastInputInfo(&raw mut info) } == 0 {
        return 0;
    }
    // `dwTime` is the low 32 bits of the tick count: compare in that width
    // so the 49.7-day wrap does not read as a very long absence.
    // SAFETY: no preconditions.
    let now = unsafe { GetTickCount64() } as u32;
    u64::from(now.wrapping_sub(info.dwTime)) / 1000
}

/// Plays an audio file once, at `volume` percent, on its own thread.
///
/// MCI rather than `PlaySound`, which only takes WAV: people pick MP3s.
/// Capped at ten seconds so a whole song chosen by mistake cannot play out
/// in full every time the machine stalls. Failure is logged, never fatal —
/// the toast is already on screen.
pub fn play_file(path: &str, volume: u8) {
    let path = path.to_owned();
    let spawned = std::thread::Builder::new()
        .name("sound".into())
        .spawn(move || {
            if let Err(error) = play_blocking(&path, volume) {
                tracing::warn!(%error, path = %path, "could not play the notification sound");
            }
        });
    if let Err(error) = spawned {
        tracing::warn!(%error, "could not start the sound thread");
    }
}

fn mci(command: &str) -> Result<(), String> {
    use windows_sys::Win32::Media::Multimedia::mciSendStringW;
    let wide: Vec<u16> = command.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: a valid wide string; no return buffer, no callback window.
    let code =
        unsafe { mciSendStringW(wide.as_ptr(), std::ptr::null_mut(), 0, std::ptr::null_mut()) };
    if code == 0 {
        Ok(())
    } else {
        Err(format!("MCI error {code} for {command:?}"))
    }
}

/// Plays an audio file once, on the calling thread, and says why not.
///
/// For the Settings test, which must fail visibly: `play_file` only logs.
pub fn play_blocking(path: &str, volume: u8) -> Result<(), String> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    if path.contains('"') {
        return Err("a quote in the path cannot be passed to MCI".into());
    }
    if !std::path::Path::new(path).is_file() {
        return Err("the file does not exist".into());
    }
    // A unique alias per play: two stalls in quick succession must not
    // close each other's sound.
    let alias = format!("vitalswd{}", NEXT.fetch_add(1, Ordering::Relaxed));
    mci(&format!(r#"open "{path}" type mpegvideo alias {alias}"#))?;
    let result = (|| {
        // MCI volume is 0..=1000.
        mci(&format!(
            "setaudio {alias} volume to {}",
            u32::from(volume.min(100)) * 10
        ))?;
        mci(&format!("set {alias} time format milliseconds"))?;
        // `to` past the end is MCIERR_OUTOFRANGE (282), not "play it all":
        // with a fixed 10000 every sound shorter than ten seconds - which
        // is every Windows sound - failed and only the log knew.
        let end = mci_length_ms(&alias)?.min(10_000);
        mci(&format!("play {alias} from 0 to {end} wait"))
    })();
    let _ = mci(&format!("close {alias}"));
    result
}

/// The length of an open MCI device, in its current time format.
fn mci_length_ms(alias: &str) -> Result<u32, String> {
    use windows_sys::Win32::Media::Multimedia::mciSendStringW;
    let command = format!("status {alias} length");
    let wide: Vec<u16> = command.encode_utf16().chain(std::iter::once(0)).collect();
    let mut buffer = [0u16; 32];
    // SAFETY: a valid wide string and a writable buffer of the stated length.
    let code = unsafe {
        mciSendStringW(
            wide.as_ptr(),
            buffer.as_mut_ptr(),
            buffer.len() as u32,
            std::ptr::null_mut(),
        )
    };
    if code != 0 {
        return Err(format!("MCI error {code} for {command:?}"));
    }
    let len = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    String::from_utf16_lossy(&buffer[..len])
        .trim()
        .parse()
        .map_err(|_| format!("MCI reported no length for {alias}"))
}

/// Per-process CPU, smoothed, from successive enumerations.
#[derive(Debug)]
pub struct Processes {
    enumerator: ProcessEnumerator,
    last: HashMap<ProcessKey, u64>,
    smooth: HashMap<ProcessKey, f64>,
    faults: HashMap<ProcessKey, u32>,
    /// Hard faults a second across every process, over the last refresh.
    pub hard_faults: f64,
    at: Option<Instant>,
    pub raw: Vec<RawProcess>,
}

impl Processes {
    pub fn new() -> Self {
        Self {
            enumerator: ProcessEnumerator::new(),
            last: HashMap::new(),
            smooth: HashMap::new(),
            faults: HashMap::new(),
            hard_faults: 0.0,
            at: None,
            raw: Vec::new(),
        }
    }

    /// Re-reads the process list and updates each process's smoothed load.
    ///
    /// Smoothed over about three seconds (EWMA, α = 0.5) so a process that
    /// happened to finish a burst just before the snapshot is not blamed
    /// over the one that has been saturating the machine for a minute.
    pub fn refresh(&mut self) -> vitals_core::error::Result<()> {
        let now = Instant::now();
        let raw = self.enumerator.enumerate()?;
        let elapsed = self.at.map(|at| now.duration_since(at).as_secs_f64());
        self.at = Some(now);

        let mut last = HashMap::with_capacity(raw.len());
        let mut smooth = HashMap::with_capacity(raw.len());
        let mut faults = HashMap::with_capacity(raw.len());
        let mut new_faults = 0_u64;
        for p in raw.iter().filter(|p| !p.is_idle_process()) {
            let time = p.cpu_time();
            if let (Some(secs), Some(&before)) = (elapsed, self.last.get(&p.key))
                && secs > 0.0
            {
                // 100 ns units → cores.
                let cores = time.saturating_sub(before) as f64 / 1e7 / secs;
                let previous = self.smooth.get(&p.key).copied().unwrap_or(cores);
                smooth.insert(p.key, 0.5 * previous + 0.5 * cores);
            }
            // Per process and only for processes seen last time: a summed
            // total would drop whenever a process exits and read a new one's
            // lifetime faults as a burst.
            if let Some(&before) = self.faults.get(&p.key) {
                new_faults += u64::from(p.hard_faults.saturating_sub(before));
            }
            faults.insert(p.key, p.hard_faults);
            last.insert(p.key, time);
        }
        self.hard_faults = match elapsed {
            Some(secs) if secs > 0.0 => new_faults as f64 / secs,
            _ => 0.0,
        };
        self.last = last;
        self.smooth = smooth;
        self.faults = faults;
        self.raw = raw;
        Ok(())
    }

    /// The current tree, excluding the idle process and ourselves.
    pub fn forest(&self) -> Forest {
        // SAFETY: no preconditions.
        let me = unsafe { GetCurrentProcessId() };
        Forest::new(
            self.raw
                .iter()
                .filter(|p| !p.is_idle_process() && p.key.pid.get() != me)
                .map(|p| Proc {
                    key: p.key,
                    parent: p.parent,
                    name: p.name.clone().unwrap_or_default(),
                    cpu: self.smooth.get(&p.key).copied().unwrap_or(0.0),
                    memory: p.private_bytes,
                })
                .collect(),
        )
    }

    pub fn key_of(&self, pid: Pid) -> Option<ProcessKey> {
        self.raw.iter().find(|p| p.key.pid == pid).map(|p| p.key)
    }

    /// Every process called `image` in `session`, by key.
    pub fn named_in(&self, image: &str, session: u32) -> Vec<ProcessKey> {
        self.raw
            .iter()
            .filter(|p| p.session_id == session)
            .filter(|p| {
                p.name
                    .as_deref()
                    .is_some_and(|n| n.eq_ignore_ascii_case(image))
            })
            .map(|p| p.key)
            .collect()
    }

    /// The session this watchdog runs in — the user's desktop.
    pub fn own_session(&self) -> Option<u32> {
        // SAFETY: no preconditions.
        let me = unsafe { GetCurrentProcessId() };
        self.raw
            .iter()
            .find(|p| p.key.pid.get() == me)
            .map(|p| p.session_id)
    }

    /// Whether the process already runs below normal priority.
    pub fn is_lowered(&self, key: ProcessKey) -> bool {
        self.raw
            .iter()
            .find(|p| p.key == key)
            .is_some_and(|p| p.base_priority < 8)
    }
}

/// Whether a process is worth proposing at all.
///
/// `System` (drivers and interrupts), `csrss`, `lsass` and protected
/// processes cannot be changed from here whatever the user clicks, so naming
/// them only produces a notification with nothing useful in it.
pub fn actionable(pid: Pid, name: &str) -> bool {
    !is_protected(pid)
        && matches!(
            assess_termination(ProcessFacts {
                pid,
                name: Some(name),
                break_on_termination: None,
                protected: false,
                is_self: false,
            }),
            Risk::Safe | Risk::Disruptive
        )
}

/// Whether Windows protects the process (antimalware, DRM, LSA).
///
/// No static list: the kernel says so. `MsMpEng.exe` showed up live at 1.2
/// cores marked endable, and ending it is refused however it is asked.
/// A process that cannot even be opened for limited query is treated as
/// protected too — nothing useful could be done to it from a toast.
fn is_protected(pid: Pid) -> bool {
    // SAFETY: `OpenProcess` validates its arguments; null on failure.
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid.get()) };
    if handle.is_null() {
        return true;
    }
    let mut info = PROCESS_PROTECTION_LEVEL_INFORMATION { ProtectionLevel: 0 };
    // SAFETY: the handle is live; `info` is exactly the declared size.
    let ok = unsafe {
        GetProcessInformation(
            handle,
            ProcessProtectionLevelInfo,
            (&raw mut info).cast(),
            u32::try_from(size_of::<PROCESS_PROTECTION_LEVEL_INFORMATION>()).unwrap_or(0),
        )
    };
    // SAFETY: opened above, closed once.
    unsafe { CloseHandle(handle) };
    ok != 0 && info.ProtectionLevel != PROTECTION_LEVEL_NONE
}

/// How ending `name` is judged before it is offered at all.
///
/// The authoritative check runs again inside `terminate`, on the live handle,
/// with the kernel's own flags. This one only decides whether to show the
/// button: a session-critical process (`dwm`, `explorer`, `audiodg`) is
/// `Disruptive` and never offered from a notification, where the user has
/// one click and no explanation.
pub fn endable(pid: Pid, name: &str) -> bool {
    assess_termination(ProcessFacts {
        pid,
        name: Some(name),
        break_on_termination: None,
        protected: false,
        is_self: false,
    }) == Risk::Safe
}

/// What happened when an action ran.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Done { others: usize },
    Gone,
    Failed(String),
}

/// Ends `key` and — when `tree` — everything it started, children first.
///
/// Children first, because ending a parent first leaves its children
/// orphaned and still running, which is the exact state that made the
/// original incident need VS Code to be killed. The descendants are read
/// again now, not taken from the snapshot that produced the notification:
/// a build started new workers in the minutes the toast was on screen.
/// Each process goes through `terminate`, which re-checks the start time
/// (PID reuse) and refuses a critical process however it got into the tree.
pub fn end(processes: &mut Processes, key: ProcessKey, tree: bool) -> Outcome {
    if let Err(error) = processes.refresh() {
        return Outcome::Failed(error.to_string());
    }
    let forest = processes.forest();
    let Some(root) = forest.find(key) else {
        return Outcome::Gone;
    };
    let below = if tree {
        forest.descendants(root)
    } else {
        Vec::new()
    };

    let mut others = 0;
    for &i in below.iter().rev() {
        if let Some(p) = forest.get(i)
            && terminate(p.key, 1, Consent::Unconfirmed).is_ok()
        {
            others += 1;
        }
    }

    match terminate(key, 1, Consent::Unconfirmed) {
        Ok(()) => Outcome::Done { others },
        Err(Error::NotFound(_)) => Outcome::Gone,
        // Another account's process: one UAC prompt, for the root only. The
        // elevated child re-verifies identity and risk on its own.
        Err(Error::AccessDenied { .. }) => {
            match run_as_admin(ElevatedAction::Terminate, key, Consent::Unconfirmed) {
                Ok(()) => Outcome::Done { others },
                Err(Error::NotFound(_)) => Outcome::Gone,
                Err(error) => Outcome::Failed(error.to_string()),
            }
        }
        Err(error) => Outcome::Failed(error.to_string()),
    }
}

/// Lowers `key` and everything it started to below-normal priority.
///
/// Every descendant, because a priority class is only inherited at
/// `CreateProcess`: the workers a build already started keep theirs.
pub fn lower(processes: &mut Processes, key: ProcessKey) -> Outcome {
    if let Err(error) = processes.refresh() {
        return Outcome::Failed(error.to_string());
    }
    let forest = processes.forest();
    let Some(root) = forest.find(key) else {
        return Outcome::Gone;
    };
    let mut others = 0;
    for i in forest.descendants(root) {
        if let Some(p) = forest.get(i)
            && set_priority(p.key, Priority::BelowNormal).is_ok()
        {
            others += 1;
        }
    }
    match set_priority(key, Priority::BelowNormal) {
        Ok(()) => Outcome::Done { others },
        Err(Error::NotFound(_)) => Outcome::Gone,
        Err(error) => Outcome::Failed(error.to_string()),
    }
}

/// What a restart may pass as consent.
///
/// Always `Unconfirmed`, even though the click or hotkey *is* the person's
/// confirmation. `explorer` and `dwm` are `Disruptive`, which `terminate`
/// already allows unconfirmed; `Confirmed` would only additionally unlock a
/// `Critical` process — one the kernel marks `BreakOnTermination` — and a
/// restart button must never be the way to reach that. If a driver ever
/// marks either critical, the restart is refused rather than bugchecking.
pub const fn restart_consent(_shell: Shell) -> Consent {
    Consent::Unconfirmed
}

/// How long winlogon gets to bring the shell back before we start it.
const SHELL_COMES_BACK: Duration = Duration::from_secs(5);

/// Restarts `shell`: the exact `key` when a toast proposed it, otherwise
/// (a hotkey) the one running in this session now.
pub fn restart(processes: &mut Processes, shell: Shell, key: Option<ProcessKey>) -> Outcome {
    if let Err(error) = processes.refresh() {
        return Outcome::Failed(error.to_string());
    }
    let Some(session) = processes.own_session() else {
        return Outcome::Failed("the desktop session could not be read".into());
    };
    match shell {
        Shell::Explorer => restart_explorer(processes, session, key),
        Shell::Desktop => restart_desktop(processes, session, key),
    }
}

/// The process to restart: the proposed one if it still runs, else for a
/// hotkey the taskbar's owner, else any of that name in the session.
fn resolve(
    processes: &Processes,
    shell: Shell,
    session: u32,
    key: Option<ProcessKey>,
) -> Option<ProcessKey> {
    let running = processes.named_in(shell.image(), session);
    if let Some(key) = key {
        return running.contains(&key).then_some(key);
    }
    let owner = (shell == Shell::Explorer)
        .then(|| window_pid(taskbar()))
        .flatten()
        .and_then(|pid| running.iter().copied().find(|k| k.pid == pid));
    owner.or_else(|| running.first().copied())
}

/// Ends the hung shell and makes sure a working one comes back.
///
/// "Restart" means "leave a working taskbar", so a shell that already
/// exited is not a failure: the shell is still checked for and started.
/// Windows normally restarts it on its own (`AutoRestartShell`); starting
/// it ourselves only when it did not avoids two shells fighting over the
/// taskbar.
fn restart_explorer(processes: &mut Processes, session: u32, key: Option<ProcessKey>) -> Outcome {
    let before = processes.named_in(Shell::Explorer.image(), session);
    let target = resolve(processes, Shell::Explorer, session, key);
    if let Some(target) = target {
        match terminate(target, 1, restart_consent(Shell::Explorer)) {
            Ok(()) | Err(Error::NotFound(_)) => {}
            Err(error) => return Outcome::Failed(error.to_string()),
        }
    }
    let killed = target.map(|k| k.pid);

    let deadline = Instant::now() + SHELL_COMES_BACK;
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(250));
        let taskbar_owner = window_pid(taskbar());
        if taskbar_owner.is_some() && taskbar_owner != killed {
            tracing::info!(by = "windows", "explorer came back");
            return Outcome::Done { others: 0 };
        }
        if processes.refresh().is_ok()
            && let Some(fresh) = appeared(
                &before,
                &processes.named_in(Shell::Explorer.image(), session),
            )
        {
            tracing::info!(by = "windows", pid = fresh.pid.get(), "explorer came back");
            return Outcome::Done { others: 0 };
        }
    }

    let root = std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into());
    let exe = std::path::Path::new(&root).join("explorer.exe");
    match std::process::Command::new(&exe).spawn() {
        Ok(child) => {
            tracing::info!(by = "watchdog", pid = child.id(), "explorer started");
            Outcome::Done { others: 0 }
        }
        Err(error) => Outcome::Failed(format!("{}: {error}", exe.display())),
    }
}

/// Ends `dwm.exe` as administrator; Windows starts a new one at once.
///
/// DWM runs as `Window Manager\DWM-n`, another account, so an unelevated
/// `TerminateProcess` is always denied and is not even attempted: it would
/// only cost a round-trip before the same UAC prompt.
fn restart_desktop(processes: &Processes, session: u32, key: Option<ProcessKey>) -> Outcome {
    let Some(target) = resolve(processes, Shell::Desktop, session, key) else {
        return Outcome::Gone;
    };
    match run_as_admin(
        ElevatedAction::Terminate,
        target,
        restart_consent(Shell::Desktop),
    ) {
        Ok(()) => Outcome::Done { others: 0 },
        Err(Error::NotFound(_)) => Outcome::Gone,
        Err(error) => Outcome::Failed(error.to_string()),
    }
}

/// Ctrl+Alt+Shift, never auto-repeating: a held key must not restart the
/// shell ten times a second.
pub const HOTKEY_MODIFIERS: u32 = {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT,
    };
    MOD_CONTROL | MOD_ALT | MOD_SHIFT | MOD_NOREPEAT
};

/// Registers the restart hotkeys on a thread of their own and forwards each
/// press to `tx`.
///
/// Hotkeys because a toast is drawn by the shell: when `explorer` hangs, the
/// notification offering to restart it may never be clickable. `WM_HOTKEY`
/// is posted by the kernel's input stack to this thread's queue, with no
/// shell involved. A combination another app already owns is logged and
/// skipped; the watchdog carries on without it.
pub fn start_hotkeys(tx: Sender<Action>) {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::RegisterHotKey;
    let spawned = std::thread::Builder::new()
        .name("hotkeys".into())
        .spawn(move || {
            // The thread sleeps in GetMessageW, so top priority costs nothing
            // and means a press is answered while a build starves the CPU.
            run_at_top_priority();
            let mut registered = 0;
            for shell in [Shell::Explorer, Shell::Desktop] {
                let (id, vk) = shell.hotkey();
                // SAFETY: a null window binds the hotkey to this thread's queue.
                if unsafe { RegisterHotKey(std::ptr::null_mut(), id, HOTKEY_MODIFIERS, vk) } == 0 {
                    tracing::warn!(
                        hotkey = shell.hotkey_text(),
                        "hotkey already taken by another app; not available"
                    );
                } else {
                    registered += 1;
                    tracing::info!(hotkey = shell.hotkey_text(), "hotkey registered");
                }
            }
            if registered == 0 {
                return;
            }
            let mut msg = MSG::default();
            loop {
                // SAFETY: `msg` is a live out-pointer; a null window reads
                // this thread's own queue, where WM_HOTKEY arrives.
                let got = unsafe { GetMessageW(&raw mut msg, std::ptr::null_mut(), 0, 0) };
                if got == 0 || got == -1 {
                    return;
                }
                if msg.message != WM_HOTKEY {
                    continue;
                }
                let Some(shell) = i32::try_from(msg.wParam).ok().and_then(Shell::from_hotkey)
                else {
                    continue;
                };
                if tx.send(Action::Hotkey(shell)).is_err() {
                    return;
                }
            }
        });
    if let Err(error) = spawned {
        tracing::warn!(%error, "could not start the hotkey thread");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_restart_never_carries_the_consent_that_unlocks_a_critical_process() {
        for shell in [Shell::Explorer, Shell::Desktop] {
            assert_eq!(restart_consent(shell), Consent::Unconfirmed);
        }
    }

    #[test]
    fn the_hotkeys_need_all_three_modifiers_and_do_not_auto_repeat() {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
            MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN,
        };
        for m in [MOD_CONTROL, MOD_ALT, MOD_SHIFT, MOD_NOREPEAT] {
            assert_eq!(HOTKEY_MODIFIERS & m, m);
        }
        assert_eq!(HOTKEY_MODIFIERS & MOD_WIN, 0);
    }
}
