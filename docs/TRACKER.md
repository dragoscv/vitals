# Vitals — work tracker

Single canonical narrative tracker. The machine-readable companion is
[`tracker.csv`](tracker.csv); keep the two in step — every ID here exists there.

**Session opened** 2026-09-10. Baseline commit `147674f`.

---

## Decisions taken (askQuestions, 2026-09-10)

| #   | Question        | Decision                                                                                                                                                                               |
| --- | --------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| D1  | Phone access    | Embedded LAN HTTP server + lean mobile PWA. QR carries `http://<ip>:<port>/mobile.html#token=…`                                                                                        |
| D2  | Third-party API | REST JSON + SSE + WebSocket + Prometheus `/metrics` + loopback HTTP on :7330 for the CLI (was: named pipe; changed 2026-09-10, one wire format instead of two) + TypeScript client SDK |
| D3  | LAN security    | Off by default, bearer token in the QR fragment, plain HTTP, revocable, scoped                                                                                                         |
| D4  | CLI             | Samples directly via `vitals-win` (works with the app closed), JSON + table output                                                                                                     |
| D5  | Upgrades        | Everything to latest **stable**, in verifiable slices with gates between                                                                                                               |
| D6  | Native testing  | **Constraint lifted** — may `tauri dev`/build and launch                                                                                                                               |
| D7  | Platforms       | Windows only; macOS/Linux stay honest trait stubs                                                                                                                                      |
| D8  | Nice-to-haves   | All nine accepted (palette, tray, alerts, HUD, flight recorder, efficiency/affinity, export, updater, handle finder)                                                                   |
| D9  | Tracker         | `docs/tracker.csv` + `docs/TRACKER.md`                                                                                                                                                 |
| D10 | Commits         | Conventional Commits per slice, explicit paths, **no push**                                                                                                                            |
| D11 | Dead crates     | Implement `vitals-store` for real; **delete `vitals-plugin`**; keep `vitals-ipc` for the helper                                                                                        |
| D12 | `apps/helper`   | Stays stubbed. Remove the fake capability claims; get disk I/O from `DiskCounters` instead                                                                                             |
| D13 | Inert settings  | Wire what is wireable; **delete** `crashReports`, `usageData`, `reputationLookups`, `advancedEnabled`                                                                                  |
| D14 | Mobile UI       | Separate lean `mobile.html` entry, own component tree, shares charts + protocol + tokens                                                                                               |
| D15 | Mobile scope    | Overview, processes, alerts, **process control** (separate token scope), **multi-machine**                                                                                             |
| D16 | Palette         | Build on Radix Dialog; remove `cmdk`                                                                                                                                                   |
| D17 | Animation       | `motion/react-m` + `LazyMotion(domAnimation)` — 19.6 kB, not the 34 kB floor                                                                                                           |
| D18 | Size budget     | Raise deliberately, per-feature cost recorded, gate stays strict                                                                                                                       |
| D19 | Git remote      | Add `origin` → `github.com/dragoscv/vitals`, **never push**                                                                                                                            |
| D20 | Updater keys    | Wire everything; pubkey stays a loud, documented TODO for the user to run once                                                                                                         |
| D21 | Slice order     | tooling → truth → server → mobile → UI → tray/HUD → metrics → docs/CI                                                                                                                  |
| D22 | Extras          | All eleven accepted (mDNS, audits, typed lint, DiskCounters, ADRs, coverage, shortcuts, search, slow-report, dependabot, ARM64)                                                        |

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

