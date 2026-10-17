//! Which kind of installation this copy of Vitals is, and how an update over
//! it can run without a UAC prompt nobody is there to answer (DEV-3).
//!
//! The installer is NSIS with `installMode: "both"` (ADR 0038): one setup
//! that installs either for everyone (`Program Files`, `HKLM`) or for the
//! current user (`%LOCALAPPDATA%\Programs`, `HKCU`). The template asks for
//! `RequestExecutionLevel highest`, so on an administrator's account every
//! start of the setup raises UAC — including the passive update the app
//! starts on quit. For a per-user installation that prompt is pure noise:
//! nothing it writes needs administrator rights. Starting the setup with the
//! `RunAsInvoker` compatibility layer (inherited through the environment by
//! the `ShellExecuteW` the updater plugin makes) runs it with the caller's
//! token instead, and `/CurrentUser` tells it which half to update.
//!
//! Proven on a real machine before this was written: an NSIS stub with
//! `RequestExecutionLevel highest`, started from an unelevated interactive
//! session with `__COMPAT_LAYER=RunAsInvoker`, ran without `consent.exe` and
//! saw a standard token.
//!
//! A per-machine installation cannot be updated by an unelevated process at
//! all — that is what the ACL on `Program Files` is for — so the decision
//! here is about which of three honest outcomes applies, never about
//! getting round it. [`plan`] is pure and tested exhaustively; the registry
//! reads that feed it are thin.

use std::path::{Path, PathBuf};

use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RRF_SUBKEY_WOW6464KEY, RegGetValueW,
};
use windows::core::HSTRING;

/// The uninstall key the NSIS template writes, under `HKLM` or `HKCU`.
const UNINSTALL_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\Vitals";

/// Written by `hooks.nsh` on a real all-users install, never on an update.
/// The template's own `AllUsers` value is no use here: every setup writes it,
/// including the passive update an elevated 1.0.3 starts.
const SCOPE_CHOSEN_VALUE: &str = "VitalsScopeChosen";

/// The argument the app's elevated child is started with to remove an old
/// per-machine copy after the user moved to a per-user one.
///
/// Public because `launch.rs` must recognise it before Tauri builds, and a
/// second spelling of the same string is exactly how that breaks silently.
pub const REMOVE_MACHINE_INSTALL_ARG: &str = "--remove-machine-install";

/// Where this copy of Vitals is installed, as far as an update cares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallScope {
    /// Installed for the current user only (`HKCU` uninstall entry naming
    /// this folder). Updates need no administrator rights.
    CurrentUser,
    /// Installed for everyone by a setup that records the choice
    /// ([`SCOPE_CHOSEN_VALUE`] under the `HKLM` uninstall key). The person
    /// chose Program Files; an update keeps it there.
    AllUsersChosen,
    /// Installed for everyone by a 1.0.0–1.0.3 setup, which was per-machine
    /// only and asked nobody. Updated to a per-user installation when the
    /// app is not elevated, because that is the only way those copies ever
    /// update again without a prompt.
    AllUsersLegacy,
    /// Neither entry names this folder: a portable copy, Scoop, a developer
    /// build. The updater would install a second copy elsewhere, so it does
    /// not run.
    Unregistered,
}

/// What to do with a downloaded, verified update when the app quits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdatePlan {
    /// Start the setup now, with these extra arguments, optionally under
    /// `RunAsInvoker` so it raises no UAC prompt.
    Install {
        args: Vec<&'static str>,
        as_invoker: bool,
    },
    /// Do not install on quit: the user would be asked for administrator
    /// approval with nothing on screen explaining why. Offer "Restart to
    /// update" instead, where the prompt follows a click.
    NeedsApproval,
    /// Not an installation the updater owns.
    NotOurs,
}

