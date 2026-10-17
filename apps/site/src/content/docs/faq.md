---
title: Frequently asked questions
description: Is Vitals safe? Why does SmartScreen warn? Does it send data anywhere? Does it need administrator rights? Answers to the questions people ask before installing.
---

## Is Vitals safe to install?

Vitals is open source under the MIT licence, so anyone can read exactly what it does. Every release
is built by a public GitHub Actions workflow, and each installer carries a signed build attestation
that ties the file to the commit and workflow that produced it. The
[download page](/download/#verify-the-download) shows the one command that checks it.

The [Microsoft Store version](https://apps.microsoft.com/detail/9P95RK5VMGPJ) is also reviewed by
Microsoft and signed by Microsoft before it is published.

## Why does Windows SmartScreen warn about the installer?

The installer from GitHub is not signed with a commercial code-signing certificate, so Windows does
not recognise the publisher and shows **"Windows protected your PC"**. Select **More info**, then
**Run anyway**. If you prefer no warning at all, install from the
[Microsoft Store](https://apps.microsoft.com/detail/9P95RK5VMGPJ): Microsoft signs that package, so
SmartScreen does not warn. The [download page](/download/#windows-smartscreen) explains the warning
in more detail.

## Does Vitals send data anywhere?

No telemetry, no analytics, no crash reporting and no account. The only connection Vitals makes by
itself is an update check to GitHub in the installer version, which you can turn off in **Settings
→ About**. The Store version does not even do that, because the Store updates it.

Remote access (viewing your PC from a phone or TV) stays on your own network and is off until you
turn it on. The [privacy policy](/privacy/) lists every connection and every file Vitals writes.

## Does it need administrator rights?

No. Vitals runs as a normal user. A few actions need administrator rights — ending a process that
belongs to another account, running Windows' own Disk Cleanup or DISM, changing a service — and for
those Windows shows its usual prompt, once, for that action only. Anything your privileges do not
allow is greyed out with the reason, rather than a button that fails when you click it.

Some readings are only available to administrators, such as the memory use of other users'
processes. Those appear as a dash when Vitals cannot read them.

## Why does it show "—" instead of 0?

Because they mean different things. Zero is a measurement: the program really used nothing. A dash
means Windows or the hardware does not report that value, or your privileges do not allow reading
it. Showing a zero there would turn "this graphics card does not report its memory" into "this
graphics card is using no memory", which is false. The dash is honest.

## What is the difference between the Store version and the installer?

It is the same program. The Store version:

- is updated by the Microsoft Store rather than by Vitals itself;
- cannot replace Task Manager for Ctrl+Shift+Esc, because Windows does not allow packaged apps to
  do that;
- does not include the optional sensors service, so CPU temperature comes only from what Windows
  reports;
- shows no SmartScreen warning when you install it.

Everything else — processes, performance, storage, startup apps, diagnosis, remote access — is
identical. You can switch between them at any time; uninstall one before installing the other.

## Will it slow my computer down?

It is built not to. The Windows sampler has a budget of 30 milliseconds per sample that the build
checks on every change — at most about 3 % of one processor core at the default one-second rate
while the window is visible. When the window is hidden it samples only every 20 seconds.

## Does it work on Windows 10, ARM or macOS?

Windows 10 (version 1809 or later) and Windows 11, on x64. An ARM64 build is in preview. There are
no dated plans for macOS or Linux; the Android phone, Wear OS and TV apps can watch a Windows PC on
your network.

## How do I uninstall it?

Open **Settings → Apps → Installed apps** in Windows, find **Vitals** and choose **Uninstall**. This
works for both the Store and the installer version. The installer version keeps your settings and
history in `%LOCALAPPDATA%\Vitals` so a reinstall picks up where you left off; delete that folder to
remove them too. If you let the installer version replace Task Manager or installed the sensors
service, the uninstaller removes both.

## Where do I report a problem?

Open an issue on [GitHub](https://github.com/dragoscv/vitals/issues). The
[troubleshooting guide](/guides/troubleshooting/) shows where the log file is, which helps.
