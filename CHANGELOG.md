# Changelog

All notable changes to Vitals are recorded here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Storage: every drive card (except network and optical drives) has a
  **Clean up** button that opens a three-step wizard: it measures what could
  be cleaned on that drive, lets you choose by group with the space each
  would give back, lists the programs that will be stopped and started again
  (Linux distributions, Docker Desktop, containers, scheduled tasks, runners,
  virtual machines) before you confirm, shows each stage live, and ends with
  a report that says what was freed and what could not be restarted. Items
  that cannot be undone need their own tick; "select all" never ticks them.
  "Remember my choices for this drive" saves the kinds of item for a
  one-click "Run my usual clean-up" that still stops at the confirmation.
  Something that is listed for its size but cannot be removed from here
  (a hibernation file Windows will not let go of, say) says why and cannot
  be ticked.
  A "drive nearly full" alert now offers "Clean up this drive". (S22-02)
- Storage clean-up: on a drive that is low on space, each Linux (WSL) or
  Docker Desktop disk has **Move to another drive…**. For a Linux
  distribution Vitals lists every drive and says why the ones that cannot
  take it are refused (the same drive, removable or network, not NTFS, not
  enough room), stops only that distribution, has WSL move its disk
  (`wsl --manage --move`, WSL 2.3.11 or later, no administrator prompt),
  shows the copy live, checks the result and starts the distribution again.
  It can be moved back the same way. Docker Desktop moves its own disk, so
  Vitals shows the exact setting and folder, opens Docker Desktop, and checks
  afterwards that the disk has left the drive. (S22-07)
- Storage: a **weekly clean-up** per drive, off until you turn it on in the
  clean-up wizard. Pick a day and time; it runs the safe part of your saved
  choices (old temporary files, download caches, Docker build cache and old
  images, Linux package caches) and never anything that needs administrator
  approval, stops WSL or Docker, compacts a disk or cannot be undone. It
  waits for the PC to be idle and plugged in (or runs within a day if it
  never is), catches up at the next start if Vitals was closed, says what it
  freed in a notification, and the wizard lists recent clean-ups. (S22-05)
- Storage: **"Why is this drive growing?"** in the clean-up wizard. Vitals
  reads how full each drive is once an hour and shows growth per day over
  24 hours, 7 days and 30 days with a small chart, and what grew since the
  previous analysis ("Docker disk file +42 GB since 12 Oct"). It says when
  there are not enough readings or no earlier analysis yet. Kept in
  `drive-history.sqlite` for 180 days (see PRIVACY.md). (S22-06)

### Fixed

- Storage clean-up: "Let a virtual disk shrink by itself" is no longer
  offered on WSL 2.5.6 or later, which refuses sparse disks because they
  were found to damage distributions — the step always failed there. It is
  still listed, with that reason, and cannot be ticked; it is also withheld
  when the WSL version cannot be read. Vitals never forces it with
  `--allow-unsafe`; if WSL refuses it anyway the report says so instead of
  showing a bare error code. (S22-03)
- The mouse wheel scrolls the page again when the pointer is over a card or
  list that has nothing left to scroll. On Storage the page stopped dead
  over the clean-up cards, the empty project clean-up card and the folder
  list once it reached its end; every page with cards or tables did the
  same. Lists still scroll themselves first when they have more to show.
- Text stays inside its box on every page, in English and Romanian, from
  900 px wide to full HD: App history keeps a readable name column and
  scrolls the table sideways instead of letting the header spill over the
  next column; a card title is no longer cut to a few letters when its badge
  is wide (Devices → thermal zones); the CPU column header of Processes fits
  "Procesor" with its sort arrow. Every page's title row now has the same
  layout, so Refresh stays in the top-right corner instead of dropping under
  a long subtitle on some pages. (S22-04)

## [1.0.6] — 2026-10-18

The first published build of the 1.0.5 changes: 1.0.5 was tagged but never
published, so everything listed under [1.0.5] below reaches you with this
release. In short:

- Updates install while you are away: after ten minutes with no Vitals window
  open, a downloaded update installs and Vitals comes back in the tray. Never
  during a Windows restart or sign-out.
- Declining the administrator prompt for an update no longer leaves Vitals
  half-stopped.
- AI page: the codai sign-in is read-only and is revoked when you sign out;
  other accounts on this PC cannot read your agent sessions.
