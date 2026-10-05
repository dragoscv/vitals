# Changelog

All notable changes to Vitals are recorded here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.9.0] — 2026-10-06

Out of beta, and the first version for the Microsoft Store.

### Added

- **A new look** — the "All clear" mark (a calm line that dips and rises past
  where it started), a fern-green theme, and the mark in motion: it assembles
  on start-up, slides into the title bar, and traces itself while a scan or a
  benchmark is running. Every icon — Windows, Store tiles, Android, watch,
  TV, website — is drawn from one source.
- **Microsoft Store version** — the same app, signed by Microsoft, so Windows
  never shows a "unknown publisher" warning. The Store updates it, so its
  own updater, the Task Manager replacement and the sensors service are off
  there; start with Windows works through the Store's own mechanism.
- **Right-click menus everywhere** — every list, the dashboard widgets and
  the title bar (minimise, maximise, hide to tray, close, quit). Menus open
  only on a right click or the menu key, never on a left click.
- **App names in the process list** — "Google Chrome", not `chrome.exe`;
  the file name is first in the right-click menu. Search finds either.
- **A hideable details panel** on the Processes screen.
- **The watchdog can rescue a frozen desktop** — restart Explorer or the
  window manager from its prompt or with Ctrl+Alt+Shift+E / D, and a warning
  before the computer runs out of memory to commit.

### Changed

- The default accent colour is fern green; the other nine remain in Settings.
- Scrollbars appear only when you point at or move through a list.
- Stray borders and text glyphs are gone in favour of cards and icons.

### Fixed

- Pointing at a process no longer pushes the table down one row.
- The watchdog's `--install` no longer hangs the shell that started it.

## [0.9.0-beta.2] — 2026-09-30

### Security

- **The local API is read-only** — `127.0.0.1:7330` no longer lets a
  program end or pause your processes without a token. Another Windows
  account on the same PC could reach it; the `vitals` CLI now acts on your
  own machine through Windows directly, with your own rights.
- **The CLI pipe is locked to your account** — an explicit access list
  (you, SYSTEM, administrators) replaces the Windows default, which also let
  everyone read it.
- **The phone page is harder to attack** — every response from the LAN
  server carries a content security policy and refuses to be framed; request
  bodies and WebSocket messages are capped.
- **Releases wait for the owner** — publishing needs approval, release tags
  can only be created by the owner, and every build action is pinned to an
  exact version.

### Added

- **Vitals for Android and Wear OS** — native phone and watch apps. The phone
  opens on This phone: processor, graphics, memory, battery, temperatures,
  storage with a folder map and cleanup, per-app time and data, sensors, a
  week of history and alerts, all read through Android's public interfaces.
  Pair a PC with a QR code to see it, its processes and sensors, wake it and,
  with a control token, end or suspend processes. The watch monitors itself
  and your PCs, with a tile and complications. Signed APKs ship with every
  release, so Obtainium can follow them.
- **Developer cleanup** — on the Storage screen, scan your project folders
  and choose what to remove: build output (`node_modules`, `target`, `.next`,
  `dist` and the like, only where the project that makes them lives), git
  worktrees that are clean, pushed and unused, package caches through each
  tool's own command, Docker's unused images and build cache (never its
  volumes), and the WSL and Docker virtual disks, compacted so the space goes
  back to Windows. Projects untouched for 30 days are ticked for you; nothing
  is removed until you confirm, and each item says how to bring it back.
- **Free Windows' own space with Windows' own tools** — in Reclaimable space,
  the Windows Update cache, system temporary files, the thumbnail cache, crash
  dumps, the component store, the previous Windows installation, the Recycle
  Bin and the hibernation file now have a button that runs the tool Windows
  provides for each (Disk Cleanup, DISM or powercfg). Windows asks for
  administrator approval once, for that action only; you see what it is doing
  while it works, and afterwards how much space was actually freed, measured
  before and after. Emptying the bin, removing the previous Windows and
  turning hibernation off say what cannot be undone and need a tick first.

- **Free space from a storage scan** — put folders and files in review with
  the + beside them (or right-click on the map), check the list and its
  total, and send them to the Recycle Bin in one step, from where any of them
  can be put back. Windows, installed programs and the folders your account
  is made of are refused, and anything the bin could not hold is left where
  it is rather than deleted for good. The sizes go down straight away,
  without scanning again.
