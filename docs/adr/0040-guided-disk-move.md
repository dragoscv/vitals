# ADR-0040: Guided move of WSL and Docker Desktop disks to another drive

- Status: accepted
- Date: 2026-10-19
- Tracker: S22-07

## Context

The per-drive clean-up (S22-01..03) can shrink a `.vhdx`, but a system drive
that is nearly full because a 120 GB distro or a 190 GB Docker data disk
lives on it needs the disk somewhere else. On the machine this was written on
`C:` holds `docker_data.vhdx` (191 GB) and two distro disks while `E:` has
hundreds of GB free.

Moving a WSL disk by hand (Explorer, `robocopy`) leaves the registration
pointing at the old path. Microsoft's own disk-space page warns against
moving WSL files with Windows tools. So the question was which supported
mechanism to drive, for each kind of disk.

## Research

### WSL: `wsl --manage <distro> --move <folder>`

- Introduced in **WSL 2.3.11** (pre-release, 17 Jul 2024): "Implement wsl
  --manage --move" — <https://github.com/microsoft/WSL/releases/tag/2.3.11>.
  Learn documents `wsl --manage` from 2.5 on (for `--resize`):
  <https://learn.microsoft.com/en-us/windows/wsl/disk-space>. We gate on
  2.3.11 (the version that implemented it) and refuse an unreadable version.
