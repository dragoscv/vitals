//! Which paths Vitals refuses to send to the Recycle Bin, decided here.
//!
//! The webview only ever offers folders and files from a scan, but it is not
//! the thing that decides. Anything that reaches the recycle command is
//! checked again in this module, because a check that lives only in the UI
//! is a suggestion: a bug in a list, a stale basket or a hand-written IPC
//! call would otherwise be able to move `C:\Windows\System32` into the bin.
//!
//! # What is refused, and why each rule exists
//!
//! - **Drive roots.** Recycling `C:\` is never what anyone meant.
//! - **Windows, Program Files, and the boot and recovery folders**, and
//!   everything under them. Removing a piece of an installed program breaks
//!   it without uninstalling it; the Apps screen does that properly.
//!   Windows-managed space (`Windows.old`, update caches) is reclaimed through
//!   Windows' own tools, never by moving its files.
//! - **The folders a profile is made of** — `C:\Users`, each profile,
//!   Desktop, Documents, `AppData\Local` — themselves, not their contents.
//!   Their contents are exactly what a person clearing space wants to review;
//!   the folders are what Windows and every app expect to exist.
//! - **Anything above a protected folder**, so a relocated Documents on
//!   `D:\Data\Documents` protects `D:\Data` too.
//! - **Files with the System attribute** (`pagefile.sys`, `hiberfil.sys`).
//! - **Drives with no Recycle Bin** (removable, network). There, "delete"
//!   means gone for good, which this feature never does.
//! - **Vitals' own folder**, which would be removing the running program.
//!
//! # The path is checked as written and as it resolves
//!
//! A short name (`C:\PROGRA~1`) or a junction can spell a protected folder
//! without matching its text. So the canonical path, as the filesystem
//! resolves it, is checked as well, and either one being protected refuses
//! the item. That can over-refuse a link that points at a system folder
//! (recycling a link removes only the link), which is the right way to err.

use std::os::windows::fs::MetadataExt as _;
use std::path::{Path, PathBuf};

use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::UI::Shell::{
    FOLDERID_Desktop, FOLDERID_Documents, FOLDERID_Downloads, FOLDERID_LocalAppData,
    FOLDERID_LocalAppDataLow, FOLDERID_Music, FOLDERID_Pictures, FOLDERID_Profile,
    FOLDERID_ProgramData, FOLDERID_ProgramFiles, FOLDERID_ProgramFilesX86, FOLDERID_Public,
    FOLDERID_RoamingAppData, FOLDERID_UserProfiles, FOLDERID_Videos, FOLDERID_Windows,
    KF_FLAG_DONT_VERIFY, SHGetKnownFolderPath,
};
use windows::core::GUID;

/// `FILE_ATTRIBUTE_SYSTEM`.
const FILE_ATTRIBUTE_SYSTEM: u32 = 0x4;

/// Why a path is not recycled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protection {
    /// Not an absolute local path Vitals can reason about: relative, a
    /// bare `C:`, `..`, a stream name or a wildcard.
    Invalid,
    DriveRoot,
    /// Windows, installed programs, boot, recovery, the bin itself.
    SystemFolder,
    /// A folder a user profile is built from (the folder, not its contents).
    UserFolder,
    /// A file carrying the System attribute.
    SystemFile,
    /// The drive has no Recycle Bin, so the item would be deleted outright.
    NoRecycleBin,
    /// The folder Vitals itself runs from.
    Vitals,
}

impl Protection {
    /// Stable translation key.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Invalid => "invalid",
            Self::DriveRoot => "driveRoot",
            Self::SystemFolder => "systemFolder",
            Self::UserFolder => "userFolder",
            Self::SystemFile => "systemFile",
            Self::NoRecycleBin => "noRecycleBin",
            Self::Vitals => "vitals",
        }
    }
}

/// Folders protected on every drive, with everything below them.
const DRIVE_LEVEL: &[&str] = &[
    "windows",
    "program files",
    "program files (x86)",
    "programdata",
    "$recycle.bin",
    "system volume information",
    "recovery",
    "boot",
    "efi",
    "$winreagent",
    "$sysreset",
    "$windows.~bt",
    "$windows.~ws",
    "windows.old",
    "config.msi",
];

/// Folders directly inside a profile that the profile is made of.
const PROFILE_FOLDERS: &[&str] = &[
    "appdata",
    "desktop",
    "documents",
    "downloads",
    "pictures",
    "music",
    "videos",
    "favorites",
    "contacts",
    "links",
    "saved games",
    "searches",
    "onedrive",
    "3d objects",
];

/// The three roots inside `AppData`.
const APPDATA_FOLDERS: &[&str] = &["local", "roaming", "locallow"];