| Slice | Theme                                                                | Status                                                                                             |
| ----- | -------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------- |
| S1    | Dependency upgrades, tooling, VS Code tasks                          | done                                                                                               |
| S2    | Truth fixes — dead settings, false capabilities, dead crates         | done (S2-15 closed 2026-09-11: get_capabilities via useHostFacts, query_machine_history via S3-05) |
| S3    | `vitals-store` for real: SQLite history, retention, flight recorder  | done (S3-05 History widget added 2026-09-11 — the store finally has a reader in the UI)            |
| S4    | LAN server: REST, SSE, WebSocket, Prometheus, mDNS                   | done (S4-11 named-pipe IPC landed 2026-09-11 with the CLI attach path)                             |
| S5    | CLI that samples directly                                            | done (6/6; attaches to the app over its named pipe, then :7330, samples directly otherwise)        |
| S6    | Mobile PWA and QR pairing                                            | done except S6-05 alerts feed (S8 engine now exposes GET /api/v1/alerts; phone UI pending)         |
| S7    | UI polish: motion, palette, ultrawide, export, shortcuts             | done (10/10)                                                                                       |
| S8    | Tray, HUD, alerts, notifications, updater                            | done (6/6; alerts engine in Rust feeds desktop, tray, toasts, LAN)                                 |
| S9    | New Windows metrics: DiskCounters, efficiency mode, handles, modules | backend done for 01/02/04/05 (vitals-win + vitals-core); 03/06/07 and the Tauri commands pending   |
| S10   | Docs, ADRs, CI, supply-chain audits                                  | done except S10-12 (ARM64 leg unproven — needs a run on `windows-11-arm`, and agents never push)   |

Per-item status lives in `tracker.csv`. This file records the reasoning; the
CSV records the state.

---

## Verification log

Every claim of "done" needs a command and its output. Recorded here as work
lands, newest first.

### 2026-09-11 — S3-05 History widget / S2-15 closed (no commit yet)

**Why.** `query_machine_history` was registered and answered correctly, and
nothing in the webview called it — `check-drift.ps1` had warned about it since
the store landed. Another "built but unfed" instance: a SQLite time series
written every second that no screen ever read. The widget is opt-in (added via
Edit layout, not in the default layout), `full` width, and reads the store on
mount and every 60 s **only while rendered** — the timer is owned by the
effect, so removing the widget stops the reads.

**Shape.** `widgets/useMachineHistory.ts` holds the one `invoke` spelling
behind an injectable `HistorySource`, and `seriesFromSamples` builds three
`RingBuffer`s (CPU %, memory % of total, GPU %). A `null` `gpuPercent` becomes
`pushGap()`, never 0 — the store carries "unknown" through rollup and the chart
must not turn it into an idle GPU. States: recording off → EmptyState with an
"Open Settings" button (fires `OPEN_SETTINGS_EVENT`, handled in `AppShell`,
same pattern as `DIAGNOSE_EVENT`); zero samples → "No history yet"; invoke
failure → inline `role="alert"` line, never a blank canvas. The widget is a
`lazy()` chunk (1.89 KB gz) so the entry does not pay for it.

**Gates.**

```
pnpm -C apps/desktop exec tsc --noEmit                      → exit 0
npx eslint apps/desktop/src/features/dashboard …             → exit 0
pnpm -C apps/desktop exec vitest run src/features/dashboard  → Test Files 7 passed, Tests 92 passed
  mutants: gpu.pushGap()→push(0)  → 1 failed;  drop clearInterval → 1 failed
pnpm format:check                                            → All matched files use Prettier code style!
pwsh scripts/check-drift.ps1   → drift: 0 failure(s), 0 warning(s) — 53 invokes, 53 commands
pnpm -C apps/desktop build:vite → dist/assets/HistoryWidget-*.js 4.11 kB │ gzip: 1.89 kB
pwsh scripts/check-size.ps1 -SkipInstaller
  initial 181.0 KB of 181.6 KB (99.7%) — unchanged budget
  shipped 366.3 KB of 366.3 KB — budget raised 372808 → 375129 B (+2321 B: the
  lazy chunk plus strings/catalogue entry), deliberately to the measured value
```

### 2026-09-11 — S4-11 / S5-06 attach pipe: one sampler per machine (no commit yet)

