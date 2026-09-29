# Architecture

How Vitals is put together, and the two principles that decide where a new
piece of code goes. Decisions with a history are in [`adr/`](adr/README.md).

## Two principles

1. **The webview renders; Rust computes.** Sampling runs on one dedicated
   Rust thread and arrives in the UI as prepared frames — a keyframe every
   30 ticks, deltas between. The frontend never opens a handle, never calls
   an OS API, never aggregates. Every client (desktop, HUD, phone, CLI,
   Prometheus, SDK) reads the same frames, so a fix in the sampler reaches
   all of them and a new client is an audit of the backend.

2. **The dependency arrow points inward.** `vitals-win` depends on
   `vitals-core`; nothing depends on `vitals-win` except the binaries that
   host it. The core is OS-agnostic and CI compiles it on Ubuntu to prove
   it. A macOS or Linux backend is an implementation of the same traits,
   not a rewrite.

And the rule that follows from both: **a number that cannot be measured is
`None`, never `0`.** The core's optional readings are `Option<T>`; the
Prometheus exporter omits absent series; the UI renders an em dash.

## Crates

```
crates/
  vitals-core    domain model, provider traits, frame/delta codec, alert
                 engine, diagnosis, fixtures (feature), ts-rs export (feature)
  vitals-win     the Windows backend: NT native API, PDH, WMI, IPHLPAPI,
                 D3DKMT; capability policy; overhead budget test
  vitals-macos   trait stubs returning Unavailable::NotImplemented
  vitals-linux   trait stubs returning Unavailable::NotImplemented
  vitals-store   SQLite time-series history, retention, flight recorder
  vitals-server  axum HTTP server: REST, SSE, WebSocket, Prometheus, mDNS,
                 token auth, static mobile assets
  vitals-ipc     frame ring buffer and the helper command protocol
  vitals-bench   reproducible CPU/memory workloads
apps/
  desktop/       Tauri 2 host (src-tauri) + React 19 webview (src)
                 three entries: index.html, hud.html, mobile.html
  cli/           vitals ps|top|info|report|serve
  helper/        planned elevated service — a stub today (ADR 0012)
  site/          Astro Starlight website + docs (vitals.dragoscatalin.ro)
packages/
  protocol       TypeScript types generated from vitals-core by ts-rs
  client         @vitals/client — typed SDK over REST, SSE, WebSocket
  ui             shadcn-style primitives on Radix, theme tokens
  charts         canvas renderer, RingBuffer
  i18n           en + ro, parity-tested
  config         shared tsconfig
```

## Data flow

```
                 ┌──────────────── Rust ────────────────┐
 OS counters ──▶ vitals-win ──▶ Frame ──▶ sampler thread ┼──▶ Tauri events ──▶ webview (index / hud)
                                             │           │
                                             ├──▶ alert engine ──▶ tray, toasts, /api/v1/alerts
                                             ├──▶ vitals-store (SQLite) ──▶ history, flight recorder
                                             └──▶ vitals-server ──┬──▶ LAN :7331  ──▶ phone, SDK, HA, Prometheus
                                                                  └──▶ loopback :7330 ──▶ vitals CLI
```

- **Frames** are delta-encoded. Exits are applied before changes so a
  recycled PID cannot inherit the previous owner's metrics. A client joining
  mid-stream receives a materialised keyframe, never a bare delta.
- **Process identity** is `ProcessKey { pid, startTime }`. Every mutating
  command takes one; a stale PID cannot end the wrong process.
- **Capabilities** are declared by the backend, not discovered by failing.
  The UI disables an affordance with the reason.

## Contract boundaries and their gates

| Boundary                  | Generated / checked by                                |
| ------------------------- | ----------------------------------------------------- |
| Rust ↔ TypeScript types   | `ts-rs` → `packages/protocol`; CI diff must be empty  |
| serde casing ↔ binding    | `scripts/check-drift.ps1` (camelCase rename required) |
| `invoke()` ↔ `#[command]` | `scripts/check-drift.ps1`                             |
| en ↔ ro locales           | `scripts/check-drift.ps1` + i18n tests                |
| HTTP routes ↔ OpenAPI     | `crates/vitals-server/tests` route-list drift test    |
| HA templates ↔ frame      | `crates/vitals-server/tests/home_assistant.rs`        |
| Bundle size               | `scripts/check-size.ps1` against `size-budget.json`   |
| Sampler cost              | `cargo test -p vitals-win --release --test overhead`  |

## Windows

- Only `vitals-win` and `apps/desktop/src-tauri` see Windows types.
- The release profile is `panic = "abort"` (ADR 0026); a panic on the
  sampler thread is a crash, not a frozen window.
- The installer is NSIS, per-machine, silent (ADR 0025).

## What is persisted, and what listens

| Thing              | When                    | Where / scope                                          |
| ------------------ | ----------------------- | ------------------------------------------------------ |
| Local API          | always, while running   | `127.0.0.1:7330`, loopback only, tokenless read        |
| Attach pipe        | always, while running   | `\\.\pipe\vitals-<USERNAME>`, default per-user DACL    |
| LAN server         | only when switched on   | `0.0.0.0:7331`, bearer token, mDNS `_vitals._tcp`      |
| Flight recorder    | always                  | last 120 frames in `history.sqlite`, no owners or MACs |
| History            | only when switched on   | `history.sqlite` tiers, 1–90 days, 512 MB cap          |
| Pairing tokens     | once a device is paired | `lan-tokens.json`, SHA-256 hashes + 8-char prefix only |
| Log and last crash | always                  | `logs\vitals.log` (capped, rotated), `logs\crash.txt`  |

All under `%LOCALAPPDATA%\Vitals`. [PRIVACY.md](../PRIVACY.md) is the
user-facing version of this table; change both together.

## Updates

`src-tauri/src/updates.rs`. About twenty seconds after launch — never before
the first paint — the app asks `releases/latest/download/latest.json` whether
there is a newer version. A newer one is downloaded in the background and its
minisign signature checked against `plugins.updater.pubkey`; the verified
bytes wait in memory, and on `RunEvent::Exit`, after the sampler and the
listeners have stopped, they are handed to the NSIS installer in passive mode.
The next launch is the new version. The "Install updates automatically"
setting stops the check and drops anything already downloaded.

## Release architecture

```
tag vX.Y.Z ─▶ plan ─▶ verify (every CI gate + version.ps1 -Check) ─┬▶ build windows-x64    ─┐
                                                                  ├▶ build windows-arm64  ─┤
                                                                  ├▶ build macos-universal ─┤ artefacts only until
                                                                  ├▶ build linux-x64      ─┤ PUBLISH_MACOS/LINUX
                                                                  └▶ sbom (CycloneDX)     ─┤
                                                                                           ▼
                           publish: latest.json (all published platforms), stable names,
                           SHA256SUMS, provenance + SBOM attestations, release notes
                                                                                           │
                               ┌───────────────┬──────────────┬───────────────┬───────────┘
                               ▼               ▼              ▼               ▼
                             winget          Scoop       Chocolatey       npm (@vitals/client)
```

Every channel job is gated on a repository variable (`PUBLISH_*`) and its
secret, so a channel that is not set up yet is skipped rather than failing a
release. Details, secrets and the manual first submissions:
[releasing.md](releasing.md). The website is built by `pages.yml` on every
push to `main` that touches `apps/site`.
