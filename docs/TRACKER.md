# Vitals — work tracker

Single canonical narrative tracker. The machine-readable companion is
[`tracker.csv`](tracker.csv); keep the two in step — every ID here exists there.

**Session opened** 2026-09-10. Baseline commit `147674f`.

---

## Decisions taken (askQuestions, 2026-09-10)

| #   | Question        | Decision                                                                                                                        |
| --- | --------------- | ------------------------------------------------------------------------------------------------------------------------------- |
| D1  | Phone access    | Embedded LAN HTTP server + lean mobile PWA. QR carries `http://<ip>:<port>/mobile.html#token=…`                                 |
| D2  | Third-party API | REST JSON + SSE + WebSocket + Prometheus `/metrics` + named-pipe IPC + TypeScript client SDK                                    |
| D3  | LAN security    | Off by default, bearer token in the QR fragment, plain HTTP, revocable, scoped                                                  |
| D4  | CLI             | Samples directly via `vitals-win` (works with the app closed), JSON + table output                                              |
| D5  | Upgrades        | Everything to latest **stable**, in verifiable slices with gates between                                                        |
| D6  | Native testing  | **Constraint lifted** — may `tauri dev`/build and launch                                                                        |
| D7  | Platforms       | Windows only; macOS/Linux stay honest trait stubs                                                                               |
| D8  | Nice-to-haves   | All nine accepted (palette, tray, alerts, HUD, flight recorder, efficiency/affinity, export, updater, handle finder)            |
| D9  | Tracker         | `docs/tracker.csv` + `docs/TRACKER.md`                                                                                          |
| D10 | Commits         | Conventional Commits per slice, explicit paths, **no push**                                                                     |
| D11 | Dead crates     | Implement `vitals-store` for real; **delete `vitals-plugin`**; keep `vitals-ipc` for the helper                                 |
| D12 | `apps/helper`   | Stays stubbed. Remove the fake capability claims; get disk I/O from `DiskCounters` instead                                      |
| D13 | Inert settings  | Wire what is wireable; **delete** `crashReports`, `usageData`, `reputationLookups`, `advancedEnabled`                           |
| D14 | Mobile UI       | Separate lean `mobile.html` entry, own component tree, shares charts + protocol + tokens                                        |
| D15 | Mobile scope    | Overview, processes, alerts, **process control** (separate token scope), **multi-machine**                                      |
| D16 | Palette         | Build on Radix Dialog; remove `cmdk`                                                                                            |
| D17 | Animation       | `motion/react-m` + `LazyMotion(domAnimation)` — 19.6 kB, not the 34 kB floor                                                    |
| D18 | Size budget     | Raise deliberately, per-feature cost recorded, gate stays strict                                                                |
| D19 | Git remote      | Add `origin` → `github.com/dragoscv/vitals`, **never push**                                                                     |
| D20 | Updater keys    | Wire everything; pubkey stays a loud, documented TODO for the user to run once                                                  |
| D21 | Slice order     | tooling → truth → server → mobile → UI → tray/HUD → metrics → docs/CI                                                           |
| D22 | Extras          | All eleven accepted (mDNS, audits, typed lint, DiskCounters, ADRs, coverage, shortcuts, search, slow-report, dependabot, ARM64) |

---

## Audit findings

### The dominant pattern: "built but unfed"

A complete, tested backend exists; the caller hardcodes `None` / `Vec::new()` /
`0`, or the frontend never reads the value. Eight prior instances are recorded
in repo memory. This audit found the ninth through the eighteenth.

- **F1** — 12 of 21 settings are pure UI (`startWithWindows`, `startMinimised`,
  `confirmEndTask`, four notification switches, `historyEnabled`,
  `retentionDays`, `crashReports`, `usageData`, `reputationLookups`,
  `advancedEnabled`).
- **F2** — `get_users` returns `rollups: Vec::new()` while `rollup_by_session`
  is implemented and covered by seven tests.
- **F3** — `Capability::HandleEnumeration` and `SetEfficiencyMode` are claimed
  unconditionally with no implementation behind either.
- **F4** — `tray-icon` feature on, `core:tray:*` permitted, no `TrayIcon` ever
  constructed; Settings already offers "start minimised to the tray".
- **F5** — `global-shortcut` registered and permitted, no shortcut registered.
- **F6** — updater `pubkey` is the literal string
  `REPLACE_WITH_TAURI_UPDATER_PUBLIC_KEY`, and nothing calls `check()`.
