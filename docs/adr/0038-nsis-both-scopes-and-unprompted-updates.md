# 0038 — NSIS `both`, and updates nobody has to approve

- Status: Accepted (supersedes the install-mode half of [0025](0025-nsis-per-machine.md))
- Date: 2026-10-17
- Tracker: DEV-3

## Context

0025 said "one UAC prompt at install, none at run time". That was wrong for
updates. The app runs unelevated, and the updater hands its downloaded,
verified setup to NSIS when the user quits. A `perMachine` setup must write
`Program Files`, so Windows raises a UAC prompt, on a desktop the app has
just left. Nobody answers it, the updater logs `os error 1223`, and the next
launch is the old version again. On 2026-10-17 a 1.0.2 install updated to
1.0.3 only when the app happened to be elevated.

Options weighed (asked twice, the second time with the evidence below):

- `currentUser`: no prompt ever, but the 1.0.0–1.0.3 per-machine copies
  would never update again without a prompt, and a second copy lands beside
  the first.
- An elevated helper service that installs updates: removes the prompt by
  running a LocalSystem service for the life of the machine. Large attack
  surface for one benefit.
- A visible "Update and restart": honest, but the update then depends on the
  user noticing it.
- `both` (chosen): one setup that installs for everyone or for the current
  user. Its template asks `RequestExecutionLevel highest`, so on an
  administrator's account it still raises UAC when started normally,
  including when the updater starts it.

## Decision

`installMode: "both"`, and the app decides per copy how an update runs
(`vitals_win::install::plan`, tested exhaustively):

| copy                                     | elevated | on quit                                   |
| ---------------------------------------- | -------- | ----------------------------------------- |
| per-user (`HKCU` entry names its folder) | any      | `/CurrentUser`, under `RunAsInvoker`      |
| per-machine, chosen in a `both` setup    | yes      | `/AllUsers`                               |
| per-machine, chosen in a `both` setup    | no       | not on quit: "Restart to update" (prompt) |
| per-machine from 1.0.0–1.0.3 (legacy)    | yes      | `/AllUsers`                               |
| per-machine from 1.0.0–1.0.3 (legacy)    | no       | `/CurrentUser`, under `RunAsInvoker`      |
| neither (portable, Scoop, a build)       | any      | never                                     |

- **`RunAsInvoker`**: the compatibility layer `__COMPAT_LAYER=RunAsInvoker`,
  set in the app's environment just before the updater's `ShellExecuteW`,
  makes the loader start the setup with the app's own token instead of
  asking for `highest`. Proven on the vivobook before any code was written:
  an NSIS stub with `RequestExecutionLevel highest`, started from an
  unelevated interactive session with the variable set, ran with a standard
  token and no `consent.exe`. The relaunched app clears the variable first
  thing, so its own elevations still prompt.
- **An explicit scope switch on every setup.** MultiUser.nsh remembers the
  last choice under a registry value the Tauri template never writes, so
  without `/CurrentUser` or `/AllUsers` an administrator's update defaults
  to Program Files and silently creates a second copy. The switch rides on
  the updater plugin's `installer_args` for the whole process.
- **"Chosen" is our own marker.** `hooks.nsh` writes `VitalsScopeChosen` on a
  real all-users install and never on `/UPDATE`. The template's own
  `AllUsers` value cannot tell the two apart: every setup writes it,
  including the passive update an elevated 1.0.3 starts, which would
  otherwise turn a legacy copy into a "chosen" one that never migrates.
- **Legacy copies move to a per-user install.** It is the only way they ever
  update again without a prompt. The setup's post-install hook gives the new
  copy a Start menu shortcut (and a desktop one if the old copy had one), the
  app repoints Start-with-Windows at itself, and Settings › About offers once
  to remove the Program Files copy, behind one UAC prompt that follows a
  click. That removal reads the folder from `HKLM`, never from its command
  line, refuses anything outside Program Files, ends only processes running
  from that folder, and switches off a Task Manager hook naming it (a
  per-user copy may not own the hook, 0025's IFEO rule).
- **The outcome is visible.** A marker written before the setup starts is
  compared with the running version at the next launch: "Vitals updated to
  x" or "x was not installed", once.
- **Package managers install per user**: winget `Scope: user`, Chocolatey
  `/S /CurrentUser`. Scoop never ran the installer and is unchanged.

## Consequences

- A person who picks "for everyone" on purpose gets the prompt, in front of
  them, after a click. That is the honest cost of Program Files.
- 1.0.0–1.0.3 copies run the OLD updater, which knows none of this. An
  unelevated 1.0.3 cannot reach 1.0.4 on quit; it reaches it when the user
  updates once by hand (or runs Vitals elevated once). From 1.0.4 on, every
  path above applies.
- Uninstalling a per-user copy without administrator rights cannot remove a
  sensors service the user once enabled by itself. Since S21-06 the
  uninstaller asks: when the helper answers NOT_ELEVATED it runs it again
  with the `runas` verb (one UAC prompt, only when the service or its folder
  exists, never on `/UPDATE`); declined, it leaves the service and shows the
  admin command that removes it. The Task Manager replacement stays
  admin-only, as 0025 required.
- `scripts/ci/update-proof.ps1 -Local` proves the per-user and legacy paths
  unelevated against two throwaway-signed builds
  (`scripts/ci/update-proof-build.ps1`).