- Remote access binds to the network adapter you pick.
- Uninstalling a per-user copy also removes the optional sensors service.
- Android: the AI agents card follows each PC's LAN sharing setting; Wear OS
  gains an open-source licences screen.

### Fixed

- **Release signing of the Android builds.** The release stopped before
  publishing anything because the signing step did not understand the output
  of the newest Android build tools, and it failed without saying why. It now
  checks every signature explicitly (one signer, the upload certificate, the
  16 KB alignment) and names whatever fails.

## [1.0.5] — 2026-10-18 (not published)

Updates that install while you are away, and a safer AI page. Tagged but
never published: the release stopped at Android signing. These changes ship
in 1.0.6.

### Changed

- **AI page: signing in to codai is read-only.** Vitals asks codai only for
  permission to read your agents, never to act for you. Signing out revokes
  the key on codai's side, and uninstalling deletes it from this PC. The
  privacy notice now describes this sign-in.
- **AI page over the network needs a pairing token, even from this PC.**
  `/api/v1/agents`, its actions and the agent series in `/metrics` are no
  longer part of the token-free local read, so another account on the same PC
  cannot read your agent sessions. A paired token presented locally is now
  honoured instead of being treated as the token-free read.
- The AI page's queue actions refuse to run while Vitals runs as
  administrator, run only agentq from your own profile folder, and stop after
  two minutes.
- The AI page stops reading agent data while its window is hidden.
- **Android:** the AI agents card follows the PC's LAN sharing setting and
  copes with agent types it does not know yet ("Other agent"); Vitals talks to
  your PCs over a VPN only when that VPN runs over Wi-Fi or Ethernet; Wear OS
  gains an open-source licences screen.

### Fixed

- A rare crash when Windows closed the background COM apartment that the
  startup-task readers were using. Vitals now keeps it open for as long as it
  runs.
- **Updates install again on PCs where Vitals lives in the tray.** An update
  used to install only when you quit, and for most people the only "quit" is a
  Windows restart, which ended the session under the setup: three releases
  were downloaded and lost that way on one PC. A downloaded update now also
  installs after ten minutes away from the PC with no Vitals window open, then
  Vitals comes back in the tray. It is never started during a restart or
  sign-out.
- **Declining the administrator prompt for an update no longer leaves Vitals
  half-stopped.** The tray icon, the window, sampling, the local API and the
  `vitals top` pipe all come back, and the download stays ready for another
  try. Updates downloaded from Settings › About now install the same way, so
  they get the same clean stop and the same way back.
- The Remote access panel binds the server to the network adapter you pick,
  instead of to every adapter on the PC. Choose the adapter before you turn
  sharing on.
- Elevated helper runs (Task Manager switch, ending a protected process,
  clean-up, removing an old copy) no longer write a log or crash dump into
  your user folder while running as administrator.
- The sensors helper installs a copy of exactly the file that is running, so
  swapping the file in a per-user install folder during the prompt cannot
  turn a different program into the SYSTEM service.
- Uninstalling a copy of Vitals installed for you only now removes the
  optional sensors service too: it asks for administrator rights with one
  Windows prompt. If you decline, the service stays and the uninstaller shows
  the command to remove it later. Before, the SYSTEM service and its folder in
  Program Files were left behind silently. Updates never remove it.
- Turning Task Manager replacement on is confirmed only when Windows names
  this exact copy, not any file called `vitals-desktop.exe`.
- Crash dumps keep working after WebView2 or a graphics driver installs its
  own crash handler; theirs is called after ours.
- Clearing app history and turning recording on or off no longer pause the
  window while the file is written.

## [1.0.4] — 2026-10-18

The AI agents page, and updates that install themselves.

### Added

- **Restart to update**, in Settings › About and in the tray menu, for a copy
  installed for everyone: Windows asks an administrator to approve its
  updates, so the prompt now follows your click instead of your Quit.
- After an update, Vitals says once whether it worked: "Vitals updated to x",
  or "x was not installed" with a way to try again.
- After moving out of Program Files, Settings › About offers to remove the
  old copy (one administrator prompt). Start with Windows follows the move.