/// The protection rules, with this machine's folder locations filled in.
#[derive(Debug, Clone)]
pub struct Rules {
    /// Protected with everything under them (and everything above them).
    subtree: Vec<(String, Protection)>,
    /// Protected themselves and everything above them; contents are fine.
    exact: Vec<(String, Protection)>,
    /// `c:\users`, for the per-profile name rules.
    users_root: Option<String>,
}

impl Rules {
    /// Builds the rules from explicit, already normalised locations.
    #[must_use]
    pub fn from_parts(
        subtree: Vec<(String, Protection)>,
        exact: Vec<(String, Protection)>,
        users_root: Option<String>,
    ) -> Self {
        Self {
            subtree,
            exact,
            users_root,
        }
    }

    /// The rules for this machine: its Windows folder, program folders, the
    /// current user's known folders wherever they were moved to, and the
    /// folder Vitals runs from.
    #[must_use]
    pub fn for_this_machine() -> Self {
        let mut subtree = Vec::new();
        let mut exact = Vec::new();
        let known = |id: &GUID| known_folder(id).and_then(|p| normalise(&p));

        for id in [
            &FOLDERID_Windows,
            &FOLDERID_ProgramFiles,
            &FOLDERID_ProgramFilesX86,
            // Installers keep what they need to uninstall and repair here
            // (`Package Cache`, `Microsoft\Windows\Containers`).
            &FOLDERID_ProgramData,
        ] {
            if let Some(path) = known(id) {
                subtree.push((path, Protection::SystemFolder));
            }
        }
        // Environment fallbacks, in case a known-folder lookup failed.
        for var in [
            "SystemRoot",
            "ProgramFiles",
            "ProgramW6432",
            "ProgramFiles(x86)",
        ] {
            if let Some(path) = std::env::var(var).ok().and_then(|p| normalise(&p)) {
                subtree.push((path, Protection::SystemFolder));
            }
        }
        let users_root = known(&FOLDERID_UserProfiles);
        if let Some(users) = &users_root {
            exact.push((users.clone(), Protection::UserFolder));
        }
        for id in [
            &FOLDERID_Public,
            &FOLDERID_Profile,
            &FOLDERID_Desktop,
            &FOLDERID_Documents,
            &FOLDERID_Downloads,
            &FOLDERID_Pictures,
            &FOLDERID_Music,
            &FOLDERID_Videos,
            &FOLDERID_RoamingAppData,
            &FOLDERID_LocalAppData,
            &FOLDERID_LocalAppDataLow,
        ] {
            if let Some(path) = known(id) {
                exact.push((path, Protection::UserFolder));
            }
        }
        if let Some(own) = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(Path::to_path_buf))
            .and_then(|dir| normalise(&dir.to_string_lossy()))
        {
            subtree.push((own, Protection::Vitals));
        }

        Self {
            subtree,
            exact,
            users_root,
        }
    }

    /// Checks a path already passed through [`normalise`]. Text only; the
    /// filesystem checks are in [`vet`].
    #[must_use]
    pub fn check(&self, path: &str) -> Option<Protection> {
        // `c:` is the whole drive once normalised.
        if path.len() <= 2 {
            return Some(Protection::DriveRoot);
        }
        for (root, why) in &self.subtree {
            if is_under(path, root) || is_under(root, path) {
                return Some(*why);
            }
        }
        for (folder, why) in &self.exact {
            if is_under(folder, path) {
                return Some(*why);
            }
        }

        let parts: Vec<&str> = path.get(3..).unwrap_or("").split('\\').collect();
        if parts
            .first()
            .is_some_and(|first| DRIVE_LEVEL.contains(first))
        {
            return Some(Protection::SystemFolder);
        }

        if let Some(users) = &self.users_root
            && is_under(path, users)
            && path.len() > users.len()
        {
            let below: Vec<&str> = path
                .get(users.len() + 1..)
                .unwrap_or("")
                .split('\\')
                .collect();
            let refused = match below.as_slice() {
                [_profile] => true,
                [_profile, folder] => PROFILE_FOLDERS.contains(folder),
                [_profile, "appdata", root] => APPDATA_FOLDERS.contains(root),
                _ => false,
            };
            if refused {
                return Some(Protection::UserFolder);
            }
        }
        None
    }
}

/// `path` is `root` or somewhere below it. Both normalised.
fn is_under(path: &str, root: &str) -> bool {
    path == root
        || (path.len() > root.len()
            && path.starts_with(root)
            && path.as_bytes().get(root.len()) == Some(&b'\\'))
}

