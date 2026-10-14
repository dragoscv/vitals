# ADR-0037: A Play build without all-files access, a GitHub build with it

- Status: accepted
- Date: 2026-10-14
- Amends: ADR-0035 (on-device monitor), consequence on `MANAGE_EXTERNAL_STORAGE`

## Context

ADR-0035 gave the phone and TV a folder scan and file cleanup through
all-files access (`MANAGE_EXTERNAL_STORAGE`) and expected Play to accept it as
a storage analyser. It did not. Google rejected 1.0.1 on 2026-10-14 by
automated review: "Not a core feature … the app feature that you declared as
dependent on the All files access permission didn't meet the policy review
requirements for critical core functionality." Vitals is a system monitor;
the folder scan is one feature among many, so the policy is unlikely to change
its mind on appeal.

ADR-0035 had planned a fallback: replace the scan with a document-tree picker
(Storage Access Framework). On a real phone that picker cannot select the
storage root or `Android/` on Android 11+, so it would measure a few folders
the user picks, not "what fills my phone", which is the question the screen
answers. That is a worse feature for every user, including those who never
install from Play.

## Decision

Build `:app` and `:tv` in two product flavours of one dimension, `store`:

- **`play`** is what Google Play receives. It declares no all-files access.
  Its storage screen shows the categories it can measure with usage access
  (apps, images, video, audio, other), the apps with the largest caches, and a
  button to the system's own storage manager (`ACTION_MANAGE_STORAGE`), which
  can see every folder and needs no permission.
- **`github`** is the APK on GitHub releases (and therefore Obtainium). It
  keeps the folder scan and file cleanup exactly as ADR-0035 describes.

The permissions live in `src/github/AndroidManifest.xml`, not in `:device`.
The monitor reads what the installed package actually requests
(`Access.allFilesAvailable`) instead of a build flag, so the UI cannot offer a
grant button for a permission the package never asked for. The watch has no
flavours and never declared the permission.

The release workflow builds `assembleGithubRelease` for the APK assets and
`bundlePlayRelease` for Play, and fails if a Play bundle's manifest mentions
`MANAGE_EXTERNAL_STORAGE`.

## Consequences

- One codebase, one versionCode, one signing key; the difference is two
  manifest lines and a capability the UI reads.
- A user who switches from the GitHub APK to Play (or back) can, since the
  signatures match. Play users lose the folder scan; the storage screen says
  where to go instead.
- The Play listing and release notes describe only the Play build (category
  view and app caches, not "largest folders and files").
- The Play Console's All-files-access declaration becomes moot; if Play asks
  again, the answer is that the app no longer uses the permission.
