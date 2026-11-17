# Reality check — 2026-11-12 (release 1.2.0 in the field, 1.3.0 planned)

Ten persona passes (product, design and motion, frontend, backend and security,
Android, Apple, QA and release, growth and competition, accessibility and
localisation, legal) read the repository at `ae5b82d` (main, two commits ahead
of origin) without editing it. Every P0 below was then checked by hand on this
workstation, on `dragos-vivobook`, against the live update endpoint and against
GitHub. Anything not confirmed is marked EXPECTED. The full persona reports are
scratch files (`.copilot-tmp/rc1112/*.md`, not committed); the facts that matter
are here and in `tracker.csv` slice **S24**.

Severity: P0 blocks a release, loses users or breaks a store or privacy
declaration; P1 is a visible defect or a false public claim on a core flow; P2 is
polish.

## What is live today

| Channel                           | Live                                                   | Gap                                        |
| --------------------------------- | ------------------------------------------------------ | ------------------------------------------ |
| GitHub release                    | v1.2.0 (2026-10-28), `latest.json` 1.2.0, all URLs 200 | release body lists Windows downloads only  |
| winget / Scoop / Chocolatey / npm | 1.2.0 everywhere (Choco approved 2026-10-30)           | Choco icon broken; Scoop notes stale       |
| Microsoft Store 9P95RK5VMGPJ      | 1.2.0.0 x64                                            | no arm64 package                           |
| Google Play (phone, wear, TV)     | 1.1.1                                                  | 1.2.0 drafts; listing still "Windows only" |
| App Store (iPhone + Watch)        | 1.2.0 READY_FOR_SALE                                   | contains the Health page, undeclared       |
| Mac App Store                     | rejected (4.3 / 4.2.6), reply sent 2026-11-11          | frozen until Apple answers (S23-16)        |
| Installed copies                  | this PC 1.1.0; vivobook 1.0.4 test build               | see P0-1 and the vivobook note             |

## P0 — verified

| id   | Finding                                                                                                                                                                                                                                                                                                                                                   | Evidence                                                                                                                                                                           | Fix (S24)      |
| ---- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------- |
| P0-1 | **A copy installed "for everyone" never updates by itself.** The unelevated app plans `NeedsApproval`; quit returns early and the idle install needs `Silent`. The only signal is a tray item that becomes clickable. This workstation has been downloading 1.2.0 since 2026-10-29 and is still on 1.1.0.                                                 | `HKLM\...\Uninstall\Vitals VitalsScopeChosen=1`; `vitals.log` 7× "update verified; it installs when Vitals quits" (10-29 … 11-10); `install.rs:119`, `updates.rs:181-183, 625-629` | S24-02         |
| P0-2 | **The iPhone app on sale contains the Health page and the listing denies it.** Build 252 = commit `25a3bc1`, which adds `HealthView.swift`; `listing.json` says the app "talks only to the PCs you pair" and App Privacy says "Data Not Collected". Health reads heart rate, SpO₂, sleep, weight and blood pressure from a vmui server over the internet. | `git ls-tree 25a3bc1 -- apps/ios` lists `HealthView.swift`, `HealthStore.swift`; `apps/ios/store/listing.json:12,24`; `PrivacyInfo.xcprivacy`                                      | S24-07         |
| P0-3 | **The next Android release breaks the Play declaration.** Play docs say "Health app: No" and Data safety lists only ML Kit; the phone app has a Health tab with no flavour gate. Play live (1.1.1) predates it, so the breach lands with the first 1.2.x/1.3.0 upload.                                                                                    | `apps/android/play/README.md:49`; `SettingsScreen.kt:153`; `GlassScaffold.kt:115`                                                                                                  | S24-07         |
| P0-4 | **The Microsoft Store listing says "Nothing leaves your computer unless you turn on remote access"**, while 1.2.0 ships codai sign-in (`ai.codai.ro`) and vmui sign-in. PRIVACY.md and the site say "one automatic request (GitHub)" and never mention codai.                                                                                             | `packaging/msix/listing/en-us.md:54`; `git grep ai.codai.ro v1.2.0 -- crates/vitals-agents/src/cloud.rs`; `PRIVACY.md`; `apps/site/.../privacy.md:17`                              | S24-06, S24-07 |
| P0-5 | **`cargo audit --deny warnings` fails on HEAD**: RUSTSEC-2026-0459 (`libsqlite3-sys` 0.38.2, fix ≥ 0.39), RUSTSEC-2026-0486 (`rustix` 1.1.4 unsound), `async-executor` 1.14.0 yanked. CI's Supply chain job is red on every open PR; a tag today would not publish.                                                                                       | `cargo audit --deny warnings` exit 1 on 2026-11-12; PR #37 "Supply chain" failing                                                                                                  | S24-03         |
| P0-6 | **On macOS, nine sidebar pages call commands compiled only for Windows** (connections, startup, services, installed apps, storage, devices, benchmarks, app history, users) and show raw errors. Only `health` is gated. The same build is what Apple is reviewing. EXPECTED on a real Mac; proven by `cfg` gates.                                        | `navigation.ts:156-160`; `lib.rs:283-385` `#[cfg(windows)]`                                                                                                                        | S24-04         |
| P0-7 | **No overlay animates.** `overlayMotion` is a CSS _transition_ on `data-state`; Radix presence only waits for `animationend` and mounts content already open, so dialogs, menus, the command palette, selects and tooltips pop with no enter or exit. The owner's bar is "animations, transitions and morphing".                                          | `packages/ui/src/lib/styles.ts:53-54`; `@radix-ui/react-presence/dist/index.mjs:85-106,169`                                                                                        | S24-05         |