- **F7** — `vitals-store` has no SQLite at all (only `retention.rs`);
  `vitals-ipc` and `vitals-plugin` are declared deps with zero references.
- **F8** — `apps/cli` and `apps/helper` both `bail!("not yet implemented")`.
- **F9** — 14 of 20 third-party workspace Cargo deps are referenced by nobody,
  including `rusqlite --features bundled` and `interprocess`.
- **F10** — six unused npm runtime deps ship in the installer.
- **F11** — app history is persisted only at sampler-loop exit; killing the
  process loses the session's whole tally.
- **F12** — `retentionDays` is enforced nowhere; history grows unbounded.

### Correctness bugs

- **F13** — density labels render the raw enum (`compact`/`default`/
  `comfortable`) in every locale.
- **F14** — `backendRef.current = backend` written during render in
  `useLayout.ts`; the exact pattern `react-hooks/refs` forbids.
- **F15** — `UNIMPLEMENTED_ACTIONS` still lists `affinity`, which is wired.
- **F16** — two independent persistence layers with duplicated fallback logic.
- **F17** — `ISSUE_TEMPLATE/config.yml` points at `vitals-app/vitals`; every
  other reference says `dragoscv/vitals`.

### Reach and polish

- **F18** — content is capped at 1600 px / three columns. On 21:9 and wider,
  roughly half the window is gutter. No container queries anywhere.
- **F19** — no mobile layout exists, by design.
- **F20** — `motion` is a declared dependency imported nowhere; all motion is
  hand-written CSS.
- **F21** — eleven `@vitals/ui` components and `TimeSeriesChart` are untested.
- **F22** — `Skeleton` shapes are duplicated across ten feature folders;
  `StatList`-shaped `<dl>` grids reimplemented in six.

### Toolchain and CI

- **F23** — no `cargo audit`, no `pnpm audit`, no dependabot. Zero
  supply-chain coverage.
- **F24** — release re-runs fmt + clippy + tests + the perf budget on **both**
  matrix legs, re-proving what CI already proved on the same commit.
- **F25** — `RUSTFLAGS: '-D warnings'` set globally in `release.yml` busts the
  shared `rust-cache` against `ci.yml`.
- **F26** — `release.yml` has no bindings-drift gate, so a tag can ship
  generated types that disagree with the Rust model.
- **F27** — no `.vscode/` at all. And `.gitignore`'s allowlist omits
  `tasks.json`, so adding one would be silently ignored.
- **F28** — ESLint uses `recommended`, not `recommendedTypeChecked`, so
  `no-floating-promises` never runs over a codebase full of `invoke()`.
- **F29** — `CHANGELOG.md` has only `## [Unreleased]` while `release.yml`
  cuts tagged releases.
- **F30** — no ADRs, no architecture doc, no API doc.

### Research verdicts that shaped the plan

- **axum 0.8.9 + tower-http 0.7.1.** Tauri already runs a multi-thread tokio
  runtime, so axum adds no second executor. actix would; `tiny_http` cannot do
  WebSockets at all.
- **No service worker.** `http://192.168.x.x` is not a secure context, so
  `navigator.serviceWorker` is `undefined` and `beforeinstallprompt` never
  fires. `vite-plugin-pwa` is therefore pointless here — ship a manifest and
  Add-to-Home-Screen meta tags only.
- **`local_ip()` is wrong on this machine.** Hyper-V and WSL adapters win the
  guess. Enumerate with `list_afinet_netifas()`, filter virtual adapters, and
  let the user choose.
