# Launch readiness — reality check, 2026-10-17

Eleven read-only specialist reviews (design, engineering, QA, security, legal,
release/PM, Android/Play, growth, accessibility + i18n, performance/SRE,
docs/DevRel). Nothing was edited while auditing. IDs are stable so fixes can
cite them. Spot-verified by the coordinator: DEV-1/QA-2 (no sensors uninstall
in `hooks.nsh`), SEC-12 (`minimumReleaseAge: 0`), DOC-2 (CLI never built in
`release.yml`), DEV-8 (`max_bytes` never read), DEV-3 (`perMachine`), PM-3
(README "Status: 0.9").

## Scores

| Area            | Score | Area                 | Score                    |
| --------------- | ----- | -------------------- | ------------------------ |
| Design / visual | 5.5   | Android / Play       | 6.5                      |
| Engineering     | 7     | Growth / market      | 6.5 (product 8.5, GTM 4) |
| QA              | 5.5   | Accessibility + i18n | 6                        |
| Security        | 7     | Performance / SRE    | 5                        |
| Legal           | 6     | Docs / site          | 5.5                      |
| Release / PM    | 6.5   |                      |                          |

## P0 — blocks launch

| ID                           | Problem                                                                                                            | Fix                                                                                          |
| ---------------------------- | ------------------------------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------- |
| DES-1/2                      | Store screenshots: pre-1.0, personal folders + username, 99 % CPU, visible Storage layout bug                      | Demo profile / fixture mode, re-shoot both themes with captions; fix Explorer footer overlap |
| DES-3, DOC-16                | Site landing has no product screenshot (Starlight splash + logo)                                                   | Custom hero with real capture/WebM, comparison strip                                         |
| DES-4, I18N-1                | 64 Processes strings exist only as English fallbacks (`processes/strings.ts` `MISSING_KEYS`), incl. RiskDialog     | Move to en+ro locales, delete `fallback()`, parity test bans `t(key, default)`               |
| DES-5                        | "Why is my PC slow?" is a ghost sm button                                                                          | Health strip across Dashboard, tray + jump list entry                                        |
| A11Y-1                       | `<html lang="en">` never follows the language                                                                      | Set `documentElement.lang` on language change                                                |
| A11Y-2                       | No `forced-colors` support (Windows Contrast themes erase selection, meters, switches, h2)                         | forced-colors blocks                                                                         |
| DEV-1, QA-2, SEC-14, PERF-11 | Uninstall leaves LocalSystem `vitals-sensors` service + PawnIO; `Run\Vitals` autostart value too (DEV-7)           | PREUNINSTALL runs `vitals-sensors.exe uninstall`, deletes Run values; prove on vivobook      |
| QA-1/3/5                     | Install/upgrade/uninstall, updater and real-backend e2e never tested                                               | Windows CI install job; minisign verify; CDP e2e vs real exe                                 |
| QA-4, PM-2                   | Release pipeline deadlocked: nightly waiting approval 2 days holds the concurrency group                           | Reject runs; nightlies get own group / no approval env                                       |
| PERF-1/2, LEG-15             | "Under 1 % CPU and 80 MB, enforced by CI" false: only a 30 ms sampler wall-time test, no RAM gate; comment off 10× | Real idle gate (exe + WebView2 tree) or reword now                                           |
| LEG-1                        | Watchdog auto-enabled + HKCU Run at first launch, contradicting PRIVACY.md                                         | Opt-in at first run, or disclose + switch                                                    |
| PM-1, DOC-7                  | Chocolatey 1.0.0 blocked on moderator (iconUrl → jsDelivr)                                                         | Fix nuspec, resubmit                                                                         |
| PM-3/4, DOC-4/5/6/8          | README/site say 0.9, winget "coming soon" (live), no MS Store link anywhere                                        | Truth pass + Store badge first                                                               |
| DOC-1                        | Docs say `@vitals-app/client@next` → installs 0.9.0-beta.1                                                         | Drop `@next`, retag                                                                          |
| DOC-2                        | Docs say `vitals` CLI ships with the app; it is never built/released                                               | Ship `vitals.exe` (resource + PATH) or change docs                                           |
| DOC-3                        | npm package has no README                                                                                          | Write it                                                                                     |
| AND-1/2/3                    | Android crash paths: body read IOException → uncaught; `START_STICKY` FGS restart; FGS start from background       | Catch in client, `START_NOT_STICKY`, lifecycle guard                                         |
| AND-4                        | Wear shows raw keys `alert.cpu_hot.title`                                                                          | Port phone mapping + strings                                                                 |

