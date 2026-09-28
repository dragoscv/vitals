//! The loop, the command line, and the logon entry.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use vitals_core::ids::ProcessKey;
use windows_sys::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError};
use windows_sys::Win32::Globalization::GetUserDefaultUILanguage;
use windows_sys::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole};
use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, RegDeleteKeyValueW};
use windows_sys::Win32::System::Threading::CreateMutexW;

use crate::action::Action;
use crate::detect::{Detector, Policy, Tick, Trigger};
use crate::forest::{Forest, Metric, Scope, Target};
use crate::notify::{self, Button};
use crate::strings::{self, EN, RO, Strings, fill};
use crate::win::{self, LagProbe, MachineCpu, Outcome, Processes};

/// Share of a subtree one child must carry for the blame to move into it.
const SHARE: f64 = 0.7;
/// A CPU target must account for at least this many cores…
const MIN_CORES: f64 = 1.0;
/// …and this share of the machine.
const MIN_CPU_SHARE: f64 = 0.10;
/// A memory target must hold at least this share of physical memory.
const MIN_MEMORY_SHARE: f64 = 0.10;
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_VALUE: &str = "VitalsWatchdog";

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn logical_cores() -> f64 {
    let n = std::thread::available_parallelism().map_or(1, usize::from);
    f64::from(u32::try_from(n).unwrap_or(1))
}

fn text() -> &'static Strings {
    // SAFETY: no preconditions.
    let lang = unsafe { GetUserDefaultUILanguage() };
    // PRIMARYLANGID: the low ten bits. 0x18 = LANG_ROMANIAN.
    if lang & 0x3ff == 0x18 { &RO } else { &EN }
}

fn data_dir() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map_or_else(std::env::temp_dir, PathBuf::from)
        .join("Vitals")
        .join("watchdog")
}

pub fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        None => watch(),
        Some("--diagnose") => with_console(diagnose),
        Some("--test-toast") => with_console(test_toast),
        Some("--install") => with_console(install),
        Some("--uninstall") => with_console(uninstall),
        Some(other) => with_console(|| bail!("unknown argument {other:?}")),
    }
}

/// Runs `f` with output reaching the terminal that started us.
///
/// A `windows_subsystem = "windows"` exe has no console, which is what keeps
/// the logon entry from flashing a window; the commands a person types
/// borrow the parent's instead.
fn with_console(f: impl FnOnce() -> anyhow::Result<()>) -> anyhow::Result<()> {
    // SAFETY: no preconditions; failure (no parent console, or output
    // already redirected to a pipe) is harmless.
    unsafe { AttachConsole(ATTACH_PARENT_PROCESS) };
    let result = f();
    if let Err(error) = &result {
        eprintln!("error: {error:#}");
    }
    result
}

fn init_log() {
    let dir = data_dir();
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("watchdog.log");
    // Events only — a proposal, a click, a result — so this grows by a few
    // lines a day; starting over past 1 MB is enough of a bound.
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > 1 << 20) {
        let _ = std::fs::remove_file(&path);
    }
    if let Ok(file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        tracing_subscriber::fmt()
            .with_ansi(false)
            .with_writer(std::sync::Mutex::new(file))
            .init();
    }
}

/// One instance per user session: a second would double every toast.
fn single_instance() -> bool {
    let name = wide(r"Local\app.vitals.watchdog");
    // SAFETY: valid wide string; the handle is deliberately leaked so the
    // mutex lives exactly as long as the process.
    let handle = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
    // SAFETY: no preconditions.
    !handle.is_null() && unsafe { GetLastError() } != ERROR_ALREADY_EXISTS
}

/// What a pending proposal was about, so a click acts on exactly that.
#[derive(Debug, Clone)]
struct Pending {
    name: String,
    scope: Scope,
    at: Instant,
}

/// How long a proposal's buttons stay valid.
///
/// A toast can sit in the notification centre for days; a click on one from
/// yesterday must not end whatever that process is doing today.
const PENDING_FOR: Duration = Duration::from_secs(15 * 60);

