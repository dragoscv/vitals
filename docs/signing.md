# Code signing

Windows warns on the first launch of a program it has not seen before. This is
the decision record for that warning: what ships unsigned today, the two routes
that remove it, the application pack for the free one, and the integration it
would need. The full application letter is in
[signing-and-store-identity.md](signing-and-store-identity.md) (tracker S26-45).

**The decision is to apply to the SignPath Foundation**, which signs
open-source projects at no cost. Submission is owner-only: a human fills their
web form and accepts their agreement. Everything that can be prepared is here.

## 1. Where we are today

- **The installers ship unsigned.** The bundle declares only NSIS
  (`apps/desktop/src-tauri/tauri.conf.json:38`) with no `certificateThumbprint`,
  `signCommand` or `pfx`. The Windows build leg reads an optional
  `WINDOWS_CERTIFICATE` secret and, with none set, prints
  `::notice::No WINDOWS_CERTIFICATE: the installer is not Authenticode-signed.`
  (`.github/workflows/release.yml:470`); with one set it imports the PFX and
  passes `certificateThumbprint` to Tauri (`release.yml:478`). Neither secret exists: `gh secret list` returns twelve and no certificate variable is among them (`docs/tracker.csv:361`).
- **The consequence is the SmartScreen prompt.** An unsigned installer makes
  Windows Defender SmartScreen show "Windows protected your PC" on first run,
  and reputation rather than a signature is what eventually clears it
  ([distribution.md](distribution.md)).
- **The SYSTEM helper is unsigned too.** `vitals-sensors.exe` runs as SYSTEM without an Authenticode signature (`docs/repository-security.md:74-75`).
- **The MSIX path is Microsoft's signature, not ours.** The Store package is
  uploaded unsigned and Microsoft signs it (`docs/releasing.md:931`,
  `docs/distribution.md:68`), so only the direct download warns.
- **The updater signature is different.** Bundles carry a minisign signature we
  hold (`release.yml:977`), which Microsoft does not check.

## 2. The two routes

| Field              | SignPath Foundation (apply)                                                                                                                                                                             | Buy a certificate (OV, or EV)                                                                                                                                                                                                                                                                                                                            |
| ------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Cost               | None; free for open-source projects (`https://signpath.org/`, read 2026-11-28)                                                                                                                          | About €250–600 a year (`docs/distribution.md:26`)                                                                                                                                                                                                                                                                                                        |
| Turnaround         | An application they review and accept at their discretion, with no published SLA, and **every release then needs manual approval to sign** (`https://signpath.org/terms`, read 2026-11-28)              | Issuing is quick (UNVERIFIED, not sourced); building SmartScreen reputation takes weeks to months (`docs/distribution.md:25`)                                                                                                                                                                                                                            |
| Key custody        | SignPath generates and stores the private key on their HSM; we never hold it (`https://signpath.org/`, read 2026-11-28)                                                                                 | Ours, but it cannot be an exportable file (next row)                                                                                                                                                                                                                                                                                                     |
| Hardware token     | Not ours — the key is SignPath's                                                                                                                                                                        | **Required.** Since 1 June 2023 the subscriber key must be generated, stored and used in a Hardware Crypto Module certified to FIPS 140-2 Level 2 or Common Criteria EAL 4+, in a cloud HSM, or by a qualifying signing service (`https://cabforum.org/working-groups/code-signing/requirements/` §6.2.7.4.1, v3.11.0 dated 2026-06-16, read 2026-11-28) |
| Suits this project | Yes, by application: an OSI-approved licence with no commercial dual-licensing, a public build, and a project already released in the form to be signed (`https://signpath.org/terms`, read 2026-11-28) | Yes — anyone may buy one                                                                                                                                                                                                                                                                                                                                 |
| If we never sign   | The SmartScreen warning stays on every first install; the Store copy stays warning-free                                                                                                                 | —                                                                                                                                                                                                                                                                                                                                                        |

Two caveats from SignPath's own terms (https://signpath.org/terms, read 2026-11-28):
they require "a certain verifiable reputation" for executables and refuse software
that scans for weaknesses such as "unprotected network ports"; Vitals probes nothing.

## 3. The application pack

Every value below is read from the repository; anything held only in a live listing or a secret is left blank for the owner.

