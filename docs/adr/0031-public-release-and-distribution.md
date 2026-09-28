# 0031 — Public beta, published from a public repository

- Status: Accepted
- Date: 2026-09-28
- Tracker: S13
- Supersedes: [0019](0019-git-remote-never-push.md) (the "agents never push" half)

## Context

The app had every screen working against live data but had never been
released: no repository on GitHub, no tag, and a release pipeline that had
never run. The owner decided on 2026-09-28 to ship a public beta and chose,
among the options put to them:

- version **0.9.0-beta.1**, not 1.0 — the pipeline, ARM64 and the idle
  performance budget are unproven until a real tagged run;
- a **public** `dragoscv/vitals`, created and pushed by the agent, with tags
  and releases;
- **no Authenticode certificate yet** (Azure Artifact Signing is not offered
  to individuals in Romania; Certum Open Source and SignPath are the paths),
  with the pipeline ready to sign as soon as a certificate secret exists;
- an **Astro Starlight** site on GitHub Pages at `vitals.dragoscatalin.ro`,
  English and Romanian;
- winget, Scoop, Chocolatey, the Microsoft Store and npm as channels;
- macOS and Linux **built and verified in the release matrix but not
  published** until their samplers exist (ADR 0007 stands);
- updates checked in the background after launch and installed on quit.

## Decision

1. `origin` is public and the agent pushes `main` and release tags. ADR 0019's
   remote half stands; its no-push half is superseded.
2. Betas are published as ordinary releases titled "(public beta)", not as
   GitHub pre-releases, because `/releases/latest/` skips pre-releases and
   both the updater endpoint and the download button resolve through it.
   The version string (`-beta.N`) carries the stability signal.
3. Installers are uploaded twice: under Tauri's versioned name, which winget
   and Chocolatey pin, and under a stable name (`Vitals_x64-setup.exe`) that
   the website links to forever.
4. Every channel job is gated on a repository variable and its secret. A
   channel not yet approved is skipped, never a failed release.
5. The update check is on by default — the one default in the settings that
   makes a network request — because a monitor that never receives its fixes
   is the worse failure. It is disclosed in the Privacy panel, `PRIVACY.md`
   and the website, and switched off in Settings → About.

## Consequences

- SmartScreen warns on first run until a certificate is bought or SignPath
  accepts the project; the download page explains it and provenance
  attestations are the substitute.
- The Store listing waits for a signed installer.
- `scripts/version.ps1 -Check <tag>` is the gate that keeps the four
  manifests in step with the tag; `scripts/third-party-notices.ps1 -Check`
  keeps the licence attributions true after a dependency bump.
