# Changelog

All notable changes to Vitals are recorded here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

Six screens work against live data. There is still no release to install —
the installer builds and runs, but several sections remain placeholders and
nothing has been through a public beta.

### Added

#### Foundations

- **Domain model** (`vitals-core`) — an OS-agnostic model of processes,
  metrics and sensors, plus the provider traits every platform backend
  implements. Units are newtypes (`Bytes`, `Percent`, `Hertz`) so a
  megabyte can never be passed where bytes are expected.
- **Race-free process identity** — `ProcessKey` pairs a PID with the process
  start time. Every mutating operation requires one, so a stale reference
  cannot terminate a process that merely inherited a recycled PID.
- **Capability model** — backends declare what they can do and why they
  cannot, so the UI disables an affordance with a reason instead of throwing
  when it is clicked.
- **Frame transport** (`vitals-ipc`) — an overwriting ring buffer that drops
  stale frames rather than stalling the sampler, and an enumerable command
  protocol for the elevated helper.
- **Canvas chart renderer** (`@vitals/charts`) — a fixed-allocation
  `RingBuffer` and a canvas time-series renderer with axis snapping, gap
  handling and HiDPI support.
- **Theme system** — light/dark, ten accents and three surface modes as three
  independent axes, built on OKLCH so every accent stays legible in both
  modes.
- **Internationalisation** — English and Romanian, with tests enforcing key
  parity, placeholder parity and Romanian's three-form plural rule.
- **Generated type bindings** — TypeScript types are generated from the Rust
  model by `ts-rs`; CI fails if the committed output has drifted.
- **Windows capability policy** (`vitals-win`) — the mapping from privilege
  and installed components to available features, unit-tested without needing
  a Windows machine in the loop.

#### Measurement

- **Windows sampler** — CPU, memory, processes, disks and network read
  through the NT native API (`NtQuerySystemInformation`) and `GetIfTable2`
  rather than WMI, which is far too slow for a per-second tick. A full
  sample of a 32-core machine with 587 processes and 30 adapters takes a
  median **13 ms**, or 0.13% of one core at 1 Hz.
- **Verified against Windows itself** — CPU matches
  `\Processor(_Total)\% Processor Time` to within 0.9 points; memory totals
  match CIM exactly; the top ten processes by private bytes match
  `Get-Process`; the service count matches at 374 = 374.
- **Delta-encoded frames** — a keyframe every 30 frames, deltas in between.
  240 KB becomes 41 KB, an 83% reduction. Exits are applied before changes so
  a recycled PID cannot inherit the previous owner's metrics.
- **GPU enumeration** via `D3DKMTEnumAdapters2`. Engine utilisation is
  reported as _unavailable_ rather than zero until `D3DKMTQueryStatistics` is
  wired against a known-good reference — an unmeasured engine and an idle one
  are different facts.
- **Storage analysis** — `FindFirstFileEx` for sizes, `FSCTL_ENUM_USN_DATA`
  for fast namespace discovery. The MFT path accelerates _discovery_, not
  measurement: a USN record carries no size, and inferring one would be
  fabrication. Totals are allocated size, cluster-rounded, with hard links
  deduplicated and unreadable directories listed rather than counted as zero.
- **Sensors** — thermals, power and battery, on their own slower cadence with
  a TTL cache. A WMI round trip costs tens of milliseconds and would blow the
  sampler's 30 ms budget on its own.

#### Screens

- **Dashboard** with configurable widgets. Layout is an ordered list plus a
  size hint rather than grid coordinates, so a layout saved on an ultrawide is
  still correct on a laptop and no stored state can describe an impossible
  arrangement.
- **Diagnostic alerts** that explain _why_, not just _what_. Memory pressure
  requires a high page-fault rate **and** low free memory — 90% RAM full of
  file cache is a healthy machine, and reporting it as a problem is the
  mistake every other monitor makes. Every threshold is sustained over 15
  samples, because CPU hits 100% during any application launch.
