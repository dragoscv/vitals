# ADR-0041: Apple platforms — a real macOS sampler and a native iPhone app

- Status: accepted
- Date: 2026-10-28
- Tracker: S23-01 … S23-14
- Supersedes, for macOS only: the stub in ADR 0007

## Context

Vitals shipped on Windows, Android (phone, Wear OS, TV), Tizen and the web.
The owner asked for the Apple side: a published macOS desktop app and a native
iPhone app on the App Store, built and signed without a Mac on the desk.

## Decisions

### macOS sampler: sysinfo plus our own Mach code

`vitals-macos` samples with `sysinfo` 0.39 (processes, disks, networks,
components) and direct Mach/libproc calls (`host_processor_info`,
`host_statistics64`, `vm.swapusage`, `proc_pid_rusage`, `proc_pidinfo`) for
the figures sysinfo gets wrong or does not have: memory in Activity Monitor's
arithmetic, per-process footprint, disk I/O rates and start time.
Rejected: our own code for everything (months of work for no gain); sysinfo
alone (its memory figures disagree with Activity Monitor, and it has no
process footprint).

GPU, battery and fans report `None` rather than `0`. A partial IOKit
implementation would show numbers we cannot verify on the test machine, which
is an Intel VM with no GPU, battery or fans.

### One frame encoder

The keyframe/delta rules moved out of `vitals-win` into
`vitals_core::frame::FrameEncoder`, which works over already-converted
`Process` values. Both platforms delegate to it. Two copies of "when is a
change meaningful" would drift, and the clients would see that drift as
flicker on one OS only.

### Windows-only memory fields became `Option`

`paged_pool`, `non_paged_pool`, `committed`, `commit_limit` and
`hardware_reserved` are Windows concepts. A Mac frame claiming "0 B
committed" would break the project's first rule (None, never 0). Every
consumer moved with the change:

- the alert rule;
- the TypeScript bindings and openapi;
- the desktop Memory panel and dashboard;
- Android `Wire.kt` and the TV screens;
- Tizen;
- the contract fixtures.

Windows now sends `hardware_reserved: None` where it used to send a made-up
zero.

### Process control on macOS

terminate, suspend, resume and priority use the same identity check as
Windows (pid plus start time, so a recycled pid is refused). They use
`kill(SIGTERM/SIGSTOP/SIGCONT)` and `setpriority`. Realtime priority, launchd,
the kernel and Vitals itself are refused.

### Two macOS distributions

- **Direct download** (Developer ID, hardened runtime, notarised, stapled,
  self-updating through `latest.json`). This is the full build.
- **Mac App Store** (sandboxed, built with the `app-store` feature). The
  sandbox forbids signalling other processes and reading SMC sensors. That
  build reports those capabilities as `Unavailable::AppSandbox` and the UI
  says so, rather than hiding them. Apple refuses a duplicate app name within
  one account, so this listing is called "Vitals: Mac System Monitor".

  Amended 2026-10-28 (S23-07): the store build is also compiled **without
  the self-updater** (`--no-default-features --features app-store`), not
  merely configured off. Guideline 2.4.5(vii) forbids an app that can replace
  its own code, and a disabled-at-runtime updater still ships the endpoint
  and the install path. `build-appstore.sh` fails the build if the updater
  URL is found in the binary. The updater's capability lives in
  `capabilities/updater/`, which `build.rs` includes only with
  `self-update`, because tauri-build rejects a permission for a plugin that
  is not a dependency. The bundle name is "Vitals Monitor" (the iPhone
  ITMS-90129 lesson), signed with Apple Distribution and packaged with the
  Mac Installer Distribution identity through `scripts/macos-appstore.ps1`
  on vmui-mac; the profile is fetched from the API at build time, never
  committed.

### Signing runs after approval, never in the build leg

The build leg runs every dependency's build script before anyone has looked
at the run, so it builds an unsigned `.app` and holds no Apple secret — the
same rule the updater and Android keys follow (SEC-5, SEC2-4).

- A tag-only `macos-sign` job in the `release` environment runs
  `scripts/macos/sign-notarise.sh`.
- The certificate travels as a base64 `.p12` into a throwaway keychain that
  is deleted when the job ends.
- The source of truth for the certificate is dragos-devcloud Secret Manager.
  The certificate's private key never existed outside vmui-mac and that
  bundle.

### iPhone: native SwiftUI, not a webview

`apps/ios` is SwiftUI on iOS 18, generated with XcodeGen. Its `VitalsCore`
Swift package mirrors the Android `:core` module:

- wire models and the delta merge;
- pairing rules and the private-address guard;
- no redirects, and backoff on reconnect.

It decodes the same contract fixtures the Rust tests write, so a field rename
fails on both phones. Strings are generated from the Android `strings.xml`
(EN+RO) plus `apps/ios/strings-ios.json` by `scripts/ios-strings.mjs`, and
`check-drift.ps1` fails on a stale catalog or an unknown key.

Rejected: Capacitor around the mobile PWA. Bonjour discovery, the Keychain,
camera QR pairing and the local-network permission all want native code, and
a wrapped PWA is a common App Store rejection reason (guideline 4.2).

iPhone builds run on the devcloud macOS VM (vmui-mac) through
`scripts/ios-build.ps1`. Signing is automatic cloud signing with the team's
App Store Connect key, so there is no local Mac and no exported profile.

### Review access

App Review cannot reach a PC on our LAN, so reviewers get two ways in:

- a built-in **Sample PC** mode, clearly labelled, replaying recorded frames;
- a read-only hosted demo server.

No Wake-on-LAN in v1: it needs the multicast entitlement, which has been
requested from Apple.

## Consequences

- macOS turns from an advisory build into a hard release gate once
  `PUBLISH_MACOS` is on.
- CI gains an `apple` job on `macos-latest` that runs:
  - clippy and tests for the macOS crates, in both sampler flavours;
  - the contract fixtures;
  - the Swift tests and the unsigned iPhone compile.
- watchOS (amended 2026-10-28, S23-13): a companion inside the iPhone
  bundle (`VitalsWatch`, embedded as Watch content), fed by the phone over
  WatchConnectivity with a small `WatchSnapshot` rather than pairing the
  watch with PCs itself — the watch then needs no local-network permission,
  Keychain token or discovery. Verified by the Swift tests and an iPhone
  build that embeds the watch app on vmui-mac (watchOS 26.5 platform
  installed); screenshots come from the hosted runner like the iPhone ones.
  The owner has no Apple Watch, so on-wrist testing is out of reach.
