# Store research — October 2026

Research for the 1.0.0 production release on the Microsoft Store and Google
Play, written on 2026-10-14. Every requirement below links to the page it came
from; anything that could not be confirmed from a primary source says so.
Listing text lives in `packaging/msix/listing/` and `apps/android/play/listing/`.

## 1. Character counts of the 1.0.0 listings

Counted as Unicode code points (what both consoles count) by a script, not by
eye. All within limits.

| Field                                         |                       EN |                       RO | Limit                          |
| --------------------------------------------- | -----------------------: | -----------------------: | ------------------------------ |
| Play title                                    |                       22 |                       25 | 30                             |
| Play short description                        |                       71 |                       72 | 80                             |
| Play full description                         |                     1923 |                     2235 | 4000                           |
| Play release notes (`changelogs/default.txt`) |                      390 |                      416 | 500                            |
| Store short description                       |                      194 |                      214 | 1000 (270 shown in some views) |
| Store description                             |                     2661 |                     3029 | 10 000                         |
| Store "What's new in this version"            |                      917 |                     1139 | 1500                           |
| Store product features                        |                20 × ≤ 75 |                20 × ≤ 95 | 20 × 200                       |
| Store keywords                                | 7 (14 words, longest 15) | 7 (14 words, longest 20) | 7 × 40, ≤ 21 words             |

Romanian text uses comma-below ș and ț throughout (checked: zero ş/ţ).

## 2. Microsoft Store (MSIX, `runFullTrust`)

### Listing limits (corrected)

The previous listing assumed a 100-character short description and 30-character
search terms. Partner Center's current limits are different:

- Description: required, up to **10 000** characters of plain text; no HTML,
  code or URLs in it.
- **What's new in this version** (formerly Release notes): up to **1500**
  characters. **Leave it blank on a first submission.** 0.9.0 was sent to
  certification on 2026-10-06; if it was published, 1.0.0 is an update and the
  field applies. _Not verified from here whether 0.9.0 went live._
- **Product features**: up to 20, no more than 200 characters each, no bullets
  of your own.
- **Short description**: 1000-character field; only the first ~270 show in
  some views; should not repeat the description's first paragraph.
- **Keywords** (formerly Search terms): up to **7**, up to **40 characters**
  each, and **no more than 21 words across all of them**. Partner Center can
  suggest keywords with an AI helper; none are added automatically.

Sources: [Store listing info for MSIX](https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/msix/add-and-edit-store-listing-info),
[Additional information (keywords)](https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/msix/add-additional-information).

### Images and trailers

- Minimum to complete the listing step: a description and **one** screenshot.
  Up to 10 desktop screenshots; Microsoft recommends 4+ (FAQ says 5–8). PNG,
  ≤ 50 MB each, no extra logos or marketing text on them.
  Desktop screenshots: 1366 × 768 or larger (third-party summary of the same
  page gives 1366 × 768 – 3840 × 2160; the Microsoft page's desktop line was
  elided in the fetch).
- **Our four screenshots are 1920 × 1080** (read from the PNG headers) — valid.
  They predate 1.0 (no program icons, temperatures or taskbar gauge); adding
  Processes-with-icons and Performance-with-temperatures shots is the cheapest
  listing improvement left.
- **1:1 App tile icon 300 × 300**: strongly recommended; overrides the
  package's icon in the Store. (Memory note: Partner Center input 4.)
- **16:9 Super hero art** 1920 × 1080 or 3840 × 2160 PNG, no title or text on
  it: recommended for all apps; required for trailers to appear at the top.
- Trailers: up to 15, ≤ 2 GB each, each with a 1920 × 1080 PNG thumbnail;
  key content centred because layouts crop top and bottom.
- Poster 2:3 (720 × 1080 / 1440 × 2160) and box art 1:1 matter for Xbox and
  games, not for this app.

Source: [Screenshots, images and trailers for MSIX](https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/msix/screenshots-and-images).

### Age rating

An IARC questionnaire on the Age ratings page, answered once; a rating ID from
another storefront can be reused. Google Play's IARC certificate for
`app.vitals` (expected Everyone / 3+) can be entered instead of re-answering.
Source: [Age ratings for MSIX](https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/msix/age-ratings).

### Restricted capability (`runFullTrust`)