fn watch() -> anyhow::Result<()> {
    init_log();
    if !single_instance() {
        tracing::info!("already running; exiting");
        return Ok(());
    }
    if !notify::register() {
        tracing::warn!("could not register the notification identity");
    }
    let top = win::run_at_top_priority();
    tracing::info!(
        top_priority = top,
        version = env!("CARGO_PKG_VERSION"),
        "watchdog started"
    );

    let probe = LagProbe::start().context("starting the lag probe")?;
    let mut cpu = MachineCpu::default();
    let mut processes = Processes::new();
    let mut detector = Detector::default();
    let mut policy = Policy::default();
    let mut pending: HashMap<ProcessKey, Pending> = HashMap::new();
    let (tx, rx) = mpsc::channel::<Action>();
    let (done_tx, done_rx) = mpsc::channel::<String>();
    let cores = logical_cores();
    let strings = text();

    let mut next_tick = Instant::now();
    let mut last_scan: Option<Instant> = None;
    let mut warm = false;

    loop {
        handle_clicks(&rx, &mut pending, &mut policy, &done_tx, strings);
        while let Ok(message) = done_rx.try_recv() {
            notify::inform(&message);
        }

        let now = Instant::now();
        if now < next_tick {
            match rx.recv_timeout(next_tick - now) {
                Ok(action) => {
                    handle(action, &mut pending, &mut policy, &done_tx, strings);
                    continue;
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => bail!("action channel closed"),
            }
        }
        next_tick += Duration::from_secs(1);
        // After a sleep or a long UAC prompt, do not replay missed ticks.
        if next_tick < Instant::now() {
            next_tick = Instant::now() + Duration::from_secs(1);
        }

        let busy = cpu.sample().unwrap_or(0.0);
        let lag_ms = probe.take_ms();
        let (memory_load, total_memory) = win::memory().unwrap_or((0, 0));
        let hung_pid = win::hung_foreground();

        // Enumerating ~500 processes costs a few ms. Every second while
        // anything looks wrong, every five when calm — the rates only need
        // to be fresh at the moment a proposal is made.
        let troubled = busy >= 0.6 || lag_ms >= 20.0 || memory_load >= 90 || hung_pid.is_some();
        if troubled || last_scan.is_none_or(|at| at.elapsed() >= Duration::from_secs(5)) {
            match processes.refresh() {
                Ok(()) => {
                    warm = last_scan.is_some();
                    last_scan = Some(Instant::now());
                }
                Err(error) => tracing::warn!(%error, "process enumeration failed"),
            }
        }

        let tick = Tick {
            busy,
            lag_ms,
            memory_load,
            hung: hung_pid.and_then(|pid| processes.key_of(pid)),
        };
        let Some(trigger) = detector.push(tick) else {
            continue;
        };
        if warm {
            pending.retain(|_, p| p.at.elapsed() < PENDING_FOR);
            let context = Situation {
                processes: &processes,
                cores,
                total_memory,
                strings,
            };
            propose_once(&context, trigger, tick, &mut policy, &mut pending, &tx);
        }
    }
}

/// What a proposal is made from.
struct Situation<'a> {
    processes: &'a Processes,
    cores: f64,
    total_memory: u64,
    strings: &'static Strings,
}

/// Chooses a target for `trigger` and shows it, unless the policy says not to.
fn propose_once(
    context: &Situation<'_>,
    trigger: Trigger,
    tick: Tick,
    policy: &mut Policy,
    pending: &mut HashMap<ProcessKey, Pending>,
    tx: &Sender<Action>,
) {
    let forest = context.processes.forest();
    let Some(proposal) = choose(
        &forest,
        context.processes,
        trigger,
        context.cores,
        context.total_memory,
    ) else {
        return;
    };
    let Some(target) = forest.get(proposal.target.index) else {
        return;
    };
    if !policy.allow(target.key, &target.name, Instant::now()) {
        return;
    }

    tracing::info!(
        ?trigger,
        name = %target.name,
        pid = target.key.pid.get(),
        load = proposal.target.load,
        descendants = proposal.target.descendants,
        busy = tick.busy,
        lag_ms = tick.lag_ms,
        memory_load = tick.memory_load,
        "proposing"
    );
    show(
        &forest,
        &proposal,
        trigger,
        context.cores,
        context.strings,
        tx.clone(),
    );
    pending.insert(
        target.key,
        Pending {
            name: target.name.clone(),
            scope: proposal.target.scope,
            at: Instant::now(),
        },
    );
}

/// A chosen target and what may be offered for it.
#[derive(Debug)]
struct Proposal {
    target: Target,
    others: Vec<Target>,
    can_end: bool,
    can_lower: bool,
}

