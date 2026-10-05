# ADR-0032: A lag watchdog that proposes, and never ends anything on its own

- Status: accepted
- Date: 2026-09-28

## Context

On 2026-09-28 the machine lagged until the mouse barely moved. The cause was a
Gradle daemon four shells deep inside a VS Code terminal, plus `tsc` and
`cargo` from other agents, all at Normal priority, holding 32 cores at 95 %.
The only way out the user found was killing VS Code, which took every
terminal and editor with it. Vitals itself measured 0.5 % of the machine.

Wanted: something that notices lag, names the process behind it, and offers
to end it from a notification. It must not blame the parent a job was
started from, and it must stay responsive while the machine is saturated.

## Decision

A separate binary, `apps/watchdog` (`vitals-watchdog.exe`). It has no UI,
uses about 3 MB, starts at logon from `HKCU\...\Run` (no administrator
rights), and runs whether or not Vitals or VS Code is open.

- **Detection.** One tick a second. A tick is bad if the machine is at or
  above 90 % CPU, or a Normal-priority probe thread wakes 40 ms or more late
  (scheduling delay is what a person feels). 5 bad ticks out of 8 makes an
  episode. Memory load at or above 95 % works the same way. A foreground
  window that `IsHungAppWindow` reports as hung for 5 s outranks both.
- **Blame.** Load is summed per subtree. The search walks _down_ to the
  smallest subtree that still carries 70 % of the load; it never walks up.
  Shells, terminals, IDEs and service hosts are transparent: each of their
  children is judged on its own. A parent link is only believed when the
  parent started first, so a recycled PID cannot adopt an old build.
- **Never offered.** A host that has children. `explorer`, `svchost`,
  `services`, `winlogon` and the rest of the session chain, even with no
  children. Anything `assess_termination` does not call `Safe`. Protected
  processes, checked live through `ProcessProtectionLevelInfo`. A process
  already at below-normal priority, for CPU episodes.
- **Actions.** "End it (and its N)", "Lower its priority" and "Ignore
  30 min". A click is honoured only for a key this process proposed, within
  15 minutes. Ending re-reads the tree and ends the children first, each
  through `terminate`, which re-checks the start time and the kernel's
  critical flags. Lowering applies below-normal to the whole tree, because a
  priority class is only inherited at `CreateProcess`.
- **Priority.** The main thread runs at `THREAD_PRIORITY_TIME_CRITICAL` in
  a Normal-class process. That is the top of the non-realtime range, so it
  keeps being scheduled under a Normal-class storm. The process is not made
  `REALTIME`: that class outranks the input stack and could cause the very
  freeze it exists to cure.
- **Toast.** `Scenario::Alarm` with the single default sound. Under Do Not
  Disturb, which was on during testing, a default or `Reminder` toast went
  straight to the notification centre.

## Alternatives rejected

- **Inside the desktop app.** Dies with the app, and costs about 200 MB of
  WebView for a background job.
- **Automatic kill after a timeout.** The user chose "never". A long build
  is often legitimate, and killing it silently loses the work.
- **Blaming the busiest process.** A `cargo` whose sixteen `rustc` workers
  each look small is invisible that way, and VS Code would top the list.

## Consequences