**Why it came back.** The 2026-09-10 decision was loopback HTTP only. That
left the CLI dependent on a discovery file, a port and a PID check, and the
fallback silently ran a second sampler beside the app. The pipe removes all
three: `vitals-ipc::attach` binds an `interprocess` local socket named
`vitals-<user>` (a Windows named pipe; Unix socket elsewhere), speaks
newline-delimited JSON (`hello` → `{status,version,modelVersion}`,
`subscribe` → frames, `snapshot` → one frame), and the desktop feeds it from
`sampling.rs` beside the LAN publish, gated on `has_clients()` so an idle pipe
costs one atomic load per tick. No tokens: the pipe ACL is the boundary (module
doc explains). Loopback HTTP is kept for `--attach <url>` and older desktops.

**Fold rule.** The server keeps a materialised view (exits before changes, as
`vitals-server::FrameSource`) so the first frame a subscriber sees is complete
and current — not the latest delta, not the stale keyframe. A subscriber that
falls 8 frames behind is re-synchronised with a keyframe instead of a delta it
cannot apply. Frames are not folded while nobody is attached, so the sampler
reads `wants_keyframe()` and forces one on the first tick after a connect.

**Defect found by looking, not by tests.** Against the live app the first
`--source app` frame took **15 s**: the window was hidden, so the sampler was
on `SampleRate::Background` (20 s), and `snapshot` gave up at 6 s. The wait is
now 25 s with the reason in a comment. Same class as every other new-consumer
audit in this file.

```
cargo clippy -p vitals-ipc -p vitals-cli -p vitals-desktop --all-targets -- -D warnings
  → Finished `dev` profile (0 warnings)
cargo test -p vitals-ipc -p vitals-cli -p vitals-desktop
  → vitals-ipc lib 20 passed; tests/attach.rs 9 passed; vitals-cli 35 passed; vitals-desktop 34 passed
vitals top --json --source local | Select-Object -First 3
  → source: sampling directly
    {"t":1789077347471,"system":{"cpu":{"total":0.0, …      (first tick primes baselines)
    {"t":1789077348517,"system":{"cpu":{"total":67.49068, …
vitals info --source app          (desktop PID 76916, rebuilt by tauri dev at 00:56)
  → source: attached to Vitals 0.1.0 over vitals-vladu
    Host  DRAGOS / OS  Windows 11 10.0 (26200)
vitals top --json --source app | Select-Object -First 3
  → source: attached to Vitals 0.1.0 over vitals-vladu
    {"t":1789077391770,"system":{"cpu":{"total":51.519672, …
    {"t":1789077407871, …   (15 s gap = Background rate with the window hidden)
```

Not updated, with reason: `docs/adr/**`, README, CHANGELOG, CONTRIBUTING,
SECURITY, workflows — owned by another agent this session; the ADR recording
the loopback-only decision should gain a "superseded 2026-09-11" note when
they next touch it. `packages/client` / SDK: unaffected, the pipe is not a
network surface. Locales: no UI string changed.

### 2026-09-11 — S10 docs, ADRs, CI, audits (no commit — docs and workflows only)

**What was written.** 29 ADRs in `docs/adr/` (D1–D22 from the decisions
table, plus the seven technical decisions named in S10-02) with a README
index; `docs/architecture.md`; README rewritten around what exists now;
CONTRIBUTING pointed at `verify.ps1`, `check-drift.ps1`, the hook install,
the prover pattern and the sentence-name rule for tests; SECURITY.md's LAN
threat model, with the helper reframed as planned-not-shipped; CHANGELOG
Unreleased grouped by commit type over `7d9128c..HEAD`.

**Every factual claim was checked with `rg` before it was written**, and
two were wrong in the brief: the pairing fragment key is `#t=`, not
`#token=` (`lan.rs:197`), and tokens are 32 CSPRNG bytes base64url-encoded
(`auth.rs:134`), which is what SECURITY.md now says. S10-03 turned out to
exist already (`docs/api/README.md` + `openapi.yaml`, commit `38aecf2`);
it is linked rather than duplicated.

