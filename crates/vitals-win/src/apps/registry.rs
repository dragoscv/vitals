//! A minimal, RAII-safe registry reader for the `Uninstall` hives.
//!
//! The advapi32 entry points are declared here rather than imported because
//! `windows-sys` is enabled in this crate without its `Win32_System_Registry`
//! feature, and the four functions below are a smaller and more auditable
//! surface than a whole additional feature module. This mirrors what
//! `network::adapters` does for `iphlpapi`.

use vitals_core::error::{Error, Result};

/// `HKEY`, an opaque handle.
///
/// Declared as `isize` rather than a pointer to match the signature
/// `startup::registry` uses for the same advapi32 imports. Two extern blocks
/// in one crate declaring the same symbol with different types is a
/// `clashing_extern_declarations` warning, and the ABI is identical either
/// way.
type Hkey = isize;

/// Predefined hive handles. These are sentinel values, never real pointers,
/// and must not be closed.
const HKEY_CURRENT_USER: Hkey = 0x8000_0001_u32.cast_signed() as Hkey;
const HKEY_LOCAL_MACHINE: Hkey = 0x8000_0002_u32.cast_signed() as Hkey;

const ERROR_SUCCESS: i32 = 0;
const ERROR_FILE_NOT_FOUND: i32 = 2;
const ERROR_ACCESS_DENIED: i32 = 5;
const ERROR_MORE_DATA: i32 = 234;

const KEY_READ: u32 = 0x0002_0019;
/// Forces the 64-bit view even from a 32-bit process.
const KEY_WOW64_64KEY: u32 = 0x0100;
/// Forces the 32-bit (`WOW6432Node`) view.
///
/// Without this, a 64-bit process reading
/// `Software\Microsoft\Windows\CurrentVersion\Uninstall` sees only 64-bit
/// products. Spelling `WOW6432Node` into the path by hand works for HKLM but
/// is not the documented mechanism and does not compose with registry
/// redirection on every hive, so the flag is used instead.
const KEY_WOW64_32KEY: u32 = 0x0200;

const REG_SZ: u32 = 1;
const REG_EXPAND_SZ: u32 = 2;
const REG_DWORD: u32 = 4;

/// `FILETIME` — 100-nanosecond intervals since 1601, split across two 32-bit
/// halves.
///
/// Required only as an out-parameter for `RegEnumKeyExW`; the key's last-write
/// time is not an install date and is not exposed as one.
/// Aligned to 8 because it is passed to `RegEnumKeyExW` through a `*mut u64`
/// (the signature the sibling `startup::registry` module declares). A
/// 4-aligned struct behind a `u64` pointer would be an under-aligned access.
#[repr(C, align(8))]
#[derive(Default, Clone, Copy)]
struct FileTime {
    low: u32,
    high: u32,
}

// The registry API lives in advapi32.dll, which is not in the default link
// set for this crate.
#[link(name = "advapi32")]
unsafe extern "system" {
    fn RegOpenKeyExW(
        key: Hkey,
        sub_key: *const u16,
        options: u32,
        desired: u32,
        result: *mut Hkey,
    ) -> i32;

    fn RegCloseKey(key: Hkey) -> i32;

    fn RegEnumKeyExW(
        key: Hkey,
        index: u32,
        name: *mut u16,
        name_len: *mut u32,
        reserved: *mut u32,
        class: *mut u16,
        class_len: *mut u32,
        last_write: *mut u64,
    ) -> i32;

    fn RegQueryValueExW(
        key: Hkey,
        value_name: *const u16,
        reserved: *mut u32,
        value_type: *mut u32,
        data: *mut u8,
        data_len: *mut u32,
    ) -> i32;
}

/// Which hive to open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Hive {
    LocalMachine,
    CurrentUser,
}

impl Hive {
    const fn handle(self) -> Hkey {
        match self {
            Self::LocalMachine => HKEY_LOCAL_MACHINE,
            Self::CurrentUser => HKEY_CURRENT_USER,
        }
    }
}

/// An owned registry key that closes itself on drop.
///
/// Registry handles are a process-wide finite resource and a leak here would
/// accumulate on every sample tick, so ownership is expressed in the type
/// rather than left to a `RegCloseKey` at the end of a function that has
/// three early returns.
#[derive(Debug)]
pub(crate) struct RegKey {
    handle: Hkey,
}

