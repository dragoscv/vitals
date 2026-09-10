//! A minimal RAII wrapper over the Win32 registry.
//!
//! ## Why the API is declared here
//!
//! `advapi32`'s registry surface is not in this crate's `windows-sys` feature
//! set, and the eight functions below are a fixed, documented ABI that has
//! not changed since NT 3.1. The same approach is already taken for
//! `iphlpapi` in [`crate::network`] and `gdi32` in [`crate::gpu`].
//!
//! `HKEY` is spelled `isize` here to match `crate::apps::registry`, which
//! declares the same advapi32 imports. Two extern blocks in one crate
//! declaring one symbol with different types is a
//! `clashing_extern_declarations` warning — correctly, since they are the
//! same function — and the ABI is identical either way.

use std::ffi::c_void;

use vitals_core::error::{Error, Result};

/// An open registry key. Opaque handle type.
type HKey = isize;

/// `HKEY_CURRENT_USER`. A sentinel value, never a real pointer.
const HKEY_CURRENT_USER_RAW: HKey = -2_147_483_647;
/// `HKEY_LOCAL_MACHINE`.
///
/// Written through `i32` so the value sign-extends on 64-bit, exactly as the
/// SDK macro does. Writing `0x8000_0002_isize` directly gives a positive
/// number the kernel rejects with `ERROR_INVALID_HANDLE`, which surfaces as
/// "the key does not exist".
const HKEY_LOCAL_MACHINE_RAW: HKey = -2_147_483_646;

/// Which predefined hive to open under.
///
/// An enum rather than the raw `HKEY` sentinel so that no safe public
/// function takes a raw pointer, and so that a caller cannot pass a handle
/// this module does not own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hive {
    /// `HKEY_LOCAL_MACHINE` — machine-wide settings.
    LocalMachine,
    /// `HKEY_CURRENT_USER` — the signed-in user's settings.
    CurrentUser,
}

impl Hive {
    const fn handle(self) -> HKey {
        match self {
            Self::LocalMachine => HKEY_LOCAL_MACHINE_RAW,
            Self::CurrentUser => HKEY_CURRENT_USER_RAW,
        }
    }
}

/// `KEY_READ` — query value, enumerate sub-keys, notify.
pub const KEY_READ: u32 = 0x0002_0019;

/// Force the 64-bit view of a redirected key.
///
/// Without this, a 32-bit build reading `HKLM\Software\...\Run` is silently
/// redirected to `WOW6432Node` and reports the wrong list. Being explicit in
/// both directions makes the process bitness irrelevant.
pub const KEY_WOW64_64KEY: u32 = 0x0100;

/// Force the 32-bit (`WOW6432Node`) view.
pub const KEY_WOW64_32KEY: u32 = 0x0200;

/// `REG_SZ`.
pub const REG_SZ: u32 = 1;
/// `REG_EXPAND_SZ` — contains `%VAR%` references still to be expanded.
pub const REG_EXPAND_SZ: u32 = 2;
/// `REG_BINARY` — how `StartupApproved` stores its 12-byte state blobs.
pub const REG_BINARY: u32 = 3;

const ERROR_SUCCESS: i32 = 0;
const ERROR_FILE_NOT_FOUND: i32 = 2;
const ERROR_ACCESS_DENIED: i32 = 5;
const ERROR_MORE_DATA: i32 = 234;
const ERROR_NO_MORE_ITEMS: i32 = 259;

// The registry entry points, `W` variants only.
#[link(name = "advapi32")]
unsafe extern "system" {
    fn RegOpenKeyExW(
        key: HKey,
        sub_key: *const u16,
        options: u32,
        desired: u32,
        out: *mut HKey,
    ) -> i32;
    fn RegCloseKey(key: HKey) -> i32;
    fn RegEnumValueW(
        key: HKey,
        index: u32,
        name: *mut u16,
        name_len: *mut u32,
        reserved: *mut u32,
        value_type: *mut u32,
        data: *mut u8,
        data_len: *mut u32,
    ) -> i32;
    fn RegQueryValueExW(
        key: HKey,
        name: *const u16,
        reserved: *mut u32,
        value_type: *mut u32,
        data: *mut u8,
        data_len: *mut u32,
    ) -> i32;
    fn RegEnumKeyExW(
        key: HKey,
        index: u32,
        name: *mut u16,
        name_len: *mut u32,
        reserved: *mut u32,
        class: *mut u16,
        class_len: *mut u32,
        last_write: *mut u64,
    ) -> i32;
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn ExpandEnvironmentStringsW(src: *const u16, dst: *mut u16, size: u32) -> u32;
}

