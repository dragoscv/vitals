# 0045 — Release creation sends no commit target

- **Status:** accepted
- **Date:** 2026-11-20
- **Tracker:** S24-36

## Context

The v1.3.1 release could not be created. Release run `44674536062`, from the
annotated tag `v1.3.1`, passed `Plan`, `Verify`, all four build legs, SBOM and
`Sign + notarise macOS`. `Publish` then failed, and the five channel jobs
(`npm`, `Google Play`, `winget`, Scoop, Chocolatey) skipped, because each is
gated on it.

```
GitHub release failed with status: 403
{"message":"Resource not accessible by integration", ...#create-a-release}
Skip retry
```

Re-running the failed job — approved 15:36:49Z — failed identically, while
githubstatus.com reported all systems operational.

The message blames the token, and the repository has had read-only default
workflow permissions since the tag ruleset went in, so that reading is the
natural one. It is wrong. Four candidate causes were each refuted by a command
rather than by argument, and the record matters more than the fix: "we were
sure it was permissions" is where the hours went.

1. **A downgraded token.** The `publish` job declares `contents: write`,
   `id-token: write` and `attestations: write`, and its four `attest-*` steps
   succeeded in that same run — a token without those scopes could not have
   written them.
2. **Our own edit to the job.** `git diff --stat` of the `publish` job between
   the tags `v1.3.0` and `v1.3.1` is empty. Nothing about the job had changed.
3. **A repository, visibility or ruleset problem.** The repository is public,
   not a fork and not archived; both rulesets date from 2026-09-29 and name the
   owner as bypass actor.
4. **Pin drift to v3 of the action.** `git ls-remote` shows `3bb12739…` is
   `refs/tags/v2.6.2`.

What refuses the call is a field nobody would suspect. GitHub rejects a release
created from `GITHUB_TOKEN` when the request carries `target_commitish` **and
that commit's `.github/workflows/` differs from the default branch** (upstream
`softprops/action-gh-release#836`, comment of 2026-10-21; reproduced with probe
workflows: a body with `tag_name` alone is accepted, the same body plus
`target_commitish` is refused). `softprops/action-gh-release` always sends the
field, defaulting to `github.sha`.

A tag is cut from `main`, so this fires as soon as **any workflow commit lands
on `main` after the tag**. v1.3.0 had none and published on 2026-11-18; v1.3.1
had five and failed two days later with no change to the job.
`git diff --name-status cf2949e main` is 22 files — five workflows, tests, docs
and scripts, **no shipped source**; `Cargo.toml` and `tauri.conf.json` both
still say `1.3.1`.

## Decision

**Create the release through `gh api`, with no commit target at all.**
`target_commitish` only means anything when a tag has to be created, and by the
time `Publish` runs the tag exists and is authoritative; a body carrying
`tag_name` alone is accepted. Measured harmlessly before the fix shipped,
against an existing release so the call could create nothing:

```
$ $env:GH_DEBUG='api'; gh release create v1.3.0 --verify-tag --title probe --notes probe
> POST /repos/…/releases
  "tag_name": "v1.3.0"                # no target_commitish
< HTTP/2.0 422 Unprocessable Entity   # already_exists
```

**Do not re-pin the action, and do not vendor it, to dodge the 403.** The 403 is
a property of the request the action builds, not of the action's version: every
version sends `target_commitish`. Re-pinning hides the cause and re-introduces
the failure on the next bump — which is exactly what the September pin to
v2.6.2 did, and why it is deleted rather than kept as a note about v3.

**Recover a tag whose release does not exist yet by deleting and re-creating
it** at a commit that carries the fix:

```powershell
git push origin :refs/tags/v1.3.1        # delete, so the push below is a creation
git tag -a -f v1.3.1 -m "Vitals 1.3.1" <sha-with-the-fix>
git push origin refs/tags/v1.3.1
```

This is a deliberate exception to the `release tags: owner only, immutable tag`
ruleset, and it is defensible **only while no release has been published against
that tag**: nothing exists on the old commit, so no consumer can be holding it.
The exception expires the moment the release is public, and never applies again
to that tag. The operator's side of this — recognising the 403 and running the
recovery — is the section of [`../releasing.md`](../releasing.md) this record is
referenced from.

Done for v1.3.1: the tag moved from `cf2949e` to `36bf23d`. The new target
touches no shipped source (the 22-file diff above is workflows, tests, docs and
scripts only), so the re-created tag names the same shipped code — `Cargo.toml`
and `tauri.conf.json` are `1.3.1` on both.

## Consequences

- `Publish release` builds its own JSON body and posts it with `gh api`, uploads
  assets with `gh release upload --clobber`, and reads the release back, so the
  run states what the API stored rather than that the calls exited 0.
- **A re-run cannot rescue a tag that already failed.** A re-run executes the
  workflow file from the tag's own commit, so a fix merged to `main` afterwards
  cannot reach it. The obvious response to a red `Publish` — "re-run the job" —
  is not available, and the tag has to be re-cut through the exception above.
- The reasoning generalises beyond releases: a permission-shaped error from a CI
  token is not evidence that permissions are missing. Refute the cheap
  explanations with a command first — here the attestation steps proved the
  scopes were honoured in the very run the 403 appeared to deny.