// The handle is not tied to the opening thread; the registry API is
// thread-safe for reads.
unsafe impl Send for RegKey {}

impl Drop for RegKey {
    fn drop(&mut self) {
        // SAFETY: `handle` came from a successful `RegOpenKeyExW` and is
        // closed exactly once, because `RegKey` is neither `Copy` nor
        // `Clone`. Predefined hive handles never reach here — they are used
        // only as the parent argument to `open`.
        unsafe {
            RegCloseKey(self.handle);
        }
    }
}

impl RegKey {
    /// Opens a subkey for reading in the requested architectural view.
    ///
    /// # Errors
    ///
    /// Returns [`Error::NotFound`] when the key does not exist — normal for
    /// the `WOW6432Node` path on an ARM or 32-bit-only system — and
    /// [`Error::AccessDenied`] when the current token cannot read it.
    pub(crate) fn open(hive: Hive, path: &str, wow64_32bit: bool) -> Result<Self> {
        let wide: Vec<u16> = path.encode_utf16().chain(Some(0)).collect();
        let view = if wow64_32bit {
            KEY_WOW64_32KEY
        } else {
            KEY_WOW64_64KEY
        };

        let mut handle: Hkey = 0;

        // SAFETY: `wide` is NUL-terminated and outlives the call; `handle` is
        // a valid out-pointer. The predefined hive handle is a documented
        // sentinel accepted as the parent key.
        let status = unsafe {
            RegOpenKeyExW(
                hive.handle(),
                wide.as_ptr(),
                0,
                KEY_READ | view,
                &raw mut handle,
            )
        };

        match status {
            ERROR_SUCCESS => Ok(Self { handle }),
            ERROR_FILE_NOT_FOUND => Err(Error::NotFound(path.to_owned())),
            ERROR_ACCESS_DENIED => Err(Error::AccessDenied {
                operation: format!("read registry key {path}"),
            }),
            code => Err(Error::Os {
                context: format!("RegOpenKeyExW({path})"),
                code,
            }),
        }
    }

    /// Opens a subkey of this key, inheriting the parent's architectural view.
    ///
    /// # Errors
    ///
    /// As [`RegKey::open`]. A subkey can vanish between being enumerated and
    /// being opened if an uninstall completes mid-scan; that surfaces as
    /// [`Error::NotFound`].
    pub(crate) fn open_child(&self, name: &str) -> Result<Self> {
        let wide: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
        let mut handle: Hkey = 0;

        // SAFETY: `self.handle` is a live key for the lifetime of `&self`;
        // `wide` is NUL-terminated; `handle` is a valid out-pointer. The
        // WOW64 view is inherited from the parent handle, so no flag is
        // passed here.
        let status =
            unsafe { RegOpenKeyExW(self.handle, wide.as_ptr(), 0, KEY_READ, &raw mut handle) };

        match status {
            ERROR_SUCCESS => Ok(Self { handle }),
            ERROR_FILE_NOT_FOUND => Err(Error::NotFound(name.to_owned())),
            ERROR_ACCESS_DENIED => Err(Error::AccessDenied {
                operation: format!("read registry subkey {name}"),
            }),
            code => Err(Error::Os {
                context: format!("RegOpenKeyExW(child {name})"),
                code,
            }),
        }
    }

    /// Lists the names of this key's immediate subkeys.
    ///
    /// A subkey that cannot be read is skipped rather than aborting: one
    /// unreadable product must not hide every other installed application.
    pub(crate) fn child_names(&self) -> Vec<String> {
        // The documented maximum registry key name is 255 characters; the
        // buffer is sized once and reused for every iteration rather than
        // running the usual two-call size protocol 600 times.
        const MAX_KEY_NAME: usize = 256;

        let mut names = Vec::new();
        let mut buffer = [0_u16; MAX_KEY_NAME];

        for index in 0.. {
            let mut len = u32::try_from(buffer.len()).unwrap_or(0);
            let mut last_write = FileTime::default();

            // SAFETY: `buffer` has `MAX_KEY_NAME` elements and `len` is set
            // to exactly that count before every call, as the API requires
            // (it overwrites `len` with the characters actually written).
            // `last_write` is a 8-byte, 4-byte-aligned FILETIME, which is
            // exactly what the kernel writes through the `*mut u64` this
            // signature declares; the size is pinned by a test below.
            let status = unsafe {
                RegEnumKeyExW(
                    self.handle,
                    index,
                    buffer.as_mut_ptr(),
                    &raw mut len,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    (&raw mut last_write).cast::<u64>(),
                )
            };

            match status {
                ERROR_SUCCESS => {
                    let end = usize::try_from(len).unwrap_or(0).min(buffer.len());
                    names.push(String::from_utf16_lossy(&buffer[..end]));
                }
                // A name longer than the documented maximum, or a transient
                // failure on one subkey. Skip it and keep going.
                ERROR_MORE_DATA => {}
                _ => break,
            }
        }

        names
    }

