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
| D30 | Visual redesign | Fluent depth + glow (not glassmorphism, not monochrome); Motion permitted and the size budget raised to measured — asked 2026-09-27, ADR 0030                                          |

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
| S11   | Task Manager replacement, HUD overlay                                | done                                                                                               |
| S12   | Look-and-feel redesign + the four backend truths it exposed          | done (S12-11 measured in CPU cycles: 23–24 ms on S12, 22–25 ms on the commit before it)            |
| S14   | Storage: fast complete scans, navigation, cleanup, Turbo, extras     | doing (01 engine, 02 explore, 03 recycle basket, 04 Windows cleanup done; 05 Turbo next)           |
| S15   | Android phone, Wear OS watch, TV: native apps + on-device monitor    | doing (01-07 done; Google TV, Tizen open)                                                          |

Per-item status lives in `tracker.csv`. This file records the reasoning; the
CSV records the state.

---

## Verification log

### 2026-09-29 — S14-04 Windows-managed space, freed by Windows' own tools

**Ask.** Slice 4 of S14: from the "Reclaimable space" card, free the space
Windows manages with the tool Windows provides — Disk Cleanup, DISM component
cleanup, `powercfg /h off` — elevated once for that action only, with
progress and the space actually freed, measured before and after. Nothing is
removed by a Vitals file delete; the irreversible ones need their own
confirmation stating the consequence.

**What was built.**

- _One tool per location_ (`storage/managed.rs`). `tool_for(kind, path)`
  maps system temp, the Windows Update cache, Delivery Optimisation, the
  thumbnail cache, the bin, `Windows.old`, minidumps and `MEMORY.DMP` to
  one Disk Cleanup handler each; the new `ComponentStore` candidate
  (`WinSxS`) to `DISM /Online /Cleanup-Image /StartComponentCleanup`
  (no `/ResetBase`, so installed updates stay uninstallable); the
  hibernation file to `powercfg /hibernate off`. Your own temp, browser and
  package caches and live kernel reports have no Windows tool and keep the
  disabled button that points to the basket. `WinSxS` is never walked for a
  size: most of it is hard links into `System32`, so it shows "Not measured".
- _Elevated once, for that action_. The app re-launches itself under
  `runas` with `--elevated-storage-cleanup <tool> <consent>`
  (`LaunchMode::ElevatedStorageCleanup`, never a window). The child accepts
  only a closed list of tools — a crafted `cleanmgr.downloadsFolder` (the
  handler that empties Downloads) is refused — resolves each program from
  `%SystemRoot%\System32`, never `PATH`, and refuses a risky tool without
  `confirmed`. Disk Cleanup runs `/sagerun:7331` with exactly one handler's
  `StateFlags7331` set, cleared again afterwards (and every handler's cleared
  first, in case an earlier run died). The tool's exit code passes through;
  Vitals' own failures use `0xE5C0_000x`; 3010 is "restart to finish".
- _Progress while it runs_. `spawn_program_elevated` returns once the UAC
  prompt is answered, and the command polls the child every 500 ms, emitting
  `vitals://storage/cleanup-progress` with the stage (measuring, approval,
  running, remeasuring), the elapsed time and the free space the drive has
  gained so far.
- _Freed is measured_. The location and the drive's free space are read
  before the prompt and after the tool exits; the report carries all four
  figures. The headline is what the location lost; when it did not shrink or
  could not be read (the component store), the drive's gain, labelled as that.
- _The command names a location, never a tool_. `run_windows_cleanup`
  looks the path up in the candidate list; a path the list did not produce
  (`C:\Users`, your temp folder) is refused before anything runs.
  Desktop-only, like recycling.
- _UI_ (`WindowsCleanup.tsx`). "Clean up…" / "Turn off…" on each
  Windows-managed row opens one dialog: what Windows will do, that approval is
  asked once for this action, then the stage and elapsed time. The bin,
  `Windows.old` and hibernation state their consequence and need "I
  understand this cannot be undone" ticked before the (danger) button works.
  The report shows the freed figure, before/after for the location, the drive
  change, and "measured, not estimated"; the row's size is updated from the
  after-measurement. A declined prompt says nothing was cleaned and is titled
  so.

**Found by looking at it** (probe app, CDP, user approving each prompt):

- _A declined prompt was titled "what Windows freed"._ Caught by the vitest
  for a refused elevation; now "nothing was run".
- _`powercfg /h off` exits 0 and frees nothing here._ Hibernation on this
  machine is already off ("An internal system component has disabled
  hibernation", `HibernateEnabled` 0), yet `C:\hiberfil.sys` is
  76.7 GB. powercfg reports success in about a second and the file
  stays. The report read "Done · 0 B", which looks like success; it now says
  "Nothing changed" and that Windows decides what it removes, and Vitals
  removes nothing in its place.

**Live, this machine:** unconfirmed hibernation over IPC → `refused`, no
prompt; `C:\Users` → `refused`. Thumbnail cache through the UI, prompt
approved: 147 MB → 3.14 MB in 258 s
(independently `Get-ChildItem` sum 3,235,509 bytes after), row updated
to 3.14 MB, no `StateFlags7331` left in the registry. The drive line
read "1.36 GB less free space" because other agents' builds were writing to C:
meanwhile, which is why the location figure is the headline. Hibernation,
ticked and approved: "Nothing changed", 76.7 GB before and after.

**Tests.** `vitals-win` storage 126 (managed 13) (managed: arguments round
trip for all ten tools; a handler outside the list and malformed arguments are
BAD_ARGS; every risky tool unconfirmed is REFUSED in the child; exactly the
bin, `Windows.old` and hibernation are risky; every Windows-managed kind has
a tool and app caches do not, live kernel reports have none; an unconfirmed
risky tool never reaches the launcher; a declined prompt stops after
Measuring, Approval; freed is the before/after of a real folder a fake tool
shrank; a tool that frees nothing reports 0; exit-code mapping incl. 3010 and
an HRESULT; tools resolve from System32; the component store is never sized).
`vitals-desktop` 66 (the command refuses a location the
candidate list did not produce; the elevated child never starts a window).
Vitest storage 114 (one confirmation then the tool for that path only;
measured report and the row updated; stage and freed-so-far while running,
Cancel disabled; a declined prompt changes nothing; Cancel runs nothing; an
irreversible tool states its consequence and is disabled until ticked, then
sends `confirmed`; drive figure labelled when the location is unreadable,
restart note; a tool failure shows its code; "Nothing changed"; Romanian
dialog; every tool, stage, outcome and consequence has a string in both
locales). Mutation: removing the elevated child's risky-needs-confirmation check turns the_child_refuses_a_risky_tool_that_was_not_confirmed red (cleanmgr.recycleBin unconfirmed ran), restored 13/13. `verify.ps1 -SkipPerf`: 10 of 10 gates, 1,213 Rust tests (two earlier runs failed only S14-03's Restart Manager holder tests with RmGetList error 5 under concurrent load; base 2e7941e and this tree both pass them alone).

**Not in this slice:** `Windows.old`, the bin, minidumps and Delivery
Optimisation were not run live (absent or empty here); their handlers are the
same code path as the thumbnail cache. The LAN API, CLI, SDK and Home
Assistant are not touched: freeing space is desktop-only on purpose.

### 2026-09-29 — S14-03 A review basket that sends items to the Recycle Bin, and "why can't I delete this"

**Ask.** Slice 3 of S14. Decided with the user before the slice: items are
reviewed in a basket and sent to the Recycle Bin through Windows' own file
operation (`IFileOperation`), never deleted outright; Restart Manager names
the program holding a locked file; Windows-managed space (DISM, cleanmgr,
powercfg) is S14-04.

**What was built.**

- _Protected paths, checked in Rust_ (`storage/protect.rs`). Every path the
  recycle command receives is checked again, whatever the UI sent: drive
  roots; Windows, Program Files (both), ProgramData, boot, recovery, the bin,
  `Windows.old` and the other drive-level system folders on every drive, with
  everything under them; the folders a profile is made of (`C:\Users`, each
  profile, Desktop, Documents, `AppData\Local`...) themselves but not their
  contents; anything _above_ a protected folder (a relocated Documents on
  `D:\Data\Documents` protects `D:\Data`); files with the System attribute;
  drives with no bin (removable, network, UNC); and Vitals' own folder.
  Locations come from the known-folder API, so a moved Documents is still
  found. Paths are checked as written _and_ as they resolve (short names,
  junctions). Ambiguous spellings are refused rather than guessed: `C:foo`,
  `..`, a stream name, wildcards, a trailing dot or space.