Store approval is needed for restricted capabilities; sideloading needs none.
The justification is one textarea in the submission's options. The 1.0.0 text
(in `en-us.md`) now names each reason concretely — process control, drive
scan and Recycle Bin, UAC-gated Windows tools, the watchdog as a second
executable through `windows.startupTask` — because a vague "it is a Win32 app"
is the usual reason for a certification query.
Source: [App capability declarations](https://learn.microsoft.com/en-us/windows/uwp/packaging/app-capability-declarations).

### Certification risks for a system monitor (Store Policies 7.20)

| Policy                       | What it says                                                                                                     | Where Vitals touches it                                                                             | Mitigation already in place                                                                                                                       |
| ---------------------------- | ---------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------- |
| 10.1 Accurate representation | Metadata must reflect features and "important limitations"                                                       | Power usage is an estimate; CPU temperature needs the sensors service the Store build does not ship | Listing says "estimated" and "where Windows reports it"; no sensors-service claim                                                                 |
| 10.2 Security                | Must not compromise device security; use **supported methods** and **obtain consent** to change Windows settings | Startup items, services, DISM/Disk Cleanup, hibernation off, Explorer restart                       | Every change is user-initiated, UAC once per action, irreversible ones need a tick; Task Manager replacement (IFEO) is refused in the Store build |
| 10.2 Security                | "Must not offer to install secondary software not developed by you"                                              | None today                                                                                          | **Blocks the "winget update manager" idea in the Store build** (see §4)                                                                           |
| 10.2 Security                | No dynamic code that changes described functionality                                                             | None (the self-updater is off in the Store build)                                                   | `distribution.rs` runtime policy                                                                                                                  |
| 10.2 Security                | All PE files signed by a trusted CA                                                                              | Applies to the EXE/MSI route                                                                        | MSIX is re-signed by Microsoft — the reason the MSIX route was chosen                                                                             |
| 10.8 Stability               | Start promptly, stay responsive, handle API failures                                                             | Elevated/denied reads                                                                               | `None` + reason rather than throwing                                                                                                              |
| 10.9 Notifications           | Respect system notification settings, work when disabled                                                         | Watchdog prompts are notifications                                                                  | _Not verified_: watchdog behaviour with notifications turned off for Vitals should be tested before submission                                    |

Source: [Microsoft Store Policies 7.20](https://learn.microsoft.com/en-us/windows/apps/publish/store-policies).

Installer vs MSIX: the EXE route requires a signed installer (10.2) and a
versioned, unchanging download URL; Vitals ships MSIX, so neither applies
(`docs/distribution.md`).

## 3. Google Play production release

### Is the 12-testers / 14-days rule in force for this account?

The rule: **personal** developer accounts created **after 13 November 2023**
must run a **closed** test with **at least 12 testers opted in continuously
for the last 14 days**, then apply for production access from the Dashboard
(a questionnaire about the test and readiness). Until then the Production and
Pre-registration pages stay disabled; open testing also unlocks only after
production access is granted. Internal testing has no requirement and **does
not count**. Organisation accounts are exempt.
Source: [Testing requirements for new personal developer accounts](https://support.google.com/googleplay/android-developer/answer/14151465).

What the repository knows: the account is the owner's own "hobby apps" account
(service account project `hai-small-apps`, tester list "Hobby apps testers"),
Play reported `app.vitals` as a **Draft app** on 2026-09-30, and every release
so far went to **internal** tracks (`qa`, `wear:qa`, `tv:qa`). Nothing in the
repo records the account type or its creation date. **If it is a personal
account created after 13 Nov 2023 (likely, given the name and that Play
registration for individuals was only opened recently), this is the main
production blocker:** create a closed track, move "Hobby apps testers" onto it
(or a new list), make sure ≥ 12 people opt in via the link and stay opted in,
wait 14 days, then apply. A tester who leaves during the 14 days resets the
count for that person. _Not verifiable from the repo — check Play Console →
Dashboard: if a "Apply for production" card with a tester counter is shown,
the rule applies._

### Target API level

From 31 August 2026 new apps and updates must target **API 36** (phone),
**API 35** for Wear OS, **API 34** for Android TV. Vitals targets **36** in
`:app`, `:wear` and `:tv` — compliant. Source:
[Target API level requirement](https://developer.android.com/google/play/requirements/target-sdk).

### Permission declarations — review risk, highest first

1. **`MANAGE_EXTERNAL_STORAGE` (All files access).** Needs the Permissions
   Declaration Form and Google's approval; the app must explain why the
   Storage Access Framework or MediaStore is not enough. "File managers" and
   disk analysers are permitted uses, but reviews are manual and often ask for
   a video. Risk: **high, and it applies to the TV bundle too** — `:tv` merges
   `:device`'s manifest, so the TV build declares All files access and usage
   access as well (Wear removes both with `tools:node="remove"`). A storage
   analyser on a TV is a weaker story; consider removing both from `:tv` the
   way `:wear` does, or prepare a TV-specific video.
   Source: [All files access policy](https://support.google.com/googleplay/android-developer/answer/10467955).
2. **Foreground service `specialUse`.** Android 14+ requires a declared type;
   Play Console asks for a description and a video for each type in App
   content → Foreground service permissions. `specialUse` is the catch-all and
   is reviewed for whether a standard type fits. "Monitoring a paired PC on the
   local network" is close to `connectedDevice` ("data transfer to an external
   device … over a network"); a reviewer may push for that type. Risk: medium.
   Source: [Foreground service requirements](https://support.google.com/googleplay/android-developer/answer/13392821).
3. **`PACKAGE_USAGE_STATS`.** A special-access permission the user grants in
   Settings; there is no dedicated Play declaration form for it (unlike All
   files access), but it falls under the sensitive-permissions policy: use
   only for the disclosed purpose, and show a **prominent in-app disclosure**
   before sending the user to Settings. Risk: low–medium. Vitals' "this
   feature needs usage access" screen is that disclosure; _not verified here
   that it appears before the Settings intent in every path._ Source:
   [Permissions and APIs that access sensitive information](https://support.google.com/googleplay/android-developer/answer/16558241)
   (the dedicated usage-stats policy page could not be fetched).

### Wear OS and Android TV quality

- **Wear OS**: same package name and signing key as the phone app (WO-G7 —
  done, ADR-0033); tile preview required if a tile is shipped (WO-V10 —
  `androidx.wear.tiles.PREVIEW` is declared); black backgrounds (WO-V13);
  standalone flag declared. Source:
  [Wear OS app quality](https://developer.android.com/docs/quality-guidelines/wear-app-quality).
- **Android TV (Tier 3 "TV Ready")**: Leanback launcher intent (TV-ML),
  320 × 180 banner containing the app name (TV-LB, TV-BN), touchscreen not
  required (TV-MT — done), full D-pad navigation (TV-DP), Back returns to the
  home screen (TV-DB), minSdk ≤ 31 (TV-PS — 29), nothing cut off by overscan
  (TV-OV — 48 × 27 dp margins, ADR-0036). Source:
  [TV app quality](https://developer.android.com/docs/quality-guidelines/tv-app-quality).
  The TV form-factor opt-in stays locked while the app is a Play draft; it is
  reviewed with the first production submission.

### Data safety

`apps/android/play/README.md` already holds the answers (ML Kit diagnostics,
optional, collected not shared, encrypted in transit). No new file was
needed. Two checks for 1.0.0: the TV and Wear bundles do not include ML Kit,
but Data safety is one form per app, so the union (phone) answer stands;
usage stats and file listings never leave the device, so they are not
"collected" under Play's definition. No change found.

### Release notes layout

The README says `listing/` follows the Gradle Play Publisher / fastlane
layout. fastlane `supply` reads `<locale>/changelogs/default.txt` (or
`<versionCode>.txt`); 1.0.0 notes are in that file for `en-US` and `ro`
(≤ 500 characters each, Play's per-language limit,
[Prepare and roll out a release](https://support.google.com/googleplay/android-developer/answer/9859348)).
**`release.yml` does not upload them**: the `play` job uses
`r0adkll/upload-google-play` without `whatsNewDirectory`, which expects
files named `whatsnew-<locale>`. Wiring that is outside this change.

## 4. Competitor scan

What each does that Vitals does not (→), and what Vitals does that it does not (←).
Sources: vendor pages fetched on 2026-10-14 where linked; the rest from
product knowledge, marked †.

| Tool                                                                                                  | → They have, Vitals lacks                                                                                                                                                                             | ← Vitals has, they lack                                                                                         |
| ----------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------- |
| Windows Task Manager †                                                                                | Per-process GPU engine breakdown, **NPU** graphs on Copilot+ PCs ([Microsoft](https://learn.microsoft.com/en-us/windows/ai/npu-devices/)), real Startup impact from the boot trace, efficiency badges | Plain-language "why slow", storage map and clean-up, temperatures and SSD health, phone/TV/Prometheus, watchdog |
| [Process Explorer](https://learn.microsoft.com/en-us/sysinternals/downloads/process-explorer)         | Handle/DLL search across all processes, VirusTotal lookup †, process tree with colour legend †, symbol-resolved thread stacks                                                                         | Readable UI, storage, hardware, remote views, risk-stated actions                                               |
| [System Informer](https://github.com/winsiderss/systeminformer)                                       | Kernel driver: per-process disk and **network** I/O, create/edit services, close connections, memory editing †                                                                                        | No driver, Store-signable, non-expert language, phone/TV                                                        |
| HWiNFO †                                                                                              | Exhaustive sensors (voltages, VRM, per-core temps) with logging to CSV and sensor-panel OSD                                                                                                           | Process management, storage clean-up, readable verdicts                                                         |
| [Rainmeter](https://www.rainmeter.net/)                                                               | Desktop skins, community themes                                                                                                                                                                       | A whole task manager; Vitals' HUD is fixed-layout                                                               |
| [Stats](https://github.com/exelban/stats) / [iStat Menus](https://bjango.com/mac/istatmenus/) (macOS) | Menu-bar modules per metric; **rules** that notify on arbitrary conditions; fan control                                                                                                               | Windows; storage; process control; remote                                                                       |
| MSI Afterburner †                                                                                     | GPU overclocking, fan curves, RivaTuner **FPS overlay**                                                                                                                                               | Everything outside the GPU                                                                                      |
| [WizTree](https://diskanalyzer.com/) / WinDirStat †                                                   | MFT-direct scans (seconds for a full drive), file-type breakdown                                                                                                                                      | Review-then-Recycle-Bin flow, developer clean-up, Windows' own tools with freed-space measurement               |
| [LibreHardwareMonitor](https://github.com/LibreHardwareMonitor/LibreHardwareMonitor)                  | Broad motherboard/GPU sensor coverage (AMD, Intel GPUs), built-in web server                                                                                                                          | Same PawnIO approach for CPU readings, plus the rest of the app                                                 |
| [Fan Control](https://github.com/Rem0o/FanControl.Releases)                                           | Fan curves, runs as a service before sign-in                                                                                                                                                          | Vitals only reads fans; it does not control them (on purpose — writes to hardware are a different risk class)   |
| [DevCheck](https://play.google.com/store/apps/details?id=flar2.devcheck) / CPU-Z (Android)            | Deeper SoC/camera/radio specs; root/Shizuku extras; battery usage tracking (Pro)                                                                                                                      | PC monitoring, Wear and TV apps, storage clean-up, free, no ads                                                 |
| Remote PC monitors (Unified Remote-style) †                                                           | Remote input, file transfer, media keys                                                                                                                                                               | Read-only by default, token-scoped control, no cloud relay                                                      |

The pattern: specialists win on depth in one area (sensors, disk speed, kernel
I/O); Vitals wins on breadth, plain language and reach (phone, TV, Prometheus).
The ideas below deepen the areas users meet most often without importing the
specialists' risk (drivers, hardware writes).

### Ten new feature ideas, ranked

Already present and therefore **not** on the list: per-process GPU % and
memory (`vitals-core` `Process.gpu`), Windows battery health (design vs full
capacity), measured startup impact (`startup/impact.rs`, 120 s boot window),
an alerts engine, history store.

| #   | Idea                                                                                                                                                                         | Effort | User value                                   | Why / caveats                                                                                                                                                                                                                 | Privacy                                                                                                                                           |
| --- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | :----: | -------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | **"What slowed my PC at 14:03"** — a local timeline that keeps the top 5 processes per minute alongside the machine metrics, so a past spike can be explained                |   M    | High: the question people ask after the fact | The history store is machine-wide only and the flight recorder is in memory; this adds process names to disk                                                                                                                  | ⚠ Stores process names on disk: must stay off by default like history, with the same retention and a clear setting (PRIVACY.md table needs a row) |
| 2   | **Runaway and leak detector** — a process whose private memory grows steadily for N minutes, or a handle count that never falls, surfaces as an alert with the culprit named |   S    | High: the commonest "slow after hours" cause | Fits the alert engine's sustain/hysteresis; notify only                                                                                                                                                                       | OK                                                                                                                                                |
| 3   | **Drive-health alerts** — SSD life below a threshold, a rise in media errors, or a drive running hot                                                                         |   S    | Medium–high: data loss prevention            | Readings exist (`slow_readings.rs`); only rules and copy needed                                                                                                                                                               | OK                                                                                                                                                |
| 4   | **Frame rate in the HUD** via PresentMon (MIT)                                                                                                                               |   M    | High for gamers, a big audience for monitors | ETW tracing needs admin or Performance Log Users membership; HUD already exists                                                                                                                                               | OK (local only)                                                                                                                                   |
| 5   | **NPU usage** on Copilot+ PCs                                                                                                                                                |   M    | Medium, growing                              | Task Manager shows it; reading it through DXCore/GPU-engine counters needs research and a test machine                                                                                                                        | OK                                                                                                                                                |
| 6   | **Reliability history** — app crashes, driver failures and unexpected shutdowns from Windows Error Reporting / the event log, joined to the timeline                         |   M    | Medium–high: explains "it froze yesterday"   | Reads local logs only                                                                                                                                                                                                         | OK                                                                                                                                                |
| 7   | **Per-process network throughput**                                                                                                                                           |   L    | Medium                                       | Needs ETW kernel network events, which need elevation (the helper is stubbed, ADR 0012); README already lists it as Planned                                                                                                   | OK                                                                                                                                                |
| 8   | **User-defined alert rules** ("tell me when GPU > 90 °C for 30 s", "when this program starts")                                                                               |   M    | Medium                                       | iStat-style rules; must remain **notify-only** — auto-killing conflicts with the watchdog's "never ends anything on its own" principle                                                                                        | OK                                                                                                                                                |
| 9   | **Windows 11 Widgets board provider**                                                                                                                                        |   L    | Medium                                       | Needs a packaged Win32 app + Windows App SDK ([widget providers](https://learn.microsoft.com/en-us/windows/apps/develop/widgets/widget-providers)); works for the MSIX build only unless a sparse package is added to NSIS    | OK                                                                                                                                                |
| 10  | **Energy estimate in Wh per app per day**                                                                                                                                    |   M    | Low–medium                                   | Built on the power column, which is already an estimate; an accumulated estimate of an estimate risks the "honest numbers" rule — only with the sensors service's measured package power as the anchor, and labelled estimate | ⚠ Honesty principle, not privacy                                                                                                                  |

Rejected on principle: **winget update management** in the Store build
(Policy 10.2 "secondary software"; also a large blast radius for a monitor),
and **network caps / connection blocking** without a signed driver (WFP
callout drivers are a different risk class; README already defers it).

**Recommended first five:** 2 (leak detector, S), 3 (drive-health alerts, S),
1 (timeline, M), 6 (reliability history, M), 4 (FPS in HUD, M).

## 5. Store optimisation keywords

Chosen for search intent rather than feature names; no competitor brand names
(Store Policy 10.1 and Play's metadata policy forbid them). "Task manager" is
a generic term and is the most searched phrase for this category.

**Microsoft Store — EN (used, 7):** task manager · system monitor · slow pc ·
cpu temperature · disk cleanup · ssd health · startup apps.
Reserve: resource monitor, process manager, pc health, storage analyzer, gpu
temperature, free up space.

**Microsoft Store — RO (used, 7):** task manager · monitor sistem · pc lent ·
temperatura procesor · curățare disc · sănătate ssd · aplicații pornire.
Reserve: manager activități, eliberare spațiu, temperatură placă video,
procese, performanță pc. Romanian users also search in English; the EN terms
remain discoverable.

**Google Play — EN** (Play has no keyword field; ranking reads title, short
and full description): system monitor, cpu monitor, battery health, storage
analyzer, phone temperature, pc monitor, wear os, android tv, widget. All
appear naturally in the current text; do not stuff.

**Google Play — RO:** monitor de sistem, temperatură, baterie, stocare,
curățare, procesor, PC, ceas, televizor, widget. Present in the text.

## 6. Not verified, and why

- Whether the Play account is personal and created after 13 Nov 2023 — not
  recorded in the repo; check Play Console Dashboard.
- Whether submission 1 (0.9.0) passed Store certification — no access to
  Partner Center from this task.
- Microsoft's exact desktop screenshot minimum — the fetched page elided that
  line; 1366 × 768 comes from a third-party summary of it. Ours are
  1920 × 1080, which satisfies any stated minimum.
- The dedicated Play policy page for usage-stats access could not be fetched;
  the requirement is stated from the general sensitive-permissions policy.
- HWiNFO and MSI Afterburner pages returned 403; their rows rely on product
  knowledge (†).
- How Play's reviewers treat `specialUse` vs `connectedDevice` for this case —
  policy text only; no precedent found.