- What it does, from the service source
  (`LxssUserSessionImpl::MoveDistribution`,
  <https://github.com/microsoft/WSL/blob/master/src/windows/service/exe/LxssUserSession.cpp>):
  - fails with `WSL_E_DISTRO_NOT_STOPPED` if the distro is running (it does
    not stop it), and while an export/conversion holds it;
  - refuses a folder another distro is registered at;
  - `E_NOTIMPL` for WSL 1;
  - creates the folder, then `MoveFileExW(MOVEFILE_COPY_ALLOWED |
MOVEFILE_WRITE_THROUGH)` — copy then delete across volumes; without
    `MOVEFILE_REPLACE_EXISTING`, so an existing file of that name fails it;
  - resets the new file's owner to the calling user;
  - writes `BasePath` and `VhdFileName` under
    `HKCU\Software\Microsoft\Windows\CurrentVersion\Lxss\{guid}`;
  - on failure moves the file back and restores `BasePath`.
- It runs impersonating the caller and opens the per-user `Lxss` key: no
  elevation is needed (and an elevated call would read another hive's view
  of nothing new). The default user and every other registration value are
  untouched.
- Disks must be uncompressed and unencrypted
  (<https://learn.microsoft.com/en-us/windows/wsl/troubleshooting>, error
  `0xc03a001a`, <https://github.com/microsoft/WSL/issues/4103>).
- Known failure after a move: `E_ACCESSDENIED` starting the distro when the
  new folder's ACL lacks the user or the file is EFS-encrypted
  (<https://github.com/microsoft/WSL/issues/13775>). The fix users report is
  granting Full control on the folder.

### WSL: export / unregister / import

`wsl --export --vhd`, `wsl --unregister`, `wsl --import --vhd` /
`--import-in-place` (<https://learn.microsoft.com/en-us/windows/wsl/basic-commands>).
Import registers with the default UID root (`LX_UID_ROOT` in the source) and
loses shortcuts and Terminal profile; `--unregister` deletes the original
disk. Every step is a chance to lose data, and the user would need twice the
disk's space during it.

### Docker Desktop

- The data disk moves from Settings > Resources > Advanced > **Disk image
  location**; Docker copies the data itself
  (<https://docs.docker.com/desktop/settings-and-maintenance/settings/>,
  <https://docs.docker.com/desktop/features/wsl/>). Support added in 4.32.0;
  moves **to another drive always failed** until 4.92.0 (robocopy exit 1 read
  as failure, <https://github.com/docker/desktop-feedback/issues/548>); 4.91
  fixed an interrupted move deleting the image; 4.68 fixed a failed distro
  move leaving it unregistered; 4.95 refuses locations inside the install
  folder (<https://docs.docker.com/desktop/release-notes/>).
- No supported CLI or key changes it after install. `docker desktop`
  (start/stop/restart/status, GA in 4.39) has no such subcommand
  (<https://docs.docker.com/reference/cli/docker/desktop/>). The settings
  store (`%APPDATA%\Docker\settings-store.json`, renamed in 4.35) has a
  `CustomWslDistroDir` key that community workarounds edit
  (<https://github.com/docker/for-win/issues/13384>), but it is absent from
  the settings reference
  (<https://docs.docker.com/enterprise/security/hardened-desktop/settings-management/settings-reference/>).
  No documented `docker-desktop://` link opens a settings page.

## Decision

1. **WSL distros move with `wsl --manage <distro> --move
<drive>:\Vitals\WSL\<distro>`**, run as the user (no UAC), never with
   export/import. WSL < 2.3.11 or an unreadable version: the disk is listed
   with the reason and not offered. WSL 1 and names WSL itself would reject
   are not offered either.
2. **Docker Desktop is guided, not driven.** The dialog shows the exact
   setting, a suggested folder `<drive>:\Vitals\Docker` on an eligible drive,
   a button that starts Docker Desktop, and "Check again", which reads
   `CustomWslDistroDir` (read-only) and looks for `disk\docker_data.vhdx`
   there or at the default. A Docker Desktop older than 4.92 is warned about.
   Vitals never writes Docker's files and never moves `docker-desktop*`
   distros with `wsl --manage`.
3. **Target eligibility** (`drive_move::assess_target`), in the order the
   user hears it: not the same drive; fixed (not removable, network or
   optical — a pulled USB disk is a corrupted distro); NTFS (FAT cannot hold
   a file over 4 GB, ReFS/exFAT are not what WSL documents; an unread file
   system counts as not NTFS); free ≥ size × 1.1 + 2 GB; an unmeasured disk
   size refuses (never assumed to fit). Then the folder: no file of the same
   name already there, nearest existing folder not compressed or encrypted.
   Every drive is listed in the picker; the refused ones say why.
4. **Pre-flight and run**: the plan is kept in the desktop behind a plan id;
   the webview sends plan id, disk id and a drive the plan marked eligible,
   with `confirmed: true`. The backend re-checks everything before stopping
   anything. It stops only that distro (`wsl --terminate`, waits ≤ 30 s for
   it to leave `--running`), runs the move (one retry if something restarted
   the distro and WSL said `DISTRO_NOT_STOPPED`, which changes nothing),
   reports bytes written at the destination every 500 ms (WSL prints no
   progress), then verifies: registration `BasePath` = new folder, file at
   target, old file gone, `wsl -d <d> --exec true` starts it, free space
   gained on the source drive. Runners inside the distro are checked active
   again; a distro that was stopped is stopped again after the start check.
   The move shares the clean-up's busy slot.
5. **Cancel only before the move.** `wsl --manage --move` is one call that
   copies and rolls back on its own; killing it mid-copy is the one way to
   leave both a half file and an uncertain registration. The dialog says so
   and cannot be closed while copying.
6. **Confirmation** needs a backup tick ("I have a backup … or I accept the
   risk") and states that the move is reversible (move it back the same
   way) and nothing is deleted before the copy is complete.
7. **Entry point**: in the clean-up wizard, each `compactVhd` row of a drive
   under 15 % or 25 GB free gets "Move to another drive…", lazily loading
   `MoveDiskDialog` (the wizard carries one small entry file).

## Consequences

- A real move was **not run on the development workstation** (live dev
  databases and CI runners in those distros). The planning half is proven by
  `cargo run -p vitals-win --example prove_drive_move -- --dry-run` against
  the real registry, volumes and Docker settings; the move itself is to be
  exercised on the test laptop (`ssh dragos-vivobook`).
- If WSL changes `--move` semantics (for example starts stopping the distro
  itself), the stop step becomes redundant, not wrong.
- Docker's path depends on the user completing Docker's own dialog; the
  check is what tells them it worked.
- `CustomWslDistroDir` is read, not relied on: if Docker renames it, "Check
  again" falls back to the default location and says it could not find the
  disk rather than claim success.