fn choose(
    forest: &Forest,
    processes: &Processes,
    trigger: Trigger,
    cores: f64,
    total_memory: u64,
) -> Option<Proposal> {
    let key_ok = |t: &Target| forest.get(t.index).map(|p| (p.key, p.name.clone()));

    if let Trigger::Hung(key) = trigger {
        let index = forest.find(key)?;
        let p = forest.get(index)?;
        let target = Target {
            index,
            load: p.cpu,
            scope: Scope::Alone,
            descendants: 0,
            can_end: true,
        };
        return Some(Proposal {
            can_end: win::endable(p.key.pid, &p.name),
            can_lower: false,
            target,
            others: Vec::new(),
        });
    }

    let (metric, floor) = match trigger {
        Trigger::Memory => (Metric::Memory, MIN_MEMORY_SHARE * total_memory as f64),
        _ => (Metric::Cpu, MIN_CORES.max(MIN_CPU_SHARE * cores)),
    };

    // A process already below normal priority is not what the user feels:
    // it yields to them. Proposing it again after "Lower its priority" would
    // be the nagging the policy exists to prevent. Memory is different — a
    // lowered process still holds its pages.
    let candidates: Vec<Target> = forest
        .targets(metric, SHARE)
        .into_iter()
        .filter(|t| t.load >= floor)
        .filter(|t| key_ok(t).is_some_and(|(key, name)| win::actionable(key.pid, &name)))
        .filter(|t| {
            metric == Metric::Memory || key_ok(t).is_some_and(|(key, _)| !processes.is_lowered(key))
        })
        .collect();

    let mut iter = candidates.into_iter();
    let target = iter.next()?;
    let (key, name) = key_ok(&target)?;
    Some(Proposal {
        can_end: target.can_end && win::endable(key.pid, &name),
        can_lower: metric == Metric::Cpu && !processes.is_lowered(key),
        others: iter.take(2).collect(),
        target,
    })
}

fn cores_text(load: f64) -> String {
    format!("{load:.1}")
}

fn chain(forest: &Forest, index: usize, separator: &str) -> String {
    let mut names: Vec<String> = Vec::new();
    for i in forest.ancestors(index) {
        let Some(p) = forest.get(i) else { continue };
        if names.last() != Some(&p.name) {
            names.push(p.name.clone());
        }
        if names.len() == 3 {
            break;
        }
    }
    names.join(separator)
}

fn show(
    forest: &Forest,
    proposal: &Proposal,
    trigger: Trigger,
    cores: f64,
    s: &Strings,
    tx: Sender<Action>,
) {
    let Some(p) = forest.get(proposal.target.index) else {
        return;
    };
    let count = proposal.target.descendants.to_string();
    let (title, first) = match trigger {
        Trigger::Hung(_) => (
            fill(s.hung_title, &[("name", &p.name)]),
            fill(
                s.hung_body,
                &[("secs", &crate::detect::HUNG_TICKS.to_string())],
            ),
        ),
        Trigger::Memory => (
            fill(s.busy_title, &[("name", &p.name)]),
            fill(
                s.busy_memory,
                &[("size", &strings::size(proposal.target.load as u64))],
            ),
        ),
        Trigger::Busy => (
            fill(s.busy_title, &[("name", &p.name)]),
            fill(
                s.busy_cpu,
                &[
                    ("cores", &cores_text(proposal.target.load)),
                    ("total", &format!("{cores:.0}")),
                ],
            ),
        ),
    };

    let mut second = String::new();
    if proposal.target.scope == Scope::Tree && proposal.target.descendants > 0 {
        second.push_str(&fill(s.with_children, &[("count", &count)]));
    }
    let from = chain(forest, proposal.target.index, " › ");
    if !from.is_empty() {
        if !second.is_empty() {
            second.push(' ');
        }
        second.push_str(&fill(s.started_from, &[("chain", &from)]));
    }
    let also: Vec<String> = proposal
        .others
        .iter()
        .filter_map(|t| forest.get(t.index).map(|o| o.name.clone()))
        .collect();
    if !also.is_empty() {
        let _ = write!(
            second,
            " {}",
            fill(s.also_busy, &[("list", &also.join(", "))])
        );
    }

    let mut buttons = Vec::new();
    if proposal.can_end {
        let label = if proposal.target.scope == Scope::Tree && proposal.target.descendants > 0 {
            fill(s.end_tree, &[("count", &count)])
        } else {
            s.end.to_owned()
        };
        buttons.push(Button {
            label,
            action: Action::End(p.key),
        });
    }
    if proposal.can_lower {
        buttons.push(Button {
            label: s.lower.to_owned(),
            action: Action::Lower(p.key),
        });
    }
    buttons.push(Button {
        label: s.ignore.to_owned(),
        action: Action::Ignore(p.key),
    });

    notify::propose(&title, [&first, second.trim()], &buttons, tx);
}

