---
title: Privacy
description: What Vitals collects (nothing), the one automatic request it makes, and how this website treats you.
---

This page is a summary. The full, authoritative text is the
[privacy policy in the repository](https://github.com/dragoscv/vitals/blob/main/PRIVACY.md); if the
two ever differ, that one applies.

## The app

- **No telemetry.** Vitals has no analytics, no usage statistics and no crash reporting. There is no
  account to create.
- **One automatic request.** About 20 seconds after launch, Vitals asks GitHub whether a newer
  version exists. GitHub sees your IP address and the app version, as with any download. You can
  turn this off in **Settings → About**. The Microsoft Store version makes no such request: the
  Store updates it.
- **Remote access is off by default.** Nothing listens on your network until you turn it on in
  **Settings → Remote access**. See [Remote access](/guides/remote-access/).
- **The local API is always on, but local only.** It listens on `127.0.0.1`, which only programs on
  the same computer can reach.
- **Your data stays on your machine.** Settings, history, logs and crash details are stored in
  `%LOCALAPPDATA%\Vitals` and are never uploaded. They leave the machine only if you send them
  yourself, for example by attaching a log to a bug report.

## The Android and Wear OS apps

- **No ads, no account, no analytics of our own.** The apps talk only to the PCs you pair, on your local
  network, and to your watch through Google Play services.
- **Special access is yours to grant.** Usage access (time and data per app) and all-files access
  (the storage map and cleanup) are off until you turn them on in Android's settings; the watch
  requests neither. What they reveal stays on the device.
- **One Google component.** The QR scanner uses Google ML Kit, which sends Google anonymous
  diagnostics about the scanner. The camera image stays on the phone.
- **Everything is deleted when you uninstall.** Paired PCs are encrypted with an Android Keystore
  key and excluded from backup; device history keeps 7 days.

## This website

The site is hosted on GitHub Pages. It sets **no cookies** and uses **no analytics**. GitHub, as the
host, processes the technical data any web server receives, such as your IP address, under
[GitHub's privacy statement](https://docs.github.com/site-policy/privacy-policies/github-general-privacy-statement).

## Who is responsible

The controller is **Dragos Catalin Vladulescu**. For any privacy question or request, write to
[dragoscv12@gmail.com](mailto:dragoscv12@gmail.com).