**Not a bug (corrected):** `dragos-vivobook` runs a 1.0.4 _test_ build whose
updater endpoint is `http://127.0.0.1:8765/latest.json` and the app is not in
the user's Run key (only the watchdog is). That machine is a stale test
install, not evidence of an updater failure. It gets a clean 1.3.0 install in
S24-16.

## P1 — by surface (the ones that change scope)

- **Tracker truth.** S21-09, S21-10, S23-06, S23-08 and S23-10 are done but
  marked `in-progress`; S23-06 and S23-10 have two status fields pasted into
  one row; S23-15 says "waiting on Apple" though the multicast entitlement was
  declined on 2026-11-06; the `TRACKER.md` Slices table stops at S20 and still
  shows S14/S15 as `doing`; S14-05 (Turbo MFT scan) is unwired code and S14-06
  was never built, with no CSV rows. → S24-01.
- **False public claims.** README/site: processes "that live for under a
  second … are recorded" (the helper is a stub, `host.rs:37-43`); the lag
  watchdog "on by default" (opt-in, `watchdog.rs:372-409`); "about 3 % of one
  core" (measured whole-app hidden idle 10.7 %, `idle-budget.json`); "Vitals
  1.0"; "Google Play listings are coming later"; "the CLI is not bundled";
  iPhone "in review" and named "Vitals: System Monitor" (shipped as "Vitals
  Monitor"). → S24-06.
- **Desktop.** A stopped sampler (crash, update) freezes every number with no
  stale indicator (`metrics.ts` `onGap` has no subscriber); raw Windows error
  text reaches Romanian users through `{{message}}` (90+ templates); the
  Columns menu (19 rows) is taller than the minimum window; `[object Object]`
  on a failed agent action; floating promises in `App.tsx`; `?? 0` renders
  invented zeros in six places; the Watchdog panel sticks on "Checking…" off
  Windows. → S24-08.
- **Android.** Alerts reach the watch twice; the watch Sensors list flips
  empty every 15 s; a revoked token shows "waiting" forever; the FGS loop has
  no wake lock; no tablet/foldable layout; `app` and `tv` modules have zero
  tests; Play description still says "Windows PC". → S24-09.
- **Apple.** The watch only updates while the iPhone UI is open; the "Scan
  code" quick action does nothing; WebSocket session has a 15 s resource
  timeout (EXPECTED to drop every 15 s); process controls ignore the PC's
  capabilities; `process_count` and disk `active_time` are fabricated zeros on
  macOS; no battery on MacBooks; Windows caption buttons instead of traffic
  lights; the MAS listing claims benchmarks and clock speed the Mac build lacks.
  → S24-04, S24-10.
- **Release plumbing.** `update-proof.yml` has never run; `version.ps1` misses
  `apps/ios/project.yml` and `apps/tizen/public/config.xml`; no scheduled CI,
  so main's supply-chain state is unknown between pushes; dependabot PRs
  #27/#34/#35/#36 red for 11+ days; MSIX build, Play "send for review", iOS
  and Tizen submissions are manual. → S24-03, S24-15.
- **Tizen / CLI / a11y / i18n.** Tizen targets Chrome 120 while `config.xml`
  claims Tizen 6.0; reconnect button labelled "Connecting…"; stale readings at
  full opacity; 20 px body text on a TV. CLI emits a raw ANSI escape ignoring
  `NO_COLOR`; `top` has no non-TTY fallback; docs say "cargo install" only.
  Axe gate runs on the no-host fallback only. Romanian glossary drift ("Închide"
  vs "Încheie procesul", "Mare" vs "Ridicată"). → S24-14.
- **Legal.** PRIVACY/TERMS predate codai and vmui; no medical-advice clause;
  pairing consent omits that other Windows accounts' processes are visible;
  LGPL PawnIO modules embedded via `include_bytes`; CRA reporting duties apply
  from 2026-09-11 (Commission page, verified). → S24-07, S24-06.

## Decisions (owner, 2026-11-12, askQuestions)

| #   | Question               | Decision                                                                                                                                                                                                                                                                                                |
| --- | ---------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| D1  | Packaging              | One release **1.3.0** with everything; no separate hotfix.                                                                                                                                                                                                                                              |
| D2  | Health in store builds | **Keep and declare** everywhere (Play Health apps form + Data safety, App Privacy, review notes with a demo vmui, MS Store text, PRIVACY, TERMS medical clause).                                                                                                                                        |
| D3  | Per-machine updater    | Toast once a day + tray badge + **one-click move to a per-user install**. No elevated scheduled task.                                                                                                                                                                                                   |
| D4  | Motion                 | **Option C**: View Transitions + CSS (`@starting-style`, exit keyframes, shared sliding indicator, Collapsible, `useViewTransition`). Not `domMax`/`layoutId`.                                                                                                                                          |
| D5  | New features in 1.3.0  | All ten: alert rules with duration; notification channels (webhook, ntfy, Discord, HA); live tray number; "PC or internet" probe; service-stopped alert; MQTT with HA discovery; read-only MCP server; iPhone local notifications + widget; Mac battery + menu-bar readout; Android tablet list-detail. |
| D6  | Apple submissions      | iPhone update **yes** (declares Health); Mac App Store stays frozen until Apple answers S23-16.                                                                                                                                                                                                         |
| D7  | macOS scope            | Hide Windows-only routes via capabilities, native window chrome, em dash for fabricated zeros.                                                                                                                                                                                                          |

## Competitive and platform notes that shaped D5

- Most-requested features across System Informer, Stats, Beszel, btop and
  LibreHardwareMonitor trackers: alerts that fire on a sustained threshold with
  delivery channels, GPU breadth, signed binaries (AV false positives), history,
  machine groups, custom metrics, translations.
- Glances ships `--enable-mcp`; Windows now has an MCP on-device registry with
  consent. A read-scope MCP server is cheap on top of the LAN API.
- Web platform: View Transitions, `@starting-style`, `transition-behavior:
allow-discrete` and `linear()` springs work in WebView2, WKWebView (Safari
  17.4/18+) and WebKitGTK 2.54; `interpolate-size` is Chromium-only, so
  collapses use the `grid-template-rows: 0fr → 1fr` form.

## Done means (for 1.3.0)

1. Every S24 row `done` with evidence, or `blocked` with the reason.
2. `scripts/verify.ps1` green on ws-vitals-win and the Linux/Apple CI jobs
   green on the release commit; `cargo audit --deny warnings` and `pnpm audit
--audit-level high` clean.
3. `update-proof.yml` run for 1.1.0 → 1.3.0 (per-machine chosen, unelevated:
   toast appears, one-click move works) and 1.2.0 → 1.3.0 (per-user, idle).
4. On `dragos-vivobook`: clean 1.3.0 install as a standard user, the S21-08
   list, overlay/tab/collapse animations seen, a phone and the TV paired.
5. This workstation reinstalled on 1.3.0 per-user, sensors service and
   watchdog running, no 1.1.0 remnants.
6. Published: GitHub + `latest.json`, winget, Scoop, Chocolatey, npm, MS Store
   submission, Play drafts sent for review with the Health declaration, iPhone
   1.3.0 submitted with App Privacy updated; MAS untouched.
7. Docs: CHANGELOG, README, site EN+RO, PRIVACY, TERMS, store listings,
   tracker, ADR for the motion system and for notification channels.