// `CoTaskMemFree` and `SHGetKnownFolderPath` resolve the Start Menu folders.
// Hard-coding `%AppData%\Microsoft\Windows\...` breaks under folder
// redirection, which is the default on any domain-joined machine.
#[link(name = "shell32")]
unsafe extern "system" {
    fn SHGetKnownFolderPath(
        folder: *const [u8; 16],
        flags: u32,
        token: *mut c_void,
        path: *mut *mut u16,
    ) -> i32;
}

#[link(name = "ole32")]
unsafe extern "system" {
    fn CoTaskMemFree(block: *mut c_void);
}

/// `FOLDERID_Startup` — the per-user Start Menu startup folder.
///
/// `{B97D20BB-F46A-4C97-BA10-5E3608430854}` in little-endian GUID byte order.
pub const FOLDERID_STARTUP: [u8; 16] = [
    0xBB, 0x20, 0x7D, 0xB9, 0x6A, 0xF4, 0x97, 0x4C, 0xBA, 0x10, 0x5E, 0x36, 0x08, 0x43, 0x08, 0x54,
];

/// `FOLDERID_CommonStartup` — the all-users startup folder.
///
/// `{82A5EA35-D9CD-47C5-9629-E15D2F714E6E}`.
pub const FOLDERID_COMMON_STARTUP: [u8; 16] = [
    0x35, 0xEA, 0xA5, 0x82, 0xCD, 0xD9, 0xC5, 0x47, 0x96, 0x29, 0xE1, 0x5D, 0x2F, 0x71, 0x4E, 0x6E,
];

/// An open key that closes itself.
///
/// Startup enumeration opens fourteen keys per sweep and the sweep runs
/// whenever the tab is shown. A leaked `HKEY` is a kernel object that
/// survives until process exit, so this is not a theoretical tidiness point.
#[derive(Debug)]
pub struct RegKey(HKey);

