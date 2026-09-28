---
title: Remote access
description: Watch a PC from your phone over the local network, and what that exposes.
---

Remote access lets a phone, tablet or another computer on the same network see this PC's load,
its process list and its alerts. It is **off by default**: until you turn it on, nothing listens
on the network and nothing is announced.

## Turn it on

1. Open **Settings → Remote access** and switch it on.
2. The first time, Windows asks whether to allow Vitals through the firewall. Allow it on
   **private** networks. If you decline, the server runs but nothing can reach it.
3. Choose **Pair a device** and pick a scope (see below).
4. Scan the QR code with your phone's camera. The page that opens can be added to the home screen.

You can pair several PCs; the phone shows them side by side.

## Read and control

Every pairing has a scope:

- **Read** (the default) sees everything and changes nothing.
- **Control** can also end, suspend and resume processes. Each action on the phone asks for a
  second tap to confirm.

Give control only to devices you would hand your unlocked PC to.

## What it exposes, honestly

- It works on your **local network only**. Vitals does not relay traffic through any server and
  does not open ports on your router.
- It speaks **plain HTTP** on port **7331**. Anyone who can watch traffic on your network could
  read the metrics and the token. Use it on networks you trust, such as your home Wi-Fi, not on
  public ones.
- The pairing token is shown once, inside the QR code. It travels in the part of the address the
  browser never sends to a server, and afterwards Vitals only ever displays its first eight
  characters.
- While it runs, the PC advertises itself on the network as `_vitals._tcp` so the phone can find
  it. The announcement stops when remote access is turned off.

## Revoke a device

**Settings → Remote access** lists every paired device with its scope. Revoke one and its token
stops working. Turning remote access off stops the server altogether, so no device can connect
until you turn it back on.

## Without the desktop app

[`vitals serve`](/guides/cli/#headless-lan-server) runs the same server from a terminal, for a
machine where you do not want the window open.
