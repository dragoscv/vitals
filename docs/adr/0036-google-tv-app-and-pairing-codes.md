# ADR-0036: A Google TV app, and pairing by six-digit code

- Status: accepted
- Date: 2026-09-29
- Extends: ADR-0033 (native apps), ADR-0035 (on-device monitor), ADR-0003 (LAN security)

## Context

A television is on for hours in the room where people sit, which makes it a
natural place to glance at a PC. Two things stood in the way. The existing
Android app is built for touch, and a TV has only a D-pad. And pairing works
by scanning a QR code or pasting a 43-character token: a TV has no camera,
and typing 43 characters with a remote is not a real option.

## Decision

**A fourth Android module, `:tv`, written with Compose for TV
(`androidx.tv:tv-material` 1.1.0, the latest stable), without Leanback.** It
uses the phone's application id (`app.vitals`) and signing key, and ships on
Play's Android TV form-factor track (`tv:qa` for internal testing).
versionCodes sit at 2 000 000 + run number, above the watch's range.
Everything below the UI is shared rather than copied:

- `:core` gains `PairCheck`, which checks a pairing before it is saved, and
  `VitalsClient.redeem`. The phone's `Pairer` now calls `PairCheck` too.
- `:shared-ui` gains the live PC stream (`PcStream`, `LiveState`), the
  sparkline history (`Series`, `DeviceLive`) and the Material-free canvases
  for rings, bars, sparklines and history charts. Phone and TV draw the same
  pixels from the same data.
- `:device` gains `SpecialAccess`. It monitors the TV exactly as it monitors
  a phone.

**The TV is laid out for the D-pad.** A persistent navigation drawer
(Overview, This TV, PCs, Add a PC, Settings) stays on screen. Choosing a
destination moves focus into the new screen. Tabs are a row of selectable
chips. Two columns fill a 1080p screen, with overscan-safe margins of
48 × 27 dp. A focused element grows and gets a white border, so focus does
not depend on colour alone. Where the phone asks for confirmation in a
dialog, the TV asks for a second press of OK within four seconds.

**Pairing by code: `POST /api/v1/pair`.** In Settings → Remote access → Pair
a TV, the desktop shows a six-digit code for five minutes. The TV finds the
PC by mDNS, or takes a typed address, and trades the code for a normal
bearer token. The scope is the one the user chose on the PC when the code
was shown. The rules:

- Only one code is live at a time, drawn from the OS CSPRNG.
- Codes are single use and compared in constant time.
- Five wrong guesses in total burn the code.
- A wrong code, an expired one and a burnt one all get the same `403`.
- A malformed body is a `400` and does not use up a guess.
- Turning remote access off withdraws the code.
- The route is public, like `/health`, because the code is the credential.
- Pasting a token is still available, for PCs running an older Vitals.

Five guesses against a million codes gives an attacker on the LAN a
1 in 200 000 chance per code the user shows. That is a small cost next to
what already guards the machine: the LAN itself, the five-minute window, and
the user standing at the PC.

## Consequences

- **CPU load on Android became measured, not estimated, on devices that
  allow it.** On the Chromecast, the frequency estimate read 64 % while
  `/proc/stat` read 20 %, because schedutil parks the cores at their _top_
  step. `:device` now reads cpuidle residency first (`stateN/time`, which
  the app can read). It sums the time per cluster over five samples, because
  the kernel only credits idle time when a core wakes. It falls back to the
  estimate only where cpuidle is hidden. `CpuState.measured` records which
  source was used, and the "estimate" caption appears only when it was the
  estimate. The phone and the watch get the same fix.
- The Tizen app (S15-09) reuses `POST /api/v1/pair` unchanged.
- The Play listing gains the Android TV form factor, TV screenshots and a
  banner. `release.yml` builds, lints and uploads the TV bundle to `tv:qa`.

## Rejected

- **Leanback.** It is the View-based toolkit that Compose for TV replaces,
  and Google no longer recommends it for new apps.
- **Reusing the phone APK with `leanback` marked optional.** A touch UI
  driven with a D-pad is the experience TV users complain about most. Play
  also requires a banner and a leanback launcher activity to list an app for
  TV at all.
- **A longer code or a PIN typed on the PC.** Typing on the PC is backwards:
  the PC is the side that already trusts the user. Anything longer than six
  digits gets mistyped on a remote.