**CI.** `ci.yml`: a `Supply chain` job — `cargo audit --deny warnings` via
`taiki-e/install-action@v2` (never `rustsec/audit-check`, which compiles
the tool from source) and `pnpm audit --audit-level high`; the frontend job
now builds the bundle and runs `check-size.ps1 -SkipInstaller`. No
tag-triggered CI run was added. `release.yml`: Verify, a new Bindings-drift
step and the perf budget are gated to the x64 leg (F24); the global
`RUSTFLAGS` is gone (F25); `setup-node@v6` on both files. `dependabot.yml`
covers cargo, npm and github-actions weekly, grouped. `ISSUE_TEMPLATE/
config.yml` points at `dragoscv/vitals` (F17).

**Not done, and why.** S10-12 (ARM64 leg): the matrix entry is correct
by inspection but proving it needs a workflow run on `windows-11-arm`,
which needs a push — and agents never push (D19). Marked `blocked` in the
CSV. No Rust or TypeScript source was touched, so no cargo or vitest gate
was re-run here; the drift gate and format check were.

```text
npx prettier --write <every md/yml touched>   → all formatted
pnpm format:check                              → All matched files use Prettier code style!
pwsh -NoProfile -File scripts/check-drift.ps1  → drift: 0 failure(s), 2 warning(s)
                                                 checked 11 ts_rs files, 52 invokes, 54 commands, 2 locales
                                                 (warnings pre-existing: export_flight_recording,
                                                  query_machine_history registered but not invoked)
rg -n "vitals-app" . --glob '!target'          → only the F17 finding above and the S10-11 tracker row
rg --files crates apps | rg "examples[\\/]"    → prove_store, prove_lan, prove_process_detail, serve_dev
                                                 all exist as named in CONTRIBUTING.md
```

### 2026-09-10 — S7-01/02/03/06 palette, shortcuts, transitions, motion (commit 45371d1) + toasts mounted

```
tsc 0 · eslint 0 · vitest 80 files / 821 desktop tests · drift 0 failures
check-size: initial 180,2 KB / 181,6 (99,3%) — sonner is a lazy 9,8 KB chunk at @vitals/ui/toast
live (CDP): Ctrl+K → 15 commands; 'proc' → Processes; Enter navigates; '?' → help sheet
live (CDP): 'slow' + Enter → verdict dialog (after deferring the event one frame — it was being
            dismissed inside the palette's own teardown; test updated to assert the deferral)
live (CDP): emit vitals://sampler-error → toast 'A reading failed … probe: GPU counters unavailable'
live (CDP): lastRoute=processes + reload → 'No readings are arriving' with the dashboard NOT mounted
            → third copy of the disposed-flag defect in useProcessSnapshot (b83ffb9); re-verified clean
```

### 2026-09-10 — S7-08/09/10/04 shared primitives, toasts, tests, ultrawide (commit 1a823a9)

**S7-08 — StatList promoted, not copied.** The label/value grid moved from
`features/performance/StatList.tsx` into `@vitals/ui`, and the perf file is
now a wrapper. The shared default renders an absent value as an em dash with
an accessible name; the perf wrapper passes `omitNull`, which drops the row
instead. Both behaviours are needed and the choice is the caller's: a panel
with a dozen optional sensors would otherwise show a column of dashes that
reads as a failure to load, while a four-row card that silently loses a row
reads as a missing feature. `Skeleton` was already shared and already tested;
the remaining duplication is nine per-feature `*Skeleton()` compositions,
which are page-specific shapes rather than a repeated primitive — the exact
replacements for the three genuine duplicates are listed in the handoff.

**S7-09 — toasts exist but nothing mounts them yet.** No `<Toaster>` was
mounted anywhere in the repo, so `sonner` was a dependency that shipped
nothing. `@vitals/ui` now exports one, dressed entirely in `var(--color-*)`
with `richColors` off: the theme has three independent axes (mode, accent,
surface translucency) and a literal colour opts out of all three. It reads
the resolved mode from the `.dark` class on the root rather than sonner's
`theme="system"`, because a user can pin dark on a light system and the
media query would leave the toast the only light surface on the screen.
Both user-visible strings are props — the package has no i18n runtime, and
the overlay window never initialises one — so a Romanian UI does not get
"Notifications" and "Close toast". `AppShell.tsx` is another agent's; the
one-line mount is in the handoff.

