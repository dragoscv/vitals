//! The `CompanyName` an executable declares in its version resource.
//!
//! ## Why this, and why it is not the publisher
//!
//! "Hide all Microsoft services" is the one filter everyone reaches for, and
//! `msconfig` answers it from exactly this field. It is a *claim* the file
//! makes about itself, not a verified signature: any binary can write
//! "Microsoft Corporation" there. That is acceptable for a view filter — the
//! worst case is that something hides from a list the user can unhide with
//! one click — and it is why this is reported as `company` and never
//! written into [`super::StartupEntry::publisher`], which is reserved for an
//! Authenticode-verified signer.

use std::collections::HashMap;
use std::path::Path;

use windows_sys::Win32::Storage::FileSystem::{
    GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW,
};

/// Reads `CompanyName` from a file's version resource.
///
/// `None` when the file is absent, has no version resource, or the resource
/// has no company string. None of those is "not Microsoft" in any measured
/// sense, which is why the caller treats `None` as *unknown* and does not
/// hide the row.
#[must_use]
pub fn company_name(path: &Path) -> Option<String> {
    version_string(path, "CompanyName")
}

/// Reads `FileDescription` — the name an executable gives itself, which is
/// what Task Manager shows as the app's name (`Code - Insiders.exe` →
/// "Visual Studio Code - Insiders").
///
/// `None` on the same terms as [`company_name`]; the caller falls back to
/// the file name.
#[must_use]
pub fn file_description(path: &Path) -> Option<String> {
    version_string(path, "FileDescription")
}

/// Reads one string from a file's version resource.
fn version_string(path: &Path, field: &str) -> Option<String> {
    let wide: Vec<u16> = path.to_str()?.encode_utf16().chain(Some(0)).collect();

    let mut handle = 0_u32;
    // SAFETY: `wide` is NUL-terminated and alive; `handle` is an out-param.
    let size = unsafe { GetFileVersionInfoSizeW(wide.as_ptr(), &raw mut handle) };
    if size == 0 {
        return None;
    }

    // `u32` storage keeps the block 4-byte aligned, which the resource's
    // WORD/DWORD fields assume when `VerQueryValueW` hands back pointers
    // into it.
    let mut block = vec![0_u32; (size as usize).div_ceil(4)];
    // SAFETY: `block` holds at least `size` bytes.
    let ok = unsafe { GetFileVersionInfoW(wide.as_ptr(), 0, size, block.as_mut_ptr().cast()) };
    if ok == 0 {
        return None;
    }

    // The declared translations first; the two code pages nearly every
    // English-built binary uses after, because some resources omit the
    // translation table while still carrying the strings.
    let mut candidates = translations(&block);
    candidates.extend(["040904B0".to_owned(), "040904E4".to_owned()]);

    candidates
        .iter()
        .find_map(|lang| query_string(&block, &format!(r"\StringFileInfo\{lang}\{field}")))
}

/// Whether a declared company is Microsoft.
///
/// A substring match rather than equality: the resource says "Microsoft
/// Corporation" in most binaries, "Microsoft Corp." in some drivers and
/// "© Microsoft Corporation" in a few, and all three mean the same thing.
#[must_use]
pub fn is_microsoft(company: Option<&str>) -> bool {
    company.is_some_and(|name| name.to_ascii_lowercase().contains("microsoft"))
}

/// Windows programs that run *someone else's* code.
///
/// Their version resource says Microsoft whatever they are asked to run, so
/// reading it for `rundll32.exe vendor.dll,Entry` hid third-party entries
/// behind "Hide Microsoft" — found on a real machine, where "Logitech
/// Download Assistant" and a smart-card vendor's registration both did. The
/// company of such an entry is unknown, and unknown stays visible.
const GENERIC_HOSTS: &[&str] = &[
    "rundll32.exe",
    "regsvr32.exe",
    "cmd.exe",
    "powershell.exe",
    "pwsh.exe",
    "wscript.exe",
    "cscript.exe",
    "mshta.exe",
    "conhost.exe",
    "svchost.exe",
    "dllhost.exe",
    "taskhostw.exe",
    "msiexec.exe",
    "explorer.exe",
];

/// Whether `path` is a launcher whose own company says nothing about what
/// it runs.
#[must_use]
pub fn is_generic_host(path: &str) -> bool {
    path.rsplit(['\\', '/'])
        .next()
        .is_some_and(|name| GENERIC_HOSTS.iter().any(|h| name.eq_ignore_ascii_case(h)))
}

/// A per-sweep memo of [`company_name`].
///
/// A hundred-odd services share `svchost.exe`, so reading the resource once
/// per distinct path rather than once per service is the difference between
/// a few milliseconds and a noticeably slower Services tab.
#[derive(Debug, Default)]
pub struct CompanyCache(HashMap<String, Option<String>>);