impl RegKey {
    /// Opens a sub-key for reading.
    ///
    /// `view` is [`KEY_WOW64_64KEY`], [`KEY_WOW64_32KEY`], or `0` to accept
    /// whatever the process bitness implies — which callers should not do.
    ///
    /// # Errors
    ///
    /// [`Error::NotFound`] when the key does not exist, which is routine:
    /// `RunOnce` is absent on a machine that has never used it.
    /// [`Error::AccessDenied`] when the key exists but is not readable.
    pub fn open(hive: Hive, path: &str, view: u32) -> Result<Self> {
        let wide: Vec<u16> = path.encode_utf16().chain(Some(0)).collect();
        let mut handle: HKey = 0;

        // SAFETY: `wide` is a NUL-terminated UTF-16 string alive for the
        // call; `handle` is a valid out-parameter. `root` is a predefined
        // key constant or a handle this type owns.
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
            ERROR_SUCCESS => Ok(Self(handle)),
            ERROR_FILE_NOT_FOUND => Err(Error::NotFound(path.to_owned())),
            ERROR_ACCESS_DENIED => Err(Error::AccessDenied {
                operation: format!("open registry key {path}"),
            }),
            code => Err(Error::Os {
                context: format!("RegOpenKeyExW({path})"),
                code,
            }),
        }
    }

    /// Enumerates every value as `(name, type, data)`.
    ///
    /// Values that cannot be read individually are skipped rather than
    /// aborting: one unreadable entry must not hide the other twenty.
    ///
    /// # Errors
    ///
    /// Never returns an error today; the signature is fallible so a future
    /// caller is not forced to change when it becomes so. Enumeration stops
    /// at the first non-recoverable status.
    #[must_use]
    pub fn values(&self) -> Vec<(String, u32, Vec<u8>)> {
        let mut out = Vec::new();

        // Value names are capped at 16383 characters by the registry itself,
        // so this cannot truncate a legal name. Heap-allocated and reused
        // across iterations: 32 KiB is far too large for the stack, and
        // re-allocating it per value would dominate the cost of the scan.
        let mut name = vec![0_u16; 16_384];

        for index in 0.. {
            let mut name_len = u32::try_from(name.len()).unwrap_or(0);
            let mut value_type: u32 = 0;
            let mut data_len: u32 = 0;

            // Two-call protocol: the first asks only for the data size. The
            // name buffer must still be supplied, and `name_len` is
            // overwritten with the actual length in characters, excluding
            // the terminator — so it has to be reset before the second call.
            //
            // SAFETY: both buffers are sized as declared; passing null for
            // `data` with a live `data_len` is the documented size query.
            let status = unsafe {
                RegEnumValueW(
                    self.0,
                    index,
                    name.as_mut_ptr(),
                    &raw mut name_len,
                    std::ptr::null_mut(),
                    &raw mut value_type,
                    std::ptr::null_mut(),
                    &raw mut data_len,
                )
            };

            if status == ERROR_NO_MORE_ITEMS {
                break;
            }
            if status != ERROR_SUCCESS && status != ERROR_MORE_DATA {
                break;
            }

            let mut data = vec![0_u8; data_len as usize];
            let mut name_len2 = u32::try_from(name.len()).unwrap_or(0);
            let mut data_len2 = data_len;

            // SAFETY: `data` has `data_len` bytes and we pass that length;
            // `name` is reset to its full capacity because the previous call
            // shrank `name_len` to the string length.
            let status = unsafe {
                RegEnumValueW(
                    self.0,
                    index,
                    name.as_mut_ptr(),
                    &raw mut name_len2,
                    std::ptr::null_mut(),
                    &raw mut value_type,
                    data.as_mut_ptr(),
                    &raw mut data_len2,
                )
            };

            if status != ERROR_SUCCESS {
                continue;
            }

            data.truncate(data_len2 as usize);
            let name = String::from_utf16_lossy(&name[..name_len2 as usize]);
            out.push((name, value_type, data));
        }

        out
    }

    /// Reads one value by name.
    ///
    /// Returns `None` when the value is absent, which for `StartupApproved`
    /// is the common case and means "never toggled", not "off".
    #[must_use]
    pub fn value(&self, name: &str) -> Option<(u32, Vec<u8>)> {
        let wide: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
        let mut value_type: u32 = 0;
        let mut len: u32 = 0;

        // SAFETY: size query with a null data pointer; `wide` is
        // NUL-terminated and outlives the call.
        let status = unsafe {
            RegQueryValueExW(
                self.0,
                wide.as_ptr(),
                std::ptr::null_mut(),
                &raw mut value_type,
                std::ptr::null_mut(),
                &raw mut len,
            )
        };

        if status != ERROR_SUCCESS && status != ERROR_MORE_DATA {
            return None;
        }

        let mut data = vec![0_u8; len as usize];
        let mut len2 = len;

        // SAFETY: `data` has exactly `len` bytes and `len2` says so.
        let status = unsafe {
            RegQueryValueExW(
                self.0,
                wide.as_ptr(),
                std::ptr::null_mut(),
                &raw mut value_type,
                data.as_mut_ptr(),
                &raw mut len2,
            )
        };

        if status != ERROR_SUCCESS {
            return None;
        }

        data.truncate(len2 as usize);
        Some((value_type, data))
    }
}

impl RegKey {
    /// Enumerates the names of immediate sub-keys.
    ///
    /// Used to walk `TaskCache\Tree`, whose shape mirrors the scheduled-task
    /// folder hierarchy. Returns an empty vector when the key has no
    /// children or cannot be enumerated; the two are indistinguishable here
    /// and neither is an error worth propagating up a tree walk.
    #[must_use]
    pub fn sub_keys(&self) -> Vec<String> {
        let mut out = Vec::new();

        for index in 0.. {
            // Key names are capped at 255 characters by the registry, so a
            // fixed buffer cannot truncate a legal name.
            let mut name = [0_u16; 256];
            let mut len = u32::try_from(name.len()).unwrap_or(0);

            // SAFETY: `name` holds `len` u16s as declared; the remaining
            // out-parameters are optional and passed as null.
            let status = unsafe {
                RegEnumKeyExW(
                    self.0,
                    index,
                    name.as_mut_ptr(),
                    &raw mut len,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                )
            };

            if status != ERROR_SUCCESS {
                break;
            }

            out.push(String::from_utf16_lossy(&name[..len as usize]));
        }

        out
    }
}

