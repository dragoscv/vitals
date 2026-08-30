# Vitals

**See what your computer is actually doing.**

A fast, native system monitor and task manager for Windows — with macOS and
Linux to follow. Free and open source, forever.

> **Status: early development.** Every section works against live data on
> Windows — there are no placeholder screens left. The Windows sampler is
> verified against Windows' own counters and costs a median 13 ms per sample.
> macOS and Linux backends do not exist yet, and nothing has been through a
> public beta. There is no stable release.

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
  network and thermal throttling, then names the cause in plain language.
- **It catches what vanishes.** Processes that live for under a second, and
  windows that flash and disappear, are recorded rather than missed between
  samples.

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

## Planned

**Core** — efficiency mode, affinity presets and a handle/DLL finder on
Processes · _measured_ startup impact scores.

**Beyond the task manager** — per-app connection blocking · GPU engine
utilisation, which needs a `D3DKMTQueryStatistics` layout we do not yet have
a known-good reference for.

**Around the edges** — tray with live graphs · a floating always-on-top HUD ·
a command palette · alerts and rules · a flight recorder that captures a
minute of everything for sharing in a bug report · a scriptable CLI.

## Installing

> Vitals is pre-release. Nightly builds are published from `main` and are
> exactly as stable as that sounds.

Download the installer from
[Releases](https://github.com/dragoscv/vitals/releases). It installs without
asking anything — no wizard, no licence page, no "Next" — and takes about
five seconds. Add `/S` to script it.

**Windows will show a SmartScreen warning on first run.** That is expected:
the builds are not signed with a commercial certificate, which costs
€400–600/year. Rather than ask you to trust us, every release carries a
cryptographic attestation linking the binary to the exact source commit and
workflow that built it:

```powershell
gh attestation verify .\Vitals_x64-setup.exe --repo dragoscv/vitals
```

Checksums are in `SHA256SUMS.txt` alongside each release. See
[docs/distribution.md](docs/distribution.md) for the full reasoning.

Updates are a separate matter and are verified regardless: the updater checks
a minisign signature before applying anything, so a compromised mirror cannot
push a malicious update.

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
  helper/      optional elevated service: ETW tracing, privileged operations
  cli/         scriptable companion
crates/
  vitals-core  OS-agnostic domain model and provider traits
  vitals-win   Windows backend (NT native API, PDH, WMI, ETW, IPHLPAPI)
  vitals-macos empty backend stub — the trait boundary, no implementation
  vitals-linux empty backend stub — the trait boundary, no implementation
  vitals-ipc   frame ring buffer and the helper command protocol
  vitals-store local time-series storage
  vitals-bench benchmark harness
  vitals-plugin plugin contract
packages/
  ui  charts  protocol (generated)  i18n  config
```

Two principles hold the design together:

1. **The webview renders; Rust computes.** All sampling happens on a dedicated
   Rust thread and arrives as prepared frames.
2. **The dependency arrow points inward.** `vitals-win` depends on
   `vitals-core`, never the reverse — which is what makes macOS and Linux an
   implementation rather than a rewrite.

## Contributing

**Please do.** This project only gets good with other people in it.

The most useful thing you can do right now is
[open an issue](https://github.com/dragoscv/vitals/issues) — a bug, a metric
your hardware reports that we miss, a feature you have wanted from Task Manager
for years, or a motherboard whose sensors read wrong. Every report makes it
better.

See [CONTRIBUTING.md](CONTRIBUTING.md) to get set up.

## Licence

[MIT](LICENSE) — free forever, for any use.

### Acknowledgements

- [System Informer](https://github.com/winsiderss/systeminformer) (MIT), whose
  source is the best documentation of Windows process internals that exists.
- [LibreHardwareMonitor](https://github.com/LibreHardwareMonitor/LibreHardwareMonitor)
  (MPL-2.0) for years of accumulated per-motherboard sensor knowledge.
- [Tauri](https://tauri.app), for making a small native app achievable.