/// The decision table. Pure, so every row is tested.
///
/// | scope            | elevated | plan                                        |
/// |------------------|----------|---------------------------------------------|
/// | `CurrentUser`    | any      | install `/CurrentUser`, `RunAsInvoker`      |
/// | `AllUsersChosen` | yes      | install `/AllUsers`                         |
/// | `AllUsersChosen` | no       | needs approval (Restart to update)          |
/// | `AllUsersLegacy` | yes      | install `/AllUsers`                         |
/// | `AllUsersLegacy` | no       | install `/CurrentUser`, `RunAsInvoker`      |
/// | `Unregistered`   | any      | not ours                                    |
///
/// `RunAsInvoker` even when elevated for `CurrentUser`: the invoker's token
/// is then the elevated one, so nothing changes for that caller, and one
/// rule per scope is easier to reason about than a rule per token.
/// The explicit `/CurrentUser` / `/AllUsers` is not decoration: MultiUser.nsh
/// reads its remembered choice from a value the Tauri template never writes,
/// so without the switch an administrator's update would default to
/// Program Files and silently create a second copy.
#[must_use]
pub fn plan(scope: InstallScope, elevated: bool) -> UpdatePlan {
    match (scope, elevated) {
        (InstallScope::CurrentUser, _) | (InstallScope::AllUsersLegacy, false) => {
            UpdatePlan::Install {
                args: vec!["/CurrentUser"],
                as_invoker: true,
            }
        }
        (InstallScope::AllUsersChosen | InstallScope::AllUsersLegacy, true) => {
            UpdatePlan::Install {
                args: vec!["/AllUsers"],
                as_invoker: false,
            }
        }
        (InstallScope::AllUsersChosen, false) => UpdatePlan::NeedsApproval,
        (InstallScope::Unregistered, _) => UpdatePlan::NotOurs,
    }
}

/// The scope switch every setup this process starts must carry: the
/// [`plan`]'s, or `/AllUsers` for the approved install of a chosen
/// per-machine copy. `None` for a copy the updater does not own.
///
/// One value per process, because neither the folder nor the token changes
/// while it runs; it is handed to the updater plugin once, so the Settings
/// panel's download path and the install on quit agree.
#[must_use]
pub fn installer_args(scope: InstallScope, elevated: bool) -> Option<Vec<&'static str>> {
    match plan(scope, elevated) {
        UpdatePlan::Install { args, .. } => Some(args),
        UpdatePlan::NeedsApproval => Some(vec!["/AllUsers"]),
        UpdatePlan::NotOurs => None,
    }
}

/// What the registry says about the two possible installations.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Registrations {
    /// `InstallLocation` of the `HKCU` entry, quotes stripped.
    pub user: Option<PathBuf>,
    /// `InstallLocation` of the `HKLM` entry, quotes stripped.
    pub machine: Option<PathBuf>,
    /// Whether the `HKLM` entry carries [`SCOPE_CHOSEN_VALUE`], written when
    /// the person chose "for everyone".
    pub machine_chosen: bool,
}

/// Classifies the folder `exe_dir` against the registrations. Pure.
///
/// By folder rather than by "which entry exists": after a migration both
/// entries can exist for a while (the old per-machine copy waits for the
/// user to remove it), and the copy that is running is the one to update.
#[must_use]
pub fn classify(exe_dir: &Path, reg: &Registrations) -> InstallScope {
    let same = |p: &Option<PathBuf>| p.as_deref().is_some_and(|p| same_dir(p, exe_dir));
    if same(&reg.user) {
        InstallScope::CurrentUser
    } else if same(&reg.machine) {
        if reg.machine_chosen {
            InstallScope::AllUsersChosen
        } else {
            InstallScope::AllUsersLegacy
        }
    } else {
        InstallScope::Unregistered
    }
}

/// Case-insensitive, separator- and trailing-slash-tolerant folder equality.
fn same_dir(a: &Path, b: &Path) -> bool {
    let norm = |p: &Path| {
        p.to_string_lossy()
            .replace('/', "\\")
            .trim_end_matches('\\')
            .to_ascii_lowercase()
    };
    norm(a) == norm(b)
}

/// Reads both uninstall entries.
#[must_use]
pub fn registrations() -> Registrations {
    Registrations {
        user: read(HKEY_CURRENT_USER, "InstallLocation")
            .as_deref()
            .map(unquoted_path),
        machine: read(HKEY_LOCAL_MACHINE, "InstallLocation")
            .as_deref()
            .map(unquoted_path),
        machine_chosen: read(HKEY_LOCAL_MACHINE, SCOPE_CHOSEN_VALUE).is_some(),
    }
}

/// The scope of the running executable.
#[must_use]
pub fn current_scope() -> InstallScope {
    let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
    else {
        return InstallScope::Unregistered;
    };
    classify(&dir, &registrations())
}

/// A per-machine copy left behind by a migration: an `HKLM` entry whose
/// folder is not the one running. `None` when there is none.
///
/// Only reported for a copy registered without the choice marker, and only
/// when the running copy is per-user: a person who deliberately has both is
/// not nagged about either.
#[must_use]
pub fn leftover_machine_install() -> Option<PathBuf> {
    let exe_dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    let reg = registrations();
    leftover(&exe_dir, &reg)
}

