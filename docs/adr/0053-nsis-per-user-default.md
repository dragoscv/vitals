# 0053 — NSIS `currentUser` by default, into `%LOCALAPPDATA%\Programs\Vitals`

- Status: Accepted (supersedes the install-mode half of [0038](0038-nsis-both-scopes-and-unprompted-updates.md))
- Date: 2026-11-28
- Tracker: S26-60

## Context

[0038](0038-nsis-both-scopes-and-unprompted-updates.md) chose `installMode:
"both"`: one setup that installs for the current user or for everyone,
selected in the installer's UI. It solved the problem it was written for —
a `perMachine` copy cannot update itself, because the app runs unelevated and
nobody answers a UAC prompt on a desktop the app has just left — by giving
everyone a way to pick the scope that updates silently.

What it did not solve is the prompt at install. A `both` setup is built with
`MultiUser.nsh` and `RequestExecutionLevel highest`, so Windows raises a
`consent.exe` prompt _before_ the installer can offer the choice. On an
administrator's account, installing Vitals — a system monitor, which is what
an administrator is installing it for — begins with a security dialog, and an
unattended install (`/S`) is impossible without first elevating or relying on
installer detection.

The scope this project actually needs is the one that installs silently,
updates silently and needs no privilege at all: the current user's.

The first version of this decision stopped there and accepted the directory
Tauri's template picks for that scope, `%LOCALAPPDATA%\Vitals`. That is the
folder the app keeps its settings, history and logs in, and the documentation
tells the user to delete that folder to start over, so the program would have
lived inside the thing it tells people to throw away. The install directory
has to be its own.

## Decision

`bundle.windows.nsis.installMode` is `"currentUser"` in
`apps/desktop/src-tauri/tauri.conf.json`, and the Trade-off below is documented
in `docs/distribution.md`.

`bundle.windows.nsis.template` is `windows/installer.nsi`: a copy of the
template this version of Tauri embeds in its platform package, differing on
one line. Tauri emits, for the per-user branch,
`StrCpy $INSTDIR "$LOCALAPPDATA\${PRODUCTNAME}"`; the copy names
`$LOCALAPPDATA\Programs\${PRODUCTNAME}`, which is where Windows puts a
per-user program. Nothing else in the file changed — the branch still emits
`RequestExecutionLevel user` and no `MultiUser.nsh`, and the per-machine
branch is untouched, so a `both` build still installs for everyone into
`%ProgramFiles%\Vitals`. The file carries a header saying what it is derived
from and how, so the next reader re-derives it rather than editing it.

One thing the copy does not do is ask the shell where per-user programs go.
`MultiUser.nsh` resolves `FOLDERID_UserProgramFiles` with `GetKnownFolderPath`
and falls back to the literal `$LocalAppData\Programs` this file uses, but that
macro is defined only in `MultiUser.nsh`, which brings
`MULTIUSER_EXECUTIONLEVEL Highest` and the install-mode page with it — the
prompt this decision exists to remove. The literal is the shell's documented
default and the macro's own fallback, so on a default machine the two agree,
and they agree with the per-user half of a `both` install too, which has landed
in `$LOCALAPPDATA\Programs\<product>` since 1.5.x: an upgrade from one of those
finds the same folder and updates in place instead of leaving a second copy
behind. A machine whose `UserProgramFiles` folder has been relocated installs
to the default anyway, under the `currentUser` rule rather than the old one.

Consequences, all of them intended:

- **No prompt, ever.** Not on install, not on update, not on an
  administrator's account. `/S` is a complete unattended install on its own.
- **The install directory is `%LOCALAPPDATA%\Programs\Vitals`**, and **the
  data directory is `%LOCALAPPDATA%\Vitals`** — settings, history, logs,
  `local-api.json`, the watchdog, the sensor helper's log. They are
  deliberately different directories, because the uninstaller leaves the data
  behind on purpose. The elevated drive-cleanup helper only _writes_
  `cleanup-progress-*.jsonl` into the data directory
  (`crates/vitals-win/src/storage/drive_cleanup/elevated.rs`), and the
  uninstaller deletes its own files individually and removes
  `%LOCALAPPDATA%\<bundle id>`, which is the data name, not the program name.
- **An all-users copy is a deliberate build.** `installMode: "both"` or
  `"perMachine"` has to be chosen when the package is built, or the MSIX/Store
  package used, which is the supported machine-wide route. Releases up to
  1.5.x are `both`; a copy installed by one of those keeps behaving as 0038
  describes until it is replaced.
- **The install-mode switch reaches the hooks, and it did not on the first
  try.** `windows/hooks.nsh` guarded two blocks with
  `$MultiUser.InstallMode`, which exists only in a `both` build (that is where
  `MultiUser.nsh` is included). In a `currentUser` build makensis warned
  `6000: unknown variable/constant "MultiUser.InstallMode" detected, ignoring`
  once per use and every comparison was false — so the AllUsers marker was
  never written (harmless here) and the per-user shortcut block 0038's
  migration depends on was skipped, in the very mode that migration now
  produces. The hook branches on `${INSTALLMODE}` at compile time instead and
  reads the variable only in a `both` build; a `perMachine` build, which had
  the same problem and never wrote the marker at all, now writes it.
- **An existing per-machine copy is never updated in place by this setup.** A
  `currentUser` build sets the registry context to `HKCU`, and the template's
  `RestorePreviousInstallLocation` reads the recorded folder under that
  context, so a record left under `HKLM` by a 1.5.x per-machine install does
  not redirect it: whichever account runs the setup, it installs per-user
  under `Programs`, and the old copy is handled by 0038's flow — offer to
  remove it, once, behind a prompt that follows a click.