- **Why can't I delete this** — when something is in use, Vitals names the
  program holding it, so you know what to close.

- **Explore a storage scan** — after a scan, a map shows every folder sized by
  the space it takes, as layers or as blocks. Click a block to go inside, use
  the breadcrumb or Up to come back, and hover for its size and share. A list
  beside it can be driven with the keyboard, and every row can be opened,
  shown in File Explorer or have its path copied.
- **Largest files** — the thousand biggest files on the scanned drive, each one
  click away from File Explorer.

- **Act on startup items and services** — right-click a row, or use the
  button at its end. Startup items switch off and on the way Task Manager
  does it; services start, stop, restart, and can be set to start
  automatically, manually or never. Both open the file location, its
  properties, a web search, or copy their details.
- **Hide Microsoft services** — ticked by default on Startup and Services,
  so the lists show what other software added; the hidden count is shown
  and one click brings them back.
- **Safe by default** — anything Windows depends on asks before it is
  switched off, what Windows forbids is not offered, and a change that
  needs administrator rights asks once, for that change only.
- **CPU temperature and package power** — install the optional sensors
  service from Devices & sensors (one administrator prompt) and Vitals shows
  the CPU package temperature, the hottest core and the package power draw
  on Devices, the Thermals chart, Prometheus (`vitals_cpu_power_watts`) and
  Home Assistant. It uses the signed PawnIO driver, only reads, and can be
  removed from the same screen.
- **Hardware** — Devices & sensors shows what the computer is made of:
  processor, every memory module, graphics cards, drives (with temperature
  and health) and the motherboard and BIOS.
- **Device Manager** — every device Windows lists, grouped the same way,
  with its driver and any problem it reports. You can search, and show
  devices that are not connected.
- **Fan speeds** — with the sensors service installed, the motherboard's fan
  speeds appear on Devices, the Thermals chart, Prometheus
  (`vitals_fan_rpm`) and the LAN API.
- **Drive temperatures** without administrator rights.
- **Lag watchdog** — `vitals-watchdog` notices when a window you are using
  stops answering, or programs cannot get the processor in time, names the
  process behind it (never the shell or IDE it was started from), and offers
  End, Lower its priority or Ignore from a notification. It never ends
  anything on its own, and `--diagnose` shows what it would decide.
- **The watchdog ships with Vitals** — it is part of the installer and turns
  itself on at the first launch. Settings → Watchdog turns it off, chooses
  the sensitivity (Relaxed, Normal or Sensitive) and the warning sound: any
  of Windows' notification sounds, none, or your own audio file with a
  volume, and a Test button.

### Changed

- **Privacy policy** covers the Android and Wear OS apps: the two kinds of
  special access and what they stay on, the phone-to-watch link, and the
  diagnostics Google ML Kit sends when the QR scanner opens.
- **Storage scans are about 80 times faster and count everything** — a whole
  drive of eight million files is read in about seven minutes instead of
  never finishing, and `Program Files` in six seconds instead of eight
  minutes. There is no depth choice any more: every folder is counted, at
  every depth, and the scan shows live progress and the folder it is reading.

- CI takes about 8 minutes instead of 49: the Windows checks run as three
  parallel jobs with their own caches, and a push that only changes the
  website or prose skips the Rust jobs.
- Scoop is available (`scoop bucket add vitals https://github.com/dragoscv/scoop-vitals`);
  winget and Chocolatey are submitted and awaiting their moderators.

### Fixed

- **A debug build takes about 10 GB instead of growing without limit** — the
  desktop crate no longer builds two extra copies of itself for mobile targets
  Vitals does not ship, and `verify.ps1` prunes abandoned incremental caches
  and fails when `target/` goes over budget (contributors only).
- **OneDrive folders are counted** in storage scans; they used to be skipped.
- **Stopping a scan no longer stops a cleanup search** (or the other way
  round) — each has its own Stop button.
- **Links are no longer reported as unreadable folders** — a scan that only
  passed over shortcuts to other places is complete, and says how many it did
  not follow.
- **A shortcut to a file no longer counts as the file's size.**
- **Scanning a drive scans the drive** — choosing C: could scan whichever
  folder Vitals was started from instead.

