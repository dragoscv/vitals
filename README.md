# Vitals

**See what your computer is actually doing.**

[![CI](https://github.com/dragoscv/vitals/actions/workflows/ci.yml/badge.svg)](https://github.com/dragoscv/vitals/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/dragoscv/vitals?include_prereleases&label=release)](https://github.com/dragoscv/vitals/releases/latest)
[![Downloads](https://img.shields.io/github/downloads/dragoscv/vitals/total)](https://github.com/dragoscv/vitals/releases)
[![Licence: MIT](https://img.shields.io/badge/licence-MIT-blue)](LICENSE)

A fast, native system monitor and task manager for Windows. Free and open
source, forever. Watch it from your phone, scrape it with Prometheus, script
it from a terminal.

**[Download for Windows](https://github.com/dragoscv/vitals/releases/latest/download/Vitals_x64-setup.exe)**
· [Website and docs](https://vitals.dragoscatalin.ro)
· [All releases](https://github.com/dragoscv/vitals/releases)

| Platform          | Download                                                                                                     | Status         |
| ----------------- | ------------------------------------------------------------------------------------------------------------ | -------------- |
| Windows 10/11 x64 | [Vitals_x64-setup.exe](https://github.com/dragoscv/vitals/releases/latest/download/Vitals_x64-setup.exe)     | Stable         |
| Windows 11 ARM64  | [Vitals_arm64-setup.exe](https://github.com/dragoscv/vitals/releases/latest/download/Vitals_arm64-setup.exe) | Preview        |
| macOS, Linux      | —                                                                                                            | On the roadmap |

> **Status: 0.9, stable on Windows.** Every section works against live data on
> Windows — there are no placeholder screens left. The Windows sampler is
> verified against Windows' own counters and stays inside a 30 ms-per-sample
> budget that CI enforces. macOS and Linux builds are compiled and checked in
> CI but not published, because their samplers do not exist yet
> ([ADR 0007](docs/adr/0007-windows-only-honest-stubs.md)).

**Privacy in one line:** no telemetry, no analytics, no accounts. The only
request Vitals makes by itself is an update check to GitHub, which you can
turn off. Details in [PRIVACY.md](PRIVACY.md).

---

## Why

Windows Task Manager is slow to open, shows a fraction of what the hardware
reports, and its most useful features have not changed in a decade. The tools
that do go deeper — Process Explorer, HWiNFO, Resource Monitor — are each
excellent at one thing and none of them are pleasant to use.

Vitals is an attempt at all of it in one place, fast, and readable.

## What makes it different

- **It measures itself.** Under 1% CPU and 80 MB of RAM at idle, enforced by a
  CI gate. A monitoring tool that shows up in its own process list as a top
  consumer has failed at its job.
- **It tells the truth.** A metric the hardware cannot report is shown as
  unknown, never as zero. A feature your privileges do not allow is greyed out
  with the reason, never a button that throws when clicked.
- **"Why is my PC slow?"** One click correlates CPU, disk, memory pressure,
  network and thermal throttling, then names the cause in plain language —
  a verdict, the culprits, and a minute of chart, with a Copy button for
  pasting into a bug report.
- **It catches what vanishes.** Processes that live for under a second, and
  windows that flash and disappear, are recorded rather than missed between
  samples.
- **It is not only a window.** The same readings reach a tray icon, an
  always-on-top HUD, your phone over the LAN, Prometheus, Home Assistant, a
  TypeScript SDK and a CLI — all from one sampler, so they always agree.

## What works today

- **Dashboard** — configurable widgets, and an attention panel that names the
  cause of a slowdown rather than restating the numbers above it.
- **Performance** — CPU (with a cell per logical processor), memory, GPU,
  every disk and adapter, and a Thermals tab that also lists what it cannot
  measure and why.
- **Processes** — grouped, searchable, with end/suspend/resume behind a risk
  model that states the actual consequence, and row ordering that holds still
  under the pointer.
- **Network connections** — every socket, grouped by the program that owns it.
- **Startup and Services** — everything that runs at sign-in, with the
  unreadable count stated rather than hidden.
- **Installed apps** — with an uninstall that launches the vendor's own
  uninstaller and deletes nothing itself.
- **Disk storage** — largest directories, per-volume usage and cleanup
  candidates, with anything unmeasurable reported as such rather than counted
  as zero.
- **Devices & sensors** — temperatures, battery and power state, each gap
  named along with the reason it cannot be read.
- **Benchmarks** — CPU and memory workloads that are reproducible run to run,
  reported with the conditions the machine was under at the time.
- **App history** — accumulated per-application CPU, disk and peak memory.
  Vitals' own tally, starting at first run; not Windows' SRUM data.
- **Users** — logon sessions with per-session process and resource rollups.
- **Processes, deeper** — efficiency mode, affinity presets, open handles
  and loaded modules per process, and file actions on the executable.
- **Alerts** — a stateful engine in Rust with sustain, hysteresis and
  cooldown, feeding the dashboard, the tray, toasts and the LAN API. Memory
  pressure needs a high page-fault rate _and_ low free memory; 90 % RAM full
  of file cache is a healthy machine.
- **Command palette** (`Ctrl+K`), keyboard shortcuts (`?` lists them), CSV
  and JSON export on every table, and search that lives in the URL so a view
  can be linked.
- **History** — a SQLite time-series store with retention, and a flight
  recorder that captures a minute of everything for a bug report.

### Around the edges

- **Tray icon** with live CPU; closing the window hides it there if you ask.
- **Replace Task Manager** — opt-in, in Settings → General. `Ctrl+Shift+Esc`,
  the taskbar's right-click menu and `Win+X` open Vitals instead. Windows asks
  for permission once to set it, and the tray keeps an "Open Windows Task
  Manager" entry so the built-in one is always a click away.
- **HUD** — an always-on-top, transparent, click-through overlay for CPU,
  memory and GPU. `Ctrl+Shift+H`.
- **Lag watchdog** — a small background program, installed and on by
  default, that notices when a window you are using stops responding or
  programs cannot get the processor, names the one responsible (never the
  terminal or editor it was started from) and offers End, Lower its
  priority or Ignore from a notification. A busy build alone never triggers
  it. Settings → Watchdog chooses how sensitive it is and its sound, which
  can be your own audio file.
- **Updater** — checks a minisign signature before installing anything; an
  unsigned update is refused.

### From your phone

Turn on **Remote access** in Settings, scan the QR code, and the phone shows
the machine's load, its process list and its alerts. Several PCs sit side by
side. Ending a process from the phone needs a separate control-scoped
token and a two-tap confirm. Off by default; nothing listens on the network until you
turn it on. (A loopback-only API on `127.0.0.1` is always open so the CLI can
attach; nothing outside this computer can reach it.) See [SECURITY.md](SECURITY.md) for the threat model.

### From anything else

The phone talks to a small HTTP server inside the app, and so can you:

- **REST, SSE and WebSocket** at `/api/v1/*` — the contract is in
  [`docs/api/openapi.yaml`](docs/api/openapi.yaml) and the short version in
  [`docs/api/README.md`](docs/api/README.md).
- **Prometheus** at `/metrics`. Absent readings are absent series, not zero.
- **Home Assistant** — a ready-made package in
  [`docs/integrations/home-assistant.md`](docs/integrations/home-assistant.md).
- **[`@vitals-app/client`](https://www.npmjs.com/package/@vitals-app/client)** (`pnpm add @vitals-app/client@next`) — a typed TypeScript SDK over all three transports,
  built on the same generated types the app uses.
- **mDNS** — the server advertises `_vitals._tcp` while it runs, and stops
  when it stops.

### From a terminal

```powershell
vitals top              # live, like the app's process list
vitals ps --top 10      # one shot; add --json to pipe it
vitals info             # the machine
vitals report --duration 30
vitals serve            # headless LAN server on :7331, token printed once
```

When the desktop app is running the CLI attaches to it over loopback and
sees the same numbers. When it is not, the CLI samples the machine itself.

## Planned

**Core** — _measured_ startup impact scores · an elevated helper for the
readings that need `SeDebugPrivilege` (per-process disk I/O is one).

**Beyond the task manager** — per-app connection blocking · GPU memory,
clocks and fan, which need a vendor SDK (NVML, ADL) rather than anything
Windows exposes.

**Platforms** — macOS and Linux backends behind the existing traits.

## Installing

Download the installer from the table above or from
[Releases](https://github.com/dragoscv/vitals/releases). It installs without
asking anything — no wizard, no licence page, no "Next" — in a few seconds.
Add `/S` to script it.

**Updates install themselves.** About twenty seconds after launch Vitals asks
GitHub whether there is a newer version, downloads it in the background,
verifies its minisign signature, and installs it when you quit. Nothing is
installed while you are using the app. Turn it off in Settings → About.

**Windows will show a SmartScreen warning on first run.** That is expected:
the builds are not yet signed with a commercial code-signing certificate.
Choose _More info → Run anyway_. Rather than ask you to trust us, every
release carries a cryptographic attestation linking the binary to the exact
source commit and workflow that built it:

```powershell
gh attestation verify .\Vitals_x64-setup.exe --repo dragoscv/vitals
```

Checksums are in `SHA256SUMS.txt` alongside each release, and CycloneDX
SBOMs for the Rust and JavaScript dependencies are attached too. See
[docs/distribution.md](docs/distribution.md) for the full reasoning.

Or with a package manager:

```powershell
scoop bucket add vitals https://github.com/dragoscv/scoop-vitals
scoop install vitals/vitals
```

winget (`winget install Vitals.Vitals`) and Chocolatey (`choco install vitals
--pre`) are submitted and become available once each registry's moderators
approve the first listing; every later release updates all three
automatically. See [docs/releasing.md](docs/releasing.md).

## Building

Requires [Rust](https://rustup.rs) (stable), [Node](https://nodejs.org) 22+ and
[pnpm](https://pnpm.io) 10+, plus the
[Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your
platform.

```bash
pnpm install
pnpm --filter @vitals/desktop dev
```

Useful commands:

```bash
pnpm test              # frontend tests
cargo test --workspace # Rust tests
pnpm lint              # eslint
cargo clippy --workspace --all-targets -- -D warnings
pnpm protocol:generate # regenerate TypeScript types from the Rust model
pwsh -NoProfile -File scripts/check-drift.ps1   # contract drift, two seconds
```

Every gate CI runs, in one command:

```bash
pwsh -NoProfile -File scripts/verify.ps1
```

Two checks are deliberately not part of `cargo test`, because they are slow
enough that people would stop running the suite:

```bash
# Assertions that need a real 512 MB working set to mean anything — that a
# dependent load actually reaches DRAM rather than being served from cache.
cargo test --workspace --release -- --ignored

# Bundle and installer size, against the budgets in size-budget.json. Pass
# -Update to accept a deliberate increase.
pwsh -NoProfile -File scripts/check-size.ps1
```

To see the LAN API against your own machine without pairing anything:

```bash
cargo run -p vitals-server --example serve_dev   # :7332, token "dev"
```

### A note on TypeScript

The repo pins two compilers on purpose. `tsc` is TypeScript 7 (the Go port) and
does the type-checking; `tsc6` is the TypeScript 6 API, which `typescript-eslint`
requires because TypeScript 7 does not yet expose a stable programmatic API.
This is Microsoft's
[documented side-by-side arrangement](https://devblogs.microsoft.com/typescript/announcing-typescript-7-0/#running-side-by-side-with-typescript-6.0)
and can be collapsed once typescript-eslint supports 7.1.

## Architecture

```
apps/
  desktop/     Tauri 2 shell — React 19 frontend, thin Rust host
               three entries: the app, the HUD overlay, the phone page
  helper/      optional elevated service (planned; a stub, not shipped — ADR 0012)
  site/        the website and docs, Astro Starlight → vitals.dragoscatalin.ro
  cli/         vitals top / ps / info / report / serve
crates/
  vitals-core  OS-agnostic domain model and provider traits
  vitals-win   Windows backend (NT native API, PDH, WMI, ETW, IPHLPAPI)
  vitals-macos empty backend stub — the trait boundary, no implementation
  vitals-linux empty backend stub — the trait boundary, no implementation
  vitals-ipc   frame ring buffer and the helper command protocol
  vitals-store SQLite time-series history, retention, flight recorder
  vitals-server axum HTTP server: REST, SSE, WebSocket, Prometheus, mDNS
  vitals-bench benchmark harness
packages/
  ui  charts  protocol (generated)  client (SDK)  i18n  config
```

Two principles hold the design together:

1. **The webview renders; Rust computes.** All sampling happens on a dedicated
   Rust thread and arrives as prepared frames.
2. **The dependency arrow points inward.** `vitals-win` depends on
   `vitals-core`, never the reverse — which is what makes macOS and Linux an
   implementation rather than a rewrite.

The longer version is in [docs/architecture.md](docs/architecture.md); the
decisions behind it are in [docs/adr/](docs/adr/README.md).

## Contributing

**Please do.** This project only gets good with other people in it.

The most useful thing you can do right now is
[open an issue](https://github.com/dragoscv/vitals/issues) — a bug, a metric
your hardware reports that we miss, a feature you have wanted from Task Manager
for years, or a motherboard whose sensors read wrong. Every report makes it
better.

See [CONTRIBUTING.md](CONTRIBUTING.md) to get set up.

## Licence and legal

[MIT](LICENSE) — free forever, for any use. The open-source components Vitals
is built on are listed with their licences in
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).

- [Privacy policy](PRIVACY.md) · [Terms of use](TERMS.md) ·
  [Security policy](SECURITY.md) · [Code of conduct](CODE_OF_CONDUCT.md)
- [Contributors](CONTRIBUTORS.md) · [Support](SUPPORT.md) ·
  [Governance](GOVERNANCE.md)

### Acknowledgements

- [System Informer](https://github.com/winsiderss/systeminformer) (MIT), whose
  source is the best documentation of Windows process internals that exists.
- [LibreHardwareMonitor](https://github.com/LibreHardwareMonitor/LibreHardwareMonitor)
  (MPL-2.0) for years of accumulated per-motherboard sensor knowledge.
- [Tauri](https://tauri.app), for making a small native app achievable.