- _Recycle_ (`storage/recycle.rs`). One `IFileOperation` per item on its own
  STA thread (Tauri's pool is MTA), `FOFX_RECYCLEONDELETE` with the shell's
  prompts off, so the app's dialog is the one confirmation. A progress sink
  sees each item before it goes: `PreDeleteItem` without
  `TSF_DELETE_RECYCLE_IF_POSSIBLE` means the shell was about to delete it for
  good (too big for the bin, no bin), and the sink aborts, so the item stays
  and is reported `wouldBePermanent`. After the engine says yes, the path is
  checked on disk before `recycled` is reported. No `std::fs::remove_*` in
  the code path.
- _Why it is locked._ A sharing violation runs Restart Manager (`RmGetList`)
  on the file, or on up to 1,000 files inside a folder, and the report names
  each program with its process ID and service name.
- _The kept scan follows._ `ScanResult::forget_recycled` detaches a
  recycled folder (every ancestor total down by exactly what it held, the
  subtree flagged so no listing, ranking or re-aggregation brings it back;
  `Node` size still 88 bytes) or removes a listed large file, and adds the
  bytes to the drive's `$Recycle.Bin` when the scan contains it, because the
  space is still used until the bin is emptied. The command returns the new
  totals, largest folders and files, and the explorer reloads the folder in
  view; no rescan.
- _UI_ (`Basket.tsx`). A + toggle on every folder row, largest-folders row
  and largest-files row, a context-menu item on each, and right-click on the
  map for the folder under the pointer; adding a folder absorbs what is
  already in the basket below it, so nothing counts twice. A strip under the
  explorer shows the count and total; Review opens the list with each item's
  size and a remove button; the confirm button says "Recycle N items · size".
  The report lists each item's outcome with the rule that refused it, the
  holders of a locked one, or why it did not move; what did not move stays in
  the basket. The confirm is `primary`, not `danger`: in this app danger
  means irreversible. Desktop only; a paired phone gets no route.

**Found by looking at it** (probe app, CDP, a test folder on `D:` with one
file held open by another process without share-delete):

- _A folder with an open file was a bare failure._ The shell reports it as
  `COPYENGINE_E_SHARING_VIOLATION_DEST` (0x80270028), not `_SRC`, so the
  report said "error -2144927704" and named nobody. Both are now sharing
  violations; a test holds a file inside a folder and asserts the holder.
  Live after the fix: "In use · PowerShell 7 · process 82852".
- _"0 items are in the Recycle Bin; 1 could not be moved and stayed where
  they were."_ A report where nothing moved now says so in its own sentence.

**Live, this machine:** a direct IPC call with
`C:\Windows\System32\drivers\etc\hosts` and `C:\` returns
`refused:systemFolder`, `refused:driveRoot`. `vitals-s14-test` 32.0 MB /
4 files: junk (16 MB) recycled, held reported in use with its holder and left
in the basket; after the holder exited, held recycled; the folder row read
8.00 MB / 1 file without a rescan; the Shell's bin namespace lists `junk` and
`held` with original location `D:\vitals-s14-test`.

**Tests.** `vitals-win` storage 113 (protect: system folders and below,
drive roots in every spelling, profile folders vs contents, the folder above
a relocated one, drive-level folders on other drives, prefix is not parent,
ambiguous spellings, this machine's Windows and profile, short name; recycle:
a system path refused and left in place, a file and a folder go to the bin,
missing, a held file and a folder with a held file are left with the holder
named, Restart Manager on a folder; tree: detach lowers every ancestor by
exactly its bytes, a detached subtree never comes back, unlink anywhere in
the sibling chain, bytes moved into the bin keep the drive total, a removed
file; scan: forget a folder and a large file without a rescan).
`vitals-desktop` 64 (the command refuses a system path whatever the UI sent;
empty and oversized baskets refused). Vitest storage 98 (add from list,
largest files, largest folders and map; a folder absorbs its children; one
confirmation, per-item report with holder and refusal reason; cancel recycles
nothing; new totals applied and the folder re-read without a rescan; the
scanned drive is not offered; every outcome and rule has a string; the
confirmation never says delete). Mutation: letting a refused path through as
`Missing` turns the system-path test red. `verify.ps1 -SkipPerf`: 10 of 10
gates, 1,185 Rust tests; drift 0 (75/75).

**Not in this slice:** Windows-managed cleanup (DISM, cleanmgr, powercfg) is
S14-04; the cleanup catalogue's Delete button stays disabled, and its tooltip
now points to the basket. The LAN API, CLI, SDK and Home Assistant are not
touched: recycling is desktop-only on purpose.

### 2026-09-29 — S14-02 Explore a finished scan: map, breadcrumb, list, largest files

**Ask.** Slice 2 of S14: navigate the scan instead of reading a flat list.
Decided with the user: the tree stays in Rust; icicle by default, treemap
optional, a virtual list; Canvas 2D with the layout and level of detail
computed in Rust (at most about 5,000 rectangles). Two more choices this
slice (askQuestions): keep the tree until the next scan and release it after
fifteen idle minutes; keep the 1,000 largest files.

**What was built.**

- _The tree is kept._ `scan_storage` stores the `ScanResult` (released
  before the next walk starts, so two trees never coexist, and by a timer
  after 15 minutes without a navigation request) and returns a `scanId`.
  `get_storage_children(scanId, node)` answers the breadcrumb and the
  subfolders largest first; `get_storage_map(scanId, node, shape, aspect)`
  answers laid-out cells. A stale `scanId` is `not-found`, which the UI
  explains ("no longer in memory") instead of showing an error.
- _Layout in Rust_ (`storage/layout.rs`): an icicle and a squarified treemap
  (Bruls et al.), each with a folder's own files as one block and folders too
  small to see folded into "N smaller folders" rather than dropped, capped at
  5,000 cells, breadth-first so the cap drops the deepest detail first.
- _Largest files._ Files have no tree nodes, so the scan keeps a bounded
  min-heap of the 1,000 largest by name. Workers copy a name only when the
  file is at least the heap's current floor (an `AtomicU64` the merging
  thread raises), so millions of small files never allocate a string. A
  repeat hard link is never listed. Ties resolve by size, logical size, name,
  so the list does not depend on which worker read what (tested: 1 vs 8
  threads identical).
- _UI_ (`Explorer.tsx`, `mapDraw.ts`): five views (Layers, Blocks, List,
  Largest folders, Largest files); a breadcrumb with Up; a canvas map with a
  hover line (name, size, share of the folder), an eased zoom onto the clicked
  block (skipped under reduced motion), colours from the theme's chart tokens;
  a virtualised folder list with share bars and a right-click menu (open,
  show in File Explorer, copy path). The canvas is `role="img"`; every action
  is also on the list, which is real buttons.

**Found by looking at it** (debug build under its own identifier, CDP):

- _Scanning "C:" walked the wrong folder._ The drive list reports `C:`
  without a backslash, which Windows reads as the current directory on C, so
  the Scan button reported eight files as the whole drive. Fixed at the
  source: `scan_directory` normalises a bare drive letter to its root, for
  every caller; tested against the real system drive (its root holds
  `Windows`).
- _The map received no clicks._ A fixed-height map overflowed its card and
  the Clean-up card covered it. The map and list now share the card by flex,
  and stacked the explorer gets three shares to Clean-up's one.
- _Clicking a leaf in the icicle did nothing._ In an icicle the parent is in
  the row above, not under the pointer; `openTarget` finds the openable
  folder spanning the column (icicle) or containing the rectangle (treemap).

**Measured** (this machine, unelevated):

| What                            | Figure                                                            |
| ------------------------------- | ----------------------------------------------------------------- |
| `C:\Program Files` layout, root | icicle 408 cells 0.31 ms, treemap 2,465 cells 0.84 ms (Rust)      |
| same, over IPC in the live app  | icicle 10 ms / 74 KB, treemap 35 ms / 451 KB                      |
| largest-files cost, A/B x2      | 3,636 / 3,597 ms with vs 3,476 / 4,282 ms without: noise          |
| whole `D:` through the UI       | 203,844 files, 767 GB of 770 GB used, 2.7 s                       |
| storage chunk (gzip)            | 16.6 KB; all assets 405.4 of 417.7 KB, initial 191.5 KB unchanged |

**Tests.** `vitals-win` storage 90/90 (layout: widths follow size, folded
bytes still in the row total, hard cap, squarify tiles without overlap and
under 3:1, children inside parents; largest files ordered, bounded, deep file
found, off when 0, one name per hard link, thread-count independent; bare
drive letter). Desktop vitest storage 82/82 (navigation, breadcrumb up, map shape
requested, released scan explained, largest files reveal, map geometry and
click targets). Mutation: removing `onOpen` from a list row turns "goes into a
folder and back up" red. `verify.ps1 -SkipPerf` all gates; drift 0 (73/73).

**Not in this slice** (their own slices): delete/recycle (S14-03), cleanup
catalogue (S14-04), MFT Turbo and the saved index (S14-05), duplicates, types
and old files (S14-06).

### 2026-09-29 — S14-01 A storage scan that counts everything, in seconds

**Ask.** Storage was incomplete and slow; the user wants ultra-fast scans,
navigation, smart cleanup and a modern UI, decided together (two question
rounds, 2026-09-28). Slice 1 of S14 is the engine and the correctness it was
missing. Navigation, cleanup, Turbo (MFT) and the extras are S14-02..07.

**Measured before changing anything** (release build, unelevated, same
machine, other agents' builds keeping it busy):

| Root               | Old walker                        | New engine                           |
| ------------------ | --------------------------------- | ------------------------------------ |
| `C:\Program Files` | 581,445 files, 474.7 s, 1,225 f/s | 581,447 files, 5.9 s, 98,600 f/s     |
| all of `C:`        | 340 f/s, unfinished after 17 min  | 8,066,836 files, 421.6 s, 19,134 f/s |

Same logical total on `Program Files` to within 5 KB (232,744,681,171 vs
232,744,676,177 bytes; two files written in between), same 17 skipped
folders, same 103 hard links suppressed.

The full `C:` scan: 1,866,492 folders, 2.71 TB allocated of the 2.88 TB the
volume reports used, 197,580 hard links suppressed (24.83 GB not counted
twice), 228 MB of tree in memory.

**What was wrong, and the fix at the source.**

- _The default scan dropped data._ "Quick (3 levels)" was the default and
  every byte below level three was missing from every total. The depth
  option is gone: a full scan is now faster than the old preview.
- _One thread, three system calls per file._ `FindFirstFileExW`, plus
  `CreateFileW` + `GetFileInformationByHandle` + `CloseHandle` on every file
  for hard-link identity, plus `GetCompressedFileSizeW` on compressed ones.
  Now `GetFileInformationByHandleEx(FileIdExtdDirectoryInfo)` with a 256 KB
  buffer returns hundreds of entries per call _with_ allocation size, file ID
  and reparse tag; no file is opened. Up to 16 scoped worker threads list
  directories; the calling thread alone owns the tree and merges each
  listing, so the tree needs no locks.
- _OneDrive was missing._ Every reparse-point folder was skipped, including
  cloud-provider folders, which hold this volume's own files. Cloud tags are
  now entered; junctions, symlinks and mount points are still recorded and
  never followed. A symlink to a file now occupies 0 (it used to be charged
  its target's size, possibly from another volume).
- _A stopped scan said "already visited"._ Unread folders were labelled
  `Cycle`. Now `Cancelled`, and `DepthLimit`/`Cycle` are gone.
- _Scan and cleanup shared one cancel flag._ "Stop the scan" also stopped
  the cleanup search, and starting a cleanup search cleared a pending stop.
  Each has its own token and an in-flight claim; a second concurrent request
  is refused instead of halving both.
- _No progress._ `scan_storage` emits `vitals://storage/scan-progress` ten
  times a second: files, bytes, rate and the folder being read. The bar is a
  real fraction of the drive's used space when the root is a drive.

- _Links were reported as unreadable folders._ The full `C:` run skipped
  97,293 reparse points; 95,160 were under
  `ProgramData\Microsoft\Windows\Containers\Layers`, container placeholders
  (tag `0x80000018`) that are not data on this volume. A link not followed
  is not a hole in the total, so `is_complete()` now counts only skips that
  leave one (`SkipReason::leaves_a_gap`): access denied, vanished, error,
  cancelled. Links are shown as their own line, "N links not followed". What
  remained on this machine unelevated: 961 access-denied folders.

**Hard links.** The listing gives the 128-bit file ID but not the link count,
so every ID is remembered for the scan's length (sharded, about 20 bytes a
file) to count a multiply-linked file once. The old per-file open cost more
than the whole new scan.

**Tests.** `vitals-win` storage 79/79 in 62 s. New: a private tree proves a
file five levels down is counted; 1 and 8 threads agree byte for byte; a hard
link counts once (and twice with detection off); a mid-scan cancel names the
unread folders; a junction back to its own parent (made with `mklink /J`) is
recorded and nothing behind it counted twice; progress carries the path. The
old junction test scanned the whole user profile: 270 s of the suite, gone.
Desktop vitest storage 71/71 (live progress, progress for another root
ignored, drive choice locked during a scan, no depth choice, cleanup has its
own stop and it does not stop a scan, links are not called unreadable).
`vitals-desktop` 60/60; clippy `-D warnings`; drift 0 (68 invokes, 68
commands).

### 2026-09-28 — S12-36 S12-31..35 land on main

**Ask.** The sensors, watchdog and devices work (S12-31..35) was four commits
in the shared clone that were never pushed, while `origin/main` moved 18
commits ahead with the 0.9.0-beta.1 release fixes. Put them on `origin/main`
without losing any of those fixes.

**How.** Cherry-picked b55b809, 7de0374, d64b701 and f7cdb8f onto 85d3800 in
a worktree (`E:\gh\.wt\vitals\s12-land`). The shared clone was not touched.
Conflicts were resolved in favour of origin:

- `tauri.conf.json` keeps `createUpdaterArtifacts` and gains the `sensors/`
  and `watchdog/` resources.
- `ci.yml` keeps origin's job split. The frontend job takes the single
  `turbo run typecheck lint test` step; origin's separate typecheck step
  above it was removed, because turbo would only replay it.
- `lib.rs` keeps `crashlog` and `updates::spawn` next to the new modules.
- `main.test.ts` keeps origin's React unmount and adds the local warm-up.

**Found on the way.**

- **Two ADRs numbered 0031.** Origin's public-release ADR and the sensors ADR
  both used 0031. The sensors ADR is now **0034**. 0033 is taken by an
  uncommitted Android ADR in the shared clone, which belongs to another
  session.
- **Release builds on macOS and Linux would have failed.** Those legs run the
  same `beforeBuildCommand`, and `bundle-sensors.ps1` and
  `bundle-watchdog.ps1` threw when the Windows `.exe` was missing. Both now
  exit 0 when not on Windows.
- **Two tracker rows were malformed.** The S12-34 and S12-35 titles had
  unquoted commas, which made 10 fields instead of 8.
- **`AppShell > opens settings` failed the full suite on unmodified origin/main
  (85d3800) at 87 % CPU**, so this was not the picks. Root cause: the lazy
  Motion runtime arrived after the click, the shell remounted and the open
  dialog was lost with the fallback. That is why no timeout ever helped.
  The test now warms `MotionRuntime` in `beforeAll`. Confirmed both ways:
  with the fix, origin/main 948/948 and this branch 985/985 at the same load.

**CI on f70442f (run 36455204409) was red in three jobs that no local gate covers:**

- Rust (linux): 11 dead-code errors. The `hardware` and `watchdog`
  commands are Windows-only, so off Windows their helpers had no caller.
  Both modules now carry `#![cfg_attr(not(windows), allow(dead_code))]`:
  the helpers stay compiled on Linux, where their tests still run. Checked
  in WSL Ubuntu 24.04 with CI's exact command (`cargo clippy --workspace