- The board's thermal zones are labelled as such, so a 28 °C chipset
  reading is no longer mistaken for the CPU temperature.
- The watchdog no longer warns during builds or while you are away from the
  computer: a busy processor on its own is not a freeze.
- The macOS build compiles again: the desktop overlay is transparent on
  Windows and Linux only, because on macOS a transparent window needs Tauri's
  private-API feature. macOS is built on every release but not yet published.

## [0.9.0-beta.1] — 2026-09-28

The first public release. A beta: every screen works against live data on
Windows, and this is the first build to go through the release pipeline end
to end.

### 2026-09-28 — real temperatures on Devices & sensors

#### Features

- **Temperatures without administrator rights** — the machine's thermal
  zones are read through Windows' performance counters when the usual query
  is refused, and NVIDIA graphics cards report temperature, fan speed and
  power through their own driver.

#### Changed

- **Devices & sensors** — Power and Thermal zones are separate cards that
  never scroll; the readings table and the list of what cannot be measured
  scroll on their own.

### Added

- **Updates install themselves.** About twenty seconds after launch Vitals
  checks GitHub for a newer version, downloads it in the background, checks
  its signature, and installs it when you quit — never while you are using
  it. Settings → About has the switch to turn it off, and shows a version
  that is downloaded and waiting.
- **A log file and a crash file.** Vitals now writes `vitals.log` (capped, so
  it cannot fill a disk) and, if it ever crashes, `crash.txt` in
  `%LOCALAPPDATA%\Vitals\logs`. Nothing is sent anywhere; attach them to a
  bug report if you want to.
- **Legal and community documents:** privacy policy, terms of use, third-party
  licence notices, contributors, support, governance, and a Contributor
  Covenant 3.0 code of conduct. The Privacy and About panels link to them.