**S7-10 — canvas tests assert the guarantee, not pixels.** happy-dom returns
`null` from `getContext('2d')`, so a naive chart test passes while every draw
is skipped. `test/canvas-mock.ts` records the path calls instead, which lets
the tests state what the component promises: it repaints when `revision`
changes and _not_ when the parent re-renders with the same revision (the
whole reason the prop exists — the buffers are mutable and live outside
React, so object identity means nothing), a `pushGap()` produces two separate
paths rather than one line dropping to the axis, and a fixed scale is
honoured rather than autoscaled.

**S7-04 — infrastructure, since the grids are not ours.** `--breakpoint-3xl/
4xl/5xl` at 120/160/240rem, matching `--container-*` so the same names work as
`@3xl/main:` container variants, and `@container/main` on the content wrapper
so a feature can respond to the width it actually gets rather than the window
(they differ by the sidebar). `--content-max` was 1600px, which capped a 4K
window at under half its width; it is now 240rem for grids, with a separate
`--reading-max: 90rem` for prose. The shared `StatList` gains a column at 3xl
and another at 4xl. The dashboard grid is another agent's.

**Mutation check — seven mutants, all red.** `StatList` null-as-dash and
`omitNull`; `Toaster` translated region label and resolved-mode following;
`TimeSeriesChart` redrawing on every render (`}, [draw, revision])` → `})`);
the renderer bridging gaps (`isGap = false`); and `--breakpoint-3xl` set to
100rem. Each broke its test and was restored; `rg` confirmed the sources are
unchanged afterwards.

```text
pnpm typecheck                → 6 successful, 6 total
pnpm test                     → 6 successful; desktop 815, ui 97, charts 34
npx eslint .                  → 1 pre-existing error in packages/protocol
                                (select.test.ts:7, committed, not ours)
pnpm format:check             → All matched files use Prettier code style!
scripts/check-drift.ps1       → 0 failures, 3 pre-existing warnings, 2 locales
scripts/check-size.ps1        → within budget: initial 179.8/181.6 KB (99.0%),
                                all assets 346.5/350.4 KB (98.9%)
rg -o "@container main \([^)]*\)" dist/assets/index-*.css
                              → @container main (width>=120rem)
rg -o "@media \([^)]*120rem\)" dist/assets/index-*.css
                              → @media (width>=120rem)
```

**Size delta.** Initial load 177.3 → 179.8 KB gz (+2.5), all assets 314.1 →
346.5 KB gz (+32.4). Only ~0.2 KB of that is the stylesheet growth from the
ultrawide utilities; `sonner` itself does not appear in any chunk yet,
because nothing imports `Toaster` until the shell mounts it — the rest of the
delta is another agent's concurrent work in the same tree. Both figures are
inside the budget, but the initial-load headroom is now 1.8 KB: mounting the
Toaster will consume part of it, so measure again after the mount lands.

### 2026-09-10 — S7-05 export + S7-07 search with URL state (commit 50c1d01)

**Scope decision.** "Every table" was read literally: a `<table>` element.
In scope: Processes, Startup, Services, App history, Network (flat socket
export of the grouped view), Storage scan results, Installed apps, and the
sensor readings table on Devices. Out of scope, with the reason: Users
(card list, no table; kept its existing `Input` search untouched),
Benchmarks (result cards, no tabular rows), Performance (charts). Dashboard,
Diagnosis and Alerts belong to another agent.

**Export writes raw values.** `ExportColumn.value` returns the underlying
number (`0.0723`, bytes, seconds, epoch as ISO), never the formatted cell;
`null` is an empty CSV cell and a JSON `null`, never `0` and never the em
dash. CSV is RFC 4180 with a UTF-8 BOM (Excel reads a BOM-less UTF-8 file
as ANSI and mangles every Romanian diacritic) and CRLF. The header row is
the translated label; the JSON keys are stable ids, so a program reading
the file does not break when the user switches locale.