--lib --bins --tests --benches --exclude vitals-win -- -D warnings`,
  exit 0), and `cargo test` passed with the same selection.
- Supply chain: `THIRD_PARTY_NOTICES.md` was stale (`windows-service`
  0.8.1, brought in by vitals-sensors). Regenerated: 388 crates. `verify.ps1`
  now runs `third-party-notices.ps1 -Check`, so this is caught locally.
- Build installer: 4,744.2 KB against a 4,550.8 KB budget. The growth is
  intended: the installer now ships `vitals-sensors.exe` and
  `vitals-watchdog.exe` (636 KB staged). The budget is CI's measurement
  plus 5 %, which is what `check-size.ps1 -Update` writes: 5,100,964 bytes.

**Verification (2026-09-28).**

```text
verify.ps1: All 10 gates passed in 232.3s (304.5s if run one after another)
  1135 Rust tests passed; vitest desktop 96 files, 985 tests
  perf budget: full sample median 16.5 ms, worst 23.7 ms, budget 30 ms
check-drift: 0 failure(s), 0 warning(s) - 12 ts_rs files, 67 invokes, 67 commands, 2 locales
```

Every claim of "done" needs a command and its output. Recorded here as work
lands, newest first.

### 2026-09-29 — S15-04/05 The phone and the watch monitor themselves

**Ask.** The phone app is first a monitor for the phone, the PC a bonus; the
watch the same. Everything the desktop does, adapted to Android, not just
remote data. Official APIs only (decided with the owner).

**What.** New `:device` module (ADR-0035) behind a `DeviceMonitor` interface,
used by both apps. Phone opens on _This phone_ (Now, Battery, Storage, Apps,
Sensors, History, About); PCs moved to the second tab. Watch has a _This
watch_ card and screen. History and alerts run through WorkManager every 15
minutes.

**Found by looking at it** — every one invisible to the unit tests:

- Battery current: the first sampler read anything under 20 000 as mA, so the
  S25's µA trickle showed as thousands of mA. A second cutoff (10 000) failed at
  5 468 µA. Fixed by following the documented µA contract; magnitude now agrees
  with `dumpsys battery`, individual readings do not (see evidence).
- Samsung broadcasts `cycle_count:0` on a used battery → unreported, not "new".
- GPU clock: Mali on the A51 reports kHz (1 053 000), Adreno MHz (1200); the
  A51 showed "0.0 GHz" top speed.
- Two-cluster naming: the S25's 3.5 GHz Oryon cores were labelled
  "efficiency". A frequency ratio cannot separate the A51 (75 %) from the S25
  (79 %); a 2.2 GHz little-core cap can.
- Storage: `queryStatsForUser` includes shared storage, so "Apps 334 GB" on a
  287 GB used disk. Subtracting the external total gives 230 GB (Settings'
  diskstats: 202 GB). The overlap can make "System" negative → unknown.
- Sensors tab showed "0 sensors" for 20 s on the A51: one-shot reads waited
  600 ms each, in turn. Now parallel.
- Idle CPU: the Now tab kept one A51 core at 50–70 %. Three guesses measured
  and reverted (shorter tween alone, plain InfoRow alone, graphicsLayer on
  the mesh). Frame counting proved it: all live animation off = 10 frames per
  10 s, springs = ~200. Kept: 200 ms tweens read in the draw phase, plain
  numbers inside lists.

**Evidence.**

```text
:device unit tests      ParseTest 15/15, AlertRulesTest 6/6
lint (all modules)      0 errors
S25 held at 85 % on USB. Before the fix the app showed 2343 and -10156 'mA'.
  After, bracketed by dumpsys: dumpsys -4687 µA both sides | app 2.3, 2.3, 7.8 mA.
  Same magnitude (unit fixed); sign and value differ reading by reading, because
  dumpsys 'current now' is Samsung's broadcast field and the app reads
  BATTERY_PROPERTY_CURRENT_NOW, sampled separately. NOT a match: open question.
S25 Storage             287 GB of 477 GB, Apps 230 GB, Photos 3.7, Videos 10.0, Audio 34.2
S25 folder scan         138551 files, 90.8 GB; Download 41.9 GB, Sounds & Loops 21.7 GB
S25 Apps (24 h)         Microsoft Launcher 7h 8m 86 opens; TikTok 3h 1m 609 MB data
Watch 7                 CPU 50 %, 1.3 of 1.7 GB, 100 % 42 °C 4.36 V Full, 38 sensors
A51 release, Now tab    frames/10 s 190-216 -> 116-128; process CPU 497-690 -> 302-375 ticks/10 s
A51 background          0 ticks in 10 s
A51 cold start          536-1013 ms (first launch after install 2988 ms)
```

**Not done here.** Play listing and upload (S15-07), the release workflow run
(S15-06), Google TV (S15-08) and Tizen (S15-09).

### 2026-09-29 — S15-08 Vitals on Google TV

**What.** There is a new `:tv` module, built with Compose for TV (`tv-material` 1.1.0, no Leanback). It has five screens:

- **Overview:** this TV and every paired PC.
- **This TV:** Now, Storage, Apps, Sensors, History and About, all read through `:device`.
- **PCs:** each PC's Now, Programs (with end task, pause and resume, priority and efficiency mode), Sensors, History and About.
- **Add a PC:** a six-digit code, found by mDNS or typed as an address, with a pasted token as the fallback.
- **Settings:** paired PCs, history recording, special access, language and About.

All text is in English and Romanian. The stream, the history series, the pairing check and the chart canvases moved into `:core` and `:shared-ui`, and the phone uses them too (ADR-0036).

There is a new server route, `POST /api/v1/pair`: one code at a time, valid for five minutes, used once, burnt after five wrong guesses. Every refusal is the same `403`. It comes with a desktop "Pair a TV" panel, the CLI and `serve_dev` print a code, and it is in OpenAPI and docs/api.

**Found by the TV, fixed at the source.** The CPU load estimated from `time_in_state` read 64 % on the Chromecast while `/proc/stat` read 20 %, because its governor parks the cores at their _top_ frequency. `:device` now measures load from cpuidle residency, summed per cluster over five samples, because the kernel credits idle time only when a core wakes; one-second per-core deltas read 32–41 %. It falls back to the estimate only where cpuidle is hidden, and says which source it used, so the phone shows its "estimate" caption only when the load really is an estimate.

The D-pad walk also found three focus traps, all fixed:

- Choosing a drawer item left focus in the drawer.
- Opening a PC removed the focused card, so focus dropped back to the drawer.
- A single-line text field swallowed Down, so Next could not be reached.

**Verification.**

```
Chromecast with Google TV (sabrina, Android 14), adb 192.168.100.31:33807
  install tv-debug.apk: Success; banner + LEANBACK_LAUNCHER listed by katniss
  Overview: CPU 27-28 %   | /proc/stat same 5 s: 23.7-28.2 %
            Memory 64-66 % | /proc/meminfo: 63.5-65 % used (MemAvailable)
            Graphics —, CPU heat — (no GPU node; thermal zones denied to apps)
  This TV: 1.2 GB of 1.9 GB, swap 391/484 MB; Wi-Fi 5 GHz; "does not share its
           temperature sensors"; storage/apps show the usage-access card
  D-pad: drawer → Add a PC → address → Next → code 395 661 typed on number keys
         → paired (POST /api/v1/pair 200) → PC screen opened with focus on Now
  PC 192.168.100.61 (this machine): CPU 97-100 % (Windows utility 145 %, i.e.
         saturated), Memory 80 % (Windows 80.7 %), RTX 3060 Ti 12 %, 1096 programs
  Programs: → Code - Insiders.exe → panel → focus on End task
gradle :core/:device/:shared-ui tests: 44 passed; :app/:wear/:tv lintDebug clean;
  :tv:minifyReleaseWithR8 ok; allWarningsAsErrors on every module
mutation: idleBusy without the core count → idle_residency_on_the_chromecast FAILED
cargo test -p vitals-server: 61 + 1 + 43 + 4 + 3 passed; clippy -D warnings clean
vitest src/settings/lan: 12 passed; check-drift: 0 failures (79 invokes, 79 commands)
```

**Not verified yet.** The Play Android TV form factor and the `tv:qa` release are still open: they wait for the signed bundle from a build-only release run, and the `play` job's `tv:qa` upload only runs on the next `v*` tag. The phone and watch builds compile against the new shared code and their unit tests pass, but they have not been reinstalled on the A51 or the watch.

### 2026-09-29 — S15-07 Vitals is on Google Play internal testing

**What.** Every Play Console declaration, the store listing in English and
Romanian, phone and Wear OS screenshots, and the first internal releases of
both, from the bundles release run 36573016766 built. CI now uploads on `v*`
tags through Workload Identity Federation: no key exists, the provider
accepts only this repository's version tags, and the account may release to
testing tracks only. The API calls internal testing `qa`, so the watch track
is `wear:qa`; `wear:internal`, the obvious guess, is not a track.

**Verification.**

```
Play Console, Internal testing (phones):  8 (0.9.0-nightly.20260929.g2f45bd2)
  Available to internal testers · 1 version code · Released on Sep 29 7:16 PM
Play Console, Internal testing (Wear OS): 1000008 (0.9.0-nightly.20260929.g2f45bd2)
  Available to internal testers · Released on Sep 29 7:25 PM; Wear OS: Active
Content rating: ESRB Everyone, PEGI 3, IARC 3+
gcloud: provider github/vitals created; vitals-play bound to
  principalSet://.../attribute.repository/dragoscv/vitals
Play Users and permissions: "Invite sent" to vitals-play@hai-small-apps
```

**Not verified yet.** The `play` job has not run: it runs on the next `v*`
tag, and that run is its proof.

### 2026-09-29 — S15-06 The release workflow builds Android

**What.** A build-only release run (`nightly=false`) exposed three defects,
each fixed on main. `./gradlew: Permission denied`: the wrapper was committed
from Windows as 100644 (`589a973`). Every Windows leg since the sensors
helper failed because `version.ps1 -Set` changes the workspace crate versions
and `bundle-sensors.ps1` then runs `cargo build --locked` against the stale
lock (`d52ccd6` refreshes it after the stamp). That refresh used `--offline`,
which fails on a fresh runner with no registry index ("no matching package
named `serde`"). `2f45bd2` runs it online, and locally that changes zero
external dependency versions.

**Verification.**

```
gh run view 36573016766   (2f45bd2, workflow_dispatch nightly=false)
run: success
  success Plan / Build (android) / SBOM / Build (macos-universal)
  success Build (windows-arm64) / Build (windows-x64) / Build (linux-x64)
  skipped Verify, Publish and the package-manager legs (build-only run)