/// Pure half of [`leftover_machine_install`].
#[must_use]
pub fn leftover(exe_dir: &Path, reg: &Registrations) -> Option<PathBuf> {
    if classify(exe_dir, reg) != InstallScope::CurrentUser || reg.machine_chosen {
        return None;
    }
    reg.machine.clone().filter(|m| !same_dir(m, exe_dir))
}

fn unquoted_path(raw: &str) -> PathBuf {
    PathBuf::from(raw.trim().trim_matches('"'))
}

/// Makes every program this process starts from now on run with this
/// process's token, even one whose manifest asks for `highest`.
///
/// The updater plugin starts the setup with `ShellExecuteW`, which takes no
/// environment of its own and so passes ours on; the compatibility layer is
/// read by the loader of the child, never by us. Called on the way out,
/// just before the install, so nothing else this process starts is
/// affected.
///
/// Through `SetEnvironmentVariableW` rather than `std::env::set_var`: the
/// Win32 environment block is guarded by the process environment lock,
/// which is what makes the call sound while other threads are still alive.
///
/// # Errors
///
/// The OS error when the variable could not be set; the setup would then
/// ask for elevation, which the caller should avoid starting on quit.
pub fn run_children_as_invoker() -> windows::core::Result<()> {
    use windows::Win32::System::Environment::SetEnvironmentVariableW;
    // SAFETY: both arguments are valid NUL-terminated wide strings that
    // outlive the call.
    unsafe {
        SetEnvironmentVariableW(
            &HSTRING::from("__COMPAT_LAYER"),
            &HSTRING::from("RunAsInvoker"),
        )
    }
}

/// Removes a `__COMPAT_LAYER` this process inherited.
///
/// The setup a per-user update starts runs under `RunAsInvoker` and then
/// restarts the app, which inherits the variable; every elevated child the
/// app later starts for the user (ending a protected process, the Task
/// Manager switch) would inherit it too. Cleared first thing, so the layer
/// lives exactly as long as the one setup it was set for.
pub fn clear_inherited_compat_layer() {
    use windows::Win32::System::Environment::SetEnvironmentVariableW;
    // SAFETY: a valid wide name and a null value, which deletes it.
    let _ = unsafe {
        SetEnvironmentVariableW(
            &HSTRING::from("__COMPAT_LAYER"),
            windows::core::PCWSTR::null(),
        )
    };
}

/// One `REG_SZ` from the Vitals uninstall key in `hive`, 64-bit view.
fn read(hive: HKEY, value: &str) -> Option<String> {
    read_value(hive, UNINSTALL_KEY, value)
}

/// The Start-with-Windows entry the autostart plugin writes:
/// `HKCU\…\Run` value `Vitals`, an unquoted `<exe> --minimized`.
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_VALUE: &str = "Vitals";

/// The Run value rewritten to start `exe`, when it names a different
/// `vitals-desktop.exe`. Pure.
///
/// After a migration the entry still names the Program Files copy, which
/// the user is about to remove; the next sign-in would then start nothing.
/// Only the path is replaced, so the arguments survive, and a value that
/// does not name Vitals is never touched.
#[must_use]
pub fn repointed_run_value(current: &str, exe: &Path) -> Option<String> {
    const NAME: &str = "vitals-desktop.exe";
    let lower = current.to_ascii_lowercase();
    let end = lower.find(NAME)? + NAME.len();
    let old = current.get(..end)?.trim_start_matches('"');
    if same_dir(Path::new(old), exe) {
        return None;
    }
    let rest = current.get(end..)?.trim_start_matches('"');
    Some(format!("{}{rest}", exe.display()))
}

/// Applies [`repointed_run_value`] to the real entry. Returns whether it
/// was rewritten. Only from a registered per-user copy: a developer build
/// or a portable copy must never steal the entry.
#[must_use]
pub fn repoint_autostart() -> bool {
    use windows::Win32::System::Registry::{REG_SZ, RegSetKeyValueW};
    if current_scope() != InstallScope::CurrentUser {
        return false;
    }
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    let Some(current) = read_value(HKEY_CURRENT_USER, RUN_KEY, RUN_VALUE) else {
        return false;
    };
    let Some(next) = repointed_run_value(&current, &exe) else {
        return false;
    };
    let wide: Vec<u16> = next.encode_utf16().chain(std::iter::once(0)).collect();
    let bytes = u32::try_from(wide.len() * 2).unwrap_or(0);
    // SAFETY: `wide` is a NUL-terminated buffer of `bytes` bytes that
    // outlives the call; the key and value names are valid wide strings.
    let rc = unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            &HSTRING::from(RUN_KEY),
            &HSTRING::from(RUN_VALUE),
            REG_SZ.0,
            Some(wide.as_ptr().cast()),
            bytes,
        )
    };
    rc.is_ok()
}