## P1 — fix before or right after launch

- **Security:** SEC-1 IFEO Debugger may point at a user-writable exe (Scoop/portable) → silent elevation; SEC-2 `ProgramData` log junction write as admin; SEC-3 env-derived `ProgramFiles`/`SystemRoot` in elevated paths (verify on vivobook); SEC-4 no Authenticode; SEC-5 minisign key reachable from unreviewed build job; SEC-12 pnpm quarantine off.
- **Engineering:** DEV-2 HUD window built in sync command (Tauri-documented deadlock); DEV-3 per-machine passive update = UAC on quit, silently dropped; DEV-4 mapped network drive stalls sampler + sync commands; DEV-5 SQLite commands on main thread; DEV-6 manual update `exit(0)` skips teardown; DEV-9 non-atomic token file; DEV-12 capabilities say NotImplemented for shipped features; DEV-15/PERF-7 any emit error kills sampler.
- **Performance:** PERF-3 sampler wakes 20×/s even hidden; PERF-4 watchdog wakes ~64–100×/s; PERF-5 flight recorder writes SQLite every tick with history off (also LEG-2 privacy); PERF-6 preload of every route on hidden autostart; PERF-8 no minidumps / no restart; PERF-9 WebView2 crash → blank window; PERF-10 update check once per lifetime.
- **Design:** DES-6 Inter/JetBrains Mono not shipped (font depends on machine); DES-7 11 px everywhere; DES-8 "Not reported by…" truncated instead of em dash; DES-9 duplicate %; DES-10/A11Y-22 settings tab 4.12:1; DES-11 no first run; DES-12 diagnosis has no actions; DES-13 empty chart history; DES-16 install flow leads with SmartScreen warning.
- **Accessibility/i18n:** A11Y-3 process grid silent to screen readers + focus off-screen; A11Y-4 order not frozen for keyboard; A11Y-5/6/8 warn/danger/subtle text contrast; A11Y-7 undefined tokens; A11Y-9 charts no text alternative; A11Y-14 no zoom; I18N-2/3/4 RO number/unit formatting; I18N-6/7/8 inconsistent terms and jargon ("Nucleu" for kernel).
- **Legal:** LEG-3 PawnIO/SYSTEM service undisclosed; LEG-4 privacy scope misses TV/Tizen/CLI; LEG-5 LAN binds 0.0.0.0 (also DEV-19, SEC-9); LEG-9 NSIS ships no notices; LEG-10 LGPL PawnIO modules unattributed; LEG-12 no OSS licences screen on Android/Tizen; LEG-21 "Vitals" trademark uncleared (GNOME "Vitals" system monitor exists).
- **Android:** AND-5 private-address check bypass (`10.0.0.1.evil.com`); AND-6 bearer token over cellular/any Wi-Fi; AND-7 control tokens copied to watch; AND-8 unknown enum breaks streams; AND-9..16 crash/state robustness; AND-19 Wear standalone dead end; AND-22..24 TV D-pad; AND-26..28 TalkBack/font scale; AND-31/32 privacy + Data safety.
- **QA:** QA-6 destructive commands untested; QA-7 WS control scope; QA-8 Android modules not compiled on PR; QA-10 decorative tests listed; QA-11 flaky wall-clock gates; QA-16 ARM64/Win10 never tested.
- **Docs/PM:** DOC-9 Android/Wear/TV never mentioned; DOC-10/11 CONTRIBUTING wrong pnpm/dev command; DOC-12 CLI flags; DOC-14/DEV-10 SDK lacks summary/history/sensors/pair; DOC-17 no JSON-LD; DOC-18 GitHub social preview; PM-6/7/8/9 terms, SECURITY, fact sheet, Store "What's new"; PM-10/11 tracker S16-12 stale.