```

### 2026-09-28 — S12-37 Startup and Services row actions

**Ask.** Nothing on the Startup tab could be acted on; add the actions, on
right-click too. Then: a "Hide all Microsoft services" box, ticked by default.

**What.** Every row on Startup and Services has a right-click menu and a
visible row-actions button with the same items (one item list feeds both).
Startup: Enable / Disable. Services: Start / Stop / Restart and a Start type
submenu. Both: Open file location, Properties, Search online, Copy details.
A "Hide Microsoft entries/services" box, ticked by default, says how many it
hid; the export follows the screen.

**How it changes things.**

- Registry and folder entries: the `StartupApproved` record Task Manager
  writes (`02`/`03` + `FILETIME`), leaving the value where it is — so the two
  tools agree afterwards and "enable" has something to restore. Scheduled
  tasks: their own `Enabled` flag. Services: the SCM.
- Unelevated first; a denial becomes one UAC prompt running
  `--elevated-startup-action` for exactly that change. Names travel hex-encoded
  because task paths and value names carry quotes, commas and backslashes that
  the `ShellExecuteExW` command line would re-parse. The child re-checks risk,
  so the elevated pass cannot make a change the desktop would refuse.
- Turning off or stopping a `Degrades`/`SystemCritical` item needs the
  confirmation; turning one back on never does. `Forbidden` is not offered.

**Microsoft detection.** The image's `CompanyName`, which is what msconfig
uses — a claim, not a signature, so it is reported as `company` and never as
`publisher`. Found on this machine: `rundll32.exe`, `svchost.exe` and `cmd.exe`
declare Microsoft whatever they launch, which hid "Logitech Download
Assistant" and two bit4id registrations. Launchers are now unknown (unknown
stays visible); svchost services use their `Parameters\ServiceDll`, falling
back to the host only when that is unreadable (all such cases here were
Windows' own: `DoSvc`, `COMSysApp`).

**Evidence.**

- `cargo run -p vitals-win --example prove_startup_control`: 56 of 118
  startup entries and 295 of 376 services hidden; a throwaway `HKCU\...\Run`
  value read back `Disabled` then `Enabled`, then was removed.
- Live app over CDP: Startup 62 rows, 62 action buttons, box checked, "56
  Microsoft items hidden", right-click menu Disable / Open file location /
  Properties / Search online / Copy details. Services 81 rows, menu Start
  (disabled: running) / Stop / Restart / Start type / … . No horizontal scroll.
- Through the real IPC: `set_startup_enabled` on a probe value went enabled →
  disabled (record `03 00 00 00` + `FILETIME`) → enabled; an unknown source
  answered `refused`.
- Mutation: removing the confirmation guard turned the critical-service test
  red. That test calls `check`, not `apply`, so a regression on an elevated
  runner cannot actually stop `RpcSs`.

**Not changed.** The LAN API, CLI and SDK gain none of these: each can raise
a UAC prompt or change what runs at boot, and a paired phone or a script must
not be able to do either (same rule as `process_action_as_admin`).

### 2026-09-28 — S13-12 CI in 8 minutes instead of 49

**Cause.** The Windows leg was one job running clippy (622 s), `cargo test`
(719 s), the performance budget and `cargo test --workspace --release --
--ignored` (710 s — a fat-LTO build of the desktop app to run two
`vitals-bench` tests) one after another. `cancel-in-progress: true` on
main killed each run when the next push landed, and rust-cache saves only
when a job finishes, so `gh cache list` held no Windows entry at all.

**Change.** Three parallel Windows jobs with a rust-cache `shared-key` each,
`cache-on-failure`, saved from main only; cancel only on pull requests;
ignored tests run with `-p vitals-bench`, guarded by a step that fails if
an `#[ignore]` appears anywhere else; `CARGO_INCREMENTAL=0` and
`CARGO_PROFILE_DEV_DEBUG=0`; bindings check on Linux; installer built in
parallel, without updater artefacts (the key is release-only — the first
run failed with "A public key has been found, but no private key"); a
`changes` job skips Rust for site/ADR/prose pushes (`docs/api` and
`docs/integrations` stay in, the server tests `include_str!` them).

**Evidence.**

```text
36421745331 (before)  first job start -> last job end 49.4 min   58 job-min
36427586540 (after)   first job start -> last job end  7.7 min   21 job-min
  Rust test (windows) 191 s · Rust lint (windows) 102 s · Performance 100 s
  Build installer 447 s · Rust (linux) 79 s · all success
guard: clean tree GREEN; planted #[ignore] in vitals-core RED
```

### 2026-09-28 — S12-35 Faster gates

**Ask.** Make typecheck, lint, build, tests and the rest of the gates as fast
as possible: cache, parallelism, anything else.

**Measured before changing anything** (same machine, other agents' builds
keeping it at 76–100 % CPU):

| What                      | Before                         | After                           |
| ------------------------- | ------------------------------ | ------------------------------- |
| desktop vitest, 985 tests | 122.8 s, 2 timeouts (31 forks) | 46.3 s, all pass (8 forks)      |
| turbo typecheck           | 16.9 s every run               | 16.9 s cold, 3.4 s unchanged    |
| turbo lint                | 83.8 s every run               | 83.8 s cold, 1.5 s unchanged    |
| verify.ps1 -SkipPerf      | sum of gates                   | 368.9 s wall, 560.7 s if serial |

**What changed and why.**

- vitest `maxWorkers: '25%'`. The default is cores minus one; each fork boots
  its own happy-dom, and 31 of them spent 51 % of the run in environment
  setup. The sweep put 4, 8 and 12 workers all ahead of 31; a quarter keeps
  that ratio on smaller machines.
- turbo transit node. `lint`/`typecheck`/`test` said `dependsOn: ^build`,
  but no library package has a build script, so they waited on nothing and
  their cache key did not include their dependencies' sources. The transit
  node fixes the key and keeps them parallel. Proof: appending one line to
  `packages/ui/src/index.ts` re-ran `ui` and `desktop` typecheck, 4 of 6
  replayed.
- verify.ps1 runs three lanes as thread jobs. Rust stays serial inside its
  lane: every cargo command takes the target-directory lock.
- CI's frontend job runs one turbo invocation and restores `.turbo/cache`.
- `main.test.ts` imports `App` in `beforeAll`: the boot test timed out at
  15.5 s on a cold transform while passing alone in 2.5 s.

**Rejected, with the number.** `eslint --cache` (67.0 s to fill, 5.7 s hit)
is wrong for type-aware rules: it keys on the linted file only, so a type
change in a dependency leaves a stale pass. `tsc --incremental` saved 0.6 s.

**Not mine, still red in the run.** Another session's uncommitted Android
work: `crates/vitals-core/src/remote.rs` (bindings drift, doc-test),
`crates/vitals-server/tests/api.rs` (rustfmt), ADR-0033 (prettier).

### 2026-09-28 — S12-34 Lag watchdog v2

**Ask.** Bundle it in the installer; choose the alarm sound (built-in or a
file); stop warning during builds and detect real UI freezes.

**Evidence v1 was wrong.** Seven proposals in ten minutes, each with the
machine at 93 %+ and probe lag 0.0 ms. At 97–100 % CPU the foreground window
still answered `WM_NULL` in 0.1–8 ms and DWM missed no frames.

**Live, after.**

- `--diagnose` at 96 % machine load: window 0.1–14.8 ms, no trigger.
- A window whose UI thread really stops (a C# busy loop; a PowerShell
  `Start-Sleep` handler kept answering in 0.2 ms, which is why the first
  two attempts proved nothing): window 1000 ms from the first tick, Stall at
  tick 4, Hung at tick 7, and the installed watchdog logged
  `proposing trigger=Hung(...) name=pwsh.exe`.
- Writing `watchdog.json` with `relaxed` then `normal`: `settings changed`
  logged within 4 s each time.
- `--play-sound`: Mail exit 0; `Alarm01.wav` exit 0 after 11.6 s; a missing
  file exit 1 "the file does not exist"; `win.ini` exit 1 "MCI error 277".
  Before the fix every Windows sound failed with MCI 282 (`to 10000` past
  the end) and the Test button reported success, because it spawned and did
  not wait.

### 2026-09-28 — S12-33 Hardware, Device Manager and fan speeds

**Ask.** Show the hardware details (CPU, RAM, GPU, SSD, HDD) that Windows
knows; Devices reported temperatures wrongly; list every device Device
Manager lists, grouped; then add fan speeds.

**What.** Devices & sensors has three tabs, each read only when opened:

- **Sensors**: the table gains drive temperatures, read unelevated with a
  zero-access handle and `IOCTL_STORAGE_QUERY_PROPERTY` (the SATA
  temperature property, or the NVMe health log), and motherboard fans.
  ACPI zones are now "Board thermal zone 0 (TZ00)": at 28 °C beside an 81 °C
  CPU, "Thermal zone 0" read as a wrong CPU temperature, which is what
  "not reported correctly" was.
- **Hardware** (`vitals-win::hardware`, WMI on demand, ~0.7 s): CPU, memory
  modules, GPUs (VRAM from the driver registry, since `AdapterRAM` caps at
  4 GiB), drives, board and BIOS. OEM placeholders become "Not available".
  A board maximum smaller than what is installed (128 GiB with 192 GiB
  fitted) and all-zero SPD serials are dropped. Serials reach the webview as
  their last four characters only, and there is no LAN route.
- **Device Manager** (`vitals-win::devices`, SetupAPI): every node grouped by
  setup class, with the driver provider, version and date, and problem codes
  in Windows' wording. A class with a problem opens by itself. There is
  search, and disconnected devices can be shown on request.

**Fans.** The sensors service now also loads PawnIO's signed `LpcIO`
module, finds the board's Super-I/O chip (IT8689E here) and reads its fan
tachometers under the shared `Access_ISABUS.HTP.Method` mutex. It writes only
protocol bytes, never a fan register. The fans reach `SystemMetrics.fans`
(serde default), Prometheus `vitals_fan_rpm`, Thermals and Devices. The "Fan
speed (RPM)" and "Drive temperature" gaps close when measured.

**Found live, not by tests.** Reinstalling from a dev build put a
two-hour-old helper without fan support back as the SYSTEM service. The
helper lookup preferred `resource_dir()/sensors`, which is `target/debug/sensors`,
a copy tauri-build makes once and never refreshes. The source-tree staging
directory is now first for dev builds. The check is the hash of
`C:\Program Files\Vitals Sensors\vitals-sensors.exe`.

```text
elevated read: superIo IT8689E, fans Fan 1 1467 RPM, Fan 6 2170 RPM, 2-5 at 0
live Devices after UI reinstall: Fan 1 1,464 · Fan 6 2,163 RPM · CPU 81/84 °C 189 W · drives 40/45/39 °C
Hardware tab: i9-14900K 24C/32T · 4 × 48 GB DDR5-5200 Corsair · RTX 3060 Ti 8 GB · 4 drives · Z790 AORUS ELITE AX
Device Manager tab: 513 devices in 29 groups · 2 with a problem
page-scroll-audit devices: page=0 at 720x560 1024x640 1280x800 1920x1080
clippy -D warnings · vitest devices+performance+mobile 161 · check-drift 0 (62 invokes, 62 commands)
```

**Still listed:** VRM/board temperatures and rail voltages, which need
per-board sensor maps a wrong guess of which is a plausible lie.

### 2026-09-28 — S12-31 CPU temperature and package power through PawnIO

**Ask.** "See how it was implemented in the codai desktop app and apply it
the same way": CPU temperature, done codai's way. Owner decisions:
Vitals' own service (not codai's pipe), PawnIO downloaded pinned by hash,
and package power as well as temperature.

**What.** `crates/vitals-sensors` is a port of `codai-sensors`. A LocalSystem
service opens the signed PawnIO driver, loads the embedded, pinned
PawnIO.Modules 0.2.11 blob for the vendor, and a one-second refresher reads
package temperature (0x1B1), hottest core (0x19C, pinned per logical CPU
across groups), TjMax (0x1A2) and RAPL energy (0x606/0x611; AMD
0xC0010299/0xC001029B), wrap-safe over 32 bits. It serves one JSON line per
connection on `\\.\pipe\vitals-sensors`. `vitals-win::sensors::cpu_service`
is the reader (sampler-safe: about 0.3 ms, 10 s backoff when absent), plus
status, pinned download (urlmon) and elevated setup via
`run_program_elevated`. Devices gains a panel that says what installing
involves (SYSTEM, signed driver, download if missing) before the one UAC
prompt; a dismissed prompt is shown as the user's answer. Removal is always
offered once installed. No LAN equivalent. The installer carries the helper
from the same commit (`scripts/bundle-sensors.ps1` in `beforeBuildCommand`).

**Found by the prover, not the tests.** The first client read 0 of 20 lines
from the live service while a .NET reader got every one: the server
disconnects right after flushing, and std maps the resulting
`ERROR_PIPE_NOT_CONNECTED` (233) to an error, not EOF, so `read_to_string`
discarded a complete line. `read_line` stops at the newline and treats a
disconnect after data as the end. A second defect showed on the first UI
install: the helper logged the refresher's `"starting"` placeholder as a
failure (exit 1) while the service was fine; install now waits for the first
real reading.