impl CompanyCache {
    /// The company behind a service: its `ServiceDll` when it runs inside
    /// svchost, otherwise its own image.
    ///
    /// When the DLL cannot be read the host's own company is used. Unlike a
    /// `Run` value, a service's command line is written by whoever installed
    /// it into an ACL'd key, and third-party shared-host services register a
    /// readable `ServiceDll` — the unreadable ones measured here (`DoSvc`,
    /// `COMSysApp`) are all Windows' own. Leaving them unknown put thirty
    /// Windows services back into a list the user had asked to be free of them.
    pub fn for_service(&mut self, name: &str, image_path: Option<&str>) -> Option<String> {
        match image_path {
            Some(image) if is_generic_host(image) => {
                let key = image.to_ascii_lowercase();
                match super::services::service_dll(name) {
                    Some(dll) => self.get(&dll),
                    None => self
                        .0
                        .entry(key)
                        .or_insert_with(|| company_name(Path::new(image)))
                        .clone(),
                }
            }
            Some(image) => self.get(image),
            None => None,
        }
    }

    /// The company for `path`, reading the file at most once per sweep.
    ///
    /// `None` for a [generic host](is_generic_host), without reading it.
    pub fn get(&mut self, path: &str) -> Option<String> {
        if is_generic_host(path) {
            return None;
        }
        let key = path.to_ascii_lowercase();
        self.0
            .entry(key)
            .or_insert_with(|| company_name(Path::new(path)))
            .clone()
    }
}

fn translations(block: &[u32]) -> Vec<String> {
    let mut pointer: *mut core::ffi::c_void = std::ptr::null_mut();
    let mut len = 0_u32;
    let sub: Vec<u16> = r"\VarFileInfo\Translation"
        .encode_utf16()
        .chain(Some(0))
        .collect();
    // SAFETY: `block` is the buffer `GetFileVersionInfoW` filled; the
    // returned pointer addresses memory inside it, read before it drops.
    let ok = unsafe {
        VerQueryValueW(
            block.as_ptr().cast(),
            sub.as_ptr(),
            &raw mut pointer,
            &raw mut len,
        )
    };
    if ok == 0 || pointer.is_null() {
        return Vec::new();
    }

    let pairs = len as usize / 4;
    // SAFETY: the translation table is `len` bytes of (WORD lang, WORD cp)
    // pairs inside `block`, and `block` is u32-aligned.
    let words = unsafe { std::slice::from_raw_parts(pointer.cast::<u16>(), pairs * 2) };
    words
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| format!("{:04X}{:04X}", pair[0], pair[1]))
        .collect()
}

fn query_string(block: &[u32], sub_block: &str) -> Option<String> {
    let mut pointer: *mut core::ffi::c_void = std::ptr::null_mut();
    let mut len = 0_u32;
    let sub: Vec<u16> = sub_block.encode_utf16().chain(Some(0)).collect();
    // SAFETY: as in `translations`.
    let ok = unsafe {
        VerQueryValueW(
            block.as_ptr().cast(),
            sub.as_ptr(),
            &raw mut pointer,
            &raw mut len,
        )
    };
    if ok == 0 || pointer.is_null() || len == 0 {
        return None;
    }
    // SAFETY: for a string value `len` is its length in characters,
    // including the terminator, inside `block`.
    let units = unsafe { std::slice::from_raw_parts(pointer.cast::<u16>(), len as usize) };
    let text = String::from_utf16_lossy(units);
    let text = text.trim_end_matches('\0').trim();
    if text.is_empty() {
        None
    } else {
        Some(text.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_windows_binary_declares_microsoft_and_a_missing_file_declares_nothing() {
        let windir = std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".to_owned());
        let notepad = Path::new(&windir).join(r"System32\notepad.exe");
        let company = company_name(&notepad);
        assert!(
            is_microsoft(company.as_deref()),
            "notepad.exe should declare Microsoft, got {company:?}"
        );
        assert_eq!(
            company_name(Path::new(r"C:\definitely\not\here.exe")),
            None,
            "an absent file is unknown, not a company"
        );
    }

    #[test]
    fn notepad_describes_itself_and_a_missing_file_has_no_description() {
        let windir = std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".to_owned());
        let notepad = Path::new(&windir).join(r"System32\notepad.exe");
        let description = file_description(&notepad);
        assert!(
            description
                .as_deref()
                .is_some_and(|d| d.to_ascii_lowercase().contains("notepad")),
            "notepad.exe should describe itself as Notepad, got {description:?}"
        );
        assert_eq!(
            file_description(Path::new(r"C:\definitely\not\here.exe")),
            None
        );
    }

    #[test]
    fn microsoft_is_matched_in_every_spelling_and_unknown_is_not_microsoft() {
        assert!(is_microsoft(Some("Microsoft Corporation")));
        assert!(is_microsoft(Some("© MICROSOFT Corp.")));
        assert!(!is_microsoft(Some("Valve Corporation")));
        assert!(
            !is_microsoft(None),
            "unknown must stay visible — hiding it would hide an unread row"
        );
    }

    #[test]
    fn a_vendor_dll_run_through_rundll32_is_not_taken_for_microsoft() {
        let mut cache = CompanyCache::default();
        let windir = std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".to_owned());
        let rundll = format!(r"{windir}\System32\rundll32.exe");
        assert!(
            company_name(Path::new(&rundll)).is_some(),
            "the premise: rundll32 itself declares a company"
        );
        assert_eq!(
            cache.get(&rundll),
            None,
            "a launcher's company says nothing about what it launches"
        );
    }
}