## P2

Kept in the agents' full reports (DES-17..26, DEV-13..23, QA-17..23, SEC-6..18,
LEG-6..30, PM-12..18, AND-33..48, A11Y-15..23, I18N-9..18, PERF-12..20,
DOC-19..28). Regenerate by re-running the persona prompts if needed.

## Growth summary

- Value prop: "The Task Manager Windows should have shipped: opens instantly, tells you why your PC is slow, follows you to your phone."
- ICP order: Windows power users/devs → homelab/self-hosted → everyday users (SEO/Store) → gamers later (needs FPS overlay).
- Closest competitor: AppControl (closed source, press coverage). Benchmark: Stats (macOS) 42.6k stars, no Windows equivalent.
- Missing for launch: signing or Store-first CTA, portable zip, README hero GIF, open-time vs Task Manager video, compare pages, 20–50 seed stars.
- Monetisation: Sponsors now; later opt-in E2E "Vitals Relay" (off-LAN remote, multi-PC history).
- Ideas: boot impact report, PresentMon FPS in HUD, elevated helper, "what slowed my PC last night" timeline, shareable local diagnosis report, local MCP server.

## Signing options (SEC research)

SignPath Foundation (free, OSS, publisher shown as SignPath) · Azure Artifact
Signing ~$10/month (EU needs a legal entity) · Certum OSS / OV ~$100–215/year ·
EV not worth it (no SmartScreen advantage since 2024). Store build already
bypasses SmartScreen.

## Decisions (owner, 2026-10-17)

1. Windows launches first; Android/TV follow after Play approval.
2. Microsoft Store is the primary download button; apply to SignPath Foundation (free OSS signing).
3. Watchdog becomes opt-in via a first-run card (LEG-1).
4. Reword the CPU/RAM claim now; add a real idle gate (exe + WebView2 tree), then re-claim measured numbers.
5. Keep the name; owner commissions a trademark clearance (classes 9 + 42).
6. Scope: all P0 + P1 on every surface, plus launch assets.
7. Small commits on main; push after `scripts/verify.ps1` passes.

## Execution waves (file-partitioned so parallel agents never share a file)