    /// Reads a `REG_SZ` or `REG_EXPAND_SZ` value.
    ///
    /// Returns `None` when the value is absent, empty, or holds a type other
    /// than a string. Environment variables in a `REG_EXPAND_SZ` are left
    /// unexpanded: expansion happens in the security context of whoever runs
    /// the command, and pre-expanding here would bake this process's
    /// environment into a command line meant for another.
    pub(crate) fn string_value(&self, name: &str) -> Option<String> {
        let (kind, bytes) = self.raw_value(name)?;
        if kind != REG_SZ && kind != REG_EXPAND_SZ {
            return None;
        }

        // The registry stores strings as UTF-16 and does not guarantee an
        // even byte count for a corrupt value, so the odd trailing byte is
        // dropped rather than being read past the end of the buffer.
        let units: Vec<u16> = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect();

        // A stored NUL terminator is included in the returned length; keeping
        // it would put an invisible character at the end of every string and
        // break equality comparisons in ways that are painful to debug.
        let end = units.iter().position(|&c| c == 0).unwrap_or(units.len());
        let text = String::from_utf16_lossy(&units[..end]);
        let trimmed = text.trim();

        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_owned())
        }
    }

    /// Reads a `REG_DWORD` value.
    ///
    /// Returns `None` when absent or when the stored type is not a DWORD.
    pub(crate) fn dword_value(&self, name: &str) -> Option<u32> {
        let (kind, bytes) = self.raw_value(name)?;
        if kind != REG_DWORD || bytes.len() < 4 {
            return None;
        }
        Some(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    /// Runs the two-call protocol: ask for the size, then read into a buffer
    /// of exactly that size.
    ///
    /// A single call with a guessed buffer is what makes registry readers
    /// truncate long `UninstallString` command lines, which then fail with a
    /// mismatched-quote error rather than an obvious one.
    fn raw_value(&self, name: &str) -> Option<(u32, Vec<u8>)> {
        let wide: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();

        let mut kind: u32 = 0;
        let mut size: u32 = 0;

        // SAFETY: a null data pointer with a live length out-pointer is the
        // documented way to request the required size without writing any
        // data.
        let status = unsafe {
            RegQueryValueExW(
                self.handle,
                wide.as_ptr(),
                std::ptr::null_mut(),
                &raw mut kind,
                std::ptr::null_mut(),
                &raw mut size,
            )
        };

        if status != ERROR_SUCCESS || size == 0 {
            return None;
        }

        let mut buffer = vec![0_u8; size as usize];
        let mut capacity = size;

        // SAFETY: `buffer` holds `capacity` bytes and `capacity` is passed as
        // the buffer length, so the API cannot write past the end. The value
        // could have grown between the two calls, in which case this returns
        // ERROR_MORE_DATA and is treated as a read failure rather than a
        // truncated success.
        let status = unsafe {
            RegQueryValueExW(
                self.handle,
                wide.as_ptr(),
                std::ptr::null_mut(),
                &raw mut kind,
                buffer.as_mut_ptr(),
                &raw mut capacity,
            )
        };

        if status != ERROR_SUCCESS {
            return None;
        }

        buffer.truncate(capacity as usize);
        Some((kind, buffer))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // A wrong struct size makes the kernel write past the field it was given
    // and the corruption shows up as plausible-but-wrong data elsewhere,
    // which is silent and expensive. Pin the layout.
    #[test]
    fn filetime_layout_is_pinned() {
        assert_eq!(
            size_of::<FileTime>(),
            8,
            "FILETIME is two u32 halves; a different size means RegEnumKeyExW \
             writes outside the struct"
        );
        assert_eq!(
            align_of::<FileTime>(),
            8,
            "FILETIME is handed to RegEnumKeyExW through a *mut u64, so anything \
             less than 8-byte alignment is an under-aligned write"
        );
    }

    #[test]
    fn predefined_hive_handles_match_the_documented_sentinels() {
        assert_eq!(
            HKEY_LOCAL_MACHINE as u32, 0x8000_0002,
            "a wrong hive sentinel silently reads the wrong hive"
        );
        assert_eq!(HKEY_CURRENT_USER as u32, 0x8000_0001);
    }

    #[test]
    fn the_two_wow64_view_flags_are_distinct() {
        assert_ne!(
            KEY_WOW64_32KEY, KEY_WOW64_64KEY,
            "identical view flags would make the 32-bit scan a duplicate of the 64-bit one"
        );
    }

    #[test]
    fn opening_a_nonexistent_key_reports_not_found_not_a_generic_failure() {
        let err = RegKey::open(
            Hive::LocalMachine,
            r"Software\Vitals\NoSuchKeyShouldEverExist",
            false,
        )
        .expect_err("this key must not exist");

        assert!(
            matches!(err, Error::NotFound(_)),
            "a missing key must be NotFound so callers can skip it silently; got {err:?}"
        );
    }

    #[test]
    fn both_registry_views_of_the_uninstall_path_are_readable() {
        // The 64-bit view exists on every Windows machine. The 32-bit view
        // exists on x64 and ARM64; its absence is a legitimate NotFound, but
        // an access failure is not.
        let native = RegKey::open(Hive::LocalMachine, super::super::UNINSTALL_PATH, false);
        assert!(
            native.is_ok(),
            "the machine-wide Uninstall key must be readable: {native:?}"
        );

        match RegKey::open(Hive::LocalMachine, super::super::UNINSTALL_PATH, true) {
            Ok(_) | Err(Error::NotFound(_)) => {}
            other => panic!("the 32-bit view must open or be absent, not fail: {other:?}"),
        }
    }

    #[test]
    fn the_two_views_do_not_return_an_identical_subkey_set() {
        // If the WOW64 flag were ignored, both scans would enumerate the same
        // key and every 32-bit application would be missing while the counts
        // still looked reasonable. On a machine with no 32-bit products the
        // 32-bit view is empty, which is also distinguishable.
        let Ok(native) = RegKey::open(Hive::LocalMachine, super::super::UNINSTALL_PATH, false)
        else {
            return;
        };
        let Ok(wow) = RegKey::open(Hive::LocalMachine, super::super::UNINSTALL_PATH, true) else {
            return;
        };

        let native_names = native.child_names();
        let wow_names = wow.child_names();

        assert!(
            !native_names.is_empty(),
            "a running Windows machine has installed products in HKLM"
        );
        assert_ne!(
            native_names, wow_names,
            "the 32-bit and 64-bit views returned identical subkeys, which means \
             KEY_WOW64_32KEY was ignored and 32-bit applications are invisible"
        );
    }

    #[test]
    fn a_missing_value_is_none_rather_than_an_empty_string() {
        let key = RegKey::open(Hive::LocalMachine, super::super::UNINSTALL_PATH, false)
            .expect("the Uninstall key is readable");

        assert_eq!(
            key.string_value("VitalsNoSuchValueName"),
            None,
            "an absent value must be None; an empty string would render as a real blank field"
        );
        assert_eq!(key.dword_value("VitalsNoSuchValueName"), None);
    }

    #[test]
    fn a_string_value_is_not_returned_as_a_dword_or_vice_versa() {
        let key = RegKey::open(
            Hive::LocalMachine,
            r"Software\Microsoft\Windows NT\CurrentVersion",
            false,
        )
        .expect("the Windows NT CurrentVersion key is readable on every install");

        assert!(
            key.string_value("ProductName").is_some(),
            "ProductName is a REG_SZ present on every Windows install"
        );
        assert_eq!(
            key.dword_value("ProductName"),
            None,
            "reading a REG_SZ as a DWORD must fail rather than reinterpret its bytes"
        );
    }
}