**Save path.** A Blob + anchor download. `dialog:allow-save` is granted so
the picker would open today, but no Rust command exists to write arbitrary
content to the chosen path and `src-tauri/**` is another agent's. The exact
change needed is documented on `saveExport()` in `lib/export.ts`; the
preferred option adds no capability string at all (a `write_export`
command, like `write_flight_recording`).

**URL state.** `#<route>?q=…&sort=…`, `history.replaceState` only, 150 ms
debounce, inert (no fragment) for a default view, parsers reject anything
outside the screen's closed union. Processes' sort/direction/kind already
persist in `localStorage`; the URL mirrors them and wins when a link names
them. A pending write is flushed on unmount so `<Activity>` hiding the
screen 50 ms after a keystroke loses nothing.

**Mutation check.** Removing the `""` doubling in `csvField` turned two
tests red (`csvField … doubles embedded quotes`, `toCsv … raw values`);
restored.

```
pnpm -C apps/desktop exec tsc --noEmit          → exit 0
pnpm -C apps/desktop exec eslint src            → exit 0
pnpm -C apps/desktop exec vitest run            → Test Files 78 passed · Tests 812 passed (812)
pwsh scripts/check-drift.ps1                    → 0 failures, 3 pre-existing warnings, 2 locales
pnpm -C apps/desktop build:vite                 → export-SqT0Y6kl.js 0.85 kB │ gzip 0.54 kB (lazy chunk)
pwsh scripts/check-size.ps1 -SkipInstaller
  initial load (gzip)  179,6 KB of 181,6 KB budget (98,9%)   baseline 177,3 KB
  all assets  (gzip)   346,1 KB of 350,4 KB budget (98,8%)   baseline 314,1 KB (S7-01 Motion chunk landed between)
  Within budget.
```

### 2026-09-10 — S11 HUD overlay window (commit f8505d5)

### 2026-09-10 — S9 Windows sampler: disk counters, efficiency mode, handles, modules (commit 7f43315)

**S9-01 finding that changed the design.** `SystemFullProcessInformation`
(class 148, the one carrying `PROCESS_DISK_COUNTERS`) is not a build question
but a privilege one: measured on this machine it returns
`STATUS_ACCESS_DENIED` (0xC0000022) unelevated while classes 5 and 57
succeed. Task Manager can show its Disk column because it holds
`SeDebugPrivilege`. The enumerator therefore tries 148 first and falls back
to 5 **permanently** for the run (a mid-run switch would emit one huge bogus
delta), and `RawProcess::storage_read_bytes` is `Option<u64>` so the
fallback is visible, never a zero. Elevated runs get the honest figure.

**S9-04 hang mitigation.** Names are read on a dedicated worker thread with
a 750 ms budget; on expiry the thread is abandoned, not terminated —
`TerminateThread` on a thread wedged in a driver leaks its stack and can
corrupt the loader lock. The worker owns a `DuplicateHandle`d process
handle so it can outlive the caller safely.

```
cargo run -q -p vitals-win --example prove_process_detail
  == S9-01 == 856 processes · source: AllIo
    the full class was refused (STATUS_ACCESS_DENIED without SeDebugPrivilege)
  == S9-04 == 64 handles in 307 ms · 20 named
    14 File · 7 Event · 6 WaitCompletionPacket · 4 Key …
    58 File \Device\HarddiskVolume3\gh\remi · 4c Directory \KnownDlls
  == S9-05 == 8 modules: prove_process_detail.exe, ntdll.dll 2460 KiB, KERNEL32.DLL …
  == S9-02 == before Some(false) · set on Some(true) · cleared Some(false) · System (pid 4): None
cargo test -p vitals-win -p vitals-core        → 564 + 112 + 6 + 4 + 1 passed, 0 failed
cargo clippy --workspace --all-targets -- -D warnings → exit 0
cargo test -p vitals-core --features ts        → HandleInfo, ModuleInfo, ProcessDetail.efficiencyMode in index.ts
pwsh scripts/check-drift.ps1                   → 0 failures (3 pre-existing warnings, other agents' commands)
New adversarial tests: exited PID → Err (handles, modules, efficiency); recycled
start time → NotFound; PID 4 → None/Err, never an empty list or Some(false);
extension reader refuses to read past NextEntryOffset; naming completes < 4×budget.
```

