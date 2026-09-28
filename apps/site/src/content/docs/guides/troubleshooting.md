---
title: Troubleshooting
description: Logs, unknown readings, features that need a helper service, and SmartScreen warnings.
---

## Logs

Vitals writes its logs to:

- `%LOCALAPPDATA%\Vitals\logs\vitals.log` — the diagnostic log, rotated when it grows large.
- `%LOCALAPPDATA%\Vitals\logs\crash.txt` — details of the last crash, if there was one.

Paste `%LOCALAPPDATA%\Vitals\logs` into the File Explorer address bar to open the folder. Attach
both files when you [report a bug](https://github.com/dragoscv/vitals/issues/new/choose). They stay
on your machine unless you send them.

## A reading shows an em dash

**—** means Vitals could not measure that value here. It is never a disguised zero. Common reasons:

- The hardware or its driver does not expose the value. Many laptops do not report fan speeds, and
  GPU memory clocks need a vendor SDK Vitals does not use yet.
- Windows only gives the value to administrators, or to processes of the same user.
- The feature is not supported on this version of Windows.

The Thermals tab and Devices & sensors list what they cannot read and why.

## "Needs the Vitals helper service"

Some readings, such as per-process disk activity and thread stacks, need a privileged Windows
service. That helper is planned but not shipped in the beta, so these features are greyed out with
this reason rather than failing when you use them.

## Windows SmartScreen blocks the installer

The builds are not signed with a commercial certificate, so Windows warns about an unknown
publisher. Select **More info**, then **Run anyway**. To confirm the file is genuine first, see
[Verify the download](/download/#verify-the-download).

## The phone cannot connect

- Check that remote access is on in **Settings → Remote access**.
- Make sure Windows Firewall allows Vitals on **private** networks, and that the network is set to
  private in Windows.
- The phone and the PC must be on the same local network. Guest Wi-Fi networks often isolate
  devices from each other.

## The app will not start

Look at `crash.txt` in the logs folder. If WebView2 is missing or damaged, reinstall it from
[Microsoft](https://developer.microsoft.com/microsoft-edge/webview2/) and run the installer again.
