# 0044 — A build-artefact gate decides on content, not on a timestamp

- **Status:** accepted
- **Date:** 2026-11-19
- **Tracker:** S24-19

## Context

Android marks a hardware feature **required** when a permission implies it:
`ACCESS_WIFI_STATE` implies `android.hardware.wifi` and `CAMERA` implies
`android.hardware.camera`, unless the manifest declares that feature
`android:required="false"`. Play then hides the app from every device that
lacks the feature. The phone build was unavailable on Ethernet-only phones and
camera-less Chromebooks for that reason, and nothing in the source said so
(2026-11-18).

The implied features are added when the APK is packaged: neither the source
manifest nor the merged manifest contains `android.hardware.camera` at all.
The only thing that can be checked is therefore the built artefact, so
`scripts/check-android-features.ps1` reads `aapt dump badging` and asserts two
things — every feature the source declares optional appears as
`uses-feature-not-required`, and every required feature is on the allowlist in
`scripts/android-features.json`.

That leaves the question of an artefact older than the source it should have
been built from. The first version of the gate compared modification times and
skipped anything older than its manifest: loudly, and failing only under
`-Strict`.

## Decision

An artefact gate decides on **content**. There is no freshness heuristic.

- Both assertions run on every APK under
  `apps/android/*/build/outputs/apk/**`, whatever its timestamp.
- A disagreement is a failure, and the message names both possibilities: the
  artefact predates the fix (rebuild it) or the fix does not work. The second
  is the one worth catching.
- The only skip left is having no APK at all. It is printed rather than
  silent, and `-RequireBuilt` refuses it where a build has just run.
- `-Build` removes even that skip: it assembles the three debug APKs first,
  through `run-build.ps1` and the agentq queue, so two agents never compile at
  once.

## Why not modification time

The heuristic was wrong in both directions, and both were seen on 2026-11-19:

- **Current content, old timestamp.** Gradle reported the assemble tasks
  `UP-TO-DATE` and reused the artefacts it had already written, so the APK kept
  its earlier mtime while consisting of the current manifest. The gate called
  a correct artefact "too old to check".
- **Unchanged content, new timestamp.** A mutation test set a manifest to
  `required="true"`, ran the gate and reverted it. The content was back to what
  the APK had been built from, and the mtime was newer, so every artefact in
  that module stayed skipped for the rest of the session.

A gate that answers "I cannot tell" whenever a file is touched is worse than
one that answers wrongly, because it looks green. And an artefact that
contradicts the source is exactly what is worth failing on, whether it is a
minute or a month old.

## Consequences

- The verdict does not depend on when anything was written, so it means the
  same thing in a local run, in `verify.ps1` and in CI.
- A stale build output that no longer matches the source produces a failure
  naming the module to rebuild, rather than a silent pass.
- `verify.ps1` runs this gate in its own lane, because this one builds; the
  `checks` lane still fails in seconds.
- The reasoning generalises: any gate that reads a build artefact should assert
  on what is in the file and use the build system — not the filesystem clock —
  to know whether it is current.