/// Lower-cased `c:\a\b`, or `None` for anything that is not a plain absolute
/// local path.
///
/// Refused rather than guessed at: `C:foo` (relative to the current folder
/// on C, the bug S14-02 found in the drive list), `..`, a stream name
/// (`file:stream`), wildcards, and a trailing dot or space (Windows strips
/// those, so `Windows.` would name `Windows`).
#[must_use]
pub fn normalise(path: &str) -> Option<String> {
    let path = path.trim().replace('/', "\\");
    let bytes = path.as_bytes();
    let drive = *bytes.first()?;
    if !drive.is_ascii_alphabetic() || bytes.get(1) != Some(&b':') {
        return None;
    }
    if bytes.len() > 2 && bytes.get(2) != Some(&b'\\') {
        return None;
    }
    let mut out = String::with_capacity(path.len());
    out.push(char::from(drive.to_ascii_lowercase()));
    out.push(':');
    for part in path.get(2..).unwrap_or("").split('\\') {
        if part.is_empty() {
            continue;
        }
        if part == "."
            || part == ".."
            || part.ends_with('.')
            || part.ends_with(' ')
            || part.contains([':', '*', '?', '"', '<', '>', '|'])
        {
            return None;
        }
        out.push('\\');
        out.push_str(&part.to_lowercase());
    }
    Some(out)
}

/// The decision for one path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Vetted {
    /// Safe to hand to the shell, spelled as the caller gave it.
    Allowed(PathBuf),
    Refused(Protection),
    /// Nothing is there (already removed, or never existed).
    Missing,
}