| Wave | Lane                                       | Items                                                          | Files owned                                                                                                                               |
| ---- | ------------------------------------------ | -------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------- |
| 1    | Installer                                  | DEV-1, DEV-7, SEC-14                                           | `apps/desktop/src-tauri/windows/hooks.nsh`                                                                                                |
| 1    | Processes i18n                             | DES-4, I18N-1, I18N-5                                          | `features/processes/strings.ts`, `packages/i18n/src/locales/*`                                                                            |
| 1    | A11y foundations                           | A11Y-1/2/5/6/7/8/22, DES-10                                    | `index/hud/mobile.html`, i18n init, `packages/ui/src/styles/theme.css`, `SettingsDialog.tsx`, `WatchdogPanel.tsx`, `Explorer.tsx` tooltip |
| 1    | Docs truth pass                            | PM-3..9, DOC-1..8/10..13/15/19..22, PERF-1 wording, LEG-15..19 | README, site content, SECURITY, CONTRIBUTING, fact sheet, `packages/client/README.md`, chocolatey nuspec                                  |
| 1    | Pipeline                                   | QA-4, PM-2, SEC-5, SEC-12, DOC-2                               | `release.yml`, `pnpm-workspace.yaml`, `tauri.conf.json`                                                                                   |
| 1    | Rust robustness                            | DEV-2/4/5/12/15, PERF-7, PERF-2                                | `hud.rs`, `sampling.rs`, `store.rs`, `host.rs`, `disk/volumes.rs`, `overhead.rs`                                                          |
| 1    | Android P0                                 | AND-1..5, AND-9, AND-11, AND-12                                | `apps/android/**`                                                                                                                         |
| 2    | First run + health strip + watchdog opt-in | DES-5, DES-11, LEG-1, DEV-22, PRIVACY                          | dashboard, `watchdog.rs`, PRIVACY.md, locales                                                                                             |
| 2    | Security P1                                | SEC-1/2/3/6/13/15, DEV-19                                      | `taskmgr.rs`, `commands.rs`, `vitals-sensors/install.rs`, router/server                                                                   |
| 2    | Performance P1                             | PERF-3/4/5/6/8/9/10, idle gate                                 | `sampling.rs`, watchdog, recorder, routes, updates                                                                                        |
| 2    | A11y/i18n P1                               | A11Y-3/4/9/14, I18N-2..8                                       | process grid, charts, format.ts, locales                                                                                                  |
| 3    | Design polish                              | DES-6..9/12/13/17/20/21/22                                     | UI components                                                                                                                             |
| 3    | Legal packaging                            | LEG-2..5/9/10/12/13                                            | notices, PRIVACY, Android licences                                                                                                        |
| 3    | QA gates                                   | QA-1/3/5/7/8/10/11                                             | CI install job, e2e vs real exe                                                                                                           |
| 3    | Launch assets                              | DES-1/2/3/26, DOC-16/17, compare pages, hero GIF               | site, store listing (captures in cloud)                                                                                                   |
| 4    | Android P1                                 | AND-6..48                                                      | `apps/android/**`                                                                                                                         |

## Status (2026-10-17, end of day)

Shipped to `main`; CI run 39016375831 green on all 12 jobs, including the installer round trip.

| Commit  | Tracker    | Scope                                                                                                                                                                                                                                                                                                                          |
| ------- | ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 98eff6e | S17-01..12 | uninstall cleanup, processes i18n, a11y foundations + P1, docs truth pass, pipeline (nightly/tag split, updater key behind approval, CLI zips, pnpm quarantine, Android CI), backend robustness, Android P0, watchdog opt-in + first run + health strip, security P1, performance P1, design polish, legal packaging, QA gates |
| f5c74bc | S17-13     | Android P1 (network-only auth, read-only watch tokens, tolerant enums, error states, TV D-pad, TalkBack, privacy)                                                                                                                                                                                                              |
| 424e438 | S17-14     | real 1920×1080 captures from a clean devcloud VM, site hero, compare pages, FAQ, JSON-LD, OG image                                                                                                                                                                                                                             |
| 0bf1eed | S17-15     | CI: Node 24, updater without `zip` (RUSTSEC-2026-0367..0369), installer budget for bundled licences                                                                                                                                                                                                                            |

Gates run on devcloud: `ws-vitals-win` (Windows, c4d-standard-16; verify.ps1 in about 4 min, perf
budget median 2.5 ms on an idle VM) and `ws-codai` (Linux, TypeScript lanes).

### S19 — launch P2 and 1.0.3 (2026-10-17)

- S19-01 PERF-8: native crash dumps (`logs\crash.dmp`) and restart after a crash.
- S19-02 DEV-10: SDK `summary()`, `history()`, `sensors()`, `VitalsClient.pair()`.
- S19-03 DEV-21: CLI `kill`, `suspend`, `resume`, `priority`.
- S19-04: version 1.0.3, changelog, Store "What's new" EN+RO, `scripts/ci/update-proof.ps1`,
  launch post drafts in `docs/launch/posts.md`.