- **AI agents page.** See which AI coding agents are working on this
  machine — VS Code Copilot Chat, Copilot CLI, Codai, Codex and Claude Code —
  what each is doing right now, how long the session has run and how much of
  that it spent actually working, its turns, tool calls and the tool running
  this second. Tabs for agentq's build queues and worktree slots, background
  jobs and releases, a live journal, cloud agents from your codai account, and
  24-hour insights (time spent waiting, slow builds, failures per repository).
  Updates live every two seconds while the page is open and costs nothing
  when it is not. Freeze or clear a queue, remove a dead ticket, release an
  abandoned worktree, stop a background job or close idle agent terminals,
  each with a reason that the other agents can read.
- **AI agents over the network.** A paired phone, a script or Prometheus can
  read the AI page at `GET /api/v1/agents` (read access) and run the same
  queue actions at `POST /api/v1/agents/actions` (control access). User
  folders and anything that looks like a password or key are hidden before
  it leaves the PC. Prometheus gains `vitals_agents_sessions` and
  `vitals_agentq_waiters`; the TypeScript client gains `agents()` and
  `agentqAction()`; the Android app shows which agents are working and
  which queues have someone waiting on each PC's Now tab.

### Changed

- The installer asks whether to install for you only or for everyone on
  this PC. `/S /CurrentUser` and `/S /AllUsers` script either; winget and
  Chocolatey now install for the current user.

### Fixed

- **Updates now install when you quit, without a hidden administrator
  prompt** (DEV-3). Vitals runs without administrator rights, and its
  Program Files installation needed approval for every update, so the prompt
  appeared as the window vanished, nobody answered it, and the next launch
  was the old version again. Vitals can now be installed for you only
  (`%LOCALAPPDATA%\Programs\Vitals`), and such a copy updates itself with no
  prompt at all. Copies installed by 1.0.3 or earlier move there on their
  next update (ADR 0038).
- **"Report a problem" and "Source code" in About did nothing.** The window
  was allowed to open links in your browser but no address was on the list,
  so every link was refused silently. The list now names exactly the two
  places Vitals opens: its GitHub page and the codai sign-in page.
- **AI page: a conversation no longer appears twice once you sign in to
  codai.** codai keeps a cloud copy of each local chat; the cloud list now
  leaves out the ones already read from this PC. Cloud sessions report no
  turns or tool calls, so those show a dash instead of a misleading 0 — in
  the app, over the network (`null` in `GET /api/v1/agents`) and in the SDK.

## [1.0.3] — 2026-10-17

The launch-readiness release: eleven specialist reviews
(`docs/audit/launch-readiness-2026-10-17.md`) and everything they found that
would embarrass a public launch.

### Added

- **Health strip** across the top of the Dashboard: a one-line answer to "why
  is my PC slow?", with a button that opens the full diagnosis.
- **First-run card** that explains what Vitals does and lets you turn on the
  freeze watchdog. The watchdog is now opt-in (it used to start itself).
- **Command line:** `vitals kill`, `suspend`, `resume` and
  `priority <pid> <level>`. Each checks the PID against a fresh sample, so a
  recycled PID is never hit, and refuses processes Windows cannot do without.
  The CLI now ships with the app as a zip on every release.
- **SDK** (`@vitals-app/client`): `summary()`, `history()`, `sensors()` and
  `VitalsClient.pair()` — every route of the local API now has a method.
- **Crash dumps:** a native crash writes `logs\crash.dmp` (thread stacks and
  modules, no memory contents) and Windows restarts Vitals to the tray.
  Nothing is sent anywhere.
- Windows Contrast themes are supported throughout.

### Changed

- Processes is fully translated into Romanian, numbers and units follow the
  language, and the page language follows the app's.
- The sampler sleeps when nothing is watching, the flight recorder keeps its
  buffer in memory with history off, and hidden autostart no longer preloads
  every screen.
- Remote access can bind to one adapter you choose instead of every interface.
- Text contrast, keyboard focus in the process list and chart summaries for
  screen readers.

### Fixed

- Uninstalling removes the CPU sensors service and the autostart entry.
- The Task Manager replacement only hooks a copy installed under Program
  Files whose folder ordinary accounts cannot write to.
- A mapped network drive that stops answering no longer stalls the sampler.
- The window reloads by itself if its WebView2 renderer crashes.

### Security

- The updater no longer pulls in zip 4 (RUSTSEC-2026-0367/0368/0369); its
  signing key is only reachable from the reviewed publish job.

## [1.0.2] — 2026-10-14

### Changed

