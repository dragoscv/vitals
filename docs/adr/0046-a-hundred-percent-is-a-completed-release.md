# 0046 — 100 % is a completed Play release, and a channel can be re-run

- **Status:** accepted
- **Date:** 2026-11-20
- **Tracker:** S24-41, S24-42

## Context

The first release that reached the channels — run `44748948289`, the first to
carry ADR 0045's publish fix — published the GitHub release and left it public,
and `npm`, `winget`, Scoop and Chocolatey all succeeded. `Google Play` failed:

```
##[error]'userFraction' must be between 0 and 1! Got 1
```

Nothing was uploaded to Play. The release was public, so four of the five
channels carried 1.3.1 and the store did not.

`PLAY_ROLLOUT` is a repository variable set to `1`, and `docs/releasing.md`
documented it as "`0.2` (the default) or `1` for everyone". The workflow passed
it straight to the upload action's `userFraction`, which validates it as
strictly between 0 and 1 — so the documented meaning of the value was the value
the action refuses. Play expresses a complete rollout as a **completed**
release (`userFraction=full` is its own spelling of it), never as the fraction
`1`.

The bug was unreachable by configuration: the job's three branches produced
`status: draft` for a draft app, `status: inProgress` with a fraction for a
stable tag, and `status: completed` only for a pre-release going to internal
testing. No combination of the two variables produced a completed production
release, so every stable tag asking for 100 % was guaranteed to fail, and the
0.2 default only ever bought a staged rollout.

## Decision

**`1` (or `100%`) is normalised to `status: completed` with no fraction at
all**, and any other value outside `0 < f < 1` fails the job naming the
variable. The range check exists so the operator reads
`PLAY_ROLLOUT=2 is not a rollout fraction` in the run rather than the action's
message after the release is already public.

**A channel can be re-run without a new version.** The fix could not reach the
run that needed it: a re-run replays the workflow file from the commit the tag
points at (ADR 0045), so a fix on `main` is invisible to it. Three routes were
available and two were rejected:

- **A patch release (1.3.2).** Rejected: `npm`, `winget`, Scoop and Chocolatey
  would hold 1.3.1 while Google Play held 1.3.2, which is a worse state for
  users than replacing the artefacts of a release nobody has taken.
- **A dispatch-only Play upload** reusing the run's `play-bundles` artefact,
  sharing the track logic through a composite action. Rejected for now: it
  splits the publish path in two, and the duplication is exactly the logic that
  had just been wrong. Worth revisiting if a channel fails again.
- **Re-point the tag at the fixed commit and re-run.** Taken, because the
  release had no human consumers: it was minutes old, its only downloads were
  the two update proofs dispatched from this machine (`latest.json`,
  `SHA256SUMS.txt` and the two installers), and nothing else in the world had
  the bytes.

Two properties were added so that this route cannot quietly damage a release
that _is_ in use:

1. `scripts/release-publish.ps1` adopts an existing release instead of failing
   with `already_exists` and instead of creating a second one, then re-uploads
   the assets with `--clobber`. A re-run for the same tag is therefore possible
   at all.
2. It refuses when any asset of that release has been downloaded, because
   replacing an artefact recomputes the release's checksums and every copy taken
   earlier stops matching them. `-ReplacePublishedAssets`, wired to the
   `REPLACE_PUBLISHED_ASSETS` repository variable, records a deliberate
   exception; the run summary and the log name what was replaced, with the
   download counts that authorised it.

## Consequences

- A stable tag with `PLAY_ROLLOUT=1` produces a completed release, which is
  what the `--require-full-rollout` gate reads as "fully rolled out". A staged
  rollout still works and still has to be raised by hand in the Console.
- Re-pointing a tag remains the recovery mechanism for a broken _workflow_, and
  it is only defensible while a tag is young and unconsumed. A release old
  enough for somebody to have copied a checksum gets a patch release.
- Re-running re-runs everything: the channels that already published have to be
  switched off (`PUBLISH_NPM` and friends) for the run, because npm refuses a
  second publish of the same version, and a red run reads as a broken release.
  The procedure is in `docs/releasing.md` next to the channel table.
- The update proofs are evidence about _bytes_. Replacing a release's assets
  invalidates the proofs that ran against the previous ones, so they are run
  again after a replacement and the tracker cites the later run ids.
