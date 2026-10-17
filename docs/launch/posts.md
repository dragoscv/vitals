# Launch posts — drafts

Written for the Windows launch of 1.0.3 (owner decision 1, 2026-10-17:
Windows first, Microsoft Store as the main button). Android, Wear OS and TV
get their own announcement once Google Play approves them, so none of these
drafts asks people to install from Play.

Rules for every post, from the launch-readiness review:

- Microsoft Store link first, GitHub second. The GitHub installer is unsigned
  until SignPath approves us, so SmartScreen warns; say so once, plainly.
- No resource numbers that CI does not measure. The claim that exists is the
  30 ms-per-sample sampler budget, enforced in CI. "Under 1 % CPU" and
  "80 MB" were removed in S17 (PERF-1, LEG-15) and must not come back here.
- The name: GNOME's "Vitals" extension exists. Say "Vitals for Windows" in a
  title where the two could be confused.
- Reply to the first comments within the first two hours; HN and Reddit rank
  on early engagement, Product Hunt on the first day.

Links used below:

- Store: <https://apps.microsoft.com/detail/9P95RK5VMGPJ>
- Repo: <https://github.com/dragoscv/vitals>
- Site: <https://vitals.dragoscatalin.ro>
- Compare pages: `/compare/task-manager/`, `/compare/process-explorer/`,
  `/compare/system-informer/`

---

## Show HN

**Title** (80 characters max):

> Show HN: Vitals – an open-source Task Manager for Windows that says why it's slow

**URL:** the repo, not the Store (HN readers want the source).

**First comment** (post it yourself, immediately):

> Hi HN, I built Vitals because every time my PC got slow I opened Task
> Manager, saw "100 % disk" and still had no idea why.
>
> It's a native Windows system monitor and task manager (Rust + Tauri 2,
> React in WebView2). The parts I think are different:
>
> - A health strip answers "why is my PC slow?" in one line from live
>   readings: the process holding the disk, memory pressure, a hot CPU.
> - It never shows a 0 for something it couldn't measure. Unknown is a dash.
>   If your GPU driver hides VRAM, you see "—", not "0 MB". It sounds small;
>   it removed half the lies from my own screenshots.
> - The sampler has a 30 ms-per-sample budget that fails CI if it's
>   exceeded, and it drops to one sample every 20 s when the window is
>   hidden. Most of the hard work is one `NtQuerySystemInformation` call per
>   tick instead of a handle per process.
> - It can replace Task Manager (Ctrl+Shift+Esc), but only when installed
>   under Program Files with a folder ordinary accounts can't write to, since
>   that hook runs as you whenever anyone opens Task Manager.
> - A LAN API (off by default, token-paired by QR code) with SSE/WebSocket
>   streams, a Prometheus endpoint, a TypeScript SDK and a CLI
>   (`vitals ps --json`, `vitals kill <pid>`) — every action takes a PID plus
>   start time, so a recycled PID is never hit.
>
> No telemetry, no accounts. The only request it makes by itself is an
> update check to GitHub, and you can turn that off.
>
> It's on the Microsoft Store (signed by Microsoft). The GitHub installer
> isn't code-signed yet, so SmartScreen will warn — I've applied to SignPath's
> free OSS programme.
>
> MIT licensed. Windows 10/11 x64, ARM64 in preview. Happy to answer
> anything about the sampler, WebView2 or the Task Manager hook.

**Likely questions, short answers ready:**

- _Why not native UI / why WebView2?_ — WebView2 ships with Windows 11 and
  updates itself; the sampler and every system call are Rust. The UI process
  sits in its own process tree and you can see its cost in the list.
- _How is it different from System Informer / Process Explorer?_ — Those are
  deeper for kernel work; Vitals is for "what is slowing me down", phone
  access and scripting. Link the compare pages.
- _Memory use?_ — Give the number you see on the screen in front of you and
  say it is not yet gated in CI (idle gate is on the roadmap). Do not quote
  a number from memory.
- _Linux/macOS?_ — Compiled in CI, no samplers yet, no dates (ADR 0007).

---

## Reddit

Post on different days; each subreddit's own rules first (most require
"I made this" disclosure and forbid repeat posts).

### r/Windows11 (or r/Windows10)

**Title:** I made a free, open-source Task Manager replacement that tells you
_why_ your PC is slow

**Body:**