- Android: two builds. The Google Play build no longer asks for access to all
  files, which Google refused for a system monitor. Its storage screen shows
  what fills the phone or TV by category, which apps keep the most cache, and
  opens the system's storage manager for the rest. The APK on GitHub keeps
  the full folder scan and file cleanup.

### Fixed

- Google Play: the release workflow now refuses a Play bundle that declares
  all-files access, so the rejection cannot recur unnoticed.

## [1.0.1] — 2026-10-14

Same app as 1.0.0; this release carries it to the channels 1.0.0 missed.

### Fixed

- Google Play: the first production release is uploaded as a draft, which is
  all Play accepts from an app that has not yet passed review. Staged
  rollouts start with the next release once the app is live.
- winget: the update job could not start, because the action it used pulls
  in an unpinned action this repository's pinning policy refuses. It now
  runs Komac directly, pinned by version and checksum.

## [1.0.0] — 2026-10-14

The first production release: Windows (installer and Microsoft Store),
Android phone, Wear OS and Android TV. It also carries everything listed
under 0.9.0, which was tagged but never published.

### Added

- **Program icons in the process list** — each row shows the program's own
  icon, as Task Manager does, including services and other accounts'
  processes that refuse to be opened. Kernel processes with no file show a
  small glyph for their kind.
- **Totals in the column headers** — CPU, memory, disk, network and GPU show
  how busy the whole machine is above each column. Disk and GPU show the
  busiest device; a reading the machine cannot give shows a dash, not 0 %.
- **Power usage columns** — "Power usage" and "Power usage trend" on Task
  Manager's five-step scale, estimated from CPU, GPU, disk and network
  activity (Windows does not report energy per process; the column's
  tooltip says so). The trend smooths over about two minutes.
- **More columns** — Type, Working set, Disk read, Disk write and Parent PID.
- **Bring to front** — right-click an app with a window to restore and raise
  it. Offered only for processes that have a window to show.
- **The Apps filter works** — processes with a window you can switch to are
  now recognised as apps, as Task Manager counts them.
- **Temperatures and drive health in Performance** — CPU, NVIDIA GPU and every
  drive show their temperature in the list and in detail. SSDs show how much
  of their rated life is left (NVMe and SATA), with hours on, total written
  and media errors where the drive reports them.
- **CPU speed** — the current and base clock, the way Task Manager computes
  them. It said "not reported by your hardware" before; the counter was
  simply never read.
- **CPU load on the taskbar** — while the window is open, its taskbar button
  fills with the current CPU load, smoothly, turning amber above 75 % and
  red above 90 %. Can be turned off in Settings.
- **Colours per kind of hardware** in Performance: memory violet, GPU red,
  disks amber, network blue, temperatures orange. CPU keeps your accent.
- **Brand kit** — logo motion (animated SVG, Lottie, MP4, GIF, WebM), splash
  screens, animated icons, social and store images in English and Romanian,
  a brand book (HTML and PDF) and a press kit, all generated from the one
  brand source (`node brand/scripts/kit.mjs`, see `docs/brand-kit.md`).

### Changed

- **The watchdog leaves builds alone** — a compiler, bundler or package
  manager keeping every core busy is no longer offered for ending when the
  desktop is still responsive. It is named only under real memory pressure
  (90 % or more) or when a window has hung. On the owner's machine it had
  offered to end `turbo`, `node` and `rg` fifteen times in two days.
- Store listings for Microsoft Store and Google Play rewritten around the
  current features, in English and Romanian, with 1.0.0 release notes.

### Fixed

- Storage, Devices, Benchmarks and Users: cards are as tall as their content
  and the page scrolls when they do not fit, instead of each card scrolling
  inside a slice of the window. Storage's clean-up cards were squeezed to
  their title with the content cut off; when everything fits, nothing
  scrolls. Only a very long list (hundreds of sensor readings, a large
  folder) scrolls inside its card, with its header kept in view.
- Charts ignored the chosen accent colour and were always drawn in the
  default green: they read a colour variable that did not exist.
- Threads, Handles and Uptime (any column off by default) could not be
  turned on from the Columns menu.
- Dragging a column edge sorted by the next column instead of resizing; the
  handle now sits on top and keeps the drag. Double-click resets a width.
- The search box now lines up with the edge of the table below it.
- A process key sent back from the app or a paired phone could fail its
  identity check as "PID reused", because the webview prints large numbers
  in a shorter form than it stores. Keys are now normalised when read.

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