- **typescript-eslint cannot support TypeScript 7** (peer `<6.1.0`; upstream
  issue closed "not planned" pending TS 7.1's new API). The dual-compiler
  arrangement is Microsoft's sanctioned pattern and stays.
- **Task Manager's Disk column is not `GetProcessIoCounters`.** It is the
  undocumented `SYSTEM_PROCESS_INFORMATION_EXTENSION.DiskCounters`, which is
  free inside a call the sampler already makes.
- **vitest 5.0.0 is GA.** The only migration risk is `clearMocks` defaulting
  to `true`.
- **cmdk is eighteen months stale** with an open defect in its core combobox
  ARIA contract.
- **`motion/react` has a 34 kB gzip floor**; `motion/react-m` under
  `LazyMotion(domAnimation)` is 19.6 kB.

---

## Slices

Status values: `todo`, `doing`, `done`, `blocked`, `dropped`.

| Slice | Theme                                                                | Status                                                                                         |
| ----- | -------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------- |
| S1    | Dependency upgrades, tooling, VS Code tasks                          | done                                                                                           |
| S2    | Truth fixes — dead settings, false capabilities, dead crates         | done (S2-03 notifications, S2-09 persistence unify, S2-15 get_capabilities carried into S8/S7) |
| S3    | `vitals-store` for real: SQLite history, retention, flight recorder  | done                                                                                           |
| S4    | LAN server: REST, SSE, WebSocket, Prometheus, mDNS                   | doing (10/12 done; mDNS, named-pipe IPC and TS SDK remain)                                     |
| S5    | CLI that samples directly                                            | todo                                                                                           |
| S6    | Mobile PWA and QR pairing                                            | todo                                                                                           |
| S7    | UI polish: motion, palette, ultrawide, export, shortcuts             | todo                                                                                           |
| S8    | Tray, HUD, alerts, notifications, updater                            | todo                                                                                           |
| S9    | New Windows metrics: DiskCounters, efficiency mode, handles, modules | todo                                                                                           |
| S10   | Docs, ADRs, CI, supply-chain audits                                  | todo                                                                                           |

Per-item status lives in `tracker.csv`. This file records the reasoning; the
CSV records the state.

---

## Verification log

Every claim of "done" needs a command and its output. Recorded here as work
lands, newest first.

### 2026-09-10 — S4 LAN server (commits 76e2c4f, 201fe3d, 217e5bc)

- `cargo test -p vitals-server` → 31 unit + 11 socket tests pass. Half the
  socket tests are adversarial: anonymous request to every data route → 401;
  wrong token byte-identical to missing; read-scope token attempting control
  → 403 and the controller is never called; `..` traversal refused.
- `cargo run -p vitals-server --example prove_lan` against the LIVE sampler:
  `/health` 200 unauthenticated; `/snapshot` 401 without token; with token
  `seq=5 kind=keyframe cpu.total=84.37 processes=648`; `/metrics` 174 series.
- `cargo clippy --workspace --all-targets -- -D warnings` → exit 0.
- `pnpm typecheck` · `eslint .` · `format:check` → clean; `pnpm test` → 60
  desktop files (6 new for the Remote access panel).
- **Two defects found by the prover, not by inspection:**
  1. `ProcessKey` serialised `start-time` (kebab-case) while the TS binding
     said `startTime`. `wire_format.rs` only checked `_`; it now checks `-`
     and fails with the bug reintroduced (verified by stash/pop).
  2. A client joining mid-stream received the newest frame — a delta — so
     `processes=0`. Serving the last keyframe was stale (≤30 s; the first
     always reads 0 % CPU). `FrameSource` now materialises deltas onto the
     keyframe, exits before changes (same rule as `metrics.ts`).
- Decisions: hand-written Prometheus exposition rather than
  `prometheus-client` (one snapshot, no mutable registry to keep in step);
  QR rendered in Rust so the secret is returned to the webview exactly once;
  `getrandom` crate over a hand-declared `ProcessPrng` (LNK1181:
  `bcryptprimitives.lib` is not in the default MSVC link set).
- Process note: commit 201fe3d landed in the same shell command as its gates
  and two test-typing errors slipped through → 217e5bc. Gates and commit are
  now separate commands.

### 2026-09-10 — S2 truth fixes (commit follows)

- `cargo clippy --workspace --all-targets -- -D warnings` → exit 0
- `cargo test --workspace` → 722 passed, 0 failed (541 in vitals-win)
- `pnpm typecheck` → exit 0 · `eslint .` → exit 0 · `format:check` → clean
- `pnpm test` → 71 files, 688 desktop tests pass
- Ripple caught by the compiler: adding `Unavailable::NotImplemented` failed
  the exhaustive match in `inventory.rs:858` — fixed, plus both locale files.
- Decision recorded: `pnpm minimumReleaseAge: 0` with a comment (user chose to
  keep latest-of-everything over the 24 h quarantine).

### 2026-09-10 — S1 upgrades (commit 7d9128c)

- vitest 5.0.0: 71 test files pass; `clearMocks` flip harmless.
- Typed ESLint found one genuine floating promise
  (`AppHistoryScreen.test.tsx:23`, `initI18n` never awaited).
- `expect_used` now warns in production code: 2 startup sites justified
  inline, 186 test sites covered by `allow-expect-in-tests`.