> Task Manager tells you the disk is at 100 %. It doesn't tell you it's
> Windows Search re-indexing after an update, or that your RAM is full and
> everything is paging.
>
> Vitals puts that answer in one line at the top of the screen, and the rest
> is what you'd expect from a task manager: processes with real app names and
> icons, end / suspend / priority / efficiency mode, temperatures (CPU,
> NVIDIA, every drive), SSD health, startup apps, and a storage cleanup that
> goes to the Recycle Bin first.
>
> - Free, MIT licence, no ads, no telemetry, no account
> - Microsoft Store: https://apps.microsoft.com/detail/9P95RK5VMGPJ
> - Source: https://github.com/dragoscv/vitals
> - English and Romanian
>
> It can take over Ctrl+Shift+Esc if you want it to (off by default).
> Feedback very welcome, especially "it said X but the real reason was Y".

_(attach: `apps/site/public/screenshots/dark-01-dashboard.png` and
`dark-02-diagnosis.png`)_

### r/homelab / r/selfhosted

**Title:** Vitals: watch and control a Windows PC from your phone or
Prometheus — local only, open source

**Body:**

> For the Windows box in the rack that you RDP into just to check if it's
> busy: Vitals runs on it as a normal app and, if you switch on Remote access,
> serves a token-protected API on your LAN only.
>
> - Pair a phone's browser by QR code (read-only by default; control is a
>   separate scope)
> - `GET /metrics` for Prometheus, and a Home Assistant package in the repo
> - SSE and WebSocket streams, TypeScript SDK on npm (`@vitals-app/client`)
> - Headless `vitals serve` if you don't want the desktop app at all
>
> No cloud relay, no account, nothing leaves your network. MIT.
> Docs: https://vitals.dragoscatalin.ro/guides/remote-access/

### r/rust

**Title:** Vitals: a Windows system monitor in Rust + Tauri 2 — lessons from a
30 ms sampler budget

**Body:** a short technical write-up rather than an ad:

> - One `NtQuerySystemInformation(SystemProcessInformation)` per tick instead
>   of Toolhelp + per-process handles (1 syscall vs ~1600/s on 400 processes)
> - `Option<T>` for every metric that can be unmeasurable, all the way into
>   the ts-rs bindings, so the UI renders "—" instead of a fake 0
> - The perf budget is a `cargo test --release` that fails the build, run on
>   a dedicated Windows VM because shared CI runners are too noisy
> - Native crashes write a minidump from a pre-spawned thread (the crashing
>   thread may have no stack left) and `RegisterApplicationRestart` brings
>   the app back
>
> Repo: https://github.com/dragoscv/vitals — the `prove_*` examples run each
> subsystem against the real machine.

---

## Product Hunt

**Name:** Vitals for Windows

**Tagline** (60 characters max):

> The Task Manager that tells you why your PC is slow

**Topics:** Windows, Open Source, Developer Tools, Productivity

**Description** (260 characters max):

> A free, open-source system monitor and task manager for Windows. One line
> tells you what's slowing your PC down; the rest shows processes,
> temperatures, storage and startup apps. Watch it from your phone. No
> telemetry, no account.

**Gallery** (1270×760, in order): dashboard dark, diagnosis, processes,
performance, storage, remote access. Export from the 1920×1080 captures in
`packaging/msix/listing/screenshots/` (crop, do not scale up).

**Maker comment:**

> Hi Product Hunt! I'm Dragos. I built Vitals because Task Manager could tell
> me _that_ my PC was slow but never _why_.
>
> Vitals answers that in one sentence from live readings, then gives you the
> tools to fix it: end or pause a process, lower its priority, put it in
> efficiency mode, clean up storage (to the Recycle Bin first), turn off
> startup apps.
>
> It never shows a fake zero, it measures its own cost in CI, and it sends
> nothing anywhere. It's MIT licensed and on the Microsoft Store.
>
> I'd love to hear what it got wrong on your machine.

**Launch day:** Tuesday–Thursday, 00:01 PT. Have the Store listing showing
1.0.3 and its new screenshots before posting.

---

## Checklist before posting

- [ ] Store shows 1.0.3 with the new captures (Partner Center submission certified)
- [ ] `latest.json` for 1.0.3 is signed and a 1.0.2 install updated itself
- [ ] Choco 1.0.x approved, or remove Chocolatey from the install table
- [ ] SignPath application sent (the SmartScreen line above stays until it is approved)
- [ ] GitHub social preview image set (Settings → General → Social preview,
      `apps/site/public/og.png`)
- [ ] README hero GIF or the dashboard capture renders on the repo page
- [ ] 20–50 people who already use it asked to star before the HN post