- **Performance** — a rail of resources with a detail panel each, including a
  **Thermals** tab that enumerates what it _cannot_ measure and why. Most
  consumer sensors need a kernel-mode driver Vitals does not install; showing
  zero would be a lie the user believes, and showing nothing makes the tool
  look broken.
- **Processes** with row ordering that holds still. Exponential smoothing, a
  deadband comparator over a stable insertion sort, and freezing while the
  pointer is over the table. Sampling noise makes dozens of adjacent pairs
  swap every tick; in a tool whose main verb is "end task", a row moving out
  from under the cursor is how you kill the wrong process.
- **Network connections** grouped by application, not by socket. Distinct
  remote hosts are counted rather than sockets, so twenty connections to one
  CDN edge read as one host instead of looking like a botnet.
- **Startup and Services**, which state their undercount out loud. Unelevated,
  some task definitions are ACL'd to SYSTEM and many service configurations
  cannot be queried; those are reported as unknown rather than assumed
  enabled or manual, and the screen says the totals are a floor.
- **Installed apps**, with an uninstall that launches the vendor's own
  uninstaller and deletes nothing itself. Sizes are labelled as declared by
  the installer, because Windows never recomputes them.

#### Actions

- **Process actions with a real safety model** — four risk levels, each with
  its own consequence text. Ending `explorer.exe` warns that the desktop goes
  away; `csrss.exe` warns that Windows bugchecks; protected processes are
  refused outright rather than offering an elevation prompt that cannot help.
- **Race-free termination** — identity is verified against the start time
  _and_ a non-zero exit code before acting. A dead-but-unreaped process opens
  successfully and then fails with `ACCESS_DENIED`, which would otherwise be
  reported to the user as "needs elevation".

#### Distribution

- **Silent installer** — no wizard, no agreement page, just a splash. NSIS
  hooks terminate a running instance before install and uninstall, because
  NSIS otherwise overwrites an open binary and the update silently no-ops.
  2.77 MB; installs in 5.2 s with no windows. Uninstalling keeps user data.
- **Nightly and tagged releases** with SLSA build provenance and SHA256SUMS,
  using only free infrastructure.

#### Application shell

- **Shell, settings and theming** — title bar, sidebar, a validated settings
  schema that degrades a single bad value to its default rather than failing
  to start, and persistence debounced so dragging a slider is not one fsync
  per frame.
- **Error boundary** around route content. A crash in one screen leaves the
  sidebar and Settings usable, shows the error message rather than burying
  it, and offers a manual retry — automatic retry would be a hot loop pinning
  a core inside a performance monitor.

### Fixed

- **The app could hang on its splash screen forever.** `registerShellStrings`
  ran at module scope, but i18next does not define `addResourceBundle` until
  `init()` has run — so the throw aborted the module graph before React
  mounted. Registration moved after the await, a descriptive guard added, and
  a bootstrap `catch` now replaces the splash with a readable error instead of
  pulsing indefinitely.
- **Loading states that could never resolve.** The dashboard sat on skeletons
  and Processes on "0 of 0 processes" whenever no frame arrived. Every source
  now resolves either way: immediately when there is no host, otherwise after
  a timeout. An app that looks busy is not reported as broken.
- **Placeholder strings would have shadowed the real translations.**
  `addResourceBundle(..., deep: false, overwrite: false)` shallow-merges, so
  the incoming bundle wins and `overwrite: false` protects nothing.

### Notes on dependencies

- `ts-rs` is used rather than `specta`: `specta-typescript` only supports a
  `2.0.0-rc` pre-release of specta, and the last stable specta line ships no
  TypeScript exporter at all.
- TypeScript 7 and TypeScript 6 are installed side by side, per Microsoft's
  [documented arrangement](https://devblogs.microsoft.com/typescript/announcing-typescript-7-0/#running-side-by-side-with-typescript-6.0).
  `tsc` is 7 and type-checks the codebase; `typescript-eslint` needs the 6.0
  API because TypeScript 7 does not yet expose a stable one.