Not done here (out of my files): Tauri commands for `efficiency_mode` /
`set_efficiency_mode` / `handles::for_process` / `modules::for_process`, the
LAN control route, the SDK methods, the UI, both locales, and `ProcessDetail`
producers filling `efficiency_mode` (it defaults to `None`, which is correct
until they do).

### 2026-09-10 — S5 CLI + loopback API (commits 9570564, c214938)

Decision (askQuestions): the CLI attaches over **loopback HTTP**, not a named
pipe. One wire format for phone, CLI and scripts; nothing new in itals-ipc.

```
cargo test --workspace                     → 872 passed, 0 failed
cargo clippy --workspace --all-targets -- -D warnings → 0
cargo test -p vitals-cli                   → 28 passed
cargo run -q -p vitals-cli -- ps --top 4   → source: sampling directly; 4 rows, em dash for unmeasured
(subagent, attached) vitals info / ps --json (796 processes) / top --json / report --duration 3
(subagent) vitals serve --port 7351 --token devtok → /snapshot 401 without token, keyframe with it
Mutation checks: loopback grant peer.filter→peer.map turns 2 tests red;
fold.rs exits/changes order swap fails the recycled-PID test.
```

### 2026-09-10 — S8 shell: tray, alerts, HUD, updater, diagnosis (commits 6a4b4f6 … 8e293b7)

Live verification drove the real WebView2 over CDP
(`WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9333`), not
the browser — the browser has no Tauri host and cannot exercise `invoke`.

```
pnpm -C apps/desktop exec vitest run      → Test Files 71 passed, Tests 755 passed
cargo test --workspace                    → 822 passed (before diagnosis.rs), +11 in vitals-core
cargo clippy --workspace --all-targets -- -D warnings → clippy=0
pwsh scripts/check-drift.ps1              → 0 failure(s), 3 warning(s) — 44 invokes, 47 commands
node .copilot-tmp/drive-diagnosis.mjs (live webview):
  button disabled? false
  dialog text: "Why is my PC slow? … Nothing is holding your computer back. …"
  copied feedback shown / dialog closed on Escape / console errors: (none)
```

Found by looking, not by tests: the dashboard showed **"No readings are
arriving"** on a machine that `get_alerts` and a raw `listen('vitals://frame')`
proved was being sampled (4 frames in 3.5 s). Root cause: both Tauri sources
set `disposed = true` on the last unsubscribe and never reset it, so the
remount `<Activity>`/StrictMode performs tore the new listener down on
arrival. Fixed in 9e65797 with two regression tests; both go red with the
one-line reset removed (verified by mutation).

Clipboard: `navigator.clipboard.writeText` rejects with `NotAllowedError:
Document is not focused` when the window is driven from outside. The Copy
button now catches the rejection instead of leaving a dangling promise.

Not verified live in this session: a verdict _with_ contributors. Synthetic
CPU load from PowerShell jobs raised `cpuSustained` once (15-sample sustain
observed at 15:03:57) but did not do so reliably enough to drive the dialog
at the same instant; the contributor path is covered by `diagnosis.rs`
tests and `DiagnosisDialog.test.tsx` with fixtures.

An always-on-top, transparent, chromeless second window showing CPU, memory
and GPU with 40 px traces, a hover-revealed toolbar (pin, click-through,
close) and `Ctrl+Shift+H`. Its own capability file grants six permissions and
nothing else — no store, no filesystem, no process control.

- `pnpm typecheck` → exit 0 (6 tasks successful).
- `npx eslint apps/desktop/src` → exit 0, no output.
- `vitest run src/hud src/settings src/lib/useHud.test.ts` → **10 files,
  58 tests passed**.