/// One `REG_SZ` from `key` in `hive`, 64-bit view.
fn read_value(hive: HKEY, key: &str, value: &str) -> Option<String> {
    let mut buf = vec![0_u16; 1024];
    let mut len = u32::try_from(buf.len() * 2).unwrap_or(0);
    // SAFETY: `buf` holds `len` bytes and outlives the call, which writes at
    // most that many and updates `len` to the count written.
    let rc = unsafe {
        RegGetValueW(
            hive,
            &HSTRING::from(key),
            &HSTRING::from(value),
            RRF_RT_REG_SZ | RRF_SUBKEY_WOW6464KEY,
            None,
            Some(buf.as_mut_ptr().cast()),
            Some(&raw mut len),
        )
    };
    if rc.is_err() {
        return None;
    }
    let units = (len as usize / 2).min(buf.len());
    let text = String::from_utf16_lossy(buf.get(..units).unwrap_or(&[]));
    let text = text.trim_end_matches('\0').trim().to_owned();
    (!text.is_empty()).then_some(text)
}

/// Runs the elevated removal of an old per-machine copy and returns the
/// process exit code. Called in the elevated child started with
/// [`REMOVE_MACHINE_INSTALL_ARG`].
///
/// The folder comes from the `HKLM` entry, never from the command line: an
/// argument an unelevated caller chose must not pick what an administrator
/// process deletes. It must sit under Program Files and must not be the
/// running copy.
///
/// Not the copy's own `uninstall.exe`: its hook ends every
/// `vitals-desktop.exe` by image name, which is the per-user app that asked
/// for this, and without `/UPDATE` it also turns off the sensors service and
/// Start with Windows, which the per-user copy goes on using. Instead, only
/// what belongs to that folder goes: processes running from it, a Task
/// Manager hook naming it, its files, its `HKLM` entry and the all-users
/// shortcuts pointing into it.
#[must_use]
pub fn remove_machine_install() -> u32 {
    let reg = registrations();
    let Some(dir) = reg.machine.clone() else {
        // Already gone: the goal is met.
        return 0;
    };
    let Some(program_files) = crate::actions::taskmgr::known_program_files() else {
        return 3;
    };
    if !is_under(&dir, &program_files) {
        return 4;
    }
    if std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(Path::to_path_buf))
        .is_some_and(|running| same_dir(&running, &dir))
    {
        return 5;
    }
    end_processes_under(&dir);
    // A hook naming the old copy would turn Ctrl+Shift+Esc into "file not
    // found" once its files are gone. The per-user copy cannot take it over
    // (taskmgr.rs refuses a user-writable target), so it is switched off.
    if let Ok(crate::actions::ReplacementStatus::ReplacedByUs { path }) =
        crate::actions::replacement_status()
        && is_under(Path::new(&path), &dir)
        && crate::actions::taskmgr::write_replacement(false, Path::new(&path)).is_err()
    {
        return 6;
    }
    // A file still mapped is retried once after its process had time to go.
    if std::fs::remove_dir_all(&dir).is_err() {
        std::thread::sleep(std::time::Duration::from_secs(1));
        if std::fs::remove_dir_all(&dir).is_err() && dir.exists() {
            return 7;
        }
    }
    if delete_machine_entry().is_err() {
        return 8;
    }
    remove_common_shortcuts(&dir);
    0
}

/// Ends every process whose executable lives under `dir` — the old copy's
/// watchdog, typically. Nothing outside it is touched, so the per-user app
/// that started this removal keeps running.
fn end_processes_under(dir: &Path) {
    use windows::Win32::System::Threading::{OpenProcess, PROCESS_TERMINATE, TerminateProcess};
    let Ok(all) = crate::ProcessEnumerator::new().enumerate() else {
        return;
    };
    for p in all {
        let pid = p.key.pid.get();
        let inside = crate::process::image_path::image_path_by_pid(pid)
            .is_some_and(|image| is_under(Path::new(&image), dir));
        if !inside {
            continue;
        }
        // SAFETY: plain handle open/terminate/close on a PID; a recycled PID
        // is re-checked by path above an instant earlier, and the worst case
        // of a race is a failed open.
        unsafe {
            if let Ok(handle) = OpenProcess(PROCESS_TERMINATE, false, pid) {
                let _ = TerminateProcess(handle, 1);
                let _ = windows::Win32::Foundation::CloseHandle(handle);
            }
        }
    }
}