impl Drop for RegKey {
    fn drop(&mut self) {
        // SAFETY: `self.0` came from a successful RegOpenKeyExW and is
        // closed exactly once, since `RegKey` is neither Copy nor Clone.
        unsafe { RegCloseKey(self.0) };
    }
}

/// Decodes `REG_SZ`/`REG_EXPAND_SZ` bytes into a string.
///
/// Returns `None` for a non-string type or an odd byte count — a truncated
/// UTF-16 buffer is corrupt, and half-decoding it would put mojibake in the
/// command column.
#[must_use]
pub fn decode_string(value_type: u32, data: &[u8]) -> Option<String> {
    if value_type != REG_SZ && value_type != REG_EXPAND_SZ {
        return None;
    }
    if !data.len().is_multiple_of(2) {
        return None;
    }

    let units: Vec<u16> = data
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .take_while(|&unit| unit != 0)
        .collect();

    let text = String::from_utf16_lossy(&units);

    if value_type == REG_EXPAND_SZ {
        Some(expand_environment(&text))
    } else {
        Some(text)
    }
}

/// Expands `%VAR%` references, returning the input unchanged on failure.
///
/// Leaving the raw `%ProgramFiles%` in place is honest — it is what the
/// registry says — and far better than blanking a command we could not
/// expand.
#[must_use]
pub fn expand_environment(text: &str) -> String {
    if !text.contains('%') {
        return text.to_owned();
    }

    let wide: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();

    // Two-call protocol: the size includes the terminator, and is returned
    // in *characters*, not bytes.
    // SAFETY: null destination with size 0 is the documented size query.
    let needed = unsafe { ExpandEnvironmentStringsW(wide.as_ptr(), std::ptr::null_mut(), 0) };
    if needed == 0 {
        return text.to_owned();
    }

    let mut buffer = vec![0_u16; needed as usize];

    // SAFETY: `buffer` holds `needed` u16s and we pass that count.
    let written = unsafe { ExpandEnvironmentStringsW(wide.as_ptr(), buffer.as_mut_ptr(), needed) };

    if written == 0 || written > needed {
        return text.to_owned();
    }

    let end = buffer
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(buffer.len());
    String::from_utf16_lossy(&buffer[..end])
}