- `cargo clippy -p vitals-desktop --all-targets -- -D warnings` → exit 0.
- `scripts/check-drift.ps1` → **0 failures**, 3 warnings, all pre-existing
  (`export_flight_recording`, `get_capabilities`, `query_machine_history`).
  Both new commands are registered _and_ invoked, so neither is warned about.
- Bundle: `hud.html`'s transitive module graph is **114.4 kB gzipped**, of
  which the shared React chunk is 68.3 kB — **46.1 kB for the overlay**,
  inside the 60 kB budget. Rolldown reports the entry chunk itself at
  3.02 kB gz; the rest is the shared `cn`/`@vitals/ui` chunk (31.8 kB) and
  the Tailwind stylesheet (9.0 kB).
- **Not verified**: nothing that needs a display. Whether Windows actually
  composites the transparent window without a decorated frame, whether
  `skip_taskbar` keeps it out of Alt+Tab, whether `startDragging` moves it
  from a pointer-down anywhere on the panel, and whether
  `setIgnoreCursorEvents(true)` really passes clicks through are all
  compositor behaviours. Tests prove the _calls_ are made with the right
  arguments; they cannot prove the window manager honours them.
- `scripts/check-size.ps1` reports "all assets" over budget (311.9 kB of
  273.1 kB). That build contained three agents' concurrent work — a tray, an
  alerts feature and this overlay — so the overage is not attributable here
  and the budget must be re-baselined once the tree settles, deliberately.

### 2026-09-10 — S6 mobile app, mDNS, SDK, gates (commits cac19cd … 2861e6f)

- `pnpm test` → 65 desktop files, 712 tests; `@vitals/client` 30 tests.
- `cargo test --workspace` → **783 passed, 0 failed**.
- `cargo clippy --workspace --all-targets -- -D warnings` → exit 0.
- `pnpm typecheck` · `eslint .` · `format:check` → clean.
- `scripts/check-drift.ps1` → 0 failures, 3 warnings (all genuine:
  `export_flight_recording`, `get_capabilities`, `query_machine_history` are
  registered and nothing invokes them).
- `scripts/verify.ps1 -SkipBuild -SkipPerf` → all 9 gates pass.
- Mobile bundle: **126 kB gzipped** including React (budget 150). React is
  67 kB of it; the next 31 kB is a shared `@vitals/ui` chunk carrying Radix
  internals the phone barely uses — splitting it is a follow-up in `ui`.
- **Live session against the real server** (`serve_dev` on :7332, real
  sampler): paired via fragment, URL stripped, card showed 70 % CPU /
  118 GB of 192 GB, processes tab listed 723 rows sorted by CPU, action
  sheet opened with the two-tap confirm.
- **Two product defects found only by looking at the phone**, both fixed at
  the source so every consumer benefits:
  1. `PID 0 · 43 %` at the top of the process list. The System Idle Process
     shipped in every frame; `is_idle_process()` existed but only a test
     called it. Filtered in `frame.rs`; regression test asserts the sampler
     still sees it and the frame does not.
  2. `GPU 0 %` for two phantom adapters beside a real GPU at 16 %.
     `utilization` was non-optional with `.unwrap_or(ZERO)` — the one
     principle broken at the type level. Now `Option<Percent>`; the compiler
     walked eight files. `ResourceEntry.utilization` had the same lie for
     network adapters and is nullable too.
- Mutation checks recorded: the mDNS lifecycle test was decorative twice
  before it was real (a browse after shutdown returns nothing whether or not
  the responder was torn down); the confirm-step test fails if the first tap
  calls `control()`; `check-drift.ps1` fails on kebab-case `ProcessKey` and
  on a misspelled `invoke`.
- Decisions: ask-questions round confirmed control-from-phone with two-tap
  confirm, several PCs side by side, mDNS only while the server runs, and
  SDK → OpenAPI → Home Assistant as the third-party order.
- Deferred, with reason: S4-11 named-pipe IPC belongs with the CLI slice
  (S5) that consumes it; S6-05 alerts feed mirrors an alert engine (S8) that
  is not built yet; pull-to-refresh skipped — data is live at 1 Hz.

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