/// Deletes the `HKLM` uninstall entry, so Settings › Apps stops listing a
/// copy that is gone and [`leftover`] stops reporting it.
fn delete_machine_entry() -> windows::core::Result<()> {
    use windows::Win32::Foundation::ERROR_FILE_NOT_FOUND;
    use windows::Win32::System::Registry::{KEY_WOW64_64KEY, RegDeleteKeyExW};
    // SAFETY: a predefined root and a valid wide string.
    let rc = unsafe {
        RegDeleteKeyExW(
            HKEY_LOCAL_MACHINE,
            &HSTRING::from(UNINSTALL_KEY),
            KEY_WOW64_64KEY.0,
            None,
        )
    };
    if rc.is_ok() || rc == ERROR_FILE_NOT_FOUND {
        Ok(())
    } else {
        Err(rc.into())
    }
}

/// `/UPDATE` keeps shortcuts (so an update does not unpin them). Here the
/// copy is going away, so its all-users Start menu and desktop links go too
/// — only ones that point into the removed folder.
fn remove_common_shortcuts(dir: &Path) {
    let roots = [
        std::env::var_os("ProgramData")
            .map(|p| PathBuf::from(p).join(r"Microsoft\Windows\Start Menu\Programs\Vitals.lnk")),
        std::env::var_os("PUBLIC").map(|p| PathBuf::from(p).join(r"Desktop\Vitals.lnk")),
    ];
    let needle = dir.to_string_lossy().to_ascii_lowercase();
    for lnk in roots.into_iter().flatten() {
        // A .lnk stores its target as UTF-16 among binary fields; a lossy
        // search for the folder is enough to tell ours from a stranger's.
        let points_here = std::fs::read(&lnk).is_ok_and(|bytes| {
            let units: Vec<u16> = bytes
                .as_chunks::<2>()
                .0
                .iter()
                .map(|p| u16::from_le_bytes(*p))
                .collect();
            String::from_utf16_lossy(&units)
                .to_ascii_lowercase()
                .contains(&needle)
        });
        if points_here {
            let _ = std::fs::remove_file(&lnk);
        }
    }
}

