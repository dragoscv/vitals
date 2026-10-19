//! What a drive is for, so the per-drive clean-up offers only what belongs
//! on it.
//!
//! The decision is a pure function over [`DriveFacts`]; gathering the facts
//! is the only part that touches Windows. That split is what lets every rule
//! be tested without a Dev Drive, a USB stick or a second Windows install.

use std::path::{Path, PathBuf};

use vitals_core::metrics::DiskKind;

/// The role the wizard shows and plans for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriveRole {
    System,
    Dev,
    Data,
    External,
    Network,
    Optical,
}

impl DriveRole {
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Dev => "dev",
            Self::Data => "data",
            Self::External => "external",
            Self::Network => "network",
            Self::Optical => "optical",
        }
    }

    /// Whether anything at all is offered: a share belongs to another
    /// machine, and a disc cannot be written.
    #[must_use]
    pub const fn offers_anything(self) -> bool {
        !matches!(self, Self::Network | Self::Optical)
    }
}

/// What was found out about one volume.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
// Independent yes/no observations; an enum would invent exclusivity
// between facts that routinely hold together (Windows and WSL disks on C:).
pub struct DriveFacts {
    pub holds_windows: bool,
    pub remote: bool,
    pub optical: bool,
    pub removable: bool,
    pub dev_volume: bool,
    pub holds_projects: bool,
    pub holds_pnpm_store: bool,
    pub holds_worktrees: bool,
    pub holds_wsl_disks: bool,
    pub holds_docker_disk: bool,
}

/// The role and the i18n keys that explain it.
///
/// Order matters: a share or a disc is never anything else; the Windows
/// volume stays `system` even when it also holds projects, because what
/// Windows itself can free is only offered there; a USB disk with a repo on
/// it is still `external`, so nothing that stops WSL is planned on it.
#[must_use]
pub fn classify(facts: &DriveFacts) -> (DriveRole, Vec<&'static str>) {
    if facts.remote {
        return (DriveRole::Network, vec!["role.reason.network"]);
    }
    if facts.optical {
        return (DriveRole::Optical, vec!["role.reason.optical"]);
    }
    let mut reasons = Vec::new();
    let role = if facts.holds_windows {
        reasons.push("role.reason.holdsWindows");
        DriveRole::System
    } else if facts.removable {
        reasons.push("role.reason.removable");
        DriveRole::External
    } else if facts.dev_volume
        || facts.holds_projects
        || facts.holds_pnpm_store
        || facts.holds_worktrees
    {
        DriveRole::Dev
    } else {
        reasons.push("role.reason.data");
        DriveRole::Data
    };
    for (fact, key) in [
        (facts.dev_volume, "role.reason.devDrive"),
        (facts.holds_projects, "role.reason.holdsProjects"),
        (facts.holds_pnpm_store, "role.reason.holdsPnpmStore"),
        (facts.holds_worktrees, "role.reason.holdsWorktrees"),
        (facts.holds_wsl_disks, "role.reason.holdsWslDisks"),
        (facts.holds_docker_disk, "role.reason.holdsDockerDisk"),
    ] {
        if fact {
            reasons.push(key);
        }
    }
    (role, reasons)
}