fn handle_clicks(
    rx: &Receiver<Action>,
    pending: &mut HashMap<ProcessKey, Pending>,
    policy: &mut Policy,
    done: &Sender<String>,
    s: &'static Strings,
) {
    while let Ok(action) = rx.try_recv() {
        handle(action, pending, policy, done, s);
    }
}

/// Acts on a click — only for a key this process proposed, and only once.
///
/// End and Lower run on their own thread: ending another account's process
/// waits on a UAC prompt, and the watchdog must keep watching meanwhile.
fn handle(
    action: Action,
    pending: &mut HashMap<ProcessKey, Pending>,
    policy: &mut Policy,
    done: &Sender<String>,
    s: &'static Strings,
) {
    let key = match action {
        Action::End(k) | Action::Lower(k) | Action::Ignore(k) => k,
    };
    let Some(p) = pending.remove(&key) else {
        tracing::warn!(?action, "click for a proposal that is not pending; ignored");
        return;
    };
    tracing::info!(?action, name = %p.name, "clicked");

    if let Action::Ignore(_) = action {
        policy.snooze(&p.name, Instant::now());
        let _ = done.send(fill(s.ignored, &[("name", &p.name)]));
        return;
    }

    let done = done.clone();
    let spawned = std::thread::Builder::new()
        .name("act".into())
        .spawn(move || {
            let mut processes = Processes::new();
            let outcome = match action {
                Action::End(k) => win::end(&mut processes, k, p.scope == Scope::Tree),
                Action::Lower(k) => win::lower(&mut processes, k),
                Action::Ignore(_) => return,
            };
            tracing::info!(?outcome, name = %p.name, "result");
            let message = match (action, outcome) {
                (_, Outcome::Gone) => fill(s.already_gone, &[("name", &p.name)]),
                (_, Outcome::Failed(reason)) => {
                    fill(s.failed, &[("name", &p.name), ("reason", &reason)])
                }
                (Action::End(_), Outcome::Done { others }) if others > 0 => fill(
                    s.ended_tree,
                    &[("name", &p.name), ("count", &others.to_string())],
                ),
                (Action::End(_), Outcome::Done { .. }) => fill(s.ended, &[("name", &p.name)]),
                (_, Outcome::Done { .. }) => fill(s.lowered, &[("name", &p.name)]),
            };
            let _ = done.send(message);
        });
    if let Err(error) = spawned {
        tracing::warn!(%error, "could not start the action thread");
    }
}

/// Samples for ten seconds and prints what the loop would decide.
fn diagnose() -> anyhow::Result<()> {
    win::run_at_top_priority();
    let probe = LagProbe::start()?;
    let mut cpu = MachineCpu::default();
    let mut processes = Processes::new();
    let mut detector = Detector::default();
    let cores = logical_cores();
    let mut trigger = None;

    cpu.sample();
    processes.refresh()?;
    println!("  s  cpu%  lag ms  mem%  hung  trigger");
    for second in 1..=10 {
        std::thread::sleep(Duration::from_secs(1));
        let started = Instant::now();
        let busy = cpu.sample().unwrap_or(0.0);
        let lag_ms = probe.take_ms();
        let (memory_load, _) = win::memory().unwrap_or((0, 0));
        let hung = win::hung_foreground();
        processes.refresh()?;
        let t = detector.push(Tick {
            busy,
            lag_ms,
            memory_load,
            hung: hung.and_then(|pid| processes.key_of(pid)),
        });
        trigger = trigger.or(t);
        println!(
            " {second:2}  {:4.0}  {lag_ms:6.1}  {memory_load:4}  {:4}  {:?}   (tick cost {:.1} ms)",
            busy * 100.0,
            hung.map_or_else(|| "-".to_owned(), |p| p.get().to_string()),
            t,
            started.elapsed().as_secs_f64() * 1000.0
        );
    }

    let (_, total_memory) = win::memory().unwrap_or((0, 0));
    let forest = processes.forest();
    println!("\nheaviest CPU subtrees (cores; blame never climbs into a shell or IDE):");
    for t in forest.targets(Metric::Cpu, SHARE).into_iter().take(8) {
        let Some(p) = forest.get(t.index) else {
            continue;
        };
        println!(
            "  {:5.2}  {:<28} pid {:<6} {:?}{}  end:{} lowered:{} actionable:{}  from {}",
            t.load,
            p.name,
            p.key.pid.get(),
            t.scope,
            if t.descendants > 0 {
                format!(" +{}", t.descendants)
            } else {
                String::new()
            },
            t.can_end && win::endable(p.key.pid, &p.name),
            processes.is_lowered(p.key),
            win::actionable(p.key.pid, &p.name),
            chain(&forest, t.index, " > "),
        );
    }
    let verdict = trigger.map_or_else(
        || "no sustained lag - nothing would be proposed".to_owned(),
        |t| match choose(&forest, &processes, t, cores, total_memory) {
            Some(p) => forest.get(p.target.index).map_or_else(String::new, |x| {
                format!(
                    "{t:?}: would propose {} (pid {}), end:{} lower:{}",
                    x.name,
                    x.key.pid.get(),
                    p.can_end,
                    p.can_lower
                )
            }),
            None => format!("{t:?}, but no single target is heavy enough to blame"),
        },
    );
    println!("\nverdict: {verdict}");
    Ok(())
}

