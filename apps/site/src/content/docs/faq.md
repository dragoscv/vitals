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

No telemetry, no analytics, no crash reporting and no Vitals account. The only connection Vitals
makes by itself is an update check to GitHub in the installer version, which you can turn off in
**Settings → About**. The Store version does not even do that, because the Store updates it.

Two connections are off until you turn them on: the Kodwen sign-in on the AI agents page, and the
Health page, which reads from a vmui server you run yourself.

Remote access (viewing your PC from a phone or TV) stays on your own network and is off until you
turn it on. The [privacy policy](/privacy/) lists every connection and every file Vitals writes.

## Does it need administrator rights?

No. Vitals runs as a normal user. A few actions need administrator rights — ending a process that
belongs to another account, running Windows' own Disk Cleanup or DISM, changing a service — and for
those Windows shows its usual prompt, once, for that action only. Anything your privileges do not
allow is greyed out with the reason, rather than a button that fails when you click it.

Some readings are only available to administrators, such as the memory use of other users'
processes. Those appear as a dash when Vitals cannot read them.

## Can an alert go somewhere other than Windows notifications?

Yes, if you want it to. **Settings → Notifications** takes a webhook address, an
[ntfy](https://ntfy.sh) topic, a Discord webhook or a Home Assistant instance, and each alert can be
routed to the destinations you choose — one rule to Discord, a disk-failure alert to all of them.
There is a **Test** button per destination, so you find out whether it works before you rely on it.

Nothing is sent until you add an address: with no destination configured the feature does nothing at
all. A webhook carries an HMAC signature in a header, so your endpoint can prove the request came
from your own machine. What leaves the computer is the alert's title and explanation, its severity,
the rule or drive it is about and this computer's name — to the exact address you typed, and nowhere
else. The [privacy policy](/privacy/) lists every field.

You can also write your own conditions here: a reading (CPU, temperature, memory, disk activity,
battery, dropped packets), above or below a number of your choosing, for as long as you decide, with
a cooldown so a condition that flickers does not fill your phone.

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
checks on every change, and the whole app's idle cost is checked separately against
`idle-budget.json`: a median of 10.7 % of one processor core while it sits hidden in the tray,
WebView2 renderer included. Sampling is one a second while the window is visible, and one every 20
seconds when it is hidden.

## Does it work on Windows 10, ARM or macOS?

Windows 10 (version 1809 or later) and Windows 11, on x64. An ARM64 build is in preview. macOS 13
or later is in preview, as a notarised disk image for Apple silicon and Intel. There are no dated
plans for Linux. The Android phone, Wear OS and TV apps, and the iPhone app, can watch a PC or Mac
on your network.

## How do I uninstall it?

Open **Settings → Apps → Installed apps** in Windows, find **Vitals** and choose **Uninstall**. This
works for both the Store and the installer version. The installer version keeps your settings and
history in `%LOCALAPPDATA%\Vitals` so a reinstall picks up where you left off; delete that folder to
remove them too. If you let the installer version replace Task Manager or installed the sensors
service, the uninstaller removes both. The sensors service runs as SYSTEM, so removing it needs
administrator rights: for a copy installed for you only, the uninstaller asks for them with one
Windows prompt. If you decline it, the service stays and the uninstaller shows how to remove it
later (see [Troubleshooting](/guides/troubleshooting/#the-sensors-service-is-still-there-after-uninstalling)).
You can also remove it on the **Devices & sensors** page before you uninstall.

## Where do I report a problem?

Open an issue on [GitHub](https://github.com/dragoscv/vitals/issues). The
[troubleshooting guide](/guides/troubleshooting/) shows where the log file is, which helps.