- Stuck nightly 38674240902 rejected (38843563046 had already been cancelled).
- 1.0.3 is tagged with `PUBLISH_PLAY=false` so the Play review in progress is untouched.
- **1.0.3 published** (tag `v1.0.3` on `d00d6a6`, release run 39022401994): GitHub release,
  winget, Scoop and npm 1.0.3. `latest.json` names 1.0.3 and both Windows signatures carry the
  app's minisign key id with 1.0.3 in the trusted comment.
- **Self-update proved** with `scripts/ci/update-proof.ps1`: on ws-vitals-win, 1.0.2 found and
  verified 1.0.3. On the vivobook, an elevated 1.0.2 found it, verified it, installed it on quit
  and relaunched as 1.0.3 ("up to date"). The same quit from an **unelevated** 1.0.2 ended in
  "operation was canceled by the user (os error 1223)": the per-machine passive installer needs
  UAC and nobody saw the prompt. That is DEV-3, now reproduced, and the most important engineering
  item left: most people run Vitals unelevated.
- **Microsoft Store**: submission 2 (MSIX 1.0.3.0 built on ws-vitals-win, 10 captured screenshots
  with captions in English and Romanian, 1.0.3 "What's new") is in certification.
- Chocolatey 1.0.3 push answered 403: 1.0.0 is still waiting for the moderator (item 2 below).
- **DEV-3 fixed** (S19-05, ADR 0038): `installMode: both`. A per-user copy updates on quit as
  the invoker (`RunAsInvoker`), so no UAC prompt. A 1.0.0–1.0.3 Program Files copy moves to a
  per-user install on its next unelevated update, and Settings › About then offers to remove
  the old copy. A Program Files copy someone chose waits for "Restart to update" (tray and
  Settings), where the prompt follows a click. Proven on the vivobook unelevated, both paths,
  with `scripts/ci/update-proof.ps1 -Local`. **Limit:** an unelevated 1.0.3 still runs the old
  updater and cannot reach the next release on quit; it gets there with one manual update.
- **Computer-use MCP on the vivobook** (S19-06): `vivobook-computer` in VS Code's `mcp.json`,
  codai-computer over ssh stdio (nothing listens on the network), proven by clicks.
- **Devcloud outage, fixed at the source:** codai's GPU reaper deleted `ws-codai` and
  `ws-vitals-win` at 12:15Z (no `reaper=keep`). The reaper now skips `app=devcloud` (codai
  `0768d3c59`), workspaces are created with `reaper=keep` (vmui `3d330fe`), `ws-vitals-win` was
  recreated. `ws-codai` is not recreated yet.

### Still open — owner only

1. Partner Center: watch certification of submission 2 (usually a few hours).
2. Chocolatey: reply to the moderator or resubmit 1.0.0 with the jsDelivr icon.
3. Apply to SignPath Foundation (free OSS signing).
4. Trademark clearance for "Vitals" (classes 9 and 42).
5. Play: wait for review; check whether production access already exists (Android launches separately).
6. `npm dist-tag rm @vitals-app/client next` (needs 2FA).
7. Real-OS checks on the vivobook (S17-02, S17-10): a portable copy is refused for the Task Manager hook, the sensors log is in Program Files, the CLI attaches from an elevated prompt, `netstat` shows the chosen adapter bind.
8. First release after ADR 0038: one hand-edited winget submission with `Scope: user` and `Custom: /CurrentUser` (Komac carries the old `Scope: machine` forward; `packaging/winget/README.md`).

### Remaining engineering (P2, not started)

DES-12 "End…" from the diagnosis dialog, DES-21 other empty-state actions,
SEC-8 query tokens, SEC-10 save path from IPC, SEC-16 PawnIO signer pin, DEV-6 manual update
teardown, DEV-9 atomic token file, DEV-11 headless serve
sensors, DEV-16 global HUD shortcut, ARM64 and Windows 10 test runs,
a JDK on devcloud for Android gates.
