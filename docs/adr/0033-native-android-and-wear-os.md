# ADR-0033: Native Android and Wear OS apps beside the PWA

- Status: accepted
- Date: 2026-09-28
- Supersedes: the "no native app" part of ADR-0001

## Context

ADR-0001 chose a PWA served by the LAN server and turned down a native app
as "a second codebase and two store accounts". The PWA works, but it cannot
do the things people now want from a phone that watches a PC:

- a notification while the phone is locked when the PC overheats or hangs,
  including a Live Update in the One UI 8 Now Bar;
- home-screen widgets and a Quick Settings tile;
- scanning the pairing QR code with the camera, and finding PCs by mDNS
  instead of typing an IP address;
- anything at all on a watch, where there is no browser: an app, a tile,
  watch-face complications, and a vibration when something goes wrong;
- Wake-on-LAN, which a web page cannot send, because browsers cannot send
  raw UDP.

The test devices are a Galaxy A51 (Android 13, 3.6 GB RAM, Exynos 9611), a
Galaxy S25 Ultra (Android 16, One UI 8.5) and a Galaxy Watch 7 (Wear OS 6,
One UI 8 Watch, 1.8 GB RAM). The apps have to run smoothly on the A51, not
just on the S25.

## Decision

Build native Kotlin apps in `apps/android`. It is one Gradle build with four
modules:

| Module       | What it holds                                                                                                                         |
| ------------ | ------------------------------------------------------------------------------------------------------------------------------------- |
| `:core`      | The wire model (kotlinx.serialization), the HTTP/WebSocket client, pairing storage, delta folding, NSD discovery, Wake-on-LAN. No UI. |
| `:shared-ui` | Formatting and colour thresholds that both apps share.                                                                                |
| `:app`       | The phone app: Jetpack Compose with Material 3 Expressive.                                                                            |
| `:wear`      | The watch app: Wear Compose Material 3, ProtoLayout tiles, complications.                                                             |

**Toolchain.** The latest stable release of each piece at the time of this
decision: AGP 9.4, Gradle 9.8, Kotlin 2.4.20, Compose BOM 2026.09.00, Wear
Compose 1.7, Tiles 1.6 and ProtoLayout 1.4. minSdk is 29 on the phone and 30
on the watch.

**Rejected alternatives.**

- _Tauri 2 Android._ It would reuse the React code, but the UI stays a
  webview, there is no Wear OS target, and it is heavy on the A51.
- _Compose Multiplatform._ Nobody has asked for iOS, and the watch module
  would be separate anyway.
- _Flutter._ It does not draw native Material components, has weak support
  for Wear tiles, and adds its own runtime.

**The contract is tested, not generated.** The Kotlin models are written by
hand. `crates/vitals-server/tests/android_contract.rs` writes real serialised
frames, summaries, sensor lines and alerts to
`apps/android/core/src/test/resources/`, and a JVM test in `:core` decodes
them with `ignoreUnknownKeys = false`. A field that is renamed or missing on
either side fails that test. `scripts/check-drift.ps1` checks that the
fixtures are current, the same way it checks the TypeScript bindings.

**New server routes.** Each one also helps existing clients:

- `GET /api/v1/summary?top=N` returns the system metrics and the top N
  processes by CPU (default 5, at most 25). That is about 3 KB, where a
  keyframe is about 250 KB. The watch, the widgets and the tile read this.
- `GET /api/v1/history?seconds=N` returns `MachineSample[]` from the
  desktop's store, at most seven days. The headless server returns `[]`.
- `GET /api/v1/sensors` returns every reading the desktop's Devices screen
  shows, flattened into `SensorLine[]`. The desktop caches it for five
  seconds so that a polling watch cannot make it run WMI on every request.

**A client that is watching raises the sample rate.** With the desktop
window hidden, the sampler used to drop to one frame every 20 seconds, and a
phone streaming live data saw the numbers freeze. While any LAN stream or
socket is open, the sampler now runs at no less than 1 Hz. Pausing still
wins, because a pause is an explicit instruction from the user.

**How the watch gets data.** When the phone is nearby, it relays a summary
through the Wearable Data Layer, which uses the existing Bluetooth link and
costs the watch almost nothing. When it is not, the watch calls
`/api/v1/summary` directly over Wi-Fi, using a pairing copied from the phone.
The watch never opens the full stream.

**Glass effects depend on the device.** Real blur (Haze and `RenderEffect`)
is used only when the device reports media performance class 12 or higher,
and only on API 31 or later. Other devices, including the A51, get tinted
translucent surfaces with no blur. The look is the same design either way,
and the A51 keeps its frame budget.

**Resource use.**

- Nothing runs in the background unless the user turns on "Watch this PC".
  That starts a `specialUse` foreground service, which holds the
  `/api/v1/summary` poll every 5 seconds and posts the alert notifications.
- Widgets and the tile refresh through WorkManager, no more often than every
  15 minutes, and immediately when the app is open.
- Baseline profiles ship with both apps.

**Security.** The LAN model is unchanged (ADR-0003): plain HTTP on the LAN,
with a bearer token. The network security config allows cleartext only to
private IPv4 ranges and `.local` names. Tokens are encrypted at rest with an
Android Keystore AES-GCM key and are never logged. The QR code's `#t=`
fragment is read directly from the scanned string and is never opened in a
browser. A read-scope pairing hides every control action.

**Distribution.** Each release publishes signed APKs on GitHub Releases, so
Obtainium can track them, and the same builds go to Google Play: the phone
app and the Wear app, on the internal track first. There is one upload key,
generated once. It lives in the GitHub secrets `ANDROID_KEYSTORE_B64`,
`ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS` and
`ANDROID_KEY_PASSWORD`. Play App Signing holds the app signing key.

## Consequences

- The repo now has a third language. CI gains an Android job, which runs
  `./gradlew :core:test lint assembleRelease`.
- The PWA stays, with the same scope as before. It is still the answer for
  iOS and for any phone without the app installed.
- Any change to the wire model now touches the Kotlin model too. The contract
  test and the drift check are what enforce that.
