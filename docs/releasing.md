# Releasing, and the signing key

Vitals updates itself. The app polls a manifest on GitHub, downloads the new
installer, and refuses to run it unless the download is signed by the private
key whose public half is baked into `apps/desktop/src-tauri/tauri.conf.json`.
That is the whole security model, so most of this document is about the key.

## The key

A minisign keypair, generated once with
`pnpm --filter @vitals/desktop exec tauri signer generate -w <path>`.

On the maintainer's machine it lives **outside the repository**, under
`%USERPROFILE%\.tauri`:

| File                                           | What it is                                                  |
| ---------------------------------------------- | ----------------------------------------------------------- |
| `%USERPROFILE%\.tauri\vitals.key`              | Private key, encrypted with a password. Never commit this.  |
| `%USERPROFILE%\.tauri\vitals.key.password.txt` | The password for that key.                                  |
| `%USERPROFILE%\.tauri\vitals.key.pub`          | Public key. Already committed, as `plugins.updater.pubkey`. |

`.tauri` is a user-profile directory, not a repository path, so there is no
`.gitignore` entry to get wrong and nothing to accidentally stage.

**Back both files up somewhere you will still have in five years** — a
password manager attachment is enough, and is better than a second copy on
the same disk.

### If the private key is lost

There is no recovery. This is the failure worth understanding before it
happens:

- Every already-installed copy of Vitals will only accept updates signed by
  the old key. A new keypair produces signatures those installs reject, so
  they will report a failed update forever and never move again.
- Generating a new keypair and shipping it in `tauri.conf.json` fixes _future_
  installs only.
- The only way to move existing users onto the new key is to have them
  download and run a new installer by hand. In practice that means a release
  announcement, a blog post, and accepting that some users will never see it.

So: losing the key does not compromise anyone, but it does permanently strand
everyone who installed before it was lost.

### If the private key is leaked

Rotate immediately, and treat it as the same migration problem in reverse:
whoever holds the key can sign an installer the app will trust and install
without a prompt. Generate a new pair, ship it, and tell users to reinstall
from GitHub.

## GitHub secrets

Two repository secrets, at
**Settings → Secrets and variables → Actions → New repository secret**:

| Secret name                          | Value                                                                                              |
| ------------------------------------ | -------------------------------------------------------------------------------------------------- |
| `TAURI_SIGNING_PRIVATE_KEY`          | The entire contents of `%USERPROFILE%\.tauri\vitals.key`, including the `untrusted comment:` line. |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | The contents of `%USERPROFILE%\.tauri\vitals.key.password.txt`.                                    |

The names are not arbitrary — the Tauri CLI reads exactly these environment
variables during `tauri build`, and `.github/workflows/release.yml` passes
them through. Get a name wrong and the build still succeeds; it simply
produces no `.sig` files, and the release ships without an update manifest.

The workflow fails loudly on that case rather than publishing a silently
broken release: the "Update manifest" step throws if there is no signature
beside the installer.

## What a release does

Push a `v*` tag. The workflow then:

1. Runs the full gate set — a release that fails its own tests is not
   published.
2. Builds the NSIS installer for windows-x64 and windows-arm64. If the signing
   secrets are present, each installer gets a `.sig` beside it.
3. Writes `latest.json` (windows-x64 only, and only for a tag) containing the
   version, the publication date, the signature, and the download URL of the
   installer.
4. Attests build provenance and writes `SHA256SUMS.txt`.
5. Publishes everything to the GitHub release.

`latest.json` is the file the app fetches, via the endpoint configured in
`tauri.conf.json`:

```
https://github.com/dragoscv/vitals/releases/latest/download/latest.json
```

`/releases/latest/download/` always resolves to the newest release marked
latest, which is why nightlies are excluded — they publish under a moving
`nightly` tag and are never marked latest.

## What a user sees

Nothing, until they ask. There is no background nagging and no automatic
download.

In **Settings → About** there is an _Updates_ section with a **Check for
updates** button. Pressing it produces one of:

- _You are running the latest version._
- _Version X is available_, with the release notes and a **Download update**
  button. Progress is shown while it downloads; if the server does not report
  a size, the bar is indeterminate rather than inventing a percentage.
- _Version X is ready to install_, with **Restart to update**. The app closes,
  the installer runs, and the new version starts.
- _Updates are not configured for this build._ — an honest answer for a
  development build or one produced without the signing secrets. It
  deliberately does **not** say "up to date", because that would hide a broken
  release pipeline behind a reassuring sentence.
- The error text, with a **Try again** button, for a network failure.
