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

- One more binary; it is not yet bundled in the NSIS installer. Install it
  from a build with `vitals-watchdog --install`.
- The pure logic (`forest`, `detect`, `action`, `strings`) is
  platform-free and tested on the Linux CI leg as well.