/// `C:` for `C:\anything`, upper-cased; `None` for a path without a drive
/// letter (a UNC path, a relative one).
#[must_use]
pub fn drive_of(path: &Path) -> Option<String> {
    let text = path.to_str()?;
    let text = text.strip_prefix(r"\\?\").unwrap_or(text);
    let mut chars = text.chars();
    let letter = chars.next()?;
    (letter.is_ascii_alphabetic() && chars.next() == Some(':'))
        .then(|| format!("{}:", letter.to_ascii_uppercase()))
}

/// Whether `path` lives on the drive `mount` (`C:` or `C:\`).
#[must_use]
pub fn is_on(path: &Path, mount: &str) -> bool {
    drive_of(path).is_some_and(|d| d.eq_ignore_ascii_case(normalise_mount(mount)))
}

/// `C:` for `C:`, `c:\` or `C:/`.
#[must_use]
pub fn normalise_mount(mount: &str) -> &str {
    mount.trim_end_matches(['\\', '/'])
}

/// Gathers the facts for `mount` (`C:`).
///
/// `project_roots` are the developer clean-up's default roots; `vhdx` the
/// WSL and Docker disks found on this machine as (path, is Docker).
#[must_use]
pub fn facts(
    mount: &str,
    kind: DiskKind,
    project_roots: &[PathBuf],
    vhdx: &[(PathBuf, bool)],
) -> DriveFacts {
    let mount = normalise_mount(mount);
    let on = |p: &Path| is_on(p, mount);
    let roots: Vec<&PathBuf> = project_roots.iter().filter(|r| on(r)).collect();
    let local_pnpm = std::env::var_os("LOCALAPPDATA")
        .map(|l| PathBuf::from(l).join("pnpm\\store"))
        .filter(|p| on(p) && p.is_dir());
    DriveFacts {
        holds_windows: windows_directory().is_some_and(|w| on(&w)),
        remote: kind == DiskKind::Network,
        optical: kind == DiskKind::Optical,
        removable: kind == DiskKind::Removable,
        dev_volume: is_dev_volume(mount),
        holds_projects: !roots.is_empty(),
        holds_pnpm_store: Path::new(&format!("{mount}\\.pnpm-store")).is_dir()
            || local_pnpm.is_some(),
        holds_worktrees: roots.iter().any(|r| r.join(".wt").is_dir()),
        holds_wsl_disks: vhdx.iter().any(|(p, docker)| !docker && on(p)),
        holds_docker_disk: vhdx.iter().any(|(p, docker)| *docker && on(p)),
    }
}

/// The Windows folder, from the kernel rather than `%SystemRoot%`: an
/// inherited environment variable must not decide which drive is treated
/// as the one Windows lives on.
#[must_use]
pub fn windows_directory() -> Option<PathBuf> {
    use std::os::windows::ffi::OsStringExt as _;
    use windows_sys::Win32::System::SystemInformation::GetWindowsDirectoryW;
    let mut buf = [0_u16; 260];
    // SAFETY: `buf` is a live, writable buffer of the length passed.
    let len = unsafe { GetWindowsDirectoryW(buf.as_mut_ptr(), buf.len() as u32) } as usize;
    (len > 0 && len < buf.len()).then(|| PathBuf::from(std::ffi::OsString::from_wide(&buf[..len])))
}

/// Whether the volume is a Dev Drive: `ReFS` with the persistent "developer
/// volume" flag, which Windows sets when the drive is created as one.
fn is_dev_volume(mount: &str) -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE,
        FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows_sys::Win32::System::IO::DeviceIoControl;
    use windows_sys::Win32::System::Ioctl::{
        FILE_FS_PERSISTENT_VOLUME_INFORMATION, FSCTL_QUERY_PERSISTENT_VOLUME_STATE,
        PERSISTENT_VOLUME_STATE_DEV_VOLUME,
    };

    let root: Vec<u16> = format!("{mount}\\").encode_utf16().chain(Some(0)).collect();
    // SAFETY: `root` is NUL-terminated; the handle is closed below.
    let handle = unsafe {
        CreateFileW(
            root.as_ptr(),
            FILE_READ_ATTRIBUTES,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return false;
    }
    let input = FILE_FS_PERSISTENT_VOLUME_INFORMATION {
        VolumeFlags: 0,
        FlagMask: PERSISTENT_VOLUME_STATE_DEV_VOLUME,
        Version: 1,
        Reserved: 0,
    };
    let mut output = input;
    let mut returned = 0_u32;
    let size = size_of::<FILE_FS_PERSISTENT_VOLUME_INFORMATION>() as u32;
    // SAFETY: both buffers are live structs of `size` bytes; `handle` is open.
    let ok = unsafe {
        DeviceIoControl(
            handle,
            FSCTL_QUERY_PERSISTENT_VOLUME_STATE,
            (&raw const input).cast(),
            size,
            (&raw mut output).cast(),
            size,
            &raw mut returned,
            std::ptr::null_mut(),
        )
    };
    // SAFETY: `handle` came from CreateFileW and is closed exactly once.
    unsafe { CloseHandle(handle) };
    ok != 0 && output.VolumeFlags & PERSISTENT_VOLUME_STATE_DEV_VOLUME != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_windows_volume_is_system_even_when_it_also_holds_projects_and_wsl_disks() {
        let (role, reasons) = classify(&DriveFacts {
            holds_windows: true,
            holds_projects: true,
            holds_wsl_disks: true,
            ..DriveFacts::default()
        });
        assert_eq!(role, DriveRole::System);
        assert_eq!(
            reasons,
            [
                "role.reason.holdsWindows",
                "role.reason.holdsProjects",
                "role.reason.holdsWslDisks"
            ]
        );
    }

    #[test]
    fn a_share_is_network_and_offers_nothing_whatever_else_is_true_of_it() {
        let (role, reasons) = classify(&DriveFacts {
            remote: true,
            holds_projects: true,
            holds_windows: true,
            ..DriveFacts::default()
        });
        assert_eq!(role, DriveRole::Network);
        assert!(!role.offers_anything());
        assert_eq!(reasons, ["role.reason.network"]);
    }

    #[test]
    fn a_usb_disk_with_a_repository_on_it_stays_external() {
        let (role, _) = classify(&DriveFacts {
            removable: true,
            holds_projects: true,
            ..DriveFacts::default()
        });
        assert_eq!(role, DriveRole::External);
    }

    #[test]
    fn a_dev_drive_or_a_drive_of_projects_is_dev_and_says_which() {
        let (role, reasons) = classify(&DriveFacts {
            dev_volume: true,
            ..DriveFacts::default()
        });
        assert_eq!(
            (role, reasons),
            (DriveRole::Dev, vec!["role.reason.devDrive"])
        );
        let (role, reasons) = classify(&DriveFacts {
            holds_worktrees: true,
            holds_pnpm_store: true,
            ..DriveFacts::default()
        });
        assert_eq!(role, DriveRole::Dev);
        assert_eq!(
            reasons,
            ["role.reason.holdsPnpmStore", "role.reason.holdsWorktrees"]
        );
    }

    #[test]
    fn a_drive_with_nothing_special_is_data_and_still_reports_its_wsl_disks() {
        let (role, reasons) = classify(&DriveFacts {
            holds_wsl_disks: true,
            ..DriveFacts::default()
        });
        assert_eq!(role, DriveRole::Data);
        assert_eq!(reasons, ["role.reason.data", "role.reason.holdsWslDisks"]);
    }

    #[test]
    fn paths_are_matched_to_their_drive_however_the_mount_is_written() {
        assert!(is_on(Path::new(r"c:\Users\x"), "C:"));
        assert!(is_on(Path::new(r"\\?\E:\wsl\ext4.vhdx"), r"e:\"));
        assert!(!is_on(Path::new(r"\\server\share\x"), "C:"));
        assert!(!is_on(Path::new(r"D:\x"), "C:"));
    }
}