/// Shows a real proposal for the current top CPU target, but a click only
/// prints what it would have done. For checking the toast end to end.
fn test_toast() -> anyhow::Result<()> {
    if !notify::register() {
        bail!("could not register the notification identity");
    }
    let mut processes = Processes::new();
    processes.refresh()?;
    std::thread::sleep(Duration::from_secs(2));
    processes.refresh()?;
    let forest = processes.forest();
    let cores = logical_cores();
    let target = forest
        .targets(Metric::Cpu, SHARE)
        .into_iter()
        .next()
        .context("no process is using any CPU")?;
    let proposal = Proposal {
        can_end: true,
        can_lower: true,
        target,
        others: Vec::new(),
    };
    let (tx, rx) = mpsc::channel();
    show(&forest, &proposal, Trigger::Busy, cores, text(), tx);
    println!("toast shown; click a button within 60 s (nothing will be changed)");
    match rx.recv_timeout(Duration::from_secs(60)) {
        Ok(action) => println!("clicked: {action:?} (test mode, nothing was changed)"),
        Err(_) => println!("no click within 60 s"),
    }
    Ok(())
}

/// Copies the exe to `%LOCALAPPDATA%\Vitals\watchdog` and starts it at logon.
///
/// A copy, so `cargo clean` or a rebuild cannot delete the program Windows
/// runs at logon; per user, so no administrator prompt.
fn install() -> anyhow::Result<()> {
    let source = std::env::current_exe()?;
    let dir = data_dir();
    std::fs::create_dir_all(&dir)?;
    let target = dir.join("vitals-watchdog.exe");
    stop_others()?;
    if source != target {
        std::fs::copy(&source, &target)
            .with_context(|| format!("copying to {}", target.display()))?;
    }
    let command = format!("\"{}\"", target.display());
    if !notify::set_user_string(RUN_KEY, RUN_VALUE, &command) {
        bail!("could not write HKCU\\{RUN_KEY}\\{RUN_VALUE}");
    }
    std::process::Command::new(&target)
        .spawn()
        .with_context(|| format!("starting {}", target.display()))?;
    println!(
        "installed: {}\nstarts at every logon; running now",
        target.display()
    );
    Ok(())
}

fn uninstall() -> anyhow::Result<()> {
    let key = wide(RUN_KEY);
    let value = wide(RUN_VALUE);
    // SAFETY: valid wide strings.
    let status = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), value.as_ptr()) };
    let stopped = stop_others()?;
    println!(
        "logon entry {}; {stopped} running watchdog(s) stopped",
        if status == 0 {
            "removed"
        } else {
            "was not present"
        }
    );
    Ok(())
}

/// Ends every other running watchdog, so the installed copy can be replaced.
fn stop_others() -> anyhow::Result<usize> {
    let mut processes = Processes::new();
    processes.refresh()?;
    let me = std::process::id();
    let mut stopped = 0;
    for p in &processes.raw {
        let ours = p
            .name
            .as_deref()
            .is_some_and(|n| n.eq_ignore_ascii_case("vitals-watchdog.exe"));
        if ours
            && p.key.pid.get() != me
            && vitals_win::actions::terminate(p.key, 0, vitals_win::actions::Consent::Unconfirmed)
                .is_ok()
        {
            stopped += 1;
        }
    }
    if stopped > 0 {
        // Let the image file be released before it is overwritten.
        std::thread::sleep(Duration::from_millis(500));
    }
    Ok(stopped)
}