- One more binary, bundled by the installer as a resource in
  `$INSTDIR\watchdog\` (see the revision below).
- The pure logic (`forest`, `detect`, `action`, `strings`) is
  platform-free and tested on the Linux CI leg as well.

## Revision 2026-09-28 (v2): strict UI signals, sounds, installer

**Why.** v1 fired on ordinary builds. Its log showed seven proposals in ten
minutes (`tsc`, VS Code, `python`, `rg`), each with the machine at 93 % or
more busy and a probe lag of 0.0 ms. Measured at 97–100 % CPU on this
32-core machine, the foreground window still answered `WM_NULL` in
0.1–8 ms, and DWM counted no missed frames at 180 Hz. A busy processor is
not a frozen computer. The "User Input Delay" performance counters that
would measure input latency directly do not exist here (they need the
per-session counters policy), so they cannot be the signal.

**Detection now.** CPU load is no longer a trigger at all. A tick is bad
when either:

- the foreground window takes longer than the threshold to answer
  `SendMessageTimeout(WM_NULL, SMTO_ABORTIFHUNG)` (capped at 1 s): the UI
  the person is looking at is not responding to input; or
- a Normal-priority probe thread wakes up late by more than the scheduler
  threshold: the machine is not giving anyone the CPU in time.

Four bad ticks out of six make an episode. Paging counts only when memory
load is at or above 90 % _and_ hard faults exceed the threshold; either
alone is normal. A window `IsHungAppWindow` reports for five seconds still
outranks everything. Nothing is proposed while the user has not touched the
mouse or keyboard for 60 s: a freeze nobody is waiting on does not need a
notification, and a build left overnight is not a problem.

Sensitivity is chosen in Settings:

|                  | window | scheduler | hard faults/s |
| ---------------- | ------ | --------- | ------------- |
| Relaxed          | 500 ms | 100 ms    | 2000          |
| Normal (default) | 250 ms | 50 ms     | 1000          |
| Sensitive        | 120 ms | 30 ms     | 500           |

After a proposal: 3 minutes before any other, 15 before the same target
again, 30 after "Ignore".

**Sound.** Windows' notification sounds, no sound, or an audio file. A toast
from an unpackaged app can only play `ms-winsoundevent:` sounds, not a local
file, so for a file the toast is silent and the watchdog plays the file
itself through MCI (volume 0–100, at most 10 s). "Default" is `Reminder`:
the crate's `Default` under the alarm scenario loops until dismissed. The
Test button runs `vitals-watchdog --play-sound`, the same code a real
proposal uses, so a file the watchdog cannot open fails in Settings rather
than silently at 3 a.m.

**Settings.** `%APPDATA%\Vitals\watchdog.json`, written atomically by the app
(temporary file, then rename) and re-read by the watchdog within a second
when its modification time changes. The Settings switch writes the file and
the `HKCU\…\Run` entry in one command, and shows what Windows reports
afterwards.

**Installer.** `scripts/bundle-watchdog.ps1` builds it in
`beforeBuildCommand` and stages it as the `watchdog/` resource, so it is
installed at `$INSTDIR\watchdog\vitals-watchdog.exe` from the same commit
as the app. The first launch with no settings file turns it on (the user's
choice: on by default, with a switch). Every later launch restarts it if it
is enabled and not running, because the installer ends it before replacing
the file. The uninstaller ends it and removes the Run entry only when it
names this install's copy.

## Revision 2026-10-05 (v3): restart the shell, warn on commit charge

**Why.** Three hard resets in two days (03.10 21:55, 05.10 09:08 and
11:32). Applications kept running; the taskbar, Start and window frames did
not. The event logs agree: `dwm.exe` crashed three times with
`0xc00001ad` (no memory) in `udwm.dll`, each time beside a
Resource-Exhaustion 2004 "low virtual memory" event naming `vmmemWSL` at
84–109 GB of commit, on a machine whose commit limit was 192 GB of RAM plus a
16 GB page file. The watchdog had seen it — its log has
`trigger=Hung(...) name=explorer.exe` and `name=dwm.exe` — and offered only
"Ignore", because the rule above never offers the session chain.

**Restart, not end.** For a hung `explorer.exe` or `dwm.exe` the
notification now offers a restart. Explorer is ended by its exact key and
Windows starts a new shell (the watchdog starts one itself if none appears
within 5 s). DWM runs as another account, so its restart goes through one UAC
prompt; Windows restarts DWM on its own. Ending either is still never
offered: a restart is the one action that returns the user to a working
desktop.

**Hotkeys.** A frozen shell can mean a notification nobody can click, so
Ctrl+Alt+Shift+E restarts Explorer and Ctrl+Alt+Shift+D restarts DWM from a
`RegisterHotKey` thread of the watchdog's own. A hung taskbar
(`Shell_TrayWnd`) counts as hung even when it is not the foreground window.

**Commit warning.** Commit charge above 90 % of the limit for four seconds of
six raises a proposal before the crash rather than after it, naming the
largest holder of private bytes ("WSL / Docker" for `vmmemWSL`).
Priority: hung > commit > paging > stall — paging is a symptom of a full
commit, and at 100 % the desktop crashes rather than slowing down.

**Found while building it.** `run_as_admin` relaunches the current exe with
`--elevated-process-action`, which the watchdog's `main` rejected as an
unknown argument: every elevated action from a notification had failed after
the UAC prompt was accepted. It is now handled.
