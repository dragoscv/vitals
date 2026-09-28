//! The Windows side: measuring, acting, notifying.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use vitals_core::error::Error;
use vitals_core::ids::{Pid, ProcessKey};
use vitals_win::actions::{
    Consent, ElevatedAction, Priority, ProcessFacts, Risk, assess_termination, run_as_admin,
    set_priority, terminate,
};
use vitals_win::process::{ProcessEnumerator, RawProcess};
use windows_sys::Win32::Foundation::{CloseHandle, FILETIME};
use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcessId, GetCurrentThread, GetProcessInformation, GetSystemTimes, OpenProcess,
    PROCESS_PROTECTION_LEVEL_INFORMATION, PROCESS_QUERY_LIMITED_INFORMATION, PROTECTION_LEVEL_NONE,
    ProcessProtectionLevelInfo, SetThreadPriority, THREAD_PRIORITY_NORMAL,
    THREAD_PRIORITY_TIME_CRITICAL,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowThreadProcessId, IsHungAppWindow,
};

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

fn filetime(ft: FILETIME) -> u64 {
    (u64::from(ft.dwHighDateTime) << 32) | u64::from(ft.dwLowDateTime)
}

/// Machine-wide CPU from `GetSystemTimes`.
#[derive(Debug, Default)]
pub struct MachineCpu {
    last: Option<(u64, u64)>,
}

impl MachineCpu {
    /// Busy share since the previous call, 0..=1. `None` on the first call.
    pub fn sample(&mut self) -> Option<f64> {
        let zero = FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        };
        let (mut idle, mut kernel, mut user) = (zero, zero, zero);
        // SAFETY: three live out-pointers.
        let ok = unsafe { GetSystemTimes(&raw mut idle, &raw mut kernel, &raw mut user) };
        if ok == 0 {
            return None;
        }
        // Kernel time includes idle time.
        let total = filetime(kernel) + filetime(user);
        let idle = filetime(idle);
        let previous = self.last.replace((total, idle));
        let (t0, i0) = previous?;
        let dt = total.checked_sub(t0)?;
        let di = idle.checked_sub(i0)?;
        (dt > 0).then(|| 1.0 - (di as f64 / dt as f64).clamp(0.0, 1.0))
    }
}

/// Physical memory: (load percent, total bytes).
pub fn memory() -> Option<(u32, u64)> {
    let mut status = MEMORYSTATUSEX {
        dwLength: u32::try_from(size_of::<MEMORYSTATUSEX>()).unwrap_or(0),
        ..empty_status()
    };
    // SAFETY: `dwLength` is set; the struct is live.
    (unsafe { GlobalMemoryStatusEx(&raw mut status) } != 0)
        .then_some((status.dwMemoryLoad, status.ullTotalPhys))
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
    if hwnd.is_null() {
        return None;
    }
    // SAFETY: `hwnd` is a window handle Windows just returned.
    if unsafe { IsHungAppWindow(hwnd) } == 0 {
        return None;
    }
    let mut pid = 0_u32;
    // SAFETY: `pid` is a live out-pointer.
    unsafe { GetWindowThreadProcessId(hwnd, &raw mut pid) };
    (pid != 0).then_some(Pid(pid))
}

/// Per-process CPU, smoothed, from successive enumerations.
#[derive(Debug)]
pub struct Processes {
    enumerator: ProcessEnumerator,
    last: HashMap<ProcessKey, u64>,
    smooth: HashMap<ProcessKey, f64>,
    at: Option<Instant>,
    pub raw: Vec<RawProcess>,
}

impl Processes {
    pub fn new() -> Self {
        Self {
            enumerator: ProcessEnumerator::new(),
            last: HashMap::new(),
            smooth: HashMap::new(),
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
            last.insert(p.key, time);
        }
        self.last = last;
        self.smooth = smooth;
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