| Field             | Value                                                                                                                                                                           | Source                                                    |
| ----------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------- |
| Project name      | Vitals                                                                                                                                                                          | `apps/desktop/src-tauri/tauri.conf.json:3`                |
| Repository        | https://github.com/dragoscv/vitals                                                                                                                                              | `Cargo.toml:27`                                           |
| Licence           | MIT, OSI-approved, no commercial dual-licensing                                                                                                                                 | `LICENSE:1`, `Cargo.toml:26`, `README.md:8`               |
| Website           | https://vitals.dragoscatalin.ro                                                                                                                                                 | `tauri.conf.json:49`                                      |
| Build system      | GitHub Actions, `.github/workflows/release.yml`                                                                                                                                 | `release.yml:29-30`                                       |
| Version / tag     | SemVer in `Cargo.toml:23` and `tauri.conf.json:4`; released from annotated tags `vX.Y.Z`, matched by `refs/tags/v*`                                                             | `release.yml:29-30`                                       |
| Artefacts to sign | `Vitals_<version>_x64-setup.exe`, `Vitals_<version>_arm64-setup.exe`, and the `vitals-sensors.exe` / `vitals-watchdog.exe` they carry; optionally `vitals.exe` from the CLI zip | `tauri.conf.json:38`, `docs/repository-security.md:78-81` |
| Maintainer        | Dragos Catalin Vladulescu ([@dragoscv](https://github.com/dragoscv)), the only human contributor, Romania                                                                       | `CONTRIBUTORS.md:5`, `PRIVACY.md:44-45`                   |
| Contact           | dragoscv12@gmail.com                                                                                                                                                            | `PRIVACY.md:46`, `SUPPORT.md:14`                          |

Ready to paste, from those facts:

```text
Project: Vitals
Repository: https://github.com/dragoscv/vitals
Licence: MIT (OSI-approved, no commercial dual-licensing)
Maintainer: Dragos Catalin Vladulescu (@dragoscv), the only human contributor
Contact: dragoscv12@gmail.com
Build: GitHub Actions, .github/workflows/release.yml
Artefacts: Vitals_<version>_x64-setup.exe and Vitals_<version>_arm64-setup.exe
  (NSIS installers), the vitals-sensors.exe and vitals-watchdog.exe they carry,
  and optionally vitals.exe from the CLI zip
Requested by: CI on a vX.Y.Z tag only, never a workstation
Website: https://vitals.dragoscatalin.ro
Legal name for the certificate: <OWNER>
```

One paragraph of description:

> Vitals is a free, open-source (MIT) system monitor and task manager for
> Windows 10 and 11, with companion apps for Android, Wear OS, Google TV, iOS
> and Samsung TVs. It shows a person what their computer is doing and why it is
> slow: processes with real names and icons, live processor, memory, graphics
> and network readings, drive health, temperatures, storage clean-up and a lag
> watchdog that names the program behind a frozen window. It is written in Rust
> (a Tauri 2 app with a React webview) and TypeScript, and tested in public CI.

Practices in favour, each enforced rather than promised: every action pinned to a
full-length commit SHA (`docs/repository-security.md:93-98`); licence and
crate-source allowlists in `deny.toml:10-12`; contract-drift
(`scripts/verify.ps1:278`), bindings-drift (`verify.ps1:309`) and binary-identity
(`verify.ps1:296`) gates; SBOMs built and attested (`release.yml:626`,
`release.yml:1072`); a SLSA provenance attestation (`release.yml:1066`); and
build legs that never hold a signing key (`release.yml:583`).

## 4. What we build once accepted

All of it inside the `publish` job of `release.yml`, in this order:

1. **Allow-path first.** Add `signpath/github-action-submit-signing-request` to
   the repository's allowed-actions list by full path, or the job fails at setup
   with "is not allowed" (`docs/repository-security.md:93-98`). Pin it to a SHA.
2. **Sign after the build, before anything is hashed.** It goes after the
   installers are assembled and **before** `Stable download names`
   (`release.yml:1046`) and `Checksums` (`release.yml:1054`): Authenticode
   changes the bytes, so `SHA256SUMS.txt` must describe what a user downloads.
3. **Sign before the updater signature.** It must also run **before**
   `Sign updater bundles` (`release.yml:977`): that signature covers the final
   bytes, so signing afterwards would invalidate it (reasoned, not measured).
4. **Nothing else moves.** Publishing copies whatever is in `publish/`, and the
   legs stay unsigned and key-free — the signing step lives in `publish`, which
   on a tag runs only after the owner approves the `release` environment
   (`release.yml:805`, `release.yml:870`).

**Only the owner can:** create the SignPath organisation, submit the
application, and store the API token issued on acceptance as a repository or
`release`-environment secret named `SIGNPATH_API_TOKEN` — never committed. The
organisation id, project slug and artefact configuration also come from SignPath
on acceptance and cannot be guessed here.

## 5. If we buy instead

An **OV** certificate is sufficient: reputation is what clears the warning, and
since 2024 an EV certificate builds it the same way, so EV's extra cost buys a
verified legal-entity subject rather than faster removal
(`docs/distribution.md:34-36`). The key cannot be a plain PFX: it must live in a
FIPS 140-2 Level 2 (or Common Criteria EAL 4+) Hardware Crypto Module, a cloud
HSM or a qualifying signing service
(`https://cabforum.org/working-groups/code-signing/requirements/` §6.2.7.4.1,
read 2026-11-28), so it would be a certified USB token kept offline or a cloud
HSM, driven through `bundle.windows.signCommand` rather than
`certificateThumbprint` (`docs/releasing.md:653`, `release.yml:478`). The choice
is reversible; the purchased-certificate branch (`release.yml:470`) is not merged
into it.

## Unverified

- The cost and turnaround of a purchased certificate come from
  `docs/distribution.md`, not a vendor page read for this note; SignPath's
  requirement wording is quoted from `https://signpath.org/terms` (read
  2026-11-28), whose form could not be read, so any field it asks for beyond
  those above is UNVERIFIED.
- That the published installers are unsigned is inferred from the absence of
  `WINDOWS_CERTIFICATE` (`release.yml:470`) and from `docs/TRACKER.md` S26-40,
  not read from the published artefacts.