```text
prove_sensors_service: pipe reads 20/20 ok · latest() 0.315 ms -> 81 °C package, 80 °C hottest, 169.3 W
UI Remove (UAC): panel "Not installed", CPU rows [], gaps + CPU core temperature, CPU package power
UI Install (UAC): panel "Reading", rows 83 °C / 83 °C / 188 W, gaps 4 (was 6), install.log ok JSON
serve_dev /metrics: vitals_cpu_temperature_celsius 81.000
cargo test vitals-sensors 21 · vitals-win 620 · desktop 52 · clippy -D warnings clean
vitest devices + performance 114 · tsc · eslint · check-drift 0 (60 invokes, 60 commands)
mutations: refused shown as error → RED · CPU gap filter removed → RED · RAPL saturating_sub → RED (2)
```

**Still listed:** board and VRM temperatures, fan RPM, rail voltages
(Super-I/O, outside PawnIO's module set), and drive temperature.

### 2026-09-28 — S12-30 Devices shows this machine's temperatures; its fact cards never scroll

**Ask.** The first two cards on Devices & sensors should be separate and not
scroll; the computer's temperatures and other sensors were missing.

**Cause.** Unelevated, `MSAcpi_ThermalZoneTemperature` answers
`WBEM_E_ACCESS_DENIED` and nothing else was tried, so a normal user saw
"needs administrator" and eight gaps. Two sources need no privilege:

- The **Thermal Zone Information** performance counter publishes the same
  ACPI zones to any user (`sensors::zone_counters`: _High Precision
  Temperature_ in tenths of a kelvin, and _% Passive Limit_, surfaced when a
  zone throttles). Used when WMI yields no zones; it carries no trip points,
  so those stay `None`.
- **NVIDIA's `nvml.dll`**, installed in `System32` by the driver
  (`sensors::nvml`): GPU temperature, fan % and board power. Loaded with
  `LOAD_LIBRARY_SEARCH_SYSTEM32` only, so a planted DLL is never used; no
  NVIDIA driver means an empty list. The GPU temperature and board power
  gaps are dropped once measured; AMD/Intel keep them.

A fan duty cycle has its own `SensorValue::Percent` rather than
`Charge`, so an export never calls a fan "charge".

**Layout.** Power, Thermal zones and (when present) Battery are separate
cards in a top row at their own height (`.devices-facts`); stacked in one
scrolling column the zones were cut off. Readings and gaps share the rest
and scroll inside themselves. Code: `5c9e6c3`.

```text
sensors_probe (unelevated): TZ00 27.85 °C · TZ10 16.85 °C · RTX 3060 Ti 46 °C, fan 80 %, 64.3 W
  cross-check nvidia-smi 47 °C, 83 %, 59.6 W · Get-Counter High Precision Temperature 3010 / 2900
live get_sensors: thermal available, 5 readings (was 0), 6 gaps (was 8)
page-scroll-audit devices: page=0 at 720x560 1024x640 1280x800 1920x1080
cargo test vitals-win 615 · desktop 50 · cli 35 · clippy -D warnings clean
mutations: facts in pane-stack → RED · source label → RED · NVML 0 °C → RED · no decikelvin band → RED
pnpm lint · typecheck · format:check · pnpm test · check-drift 0 · check-size within budget
```

**Still not measurable, and listed:** CPU core temperature, board and VRM
temperatures, fan RPM, CPU package power, rail voltages (ring 0), and drive
temperature (administrator).

### 2026-09-28 — S12-29 show-hidden switch above the rail, explanation behind (i)

**Ask.** Put "Show hidden devices" above the list, and its description in a
popover on an info icon.

**Why it was wrong.** Below the list, the switch that shortens a 43-entry
rail was reachable only by scrolling to the end of it, and the description
took three lines of a 224 px column.

**Change.** The switch heads the rail; the text opens from an (i) next to
it. New `InfoPopover` in `@vitals/ui` on Radix Popover — a popover, not a
`Tooltip`, because the text is a paragraph the user opens on purpose and a
tooltip is hover-only and one sentence wide. Radix supplies Enter/Space to
open, Escape and outside click to close, focus returned to the (i), and
`aria-expanded` with `aria-haspopup="dialog"`; the icon button is named
"About hidden devices" / "Despre dispozitivele ascunse".

```text
rail-info.mjs (CDP, tauri dev): switch top 122 px, first entry 150 px,
  hint not inline; (i) click -> aria-expanded true, popover 288x77 with the text;
  Escape -> closed
InfoPopover.test (3) + PerformanceScreen.test (2 new);
  mutation: switch moved back below the list -> RED, restored
vitest desktop performance 57 · ui 3 · tsc desktop + ui · eslint · prettier · drift 0
```

### 2026-09-28 — S12-27 network chart follows the selected adapter

**Ask.** The network chart did not change when switching adapters, and the
rail's scrollbar sat on top of the entries.

**Cause.** Not a rendering bug: `NetworkPanel` charted `history.core.netRx/Tx`,
the machine-wide sum, for every adapter, because per-NIC history was never
collected. The comment said so; the user saw it as a broken click. The rail's
list padded 8 px under a 10 px Radix scrollbar that overlays the viewport.

**Fix.** `HistoryCollector` keeps `netRx`/`netTx` per adapter id, with the
same gap and reset handling as the disk and GPU series (34 adapters × 2 ×
180 samples ≈ 100 KB). `NetworkPanel` charts the selected adapter's pair,
keyed by id so a switch mounts a fresh chart, and names it after the
adapter. The rail pads the bar's side by 16 px: right in the column layout,
bottom in the strip.

```text
live CDP (tauri dev, this machine):
  chart canvas Ethernet != Tailscale                      true
  rail column  button right 472 -> 464, bar left 470      overlap 2 px -> -6 px
  rail strip   900x700, button bottom 204, bar top 210    overlap -6 px
history.test "records each network adapter separately" + PerformanceScreen.test
  "charts the selected network adapter" — mutation (every adapter into one series) -> 2 RED
vitest performance + dashboard: 11 files, 155 tests · tsc · eslint · prettier
```

### 2026-09-28 — S12-26 hide devices on the Performance page

**Ask.** Hide the devices that should not be listed, hide any device from a
right-click menu, and a checkbox that shows everything, hidden ones included.

**Measured before** (this machine, `vitals info --json`): the rail listed
43 entries — 3 GPUs, 4 disks, 34 network interfaces — of which the user
recognises about eight. The existing "Show virtual adapters" switch hid
nothing, and that was a backend bug, not a UI one: `classify()` in
`vitals-win/src/network/adapters.rs` read only the IANA ifType, and Hyper-V
switches, `vEthernet`, WAN miniports and Wi-Fi Direct all declare Ethernet
or 802.11. `NetworkKind::Virtual` was never emitted on Windows. The
`HardwareInterface` bit was already read, into a field nobody consulted;
`network_probe` now prints it, and it marks exactly two rows here:
`Ethernet` and `Wi-Fi`.

**Design.** Fixed at the source: a software Ethernet or 802.11 interface is
`Virtual` (every consumer — LAN API, SDK, Prometheus labels — now gets the
truth). The rail (`resources.ts`) builds every entry and flags it
`hidden`; defaults hide loopback/virtual, a VPN or unknown adapter that is
down or has never carried a byte (WAN miniports, 6to4, IP-HTTPS — Tailscale
and a live Teredo stay), and a GPU with no counters at all. Physical
adapters are never hidden by default. A right-click (Radix `ContextMenu`,
so Shift+F10 reaches it) toggles Hide/Show; CPU, memory and thermals have
no menu. "Show hidden devices (N)" reveals the rest dimmed with an eye-off
mark. Choices persist in settings as `resourceVisibility`, keyed by name or
mount point rather than by id — a GPU id is half a LUID and an interface
index is renumbered — and only overrides of the default are stored, so a
better default later still reaches untouched devices.

```text
perf-hide-live.mjs (CDP, live app on this machine):
  default        11 entries  "Show hidden devices (32)"
  Hide H:        10 entries  "Show hidden devices (33)"
  show hidden    43 entries  (virtual display, Basic Render, vSwitch x7, WAN miniports ...)
  Show H: + untick  11 entries, resourceVisibility {}
cargo test -p vitals-win --lib network::adapters   19 passed
vitest src/features/performance src/settings       11 files, 103 tests
mutation: a stored "hidden" ignored -> 4 RED, reverted
verify.ps1: clippy, rust tests (470 s), drift, bindings, perf budget, typecheck, lint, format,
  bundle, size PASS; ts tests 1 fail = known AppShell lazy-dialog load flake under the parallel
  cargo run, then 93 files / 937 tests PASS alone; cargo fmt applied, --check clean
```

### 2026-09-28 — S12-28 no page scroll on any screen, only sections

**Ask.** The same as the dashboard for every other screen — no vertical or
horizontal page scroll, only sections — but done properly: some screens
still need to scroll vertically at small resolutions to stay responsive.

**Rule, now in styles.css.** A screen is a fixed top (title, toolbar,
selection) over a body that fills the rest. What grows without bound — a
table, a list, a card's contents — scrolls inside itself. When the window is
too short to hold the panes at their minimum useful height, the body
(`.screen-body`) scrolls as one region, so nothing is squeezed to
nothing; that is the responsive case. The document and `main` never
scroll, at any size.

- `.pane-stack` / `.pane-scroll`: a card whose header stays put and
  whose body scrolls (Storage scan and cleanup, Devices readings and gaps,
  Benchmarks results, Users sessions).
- `.list-scroll`: Network's connection list.
- `.perf-panel` / `.perf-chart`: Performance's chart takes the height
  left instead of a fixed `h-40`.

`DevicePanels.tsx` carries the same change but also the other session's
per-adapter charts (S12-27), so it is committed with that work.

```text
page-scroll-audit, 12 screens x 4 sizes = 48 runs: page scroll 0 in all 48
  1280x800 / 1920x1080: only tables, lists and panes scroll
  720x560 / 1024x640: .screen-body scrolls on Storage, Devices, Benchmarks, Users (short window)
shadow-audit 1024x640 1280x800 1920x1080 → TOTAL clipped 0
styles.test.ts guards; mutations RED; lint · typecheck · prettier · vitest · check-drift green
```

### 2026-09-28 — S12-25 the dashboard fits the window

**Ask.** Dashboard elements were too large; redesign it to look modern,
with no vertical or horizontal page scroll — only sections scroll when
they must.

**Before** (live CDP, `.copilot-tmp/dash-shot.mjs`): two columns of
content-height cards, 491×448 at 1280×800; the page scrolled 438 px there,
522 px at 1920×1080 and 2054 px (plus 17 px sideways) at 720×560.

**Design.** The grid is the window. `.dashboard-grid` (styles.css) fills
the content column with equal-share rows (`grid-auto-rows: minmax(0, 1fr)`)
and a column count from the container width — 1/2/3/4/5 at
34/60/110/150 rem — then balanced to the widget count with CSS `round()`,
so six widgets are 3+3 rather than 5+1. Each card leads with one number in
its header (`widgets/headline.ts`: CPU load, memory %, total disk and
network throughput, the busiest GPU, battery charge; toned amber at 75 %
and red at 90 %; nothing for lists and nothing when unmeasured). The chart
takes whatever height is left, the stats are a compact auto-fill grid, and
the body scrolls inside the card when a short window leaves too little.
Figures the header now carries (CPU total, the memory meter) and the
rarely-read handle count left the body.

```text
dash-shot, dark + light, doc/main scroll and inner scrollers:
  1280x800   doc 0 main 0  0 scrollers  cards 328x325
  1366x768   doc 0 main 0  0 scrollers  cards 357x309
  1920x1080  doc 0 main 0  0 scrollers  cards 541x465
  3440x1440  doc 0 main 0  0 scrollers  cards 1048x645 (3+3)
  1024x640   doc 0 main 0  4 cards scroll inside (5-56 px)
  720x560    doc 0 main 0  cards scroll inside
layout-audit dashboard: main=0 at 720x560, 1280x800, 1920x1080 · shadow-audit 0 clipped
guards: styles.test.ts "the dashboard fits the window" (3) + headline.test.ts (5);
  mutations: auto rows → RED · body not scrollable → RED · warn threshold → RED · GPU 0 % → RED
pnpm lint · typecheck · format:check · pnpm test · check-drift 0 · size 186.7 KB of 195.2 KB
```

### 2026-09-28 — S12-24 every tab is ready before its first visit

**Ask.** The first visit to a tab waited for its data; the second was
instant because the screen stayed mounted. The whole app should be
preloaded.

**Measured before** (live CDP, `.copilot-tmp/firstvisit-split.mjs`): a first
visit took 300–1300 ms — lazy chunk (Performance 1241 ms), then the data
read. `get_startup` 231–322 ms and `get_installed_apps` 223 ms ran as
synchronous commands on the main thread, so parallel reads serialised
(11 commands: 718 ms wall, each "taking" 711 ms). And a defect: every
metrics source ran its own `listen()` and reconciled deltas from an empty
map, so Processes opened between keyframes showed **116 of 780** processes
for up to thirty seconds.

**Fix.**

- `lib/metrics`: one session-long frame hub. It asks the sampler for a
  keyframe (new `request_keyframe` command, flag on `AppState`), folds no
  delta before a baseline, and hands every new subscriber the current
  snapshot. Stream sources (Processes, Performance, Network, Dashboard)
  seed their initial state from it.
- `lib/prefetch`: background reads keyed per screen, with a max age; a
  hook seeds its first render from `peekPrefetched` and reuses the read
  (`takePrefetched`) only when younger than 15 s, otherwise it shows it
  and reads again. Failed reads leave no entry.
- `routes.tsx`: `preloadRoutes` loads every chunk and prefetches every
  screen's data, one step per idle callback, after first paint; then every
  route is mounted hidden inside a transition. Chunks are memoised so
  `lazy` and the preloader share one import.
- Four inventory commands are `async` + `spawn_blocking`.
- `HistoryCollector` ignores a frame it already folded (the hub replays
  the current frame to late subscribers).

Not preloaded, by design: storage scans, cleanup candidates, benchmark runs
— each loads the disk or every core and is the user's call.

```text
skeleton-check (reload, idle, click each tab once, inspect its FIRST frame):
  idle 1.5 s → 11/11 data, 0 skeletons · idle 6 s → 11/11 data, 0 skeletons   (before: skeleton on 11/11)
proc-count: first Processes visit "804 of 804 processes"                      (before: "116 of 116" on 780)
backend-check: request_keyframe ok · startup+apps+sensors+connections parallel 239 ms vs 513 ms sequential, 0 frame gaps
mutations (each restored): hub replay removed → RED · baseline gate removed → RED · hub re-installed per subscribe → RED ·
  useStartup seed ignored → RED · prefetched read ignored → RED · take does not consume → RED · process source not seeded → RED ·
  history replay dedupe removed → RED
pnpm test 92 files 916 · cargo test -p vitals-desktop 50 · clippy -D warnings · lint · typecheck · prettier · cargo fmt
check-drift 0 (58 commands) · check-size within budget (initial 186.1 KB of 195.2 KB)
```

**Remaining cost.** A click still takes 200–600 ms to paint in the dev
build even with everything mounted: the profile is React dev-mode JSX
validation and the 800-row table re-rendering as it becomes visible. That
is navigation cost, present on revisits too, not loading.

### 2026-09-28 — S12-23 one window background, no borders between regions

**Ask.** Title bar, sidebar and content on ONE background for the whole
app, with no border between them; restyle only the chrome, keep the
content's background as it is.

**Cause.** The chrome carried `.surface-chrome`: a `--color-bg-subtle`
fill (80 % + 48 px blur under mica, the default; 62 % under acrylic; opaque
under solid) and hairlines — `border-b` on the title bar, `border-r` on the
sidebar, `border-t` over the sidebar footer. The ambient canvas was painted
on `#main-content` only.

**Fix.** The canvas (base colour + the two fixed accent pools) moved to
`#root`; `header`, `nav` and `main` paint nothing. With
`background-attachment: fixed` the pools are positioned against the
viewport either way, so the content looks exactly as before. The
solid / mica / acrylic setting only ever tinted the chrome, so with no chrome
fill it did nothing: removed per ADR-0013 (types, schema, `applyTheme`,
Settings row, four keys in en and ro, the three theme.css rules; a stored
`surface` value is dropped on parse, tested). The class became
`.shell-chrome` (it still names the View Transition snapshots).

**Contrast found on the way.** The active nav label used `--color-accent`
on the indicator's `--color-accent-subtle` fill: 4.12:1 in light blue and
4.46:1 in light teal, under AA. New `--color-accent-text` step (L 0.46
light / 0.8 dark); every chrome text and icon now clears its threshold for
all ten accents in both modes.

```text
styles.test.ts "one window background": 4 tests; mutations (each restored):
  title bar border-b → RED · sidebar bg-subtle → RED · footer border-t → RED ·
  canvas back on #main-content → RED · nav.shell-chrome background rule → RED
live CDP, .copilot-tmp/chrome-audit.mjs, 720x560 1280x800 1920x1080 3440x1440, light + dark:
  header/nav/main/footer border 0/0/0/0, background rgba(0,0,0,0), no image, no backdrop-filter
  #root: oklch(0.98 0.002 260) light / oklch(0.17 0.006 260) dark + gradients, attachment fixed
  contrast on base / on base+ambient: fg-default 16.3/14.3 (light) 16.5/14.4 (dark) · fg-muted 6.14/5.39 · 7.74/6.77
chrome-contrast (every text node + icon vs its real backdrop), 10 accents x 2 modes → 0 below threshold
  (before --color-accent-text: light blue 4.12, light teal 4.46)
layout-audit 12 sections x 4 sizes → doc=0 main=0 in all 48
shadow-audit 12 sections x 4 sizes → TOTAL clipped 0
pnpm typecheck 6/6 · pnpm lint 6/6 · prettier clean · pnpm test 89 files 898 tests · check-drift 0
```

**Not changed.** Overlays (menus, dialogs, tooltips, toasts) keep their
raised fill and border: they float above the window and are not regions of
it. The mobile page and the HUD never used the surface axis beyond setting
`solid`; that line was removed from both.

### 2026-09-28 — S12-16..22 audit: security, honesty and lifecycle gaps

**Ask.** "mai caută bug-uri și gap-uri și rezolvă. gândește-te la toate."
Read every boundary the way a new client would (AGENTS.md: a new client is
an audit): loopback auth, the LAN stream lifecycle, the sampler's exit path,
the IPC attach server, the enum wire contract, the phone, and every
fire-and-forget button. Twenty-two findings, all fixed at the source, each
with a test that was mutation-checked red. Grouped:

**Security (S12-16, S12-17).** The loopback bypass keyed on the peer only,
so any web page could open `ws://127.0.0.1:7330/api/v1/ws` (WebSockets are
exempt from CORS) and be granted Control with no token; DNS rebinding gave
the same from a browser tab. Now `is_local_caller` refuses any `Origin` and
requires a loopback `Host`. Auth ran once per connection, so a revoked
phone kept streaming every process name until it left; streams re-check
their token per frame and `stop()` ends them. The risk gate ran at _plan_
time only; `gate()` re-reads the live facts at execution and needs
`Consent::Confirmed` for a critical process — carried through the elevated
child and never granted over LAN.

**Contract (S12-19).** Fourteen enums were kebab-case on the wire against
camelCase unions. Invisible to every gate because the drift script only
checked structs and the wire test only swept keys; the Connections table
showed `state.syn-sent` live. Both gates now cover values.

**Lifecycle (S12-18, S12-20, S12-21).** Quit never stopped the sampler
(Tauri never drops managed state), so the closing flush was dead code. The
attach server served a frozen view to the next CLI. Pairing a second phone
closed the first. A StrictMode-shaped stop→start leaked a frame listener
for the life of the app.

**Honesty (S12-22).** `Alerts 0` on a failed request, a 2.1 GW battery
drain from `BATTERY_UNKNOWN_RATE`, six capabilities advertised with no
code behind them, and a HUD that could be made unclickable with no way
back.

```text
cargo clippy --workspace --all-targets -- -D warnings → clean
cargo test --workspace → all green (vitals-win 607, server api 29, ipc 10, core 144+8, cli 35, desktop 50, store 15)
pnpm typecheck 6/6 · pnpm lint 6/6 · vitest 89 files 894 tests (desktop) + packages green · prettier · check-drift 0
protocol:check → only Contributor.ts doc comment regenerated (intended)
check-size → initial 185.9 KB (+0.2 %), budget raised with approval
mutations (each restored, verified by rg): IPC clear→FAILED · ProcessState kebab→4 FAILED + drift FAIL · SSE Lagged skip→FAILED ·
  still_valid always true→FAILED · stop no signal→FAILED · [machines] dep→FAILED · always-install→FAILED
live over CDP: get_lan_status running=false port=null · capabilities: managePowerPlans/… notImplemented ·
  Connections renders "Connecting" (was state.syn-sent) · zero raw i18n keys on Connections/Processes/Performance
```

**Not done, by decision.** Slow sync commands (`get_installed_apps`,
`get_startup`, `get_sensors`, `get_connections`, `run_benchmarks`) still
block Tauri's main thread; moving them to `async` commands is a separate
slice with its own measurement. `vitals serve --control` still has no
controller (ADR-0004 stands); it now says so when it starts. Mobile memory
sparkline pushes `0` when `total` is 0 — unreachable for a machine that
booted, left as is.

### 2026-09-28 — S12-15 shadows are no longer cut by the edges of scroll regions

**What the user saw.** "Shadows of cards in lists are being cut on the
sides by other elements." Not a z-index problem: `overflow: auto` clips at
the padding box, and S12-12 made every screen scroll inside a region with an
8 / 4 px gutter. Measured with `.copilot-tmp/shadow-audit.mjs` (every element
with an outer shadow, against its nearest clipping ancestor, visible reach =
0.7 x blur + spread, i.e. where the Gaussian tail drops under 5 % opacity):
21 clipped shadows across 12 sections, in four places —

1. **Card sides and hover lift** in every `.screen-scroll` (Users, Storage,
   Devices, Benchmarks, Dashboard). Gutter is now `--scroll-gutter-x` 20 px
   sideways and `--scroll-gutter-end` 40 px after the last item — sized to
   `--shadow-card-hover` (20 px sideways, 40 px below in dark). The column's
   own padding is derived from the same token so the negative margin can
   never reach `<main>`'s horizontal clip.
2. **Meter fill glow** sliced flat by the track's `overflow-hidden`. The
   track no longer clips; the fill cannot escape because its ratio is
   clamped to 0..1.
3. **Sidebar active pill** — `--glow-accent` reaches ~11 px sideways inside
   an 8 px list padding. New `--glow-accent-contained` (≈4 px) for items in
   narrow scrollers.
4. **Performance rail** selected item, cut 16 px by the ScrollArea viewport:
   contained glow plus padding inside the viewport, pulled out by `-m-2`.

```text
shadow-audit before (1280x820, dark)  → TOTAL clipped 21
shadow-audit after, resting           → 0 at 720x520, 1280x820, 1920x1080, 3440x1440
shadow-audit after, hover lift forced → 0 at 720x520, 1280x820, 3440x1440 (override proven applied)
light theme, resting + hover          → 0
last list item scrolled to end (Network) → 12 px room, then 40 px after the gutter change
layout-audit 720x520, 1280x820        → document and main scroll 0 in both axes (no regression)
styles.test.ts scroll gutters (new, 2 tests) → pass; mutation --scroll-gutter-x 0.5rem → FAILED; restored
pnpm typecheck 6/6 · pnpm lint 6/6 · pnpm test → green · prettier · drift 0
check-size.ps1 → within budget after the documented raise (see size-budget.json)
```

Not changed: Dialog, Toaster and menus paint `--shadow-overlay` from a
portal at the document root, outside every scroller, so nothing clips them.

### 2026-09-28 — S12-14 "Retry as administrator" performs the action, one UAC prompt each

The risk dialog has shown the button since it was written; `RiskDialog`
never received an `onElevate`, so a process owned by SYSTEM or another
account (230 of 717 here) could be neither ended nor paused. Chosen with the
user: one UAC prompt per action, not an elevated app.

- `vitals-win::actions::elevated` — `run_as_admin` relaunches the exe under
  `runas` with `--elevated-process-action <terminate|suspend|resume> <pid> <start>`
  and maps the child's exit code back to `NotFound` / `AccessDenied` /
  `Refused` / `Os`. The child re-verifies identity (a PID can be recycled
  while the prompt is up) and re-assesses risk, refusing `forbidden` so it
  cannot bypass the dialog. The UAC relaunch was extracted from the Task
  Manager switch into `taskmgr::run_elevated`, so both share one path.
- Found live: the first version reported "access denied, even as
  administrator" on a SYSTEM `ping.exe`. An elevated token holds
  `SeDebugPrivilege` disabled; the child now enables it, only for itself.
- `launch.rs` routes the argument before Tauri builds, and never falls
  through to a window even when malformed — the parent is blocked on it.
- `process_action_as_admin` command (async, `spawn_blocking`), registered.
  Desktop-only: a paired phone must never raise a UAC prompt.
- UI: a denial on a single process reopens the dialog marked denied, with
  the retry as the primary action and the plain confirm removed. A
  dismissed prompt reads "approval was declined, so nothing was changed",
  not an error. Resume (no dialog) goes straight to the prompt on denial.
  EN + RO strings.

```text
live: SYSTEM-owned PING.EXE 29080 (schtasks /RU SYSTEM), unelevated Vitals
  invoke terminate_process      → {"kind":"access-denied",…}
  Delete key → dialog "…Windows refused: this process belongs to another account
               or to the system… Cancel | Retry as administrator"
  Retry → UAC approved → dialog closed, no alert → Get-Process 29080: gone
  (before the SeDebugPrivilege fix: "access denied: act on process 29080, even as administrator")
cargo test -q -p vitals-win --lib → 601 passed (elevated: 5 new)
cargo test -q -p vitals-desktop --lib -- launch → 9 passed (1 new)
cargo clippy -p vitals-win -p vitals-desktop --all-targets -D warnings → clean
vitest processes + lib → 140 passed; mutation: onElevate no-op → retry test FAILED; restored
pnpm typecheck 6/6 · pnpm lint 6/6 · pnpm test → green
scripts/check-drift.ps1 → 0 failure(s), 0 warning(s); 57 invokes, 57 commands, 2 locales
```

`AppShell.test > opens settings` now imports the lazy SettingsDialog in
`beforeAll`: the cold vite transform, not the behaviour, was crossing 8 s
then 12 s at 87-91 % CPU. With the chunk warm the test takes 889 ms.

Not done, with reason: a tree kill that hits a denial reports it rather than
prompting once per descendant; priority/affinity/efficiency have no elevated
path (reversible, rarely denied on processes users care about).

### 2026-09-27 — S12-13 process actions: Suspended state measured; readable errors everywhere

**What the user saw.** "Killing processes and other actions from other
menus and tabs aren't working." Every process command was driven through
the real IPC first, and all twelve worked — End task, Suspend, Resume,
priority, affinity, efficiency mode, handles, modules, path — so the backend
was not the failure. Driving the menus themselves over CDP found two:

1. **Resume was unreachable.** `frame.rs` set `state: ProcessState::Running`
   for every process, unconditionally. Suspend worked (OS: 6/6 threads
   suspended) but the row still said Running, so the menu offered Suspend
   again and never Resume. A process paused from Vitals could not be
   resumed from it. Now read from the thread records that already follow
   each `SYSTEM_PROCESS_INFORMATION` entry — no extra syscall. The rule was
   measured, not assumed: on a suspended `cmd`, three threads report wait
   reason `Suspended` and the fourth stays in a kernel `Executive` wait (the
   console read the suspend APC cannot interrupt), so "every thread says
   Suspended" — the first hypothesis — missed it and the new test failed.
   Rule: every thread waiting; each `Suspended`/`WrSuspended` or `Executive`;
   at least one suspended.
2. **Errors on other tabs said `[object Object]`.** Commands reject with a
   `{ kind, message }` object; ten hooks/screens did
   `cause instanceof Error ? cause.message : String(cause)`. Processes had a
   correct reader of its own; it moved to `lib/commandError.ts` and every
   screen uses it. The TS kind union also lacked `refused`, which the backend
   sends.

```text
IPC probe (ping.exe, 12 commands)     → all OK
live UI over CDP, ping.exe:
  Suspend         → OS 6/6 threads suspended, row "PING.EXE 58036 Suspended"
  Resume          → OS 0/6,                   row "PING.EXE 58036 Running"
  Priority ▸ High → PriorityClass High
  End task        → process gone;  disruptive name → confirm dialog → ended
  SYSTEM process  → "Could not complete the action: access denied: … requires elevation"
cargo test -q -p vitals-win --lib     → 596 passed
  mutation: read_suspended → Some(false) → "a paused process must read as suspended" FAILED; restored
cargo clippy -p vitals-win --all-targets -D warnings → clean · cargo fmt --check → clean
pnpm typecheck 6/6 · pnpm lint 6/6 · pnpm test → green (desktop 89 files)
scripts/check-drift.ps1 → 0 failure(s), 0 warning(s)
```

The settings-dialog wait in `AppShell.test` (a lazy chunk) ran out at 8 s in
one full run at 87 % CPU and passed alone; raised to 12 s, under the 15 s
test limit.

Not changed here: LAN control, SDK, CLI — no wire shape changed; `state` was
already in the protocol and now carries a real value. The unwired
"Retry as administrator" button is S12-14, below.

### 2026-09-27 — S12-12 screens fill the window; only regions inside them scroll

**What the user saw.** "Content does not size correctly — grids and tables
should be responsive, fill the viewport height, no scroll on the body, only
inside components." Measured over CDP before touching anything (1280×820):
`<main>` scrolled on six sections — Installed apps 14 765 px, Startup
5 691 px, Network 740 px, Devices 410 px — with the title and the search box
scrolling away; Startup's table was 1 903 px wide in a 1 280 px window;
Processes pushed `<main>` 306 px sideways; App history and Services filled
32 % and 46 % of the height.

**Root cause.** Nothing between `<main>` and a screen had a height: the
content column, `RouteTransition`'s wrapper and the `<Activity>` wrapper were
plain blocks, so `h-full` on Processes resolved to auto (its virtualiser only
worked through the 640 px fallback) and every other screen grew to its
content. Separately, `truncate` in an auto-layout table does nothing — the
cell grows to the longest command line.

**Fix.** The chain is a flex column end to end (`Content.tsx`, `routes.tsx`).
`styles.css` adds `.screen` (fills the column), `.screen-scroll` (the part
that grows scrolls; padded so card shadows and focus rings are not clipped),
`.table-scroll` (own scroll box, sticky header cells, card surface) and
`.cell-fill` (the free-text column takes the slack and truncates; short
columns never wrap). Tables: Startup, Services, Installed apps, App history,
Storage paths (full text in `title`). Card stacks: Network, Users, Devices,
Storage, Benchmarks, Dashboard. Performance: rail and detail scroll
independently; below `lg` the rail scrolls sideways instead of clipping.
Processes: `min-w-0` keeps its wide grid scrolling inside itself; the toolbar
wraps at 720 px. Dashboard columns come from a container query on the width
the content actually gets — 1/2/3/4/5 at 0/42/64/110/150 rem — and a `full`
widget is `col-span-full` at any count.

**Tests.** `AppShell.test > passes a definite height from main down to the
screen` walks every wrapper from the visible route to the column; with
`flex-1` removed from the Activity wrapper it failed ("wrapper <DIV> breaks
the chain"), restored it passes. The desktop suite's `testTimeout` is 15 s:
at 90 % machine CPU a different AppShell/main test crossed vitest's 5 s
default on each of two full runs, each passing alone — and the 8 s
`findByRole` from S12 could never be reached under a 5 s test limit.

```text
node .copilot-tmp/layout-audit.mjs 720x520,1024x700,1920x1080,3440x1440 + native 1280x820
  12 sections × 5 sizes → docScroll 0, docScrollX 0, mainScroll 0, mainScrollX 0 on all 60
  route fills main: 92 % (720x520) · 94 % · 95 % · 96 % · 97 % (3440x1440) — rest is padding
  e.g. installedApps 1280x820: table box h=546 scrollHeight=15309 (was: main scrolled 14765)
run-build.ps1 pnpm typecheck → 6/6 · pnpm lint → 6/6 · pnpm test → all 88 desktop files, ui 13, charts 4
prettier --check (changed files) → All matched files use Prettier code style!
scripts/check-drift.ps1 → 0 failure(s), 0 warning(s)
scripts/check-size.ps1 (after raise) → initial 185.3/185.3 KB, shipped 375.3/375.3 KB, hud 96.9 %, mobile 99.5 %,
                                       installer 4318.2/4318.2 KB — Within budget.
```

Size budget raised to the measured bytes, not +5 %: initial 188 880 → 189 708
(+828 B gz), shipped 382 460 → 384 311 (+1 851 B), for the four layout rules
(1 298 B raw) and the container-query grid — after removing the redundant
`whitespace-nowrap` classes the `.table-scroll` rule already covers. The
installer budget 4 409 993 → 4 421 851 B: the first installer measured since
S12, so it carries S12's backend (disk IOCTLs, DXGI fallback, owner SIDs) as
well as this frontend. Reason recorded in `size-budget.json`.

Not updated, with reason: locales — no new string (the `title` attributes
carry data, not copy); mobile and HUD — separate entries with their own
layouts, untouched; SDK/CLI/OpenAPI — no wire change.

**Still open from S12-11.** The wall-clock overhead gate was re-run this
session: 34.9 ms median at 91 % machine CPU (other agents' builds), with the
cycle counter reading 27.0 ms CPU in the same minute. It stays unproven on an
idle machine.

### 2026-09-27 — S12 redesign: depth, morphing nav, View Transitions, rolling numbers; disk/GPU/drive-kind/owner fixes

**What the user saw.** "The desktop app is ugly and some pages do not
work." A CDP walk over all twelve sections found no crash — Devices and
Benchmarks simply load slower than the first probe waited — but four
numbers were wrong on screen: Disk read/write were `0 B/s` on the dashboard
and every Performance drive at `0 %` (the rate maths in `disk/rate.rs` had
no caller); the Performance rail listed `Display adapter 0x00033f83` beside
the real card; Storage said `Unknown type` for every drive; the Processes
User column was an em dash on every row. Separately, eleven CSS custom
properties were referenced and never defined, so the per-core bars, alert
icons and HUD sparklines had no colour.

**Backend, fixed at the source so every client benefits.**
`disk/device.rs` opens `\\.\X:` with zero access rights (no elevation) and
asks `IOCTL_DISK_PERFORMANCE` every tick and `IOCTL_STORAGE_QUERY_PROPERTY`
once for bus type and seek penalty — applied inside `enumerate_volumes`,
so the Storage screen and the sampler cannot disagree. `gpu/adapters.rs`
falls back to DXGI's description and dedicated VRAM, joined by LUID; an
indirect display that borrows its render GPU's name is labelled
`(virtual display)` and does not double-count memory. `process/owner.rs`
reads the token SID once per process lifetime and resolves each SID once,
pruned with the live set; other-account processes stay `None` without
elevation rather than a guessed "SYSTEM".

**Frontend.** Tokens for depth (`--surface-card`, `--edge-highlight`,
`--shadow-card`, `--glow-accent`, `--ambient-a/b`) in both modes; `Card`,
`Button`, `Meter`, the process grid and the Performance rail restyled;
two static accent pools behind the content. Navigation is a View Transition
(`AppShell.navigate`, `flushSync`), the page title morphs, the chrome is
pinned. One absolutely positioned sidebar pill on a spring — not a shared
`layoutId`. `AnimatedValue` rolls the number inside an already formatted
string. Dashboard cards stagger in; skeletons shimmer; charts get a
gradient fill and a glowing head dot.

**Found live, fixed, tested.** (1) The pill sat on Dashboard whatever was
active: each `<li>` is positioned, so `offsetTop` was 0 — now measured by
bounding rect, with a layout-stubbed test. (2) Every transition aborted
with `InvalidStateError: Snapshot capture failed`: `page-title` was on all
ten parked titles under `<Activity>` and `sidebar` matched the Performance
rail's `<nav>` too — now `data-route-visible` marks the one visible route
and the chrome is matched by class. (3) `AnimatedValue`'s separator parser
read `1,351` as a decimal — rule rewritten, eleven cases.

**Not a regression, verified on HEAD.** `AppShell.test > opens settings`
failed on the unmodified tree in a clean worktree under three concurrent
builds; its `findByRole('dialog')` waits on a lazy chunk. Timeout raised to
8 s. The 30 ms overhead budget failed at 143 ms — and at 98 ms on HEAD
under the same 100 % CPU; the two new per-tick operations cost 0.019 ms
(disk counters, 4 volumes) and 0.035 ms (owners, 705 processes, warm).

**The budget, measured where load cannot reach it.** The gate is
wall-clock, and the machine did not drop below 74 % for the twenty minutes
a watcher waited. `examples/cpu_cost.rs` counts the process's own cycles
across one `sample()` (`QueryProcessCycleTime`; `GetProcessTimes` ticks
every 15.6 ms and cannot resolve 30 ms) at the nominal 3187 MHz. Three runs
each on S12 and on `cb8a0d8` in a clean worktree, same load:

```text
S12     CPU median 23.87 / 24.18 / 23.34 ms   wall median 29.9 / 30.4 / 28.8 ms
cb8a0d8 CPU median 25.36 / 22.58 / 22.23 ms   wall median 113.8 / 33.9 / 30.7 ms
```

Within noise of each other and under 30 ms; the nominal-frequency
conversion overestimates on a boosting chip, so the true figure is lower.
The wall-clock gate itself still needs an idle machine and was not made
green here.

```text
cargo test -q -p vitals-win --lib                 → 594 passed (before the last two disk/owner tests)
cargo test --workspace --exclude vitals-win       → every crate ok
cargo clippy --workspace --all-targets -D warnings → Finished (0 warnings)
cargo fmt --all -- --check                        → clean
scripts/check-drift.ps1                           → 0 failure(s), 0 warning(s); 12 ts_rs, 56 invokes, 56 commands, 2 locales
run-build.ps1 pnpm typecheck                      → 6/6 tasks
run-build.ps1 pnpm lint                           → 6/6 tasks
run-build.ps1 pnpm test                           → ui 104, desktop 874 (+ Sidebar indicator, AnimatedValue 11)
prettier --check                                  → All matched files use Prettier code style!
scripts/check-size.ps1                            → initial 184.5 KB / 184.5 KB, shipped 373.5 / 373.5, hud 96.6 %, mobile 99.2 %
cargo run --release --example prove_devices       → C: Nvme 16345302 B/s read 12.2 % 0.19 ms; D: Ssd; E: Nvme; H: Removable
                                                    GPU NVIDIA GeForce RTX 3060 Ti (virtual display) vram -
                                                    GPU NVIDIA GeForce RTX 3060 Ti util 29.2 % vram 8.4 GB
cargo run --release --example cost_probe          → disk counters x4: 0.019 ms; owners x705: cold 3.945 ms, warm 0.035 ms
node .copilot-tmp/ui-audit.mjs (12 sections, CDP) → 0 console errors after the transition fix (3 InvalidStateError before)
live probe                                        → indicator translateY(84px), active row top 116 == indicator top 116
live text                                         → "DISK Read 13.7 MB/s" · "NVIDIA GeForce RTX 3060 Ti (virtual display)" ·
                                                    "C: Windows NVMe SSD … H: External Removable" · "explorer.exe 2188 Running vladu"
```

Not updated, with reason: `packages/client` SDK and `openapi.yaml` — no
field changed shape, only values that were `0`/`None`/hex are now real, so
the wire contract and generated bindings are untouched (drift gate green).
Locales — no new user-facing string; "(virtual display)" is an adapter name
from the backend, in line with every other hardware name. CLI — reads the
same `SystemMetrics`, so `vitals top` gains disk rates and owners for free;
not re-driven live this session. Installer budget — not built.

### 2026-09-11 — S11-11 Replace Task Manager with Vitals (no commit yet)

**Why.** The user asked for Vitals in the taskbar's right-click menu, above
Task Manager. Windows has no API for adding an entry there; the only
supported mechanism — the one Process Explorer has used for twenty years —
is the Image File Execution Options `Debugger` value on `taskmgr.exe`, which
redirects _every_ route to Task Manager (taskbar menu, `Ctrl+Shift+Esc`,
`Ctrl+Alt+Del`, `Win+X`, typing "taskmgr") to us.

**Shape.** `vitals-win::actions::taskmgr` owns the key. Three guards, each
for a real hazard: it is `HKLM`, so a denied write re-launches Vitals
elevated with `--set-taskmgr-replacement on|off` and then **re-reads the key**
(an elevated child exiting 0 is not evidence); it is shared, so a hook owned
by another tool is reported as `ReplacedByOther` and refused, never
clobbered; and it intercepts our own launches, so the real Task Manager is
started as a debuggee (`DEBUG_ONLY_THIS_PROCESS`, drain the initial events,
`DebugSetProcessKillOnExit(FALSE)`, detach). The switch is not an
`AppSettings` field on purpose — the truth is machine-wide and can change
while Vitals is closed, so it is read from the registry on open and after
every change.

**The bug that made this hard.** `taskmgr.exe` always starts unelevated and
immediately re-launches _itself_ elevated via AppInfo. The debuggee
exemption covers only the process we create; the elevation hop is an ordinary
`CreateProcess`, so IFEO caught it and turned it into a second Vitals. The
real Task Manager lived about 400 ms and vanished, while the command
returned `Ok`. My first hypothesis — check whether the parent is
`taskmgr.exe` — was wrong, and the prover said so: the hop arrives with
**parent `None`**, because AppInfo spawns it and the first Task Manager has
already exited. The discriminator that works is the **token**: a person's
shortcut is never elevated, the hop always is. So an elevated launch carrying
Task Manager's command line hands straight back to the real program and
exits without creating a window. Documented limitation: a user whose whole
shell is elevated gets the real Task Manager from the hotkey.

**Ripple.** Settings → General gains the switch (disabled, with the owner's
name, when another tool holds the hook) and an "Open Windows Task Manager"
button; the tray menu gains the same item, enabled only on Windows. EN + RO.
The NSIS `PREUNINSTALL` hook now deletes the `Debugger` value **only when it
names Vitals** — without it, uninstalling leaves `Ctrl+Shift+Esc` pointing at
a deleted file. Not added to the CLI, SDK or Prometheus: this is a
desktop-shell setting, not a reading, and none of them has a surface for it.

**Gates.**

```
cargo fmt --all -- --check                                → exit 0
cargo clippy -p vitals-win -p vitals-desktop --all-targets -- -D warnings → exit 0
cargo test -p vitals-win --lib taskmgr                    → 7 passed
pnpm -C apps/desktop exec vitest run                      → 88 files, 874 tests passed
pnpm typecheck                                            → exit 0
pnpm exec eslint apps/desktop/src packages/protocol/src   → exit 0
pwsh scripts/check-drift.ps1                              → 0 failure(s), 0 warning(s), 56 invokes / 56 commands
pwsh scripts/check-size.ps1                               → within budget (shipped raised 1467 B to measured 376596)
```

**Live** (dev binary, `StartTime 09:12` after the edit; hook pointed at
`target\debug\vitals-desktop.exe`):

```
get_taskmgr_replacement (hook = prover) → {"enabled":false,"replacedBy":"...prove_taskmgr_parent.exe"}
set_taskmgr_replacement(true) on that   → refused: "... is currently registered ... remove it in that application"
set_taskmgr_replacement(true)           → {"enabled":true,"path":"E:\\gh\\remi\\target\\debug\\vitals-desktop.exe"}
launch_real_taskmgr, hook ON            → Taskmgr(65756) → vitals(60380) → Taskmgr(72920) ALIVE, title "Task Manager"
Start-Process taskmgr.exe, window minimised → vitals-desktop launched with "C:\WINDOWS\system32\Taskmgr.exe",
                                             minimised:False, foreground:True, taskmgr count 0, vitals instances 1
```

The middle line is the whole feature: the hop through Vitals is invisible,
and the real Task Manager survives it.

### 2026-09-11 — S9-07 Measured startup impact (no commit yet)

**Why.** The Startup screen listed what runs at logon but had no cost figure;
`startup/mod.rs` explained that Task Manager's rating comes from an OS boot
trace and refused to fake one from image size. This measures instead: during
the first 120 s of **uptime** (not of the app's life) the sampler's own
per-process CPU % and disk rates are folded, per executable, into CPU-ms and
bytes using the _measured_ `elapsed_ms`. Anchoring on `uptime_secs` is the
point — a Vitals launched ten minutes into a session yields `None`, never a
window of zeroes. The finished window is written to
`%LOCALAPPDATA%\Vitals\startup-impact.json` (same best-effort pattern as
`lan-tokens.json`) and reloaded at start so the screen shows _last_ boot's
figures with the date they were measured.

**Shape.** `vitals-win::startup::impact` is pure (no syscalls); the desktop's
`startup_impact.rs` owns the store, the file, and the `executable_path` resolve
closure — paid once per process and only inside the window; the sampler skips
building observations at all once it closes. Matching is by normalised image
path (case-folded, `/`→`\`, quotes stripped) with file-name fallback for
processes that deny the handle. `vitals-core::StartupImpact` (three `Option`
fields, `number | null` bindings) rides on `StartupEntryDto.impact`; the
snapshot carries `impactMeasuredAtMs` for the caption. Column renders
`formatCpuSeconds` + `formatBytes`, em dash for `null`. Desktop-only: not
added to Prometheus, the SDK or the CLI — none of them exposes the startup
inventory today, so there is no sibling surface to keep in step.

**Gates.**

```
cargo fmt --all -- --check                                   → exit 0
cargo clippy --workspace --all-targets -- -D warnings        → exit 0
cargo test -p vitals-win -p vitals-core -p vitals-desktop    → 143 + 36 + 579 passed
  (impact.rs: 11 tests incl. C:\A\b.EXE ≡ c:\a\b.exe, outside-window
   not counted, uptime>120 at first sample → None, boundary tick clipped,
   path resolved once per process)
cargo test -p vitals-core --features ts                      → StartupImpact.ts generated, in index.ts
pnpm typecheck                                               → exit 0
npx eslint apps/desktop/src                                  → exit 0
pnpm -C apps/desktop exec vitest run src/features/startup    → 4 files, 52 tests passed
pwsh scripts/check-drift.ps1                                 → 0 failure(s), 0 warning(s)
curl.exe -s http://127.0.0.1:7330/api/v1/health              → {"ok":true,"version":"0.1.0","modelVersion":1}
```

Live: dev binary relaunched after the edit (StartTime 06:50 > edit 06:34).
This machine's uptime was 5688 s, so no `startup-impact.json` was written and
the column shows em dashes under "Startup cost has not been measured yet" —
the honest state; the first boot with Vitals in the Run key fills it in.

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

## S13 — public beta 0.9.0-beta.1 (2026-09-28)

Decided with the owner (ADR 0031): public repository, beta version, no code
signing yet, Starlight website, every channel wired but gated, macOS/Linux
built but not published, background updates installed on quit.

Why these and not others is in ADR 0031; what shipped is S13-01..08 in
`tracker.csv`. The two findings the pre-release audit turned up that were not
features: the Privacy panel and README claimed "nothing listens" while
127.0.0.1:7330 and the attach pipe are always open, and pairing secrets sat in
plain text on disk. Both are fixed at the source.

### Verification log

- Gates in a clean worktree at 4f6f283 before any change: all green except a
  doctest resolution error introduced mid-slice (fixed); perf budget median
  19.0 ms against 30 ms on a quiet machine.
- After the slice: clippy `-D warnings` workspace green; vitals-server 144,
  vitals-store, vitals-core, cli, desktop tests green; vitest 946/946;
  drift 0 failures 0 warnings.
- Signed release build: `Vitals_0.9.0-beta.1_x64-setup.exe` 4.28 MB (budget
  4.55 MB) plus `.sig`; payload unpacks in 387 ms to 12.05 MB.
- Site: 25 pages built; /privacy/, /terms/, /ro/privacy/, /ro/terms/ present.
