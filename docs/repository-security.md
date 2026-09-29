# Repository security: what protects a release

A tag on this repository ends up on every installed copy: the updater
installs signed releases on its own (`installMode: passive`). So the
controls below are about one question — who can make a release happen — and
each exists because its absence was found in an audit on 2026-09-29.

## What is configured (and how to check it)

| Control                                                      | Where                                                                           | Check                                                                          |
| ------------------------------------------------------------ | ------------------------------------------------------------------------------- | ------------------------------------------------------------------------------ |
| `main` cannot be deleted or force-pushed                     | Ruleset "main: no force-push, no deletion"                                      | `gh api repos/dragoscv/vitals/rules/branches/main`                             |
| `v*` tags: only repository admins may create, move or delete | Ruleset "release tags: owner only, immutable"                                   | `gh api repos/dragoscv/vitals/rulesets`                                        |
| Publishing waits for the owner                               | Environment `release`, required reviewer, only `main` and `v*`                  | `gh api repos/dragoscv/vitals/environments/release`                            |
| Every action pinned to a commit                              | `.github/workflows/*.yml`, `uses: owner/repo@<sha> # vX`                        | `rg -n "uses:\s*\S+@(?![0-9a-f]{40})" --pcre2 .github/workflows` finds nothing |
| CodeQL                                                       | Default setup (Actions, JavaScript/TypeScript)                                  | Security → Code scanning                                                       |
| Licences, crate sources                                      | `deny.toml`, CI Supply chain job, `verify.ps1`, pre-push                        | `cargo deny check licenses bans sources`                                       |
| Advisories                                                   | `cargo audit --deny warnings`, `pnpm audit`, Dependabot alerts + malware alerts | Security → Dependabot                                                          |
| Secret scanning + push protection                            | Repository settings                                                             | Security → Secret scanning                                                     |

The `release` environment means a nightly or a tag builds everything and
then stops at **Publish** until the owner approves it in the Actions run.
Nothing is uploaded, and no updater manifest changes, before that click.

## Owner actions (need the account, not the code)

### 1. Move the signing secrets into the `release` environment

The workflows reference the environment already; the secrets still live at
repository level, where any workflow on `main` can read them. GitHub never
shows a secret's value again, so this cannot be copied by a script:

1. Settings → Environments → `release` → Environment secrets → Add.
2. Re-enter `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`,
   `CHOCO_API_KEY`, `SCOOP_DEPLOY_KEY`, `WINGET_TOKEN` and the four
   `ANDROID_*` secrets with the same names.
3. Delete the repository-level copies (Settings → Secrets and variables →
   Actions).

Until then the environment still enforces the approval; it just does not
yet scope the secrets.

### 2. A fine-grained `WINGET_TOKEN`

The current classic token with `public_repo` can push to every public
repository the account owns. Replace it:

1. github.com/settings/personal-access-tokens/new → Fine-grained.
2. Resource owner: your account; repository access: **only** your fork of
   `microsoft/winget-pkgs`.
3. Permissions: Contents read/write, Pull requests read/write. Nothing else.
4. Expiry: one year; put the renewal date in your calendar.
5. Save as the `WINGET_TOKEN` environment secret (step 1).

### 3. Signed commits and tags (SSH)

```powershell
ssh-keygen -t ed25519 -C 'vitals signing' -f "$HOME\.ssh\vitals_signing"
git config --global gpg.format ssh
git config --global user.signingkey "$HOME\.ssh\vitals_signing.pub"
git config --global commit.gpgsign true
git config --global tag.gpgsign true
Get-Content "$HOME\.ssh\vitals_signing.pub"   # public key only
```

Add the printed public key at github.com/settings/ssh/new with **Key type:
Signing Key**. Commits then show "Verified". Once every agent machine signs,
add "Require signed commits" to the `main` ruleset — not before, or unsigned
agent commits are refused.

### 4. Authenticode through SignPath Foundation (free for open source)

Windows shows SmartScreen for an unsigned installer, and `vitals-sensors.exe`
runs as SYSTEM unsigned. `release.yml` already signs when a certificate is
present (see [releasing.md](releasing.md#authenticode-when-a-certificate-exists)).

1. Apply at https://signpath.org/apply with: project `dragoscv/vitals`,
   licence MIT, the release workflow `.github/workflows/release.yml`, and
   the artefacts `Vitals_*_x64-setup.exe`, `Vitals_*_arm64-setup.exe`,
   `vitals-sensors.exe`, `vitals-watchdog.exe`.
2. They require: an OSI licence (MIT, yes), builds from a public CI (yes),
   a code of conduct and security policy (both in the repo), and that the
   signing request comes from CI, never a workstation.
3. On approval they give an organisation id, project slug and API token;
   the signing step replaces the PFX branch in `release.yml` with
   `signpath/github-action-submit-signing-request`, pinned by SHA like every
   other action.

### 5. Require pinned actions (after the pinned workflows are on `main`)

Settings → Actions → General → "Allow dragoscv, and select non-dragoscv,
actions", tick "Allow actions created by GitHub" and "verified creators",
list `dtolnay/rust-toolchain, Swatinem/rust-cache, taiki-e/install-action,
dorny/paths-filter, pnpm/action-setup, softprops/action-gh-release,
vedantmgoyal9/winget-releaser, gradle/actions`, and tick **Require actions to
be pinned to a full-length commit SHA**. Turning this on before the pinned
workflows land would fail every run.