fn is_under(path: &Path, root: &Path) -> bool {
    let p = path
        .to_string_lossy()
        .replace('/', "\\")
        .to_ascii_lowercase();
    let r = root
        .to_string_lossy()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_ascii_lowercase();
    p.len() > r.len() + 1
        && p.starts_with(&r)
        && p.as_bytes().get(r.len()) == Some(&b'\\')
        && !p.split('\\').any(|part| part == "..")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reg(user: Option<&str>, machine: Option<&str>, chosen: bool) -> Registrations {
        Registrations {
            user: user.map(PathBuf::from),
            machine: machine.map(PathBuf::from),
            machine_chosen: chosen,
        }
    }

    const USER_DIR: &str = r"C:\Users\a\AppData\Local\Programs\Vitals";
    const MACHINE_DIR: &str = r"C:\Program Files\Vitals";

    #[test]
    fn a_per_user_copy_updates_as_the_invoker_whether_or_not_elevated() {
        for elevated in [false, true] {
            assert_eq!(
                plan(InstallScope::CurrentUser, elevated),
                UpdatePlan::Install {
                    args: vec!["/CurrentUser"],
                    as_invoker: true
                }
            );
        }
    }

    #[test]
    fn an_unelevated_legacy_machine_copy_moves_to_a_per_user_install_without_a_prompt() {
        assert_eq!(
            plan(InstallScope::AllUsersLegacy, false),
            UpdatePlan::Install {
                args: vec!["/CurrentUser"],
                as_invoker: true
            }
        );
    }

    #[test]
    fn an_elevated_machine_copy_stays_in_program_files() {
        for scope in [InstallScope::AllUsersChosen, InstallScope::AllUsersLegacy] {
            assert_eq!(
                plan(scope, true),
                UpdatePlan::Install {
                    args: vec!["/AllUsers"],
                    as_invoker: false
                }
            );
        }
    }

    #[test]
    fn a_chosen_machine_copy_is_never_installed_unelevated_on_quit() {
        assert_eq!(
            plan(InstallScope::AllUsersChosen, false),
            UpdatePlan::NeedsApproval
        );
    }

    #[test]
    fn an_unregistered_copy_is_never_updated_over() {
        for elevated in [false, true] {
            assert_eq!(
                plan(InstallScope::Unregistered, elevated),
                UpdatePlan::NotOurs
            );
        }
    }

    #[test]
    fn the_running_folder_decides_the_scope_not_which_entry_exists() {
        let both = reg(Some(USER_DIR), Some(MACHINE_DIR), false);
        assert_eq!(
            classify(Path::new(USER_DIR), &both),
            InstallScope::CurrentUser
        );
        assert_eq!(
            classify(Path::new(MACHINE_DIR), &both),
            InstallScope::AllUsersLegacy
        );
        let chosen = reg(None, Some(MACHINE_DIR), true);
        assert_eq!(
            classify(Path::new(r"c:\program files\vitals\"), &chosen),
            InstallScope::AllUsersChosen
        );
        assert_eq!(
            classify(Path::new(r"D:\portable\Vitals"), &both),
            InstallScope::Unregistered
        );
    }

    #[test]
    fn a_leftover_machine_copy_is_reported_only_after_a_migration() {
        let migrated = reg(Some(USER_DIR), Some(MACHINE_DIR), false);
        assert_eq!(
            leftover(Path::new(USER_DIR), &migrated),
            Some(PathBuf::from(MACHINE_DIR))
        );
        // A person who chose Program Files on purpose is not nagged.
        let chosen = reg(Some(USER_DIR), Some(MACHINE_DIR), true);
        assert_eq!(leftover(Path::new(USER_DIR), &chosen), None);
        // Running from the machine copy: nothing is left over.
        assert_eq!(leftover(Path::new(MACHINE_DIR), &migrated), None);
        // Only a per-user copy.
        assert_eq!(
            leftover(Path::new(USER_DIR), &reg(Some(USER_DIR), None, false)),
            None
        );
    }

    #[test]
    fn removal_only_targets_folders_strictly_inside_program_files() {
        let root = Path::new(r"C:\Program Files");
        assert!(is_under(Path::new(r"C:\Program Files\Vitals"), root));
        assert!(!is_under(Path::new(r"C:\Program Files Evil\Vitals"), root));
        assert!(!is_under(Path::new(r"C:\Program Files"), root));
        assert!(!is_under(
            Path::new(r"C:\Program Files\..\Users\a\Vitals"),
            root
        ));
        assert!(!is_under(Path::new(USER_DIR), root));
    }

    #[test]
    fn the_registry_is_readable_unelevated_and_never_panics() {
        // Whatever this machine has installed, reading must not fail loudly.
        let _ = registrations();
        let _ = current_scope();
    }

    #[test]
    fn every_setup_names_its_half_so_an_admin_update_never_lands_in_the_other() {
        // Without the switch MultiUser.nsh falls back to Program Files for an
        // administrator, creating a second copy beside the per-user one.
        assert_eq!(
            installer_args(InstallScope::CurrentUser, true),
            Some(vec!["/CurrentUser"])
        );
        assert_eq!(
            installer_args(InstallScope::AllUsersChosen, false),
            Some(vec!["/AllUsers"])
        );
        assert_eq!(installer_args(InstallScope::Unregistered, false), None);
    }

    #[test]
    fn start_with_windows_follows_the_copy_that_moved_and_keeps_its_arguments() {
        let new = Path::new(r"C:\Users\a\AppData\Local\Programs\Vitals\vitals-desktop.exe");
        assert_eq!(
            repointed_run_value(
                r"C:\Program Files\Vitals\vitals-desktop.exe --minimized",
                new
            )
            .as_deref(),
            Some(r"C:\Users\a\AppData\Local\Programs\Vitals\vitals-desktop.exe --minimized")
        );
        assert_eq!(
            repointed_run_value(
                r#""C:\Program Files\Vitals\vitals-desktop.exe" --minimized"#,
                new
            )
            .as_deref(),
            Some(r"C:\Users\a\AppData\Local\Programs\Vitals\vitals-desktop.exe --minimized")
        );
        // Already right: no write.
        assert_eq!(
            repointed_run_value(
                r"c:\users\a\appdata\local\programs\vitals\VITALS-DESKTOP.EXE --minimized",
                new
            ),
            None
        );
        // Not ours: never touched.
        assert_eq!(repointed_run_value(r"C:\Tools\other.exe", new), None);
    }
}
