---
title: Privacy
description: What Vitals collects (nothing), the one automatic request it makes, and how this website treats you.
---

This page is a summary. The full, authoritative text is the
[privacy policy in the repository](https://github.com/dragoscv/vitals/blob/main/PRIVACY.md); if the
two ever differ, that one applies.

It covers Vitals 1.0.x and later: the Windows app, the `vitals` command-line tool (including
`vitals serve`), the optional sensors service and lag watchdog, the Android phone, Wear OS and
Google TV apps, and the Samsung (Tizen) TV app. Effective 17 October 2026.

## The app

- **No telemetry.** Vitals has no analytics, no usage statistics and no crash reporting. There is no
  account to create.
- **One automatic request.** About 20 seconds after launch, Vitals asks GitHub whether a newer
  version exists. GitHub sees your IP address and the app version, as with any download. You can
  turn this off in **Settings → About**. The Microsoft Store version makes no such request: the
  Store updates it.
- **Remote access is off by default.** Nothing listens on your network until you turn it on in
  **Settings → Remote access**. Then it listens on the adapter you chose, or on every network if
  you chose none (`vitals serve` always listens on every network). Only `/api/v1/health` and the
  pairing exchange answer without a token; everything else, including `/metrics` for Prometheus,
  needs one. See [Remote access](/guides/remote-access/).
- **"Search online" is a click away, never automatic.** It opens DuckDuckGo in your browser with the
  name of the process, app, device or startup entry you clicked.
- **The local API is always on, but local only.** It listens on `127.0.0.1`, which only programs on
  the same computer can reach.
- **The lag watchdog is off until you turn it on.** It is a small separate program that warns you
  when a window freezes and names the program behind it. You choose it on the welcome card or in
  **Settings → Lag watchdog**; then it starts when you sign in, keeps its settings in
  `%APPDATA%\Vitals\watchdog.json` and a log with process names in
  `%LOCALAPPDATA%\Vitals\watchdog\watchdog.log`. Turning it off removes the sign-in entry; delete
  those files to remove the rest.
- **The sensors service is optional.** Installing it (one administrator prompt) downloads
  `PawnIO_setup.exe` from [github.com/namazso](https://github.com/namazso/PawnIO.Setup), checked
  against a built-in SHA-256 hash, which installs the PawnIO kernel driver; Vitals then adds a
  SYSTEM service in `C:\Program Files\Vitals Sensors` that serves temperature, power and fan
  readings on this computer only. Uninstalling Vitals removes the service; PawnIO stays installed
  because other tools share it — uninstall **PawnIO** from Windows Settings → Apps to remove it.
- **Your data stays on your machine.** Settings, history, logs and crash details are stored in
  `%LOCALAPPDATA%\Vitals`, `%APPDATA%\Vitals` and `%APPDATA%\app.vitals.desktop` (the WebView2 data
  is in `%LOCALAPPDATA%\app.vitals.desktop`) and are never uploaded. They leave the machine only if
  you send them yourself, for example by attaching a log to a bug report. Uninstalling keeps them;
  the full list of what to delete is under "How to remove everything" in the policy.

## The Android, Wear OS and Google TV apps

- **No ads, no account, no analytics of our own.** The apps talk only to the PCs you pair, on your local
  network, and to your watch through Google Play services. When you pair, the PC receives the code
  and this device's name.
- **The installed-apps list stays on the device.** The phone and TV apps list apps with a launcher
  icon (names, versions, install times) without any special access.
- **Special access is yours to grant.** Usage access (time and data per app) and all-files access
  (the storage map and cleanup) are off until you turn them on in Android's settings; the watch
  requests neither, and only the APK from GitHub can request all-files access. What they reveal
  stays on the device. The full permission list, per app, is in the policy.
- **One Google component.** The QR scanner uses Google ML Kit, which sends Google anonymous
  diagnostics about the scanner. The camera image stays on the phone.
- **Everything is deleted when you uninstall.** Paired PCs are encrypted with an Android Keystore
  key and excluded from backup; device history keeps 7 days.

## The Samsung TV app

It keeps the PCs you pair and your language choice in its own storage on the TV, talks only to those
PCs on your local network, and has no analytics or ads. Uninstalling it deletes everything.

## This website

The site is hosted on GitHub Pages. It sets **no cookies** and uses **no analytics**. GitHub, as the
host, processes the technical data any web server receives, such as your IP address, under
[GitHub's privacy statement](https://docs.github.com/site-policy/privacy-policies/github-general-privacy-statement).

## Who is responsible

The controller is **Dragos Catalin Vladulescu**. For any privacy question or request, write to
[dragoscv12@gmail.com](mailto:dragoscv12@gmail.com).