- **A website** at [vitals.dragoscatalin.ro](https://vitals.dragoscatalin.ro),
  in English and Romanian, with the download, guides and reference.
- **The installer speaks Romanian** as well as English.

### Changed

- **The Privacy panel tells the whole truth.** It used to say Vitals "sends
  nothing anywhere"; it now names the update check and the local-only
  connection on `127.0.0.1` that the `vitals` command-line tool uses.
- **Running benchmarks no longer freezes the window.** The run moved off the
  main thread.

### Security

- **Pairing tokens are stored as hashes.** `lan-tokens.json` used to hold
  every paired phone's secret in plain text; it now holds a SHA-256 hash and
  the eight-character prefix you see in Settings. An existing file is
  converted the first time this version starts, and existing pairings keep
  working.
- **The flight recorder no longer writes who is signed in or your network
  adapters' hardware addresses.** It still records process names and
  resource use for bug reports.

### 2026-09-28 — "show hidden devices" at the top of the list

#### Changed

- **The "Show hidden devices" switch is at the top of the device list** on
  the Performance page, so it is in reach however long the list gets. Its
  explanation opens from the (i) beside it.

### 2026-09-28 — the network chart shows the adapter you chose

#### Fixed

- **The network chart shows the adapter you chose** — on the Performance
  page every adapter drew the same total for the whole machine, so picking
  another one seemed to do nothing. Each adapter now has its own chart.
- **The scrollbar no longer covers the device list** on the Performance
  page.

### 2026-09-28 — hide the devices you do not need

#### Added

- **Hide devices on the Performance page** — right-click any disk, GPU or
  network adapter and choose Hide; "Show hidden devices" brings them all
  back, dimmed, and the same menu shows one again. The choice is
  remembered across restarts.

#### Fixed

- **Virtual network adapters are recognised as virtual** — Hyper-V
  switches, WAN miniports and Wi-Fi Direct reported themselves as Ethernet
  or Wi-Fi, so they could not be filtered out. They now start hidden, with
  unused tunnels and displays that report nothing.

### 2026-09-28 — no page scroll on any screen

#### Changed

- **Every screen fits the window** — titles, toolbars and selections stay
  put, and only the part that needs it scrolls: a table, a list, a card's
  contents. In a very short window a screen's body scrolls as one, so
  nothing gets squeezed unreadably small.

### 2026-09-28 — a dashboard that fits the window

#### Changed

- **The dashboard fits the window** — cards share the window's height and
  width instead of growing with their content, so the page never scrolls;
  a card that is short on room scrolls inside itself. Each card leads with
  its key number in the header, and charts grow into the space left.

### 2026-09-28 — every tab is ready before you open it

#### Changed

- **Every tab is ready before its first visit** — after the window paints,
  each section's code and data are loaded in the background while the app
  is idle, so opening a tab for the first time shows its content at once
  instead of a loading skeleton. Disk scans and benchmarks still only run
  when you start them.

#### Fixes

- Processes opened for the first time could list only the processes that
  had changed recently (116 of 780) for up to thirty seconds.
- Reading startup items, installed apps, sensors and connections no longer
  freezes the window while it runs.

### 2026-09-28 — one window background

#### Changed

- **One window background** — the title bar, sidebar and content share one
  canvas with no borders between them; the window no longer reads as three
  panels.
- **Removed the Surface setting** (solid / mica / acrylic). It only tinted
  the title bar and sidebar, which now have no fill of their own; a saved
  value is ignored.

#### Fixes

- The active sidebar item's label now meets WCAG AA contrast for every
  accent (it measured 4.12:1 in light mode).

### 2026-09-27 — redesign and the truths it exposed

#### Features

- **Fluent depth** — cards are lit from above with a gradient surface, an
  edge highlight and a layered shadow; the accent is used as light on the
  active nav item, the primary button and the selected rail entry; two
  static pools of the accent hue sit behind the content. Tokens in
  `theme.css`, both modes (ADR 0030).
- **Navigation morphs** — section changes run as a View Transition: the old
  screen blurs out, the new one rises, the page title morphs between its
  boxes, and the chrome stays put. A single sidebar indicator springs
  between items. Skipped entirely under reduced motion.
- **Numbers roll** — every meter's reading tweens to its new value inside
  the formatter's own string; dashboard cards stagger in; skeletons
  shimmer; charts fade their fill and mark the live edge with a glowing dot.
- **GPU memory** — dedicated VRAM per adapter from DXGI, shown where a
  dash used to be.

#### Fixes

- Disk read/write throughput, active time and response time were `0` on
  every volume: the rate arithmetic had no caller. Counters are now read
  per volume every tick, without administrator rights.
- Drive kind read "Unknown type" for every fixed disk; now NVMe / SSD / HDD
  / Removable from the device itself, on the Storage screen and the
  dashboard alike.
- A GPU the driver would not name showed as `Display adapter 0x00033f83`;
  DXGI names it, and an indirect display that borrows its render card's
  name is marked `(virtual display)`.
- The Processes User column was empty on every row; it now shows the
  account for every process the current user may open.
- Eleven CSS tokens (`--color-accent-solid`, `--color-chart-1`, …) were
  referenced but never defined, leaving per-core bars, alert icons and HUD
  sparklines colourless.
- `AnimatedValue` and the sidebar indicator honour the app's own
  reduce-motion setting, not only the operating system's.

#### Build and tooling

- Size budget raised to the measured 188 880 B initial / 382 460 B shipped
  (gzip) for the above; HUD and mobile unchanged.
- `prove_devices` and `cost_probe` examples in `vitals-win`.

### Since the S1 upgrade commit (`7d9128c`)

The session of 2026-09-10 turned a single-window monitor into a set of
clients over one sampler. Grouped by Conventional Commit type; the hashes
are the commits in `git log --oneline 7d9128c..HEAD`.

#### Features

- **LAN server** (`vitals-server`) — REST, SSE, WebSocket and Prometheus
  `/metrics` over axum, hosted by the desktop app and off by default
  (`76e2c4f`, `201fe3d`). Advertised as `_vitals._tcp` over mDNS only while
  running (`cac19cd`). A loopback listener on `:7330` lets the CLI attach
  without a token (`9570564`).
- **Remote access UI** — pairing with a QR code rendered in Rust so the
  secret reaches the webview once; read and control token scopes
  (`201fe3d`).
- **The phone app** — `mobile.html`, a lean PWA: machines, processes,
  control with two-tap confirm, several PCs side by side (`2861e6f`), and an
  alerts feed per machine card polled from `/api/v1/alerts` (`7c9fc20`).
- **`@vitals/client`** — a typed TypeScript SDK over REST, SSE and WebSocket
  (`9b69325`).
- **CLI** — `vitals ps / top / info / report / serve`, attached to the app
  over loopback or sampling directly when it is closed (`c214938`).
- **History for real** — `vitals-store` gains SQLite, retention and a flight
  recorder (`89f189a`).
- **Alerts engine** in Rust with sustain, hysteresis and cooldown
  (`6a4b4f6`), run on the sampler thread and fanned out to the dashboard,
  tray, toasts and LAN (`3c5f946`).
- **"Why is my PC slow?"** — a verdict, the culprits and a minute of chart
  (`8e293b7`).
- **Tray icon** showing CPU; the close button can mean hide (`f1d1112`).
- **HUD** — an always-on-top transparent overlay, `Ctrl+Shift+H`
  (`f8505d5`).
- **Updater** — a real minisign key, a published `latest.json`, an honest UI
  state (`c0486da`).
- **Command palette**, keyboard shortcuts, route transitions and a motion
  config on `motion/react-m` (`45371d1`).
- **CSV/JSON export and URL-backed search** on every table (`50c1d01`).
- **Shared `StatList` and `Toaster` primitives**, ultrawide breakpoints and
  container queries, chart tests that assert the guarantee (`1a823a9`).
- **Windows sampler** — storage-stack disk counters (Task Manager's Disk
  column), efficiency mode, per-process handles and modules (`7f43315`).
- **Processes** — efficiency mode, affinity presets, handles, modules and
  file actions in the UI (`bba9c51`); set-efficiency-mode across the LAN
  API, SDK and CLI (`1d53032`).

#### Fixes

- Settings that did nothing now do something, or are gone (`b19233b`).
- The System Idle Process no longer ships in every frame — `PID 0 · 43 %`
  was the first thing the phone showed (`d093acc`).
- GPU utilisation is `Option`: `None` for an adapter with no counters, not
  `0 %` beside a real GPU (`5b9ed32`). The phone showed the real GPU rather
  than the virtual display enumerated first (`f08f1ff`).
- The control route checks scope before parsing the body, so an
  unauthorised caller cannot probe the schema (`2adcd8e`).
- A route hidden and shown again stopped receiving frames — the disposed
  flag was never reset (`9e65797`); the third copy of the same defect in
  `useProcessSnapshot` (`b83ffb9`).
- `ProcessKey` survives the trip through a JavaScript number (`c44faf8`).
- Each entry point is budgeted from its real module graph; the filename
  pattern was wrong in both directions (`8b00516`).

#### Build and tooling

- Contract-drift gate (`scripts/check-drift.ps1`) and a pre-commit hook
  that refuses what shipped before: kebab-case serde against camelCase
  bindings, `invoke()` with no command, locale parity, secrets, `.only`
  (`6ba9f02`).

#### Refactoring

- Dashboard alerts render from the Rust engine; the TypeScript copy is
  deleted (`acf1156`).

#### Tests

- Typed lint and typecheck satisfied in the LAN panel test (`217e5bc`);
  a protocol assertion typed lint rejects dropped (`539f5f1`).

#### Documentation

- OpenAPI 3.1 document and integration guide with a route drift test
  (`38aecf2`); Home Assistant package with a test that keeps the templates
  honest (`739fa15`); `AGENTS.md` and two skills written from the day's
  defects (`1d88d42`, `41cce6a`); tracker entries for S3–S9
  (`8a30c37`, `32655b0`, `e3c0b45`, `bb8f2e7`, `95819b2`).

### Before that

Every section now works against live data — there are no placeholder screens
left. There is still no release to install: the installer builds and runs,
but nothing has been through a public beta.

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
  independent axes (the surface modes were removed on 2026-09-28), built on OKLCH so every accent stays legible in both
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
- **Disk storage** — largest directories, per-volume usage and cleanup
  candidates. A location whose size could not be measured is reported as
  unmeasured rather than contributing zero, so the "you can reclaim this
  much" figure is never quietly short. Cancelling a scan reports unknown
  instead of presenting a truncated total as a measurement.
- **Devices & sensors** — temperatures, battery and power state, with the
  gaps enumerated. Most consumer sensors need a ring-0 driver Vitals does not
  install, and `MSAcpi_ThermalZoneTemperature` returns access-denied without
  elevation; both are reported as unavailable, never as zero. Sampled on its
  own 5-second cadence because a cold WMI query costs ~50 ms and has no
  business on the 1 Hz tick.
- **Benchmarks** with reproducible workloads and stated run conditions. A
  seeded PRNG makes two runs on the same machine comparable, results report
  the median and spread rather than a best case, and each result carries what
  the machine was doing at the time — a score without its conditions is a
  number without a meaning.
- **App history** — accumulated per-application CPU, disk and peak memory.
  This is Vitals' own tally, not Windows' SRUM database, and the UI says so:
  history starts empty at first run. A process seen for the first time
  credits nothing, because its existing counters describe time this history
  did not observe.
- **Users** — logon sessions with per-session process and resource rollups.
  Logon time is reported as unavailable rather than guessed: the buffer
  `WTSQuerySessionInformationW` returns does not match the documented
  `WTSINFOW` layout, and parsing it anyway would produce a plausible wrong
  answer.
- **GPU engine utilisation**, per engine and per process, from the WDDM
  performance counters — the same source Task Manager reads. Per-engine
  rather than one blended number: a machine transcoding video is 100% busy
  on the encode engine and idle on 3D, and averaging them reports 50%, which
  describes neither. GPU memory, clocks and fan still need a vendor SDK and
  remain unavailable rather than guessed.

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
- **App history read and wrote nothing.** Each command held its own
  function-local `static` store and neither was ever populated, so the screen
  would have shown an empty list forever and "clear history" would have
  cleared nothing. The store now lives in the sampler — the only thing that
  sees every tick — and the commands hold a handle to that same one.
- **An out-of-bounds read in session enumeration.** The wide-string scan
  bounded its length with `take` applied _after_ an unbounded `take_while`,
  so it dereferenced past the end of the allocation before the limit was ever
  consulted. The range is bounded up front now.
- **CI could never upload the installer.** The artifact path pointed inside
  `src-tauri`, but a Cargo workspace shares one target directory at the repo
  root. With `if-no-files-found: error`, that step could only fail.

### Performance

- **The test suite spent 29.6 seconds waiting on an orphan.** Process-action
  tests used `cmd /c ping -n 30` as their victim, which spawns a grandchild:
  killing `cmd` left `ping` alive holding the inherited stdout pipe, and the
  harness blocked until it finished on its own — for tests whose assertions
  take 0.07 s.
- **Prettier walked the entire tree**, `target/` and its 2281 JSON files
  included, spending 28 seconds to find 3 formattable files. Scoping the
  globs to the source roots is the fix; a `.prettierignore` alone is not,
  because the glob is expanded before ignores are consulted.
- **Benchmark tests ran the real workload in a debug build** — 12 million
  iterations and a 512 MB working set — to assert that a score is finite.
  The two assertions that genuinely need DRAM are now `#[ignore]`d and run in
  release by CI.
- Full gate run: 95 s to 27 s.
- **Source maps were shipping inside the installer.** `frontendDist` is the
  whole `dist` directory, so 3 MB of maps went into the download and would
  have gone into every differential update. Vite's `hidden` is not enough —
  it drops the comment but still writes the files.
- **Every screen loaded eagerly** although only one is ever visible, and
  `@vitals/ui` declared no `sideEffects`, so no unused Radix primitive could
  be dropped. Routes are lazy now, each registering its translations inside
  its own chunk. Initial parse fell from 236 KB to 151 KB gzipped, the
  packaged `dist` from 3866 KB to 815 KB, and the installer from 3.41 MB to
  2.86 MB.
- **`.pnpm-cache/` was committed** — 399 files, 214 MB. `.gitignore` named
  `.pnpm-store/`, which is not the directory pnpm uses here.

### Notes on dependencies

- `ts-rs` is used rather than `specta`: `specta-typescript` only supports a
  `2.0.0-rc` pre-release of specta, and the last stable specta line ships no
  TypeScript exporter at all.
- TypeScript 7 and TypeScript 6 are installed side by side, per Microsoft's
  [documented arrangement](https://devblogs.microsoft.com/typescript/announcing-typescript-7-0/#running-side-by-side-with-typescript-6.0).
  `tsc` is 7 and type-checks the codebase; `typescript-eslint` needs the 6.0
  API because TypeScript 7 does not yet expose a stable one.