/// Decides whether `raw` may be recycled: text rules, then what the
/// filesystem says about it.
#[must_use]
pub fn vet(raw: &str, rules: &Rules) -> Vetted {
    let Some(normal) = normalise(raw) else {
        return Vetted::Refused(Protection::Invalid);
    };
    if let Some(why) = rules.check(&normal) {
        return Vetted::Refused(why);
    }
    let spelled = PathBuf::from(raw.trim().replace('/', "\\"));
    let Ok(meta) = std::fs::symlink_metadata(&spelled) else {
        return Vetted::Missing;
    };

    if let Ok(canonical) = std::fs::canonicalize(&spelled) {
        let text = canonical.to_string_lossy();
        if text.starts_with(r"\\?\UNC\") {
            return Vetted::Refused(Protection::NoRecycleBin);
        }
        let resolved = text.strip_prefix(r"\\?\").unwrap_or(&text);
        match normalise(resolved) {
            Some(resolved) => {
                if let Some(why) = rules.check(&resolved) {
                    return Vetted::Refused(why);
                }
            }
            None => return Vetted::Refused(Protection::Invalid),
        }
    }

    if !has_recycle_bin(&normal) {
        return Vetted::Refused(Protection::NoRecycleBin);
    }
    // Files only: a folder given a custom icon carries the System attribute
    // too, and that is a user's folder, not Windows'.
    if !meta.is_dir() && meta.file_attributes() & FILE_ATTRIBUTE_SYSTEM != 0 {
        return Vetted::Refused(Protection::SystemFile);
    }
    Vetted::Allowed(spelled)
}

/// Whether the drive of a normalised path is a fixed disk, the only kind
/// Windows keeps a Recycle Bin on.
fn has_recycle_bin(normal: &str) -> bool {
    use windows::Win32::Storage::FileSystem::GetDriveTypeW;
    use windows::core::PCWSTR;
    const DRIVE_FIXED: u32 = 3;
    let root: Vec<u16> = format!("{}\\", normal.get(..2).unwrap_or("c:"))
        .encode_utf16()
        .chain(Some(0))
        .collect();
    // SAFETY: `root` is a live NUL-terminated UTF-16 string for the call.
    unsafe { GetDriveTypeW(PCWSTR(root.as_ptr())) == DRIVE_FIXED }
}

/// The location of a known folder, or `None`.
fn known_folder(id: &GUID) -> Option<String> {
    // SAFETY: `id` is a valid GUID; the returned buffer is freed below.
    let raw = unsafe { SHGetKnownFolderPath(id, KF_FLAG_DONT_VERIFY, None) }.ok()?;
    // SAFETY: on success `raw` is a NUL-terminated string from the shell.
    let text = unsafe { raw.to_string() }.ok();
    // SAFETY: allocated by the shell with CoTaskMemAlloc, freed once.
    unsafe { CoTaskMemFree(Some(raw.0.cast_const().cast())) };
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules() -> Rules {
        Rules::from_parts(
            vec![
                ("c:\\windows".into(), Protection::SystemFolder),
                ("c:\\program files".into(), Protection::SystemFolder),
            ],
            vec![
                ("c:\\users".into(), Protection::UserFolder),
                ("d:\\data\\documents".into(), Protection::UserFolder),
            ],
            Some("c:\\users".into()),
        )
    }

    fn check(path: &str) -> Option<Protection> {
        rules().check(&normalise(path).expect("absolute"))
    }

    #[test]
    fn a_system_folder_and_everything_under_it_are_refused() {
        assert_eq!(check(r"C:\Windows"), Some(Protection::SystemFolder));
        assert_eq!(
            check(r"C:\Windows\System32\drivers\etc\hosts"),
            Some(Protection::SystemFolder)
        );
        assert_eq!(
            check(r"C:\Program Files\App"),
            Some(Protection::SystemFolder)
        );
    }

    #[test]
    fn a_drive_root_is_refused_in_every_spelling() {
        assert_eq!(check(r"C:\"), Some(Protection::DriveRoot));
        assert_eq!(check("c:/"), Some(Protection::DriveRoot));
        assert_eq!(check("C:"), Some(Protection::DriveRoot));
    }

    #[test]
    fn profile_folders_are_refused_but_their_contents_are_not() {
        assert_eq!(check(r"C:\Users"), Some(Protection::UserFolder));
        assert_eq!(check(r"C:\Users\me"), Some(Protection::UserFolder));
        assert_eq!(
            check(r"C:\Users\me\Documents"),
            Some(Protection::UserFolder)
        );
        assert_eq!(
            check(r"C:\Users\me\AppData\Local"),
            Some(Protection::UserFolder)
        );
        assert_eq!(check(r"C:\Users\me\Documents\old-project"), None);
        assert_eq!(check(r"C:\Users\me\AppData\Local\Temp\x"), None);
        assert_eq!(check(r"C:\Users\me\Videos\holiday.mp4"), None);
    }

    #[test]
    fn a_folder_above_a_protected_one_is_refused() {
        // Recycling D:\Data would take the relocated Documents with it.
        assert_eq!(check(r"D:\Data"), Some(Protection::UserFolder));
        assert_eq!(check(r"D:\Data\Documents\notes.txt"), None);
        assert_eq!(check(r"D:\Data\Other"), None);
    }

    #[test]
    fn drive_level_system_folders_are_refused_on_every_drive() {
        assert_eq!(check(r"E:\Windows.old"), Some(Protection::SystemFolder));
        assert_eq!(
            check(r"D:\$Recycle.Bin\S-1-5"),
            Some(Protection::SystemFolder)
        );
        assert_eq!(
            check(r"D:\Program Files\Game"),
            Some(Protection::SystemFolder)
        );
        assert_eq!(check(r"D:\Games"), None);
    }

    #[test]
    fn a_prefix_that_is_not_a_parent_is_not_protected() {
        // `c:\windowsapps-backup` starts with `c:\windows` as text only.
        assert_eq!(check(r"C:\Windows-backup"), None);
        assert_eq!(check(r"C:\Users2"), None);
    }

    #[test]
    fn ambiguous_spellings_are_invalid_rather_than_guessed() {
        for bad in [
            "Windows",
            r"C:Windows",
            r"C:\Users\me\..\..\Windows",
            r"C:\a\file.txt:stream",
            r"C:\a\*",
            r"C:\Windows.",
            r"\\server\share\x",
            "",
        ] {
            assert_eq!(normalise(bad), None, "{bad:?} should not normalise");
        }
    }

    #[test]
    fn this_machines_windows_folder_is_refused_by_vet() {
        let rules = Rules::for_this_machine();
        assert_eq!(
            vet(r"C:\Windows\System32\drivers\etc\hosts", &rules),
            Vetted::Refused(Protection::SystemFolder)
        );
        assert_eq!(vet(r"C:\", &rules), Vetted::Refused(Protection::DriveRoot));
        let profile = std::env::var("USERPROFILE").expect("profile");
        assert_eq!(
            vet(&profile, &rules),
            Vetted::Refused(Protection::UserFolder)
        );
    }

    #[test]
    fn a_short_name_for_a_system_folder_is_caught_through_its_real_path() {
        let rules = Rules::for_this_machine();
        // PROGRA~1 exists only where 8.3 names are enabled; when it does not,
        // the case is Missing, never Allowed.
        let result = vet(r"C:\PROGRA~1", &rules);
        assert_ne!(
            std::mem::discriminant(&result),
            std::mem::discriminant(&Vetted::Allowed(PathBuf::new()))
        );
    }

    #[test]
    fn an_ordinary_temp_file_is_allowed_and_a_missing_one_is_missing() {
        let rules = Rules::for_this_machine();
        let dir = std::env::temp_dir().join(format!("vitals-vet-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let file = dir.join("ok.txt");
        std::fs::write(&file, b"x").expect("write");
        let text = file.to_string_lossy().into_owned();
        assert_eq!(vet(&text, &rules), Vetted::Allowed(file.clone()));
        std::fs::remove_dir_all(&dir).expect("cleanup");
        assert_eq!(vet(&text, &rules), Vetted::Missing);
    }
}