- **One feature needs an all-users copy and is unavailable by default**: the
  Task Manager replacement, whose guard refuses to point the IFEO hook at any
  executable an ordinary account could swap
  (`crates/vitals-win/src/actions/taskmgr.rs`). That refusal is final, not a
  gap waiting to be filled: Task Manager auto-elevates, so the loader runs
  whatever the `Debugger` value names with Task Manager's administrator token,
  and the account that owns `%LOCALAPPDATA%\Programs\Vitals` can replace the
  file — and undo any ACL tightened on the folder — so a per-user copy can
  never be a safe target. The route that exists is a copy Windows vouches for:
  one installed for all users under `%ProgramFiles%\Vitals`, or the MSIX/Store
  copy under `Program Files\WindowsApps`. The refusal names both, the settings
  panel already explained the same thing in both locales, and
  `hook_location_problem` keeps "Programs" and "Program Files" apart — the
  per-user default is pinned by its own test.
- **The scripts and gates follow the config rather than stating a scope.**
  `scripts/ci/installer-roundtrip.ps1` reads `installMode` and reports the
  machine and upgrade passes as skipped for a `currentUser` build instead of
  failing them, so the CI job still measures the idle budget against the
  per-user copy; `scripts/ci/idle-budget.ps1`,
  `scripts/ci/update-proof-local.ps1`, `scripts/prove-hardware.ps1`,
  `scripts/vivobook-pass.ps1` and the MSIX sparse registration helper resolve
  the per-user copy under `%LOCALAPPDATA%\Programs\Vitals`.
- **The copy is held to the template Tauri ships.**
  `scripts/check-drift.ps1` check 12 extracts the template from the installed
  CLI's platform package, strips this file's provenance header, and requires
  the same number of lines with exactly one difference — the approved
  `StrCpy`. Identical files fail as a lost change, a different line count
  fails as an upgrade that moved the ground, an unrecognised line fails as an
  unapproved edit, and a `template` key pointing anywhere else fails as a
  check that is not checking anything. Re-derive the copy by running the
  extraction again; never edit around the gate.

## Trade-off

Per-user by default is what makes an unattended install and a silent
self-update possible at all: no administrator, no `consent.exe`, nothing for
anyone to answer on a desktop the app has just left. Two costs come with it.
The first is reach: the shipped setup can no longer install for everyone, so
an all-users copy needs `installMode: "both"` or `"perMachine"` built
deliberately, or the MSIX/Store package, which is the supported machine-wide
route, and the Task Manager replacement — which refuses to point the IFEO hook
at an executable the user's own account could swap — stays out of reach for
the default copy. The second is upkeep: a file derived from Tauri's template
must be re-derived whenever Tauri changes its own, and a project would
normally dodge that chore by accepting Tauri's directory. It is worth paying
here, because accepting it would install the program inside the folder the
documentation tells people to delete for a clean slate, and because the gate
turns the chore into a red build rather than a silent stale copy. The rejected
alternative was exactly that: take `%LOCALAPPDATA%\Vitals` as both the program
and the data directory, and say so in the docs.

## Evidence

| Fact                                             | Command                                                                                                       | Output                                                                                                                                                                                                                       |
| ------------------------------------------------ | ------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| The config says `currentUser` and names the copy | `rg -n 'installMode\|template' apps/desktop/src-tauri/tauri.conf.json`                                        | `"installMode": "currentUser",` and `"template": "windows/installer.nsi",`                                                                                                                                                   |
| The copy is one approved line away from Tauri's  | `pwsh -NoProfile -File scripts/check-drift.ps1`                                                               | `drift: 0 failure(s), 0 warning(s) - checked 16 ts_rs files, 144 invokes, 144 commands, 2 locales, 3 shipped icon binaries, 979 installer-template lines`                                                                    |
| Five mutations each go red                       | the same gate after each mutation                                                                             | `identical to the template Tauri ships ... the deliberate change was lost`; `980 lines against 979`; `the header ... is missing or shortened`; `515 is the one line that may differ`; `template is 'windows/nope.nsi'`       |
| The per-user script compiles                     | `makensis` on a rendered copy with the changed line                                                           | exit 0, 18,170,028 bytes, `Programs\Vitals` present as ASCII and as UTF-16                                                                                                                                                   |
| The guard test pins the scope                    | `cargo test -p vitals-desktop --lib the_default_installer_is_per_user_so_no_update_ever_raises_a_prompt`      | pass                                                                                                                                                                                                                         |
| The hooks compile in every mode                  | `makensis` on the same render built as `currentUser`, `both` and `perMachine`                                 | all exit 0; warnings 6000: **2 before the split, 0 after**; the marker string reaches the installer's own string table in the `both` and `perMachine` builds and only in the embedded app's payload in the `currentUser` one |
| The split is real, not inert                     | `!error "MUTANT"` inserted inside the compile-time `currentUser` arm, then both arms compiled                 | `currentUser` exit 1 with the error reported, `both` exit 0 and the mutant never compiled; the live hook file unchanged by SHA256                                                                                            |
| That split cannot rot back                       | `cargo test -p vitals-desktop --lib the_hooks_still_give_a_migrated_copy_its_shortcuts` after three mutations | `1 passed` unmutated; `FAILED` when the `currentUser` arm is deleted, when a read leaves its `both` arm, and when both reads go while the marker writes stay                                                                 |
| The new test is clippy-clean                     | `cargo clippy -p vitals-desktop --all-targets -- -D warnings`                                                 | `Finished` + `EXIT=0`; the first version failed the rust lane with `clippy::needless_continue` (`exit 101`)                                                                                                                  |
| The installer asks for no elevation              | the compiled artefact's own manifest, read out of the 18 MB probe                                             | `requestedExecutionLevel level="asInvoker"`, zero occurrences of `requireAdministrator`                                                                                                                                      |