/// Resolves a known folder to a path.
///
/// Used instead of composing `%AppData%\Microsoft\Windows\Start Menu\...`
/// because folder redirection — the default on domain-joined machines —
/// moves the Start Menu somewhere the composed path never points.
///
/// Takes the GUID by value rather than by reference so the signature does
/// not expose a raw pointer from a safe function.
#[must_use]
pub fn known_folder(id: [u8; 16]) -> Option<std::path::PathBuf> {
    let mut raw: *mut u16 = std::ptr::null_mut();

    // SAFETY: `id` is a 16-byte GUID as the API requires and lives for the
    // duration of the call; `raw` is a valid out-parameter. A null token
    // means "the calling user".
    let status = unsafe {
        SHGetKnownFolderPath(
            std::ptr::from_ref(&id),
            0,
            std::ptr::null_mut(),
            &raw mut raw,
        )
    };

    if status < 0 || raw.is_null() {
        return None;
    }

    // SAFETY: on success the shell wrote a NUL-terminated string we own.
    let len = unsafe {
        let mut n = 0;
        while *raw.add(n) != 0 {
            n += 1;
        }
        n
    };

    // SAFETY: `raw` points to `len` initialised u16s.
    let slice = unsafe { std::slice::from_raw_parts(raw, len) };
    let path = std::path::PathBuf::from(String::from_utf16_lossy(slice));

    // SAFETY: the buffer was allocated by the shell with CoTaskMemAlloc and
    // is freed exactly once, after the copy above.
    unsafe { CoTaskMemFree(raw.cast()) };

    Some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn predefined_key_constants_match_the_sdk() {
        // These are sentinel values, not real pointers, and must be passed
        // through unchanged. Any sign-extension or truncation en route
        // yields ERROR_INVALID_HANDLE, which reads as "the key is missing".
        // Sign-extension trap: HKEY_LOCAL_MACHINE is 0x80000002 as a u32 and
        // must reach the kernel as 0xFFFFFFFF80000002 on 64-bit. A plain
        // `0x8000_0002_isize` is positive and is rejected with
        // ERROR_INVALID_HANDLE, which reads as "the key does not exist".
        assert_eq!(Hive::LocalMachine.handle(), -2_147_483_646_isize);
        assert_eq!(Hive::CurrentUser.handle(), -2_147_483_647_isize);
    }

    #[test]
    fn wow64_view_flags_are_distinct_bits() {
        assert_ne!(KEY_WOW64_32KEY, KEY_WOW64_64KEY);
        assert_eq!(
            KEY_WOW64_32KEY & KEY_WOW64_64KEY,
            0,
            "the two views must be selectable independently"
        );
    }

    #[test]
    fn decode_rejects_non_string_types() {
        let bytes = b"a\0b\0";
        assert_eq!(
            decode_string(REG_BINARY, bytes),
            None,
            "REG_BINARY decoded as text would render StartupApproved blobs \
             as garbage in the command column"
        );
        assert_eq!(decode_string(REG_SZ, bytes), Some("ab".to_owned()));
    }

    #[test]
    fn decode_rejects_odd_length_buffers() {
        assert_eq!(
            decode_string(REG_SZ, &[0x41]),
            None,
            "an odd byte count cannot be UTF-16; decoding it would silently \
             drop the trailing byte"
        );
    }

    #[test]
    fn decode_stops_at_the_terminator() {
        // The registry stores the NUL, and some writers append trailing
        // padding after it. Including either puts a U+0000 in the string.
        let bytes = b"h\0i\0\0\0X\0";
        assert_eq!(decode_string(REG_SZ, bytes), Some("hi".to_owned()));
    }

    #[test]
    fn expansion_leaves_unknown_variables_alone() {
        let input = "%VITALS_DEFINITELY_NOT_SET%\\app.exe";
        assert_eq!(
            expand_environment(input),
            input,
            "an unresolvable variable must stay visible, not be blanked"
        );
    }

    #[test]
    fn expansion_resolves_a_real_variable() {
        let expanded = expand_environment("%SystemRoot%\\explorer.exe");
        assert!(
            !expanded.contains('%'),
            "SystemRoot always exists, so expansion must have happened: got {expanded}"
        );
        assert!(expanded.to_lowercase().ends_with("explorer.exe"));
    }

    #[test]
    fn text_without_variables_is_untouched() {
        assert_eq!(expand_environment("C:\\a\\b.exe"), "C:\\a\\b.exe");
    }

    #[test]
    fn run_key_opens_in_both_registry_views() {
        // Both views must be openable on any 64-bit Windows. If the 32-bit
        // view were silently unavailable we would lose every WOW64 entry
        // and never notice, because an empty list looks like "nothing here".
        const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
        for view in [KEY_WOW64_64KEY, KEY_WOW64_32KEY] {
            assert!(
                RegKey::open(Hive::LocalMachine, RUN, view).is_ok(),
                "HKLM Run must open with view flag {view:#x}"
            );
        }
    }

    #[test]
    fn missing_key_is_not_found_rather_than_an_os_error() {
        let result = RegKey::open(Hive::CurrentUser, r"Software\VitalsNoSuchKey_9f3a", 0);
        assert!(
            matches!(result, Err(Error::NotFound(_))),
            "a missing key is routine and must map to NotFound so callers \
             can skip it quietly; got {result:?}"
        );
    }

    #[test]
    fn known_folders_resolve() {
        assert!(
            known_folder(FOLDERID_STARTUP).is_some(),
            "the per-user Startup folder exists on every Windows install"
        );
        assert!(known_folder(FOLDERID_COMMON_STARTUP).is_some());
    }
}
