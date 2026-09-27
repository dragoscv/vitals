//! Standing in for Task Manager on the taskbar menu and Ctrl+Shift+Esc.
//!
//! Windows offers no API for adding an entry to the taskbar's context menu.
//! The supported mechanism — the one Process Explorer's "Replace Task
//! Manager" has used for twenty years — is the **Image File Execution
//! Options** debugger hook:
//!
//! ```text
//! HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution Options\taskmgr.exe
//!   Debugger = "C:\Program Files\Vitals\vitals-desktop.exe"
//! ```
//!
//! Windows then launches the debugger *instead of* `taskmgr.exe`, passing
//! the original command line as arguments. Every route to Task Manager —
//! the taskbar menu, Ctrl+Shift+Esc, Ctrl+Alt+Del, `Win+X`, typing
//! "taskmgr" — goes through `CreateProcess` and so lands on us.
//!
//! Three things about this key make it dangerous, and each is guarded here:
//!
//! 1. **It is `HKLM`.** Writing needs elevation, so [`set_replacement`]
//!    re-launches Vitals elevated with an internal argument rather than
//!    failing, and then re-reads the key to prove the write happened.
//! 2. **It is shared.** Process Explorer, Process Hacker and System
//!    Informer all use the same value. Overwriting or deleting a debugger
//!    that is not ours would silently break another tool the user chose, so
//!    [`ReplacementStatus::ReplacedByOther`] is refused rather than
//!    clobbered.
//! 3. **It intercepts every launch, including ours.** Spawning
//!    `taskmgr.exe` normally while the hook is on would launch Vitals
//!    again, forever. [`launch_real_task_manager`] therefore starts it as a
//!    debuggee — the documented way to bypass the hook — and detaches
//!    immediately.
//! 4. **It intercepts Task Manager's own second launch.** `taskmgr.exe`
//!    always starts unelevated and immediately re-launches itself elevated
//!    (observed: `Taskmgr(a) → Taskmgr(b)`, then `a` exits). That second
//!    `CreateProcess` is *not* made by a debugger, so the hook catches it
//!    and the elevated child is Vitals. [`is_task_manager_elevation_hop`] lets
//!    the app recognise that case and hand back to the real program.

use std::path::Path;

use vitals_core::error::{Error, Result};
use windows_sys::Win32::Foundation::{
    CloseHandle, DBG_CONTINUE, ERROR_CANCELLED, GetLastError, HANDLE,
};
use windows_sys::Win32::System::Diagnostics::Debug::{
    CREATE_PROCESS_DEBUG_EVENT, ContinueDebugEvent, DEBUG_EVENT, DebugActiveProcessStop,
    DebugSetProcessKillOnExit, WaitForDebugEventEx,
};
use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_LOCAL_MACHINE, KEY_READ, KEY_SET_VALUE, KEY_WOW64_64KEY, REG_OPTION_NON_VOLATILE,
    REG_SZ, RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW,
    RegSetValueExW,
};
use windows_sys::Win32::System::Threading::{
    CreateProcessW, DEBUG_ONLY_THIS_PROCESS, GetExitCodeProcess, INFINITE, PROCESS_INFORMATION,
    STARTUPINFOW, WaitForSingleObject,
};
use windows_sys::Win32::UI::Shell::{
    SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;

/// The key Windows consults before launching `taskmgr.exe`.
const IFEO_TASKMGR: &str =
    r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution Options\taskmgr.exe";

/// The value that redirects the launch.
const DEBUGGER_VALUE: &str = "Debugger";

/// The argument the elevated instance is started with.
///
/// Public because `main`/`lib` must recognise it before Tauri builds, and a
/// second spelling of the same string is exactly how that breaks silently.
pub const SET_REPLACEMENT_ARG: &str = "--set-taskmgr-replacement";

const ERROR_SUCCESS: u32 = 0;
const ERROR_FILE_NOT_FOUND: u32 = 2;
const ERROR_ACCESS_DENIED: u32 = 5;

/// Who currently owns the `taskmgr.exe` debugger hook.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplacementStatus {
    /// No debugger is set: Ctrl+Shift+Esc opens the real Task Manager.
    NotReplaced,
    /// The hook points at this installation of Vitals.
    ReplacedByUs {
        /// The path exactly as stored, so the UI can show it.
        path: String,
    },
    /// Another tool — Process Explorer, System Informer — owns the hook.
    ///
    /// Reported rather than overwritten. Silently hijacking a replacement
    /// the user deliberately configured is the behaviour that makes people
    /// distrust this feature in the first place.
    ReplacedByOther {
        /// The debugger currently registered.
        debugger: String,
    },
}

impl ReplacementStatus {
    /// Whether Vitals is what Ctrl+Shift+Esc currently opens.
    #[must_use]
    pub const fn is_ours(&self) -> bool {
        matches!(self, Self::ReplacedByUs { .. })
    }
}

/// Reads the current state of the hook.
///
/// A missing key and a missing value are both [`ReplacementStatus::NotReplaced`]:
/// the key exists on many machines for unrelated reasons (`MitigationOptions`,
/// `GlobalFlag`), so its presence says nothing on its own.
///
/// # Errors
///
/// [`Error::Os`] when the key exists but cannot be read for a reason other
/// than it being absent. Reading IFEO does not need elevation, so a denial
/// here is a genuinely unusual machine and worth surfacing.
pub fn replacement_status() -> Result<ReplacementStatus> {
    let Some(key) = open_ifeo(KEY_READ)? else {
        return Ok(ReplacementStatus::NotReplaced);
    };

    let debugger = read_string_value(key.raw(), DEBUGGER_VALUE)?;

    let Some(debugger) = debugger else {
        return Ok(ReplacementStatus::NotReplaced);
    };

    let trimmed = unquote(&debugger);
    if trimmed.is_empty() {
        return Ok(ReplacementStatus::NotReplaced);
    }

    if is_vitals_executable(trimmed) {
        Ok(ReplacementStatus::ReplacedByUs {
            path: trimmed.to_owned(),
        })
    } else {
        Ok(ReplacementStatus::ReplacedByOther {
            debugger: trimmed.to_owned(),
        })
    }
}

/// Turns the replacement on or off, elevating if that is what it takes.
///
/// The write itself is one registry value; everything around it is the
/// consent and safety story:
///
/// - Another tool's hook is never touched — [`Error::Refused`], naming it.
/// - `HKLM` needs elevation, so a denial re-launches this executable with
///   [`SET_REPLACEMENT_ARG`] under the `runas` verb and waits for it.
/// - Declining the UAC prompt is [`Error::Refused`], not a failure: the user
///   answered the question, and the answer was no.
/// - The key is re-read afterwards, because an elevated child that exits 0
///   is not evidence the value landed.
///
/// # Errors
///
/// - [`Error::Refused`] when another debugger owns the hook, or the user
///   dismissed the elevation prompt.
/// - [`Error::Os`] when the registry or the shell refuses for any other
///   reason, or when the elevated pass reported success and the key still
///   disagrees.
pub fn set_replacement(enabled: bool, our_exe: &Path) -> Result<()> {
    // Read first, so we never delete or overwrite a hook that is not ours —
    // including when *enabling*: pointing the key at Vitals while Process
    // Explorer owns it is the same theft, just in the other direction.
    if let ReplacementStatus::ReplacedByOther { debugger } = replacement_status()? {
        return Err(Error::Refused(format!(
            "{debugger} is currently registered as the Task Manager replacement; \
             remove it in that application before Vitals can take over"
        )));
    }

    match write_replacement(enabled, our_exe) {
        Ok(()) => Ok(()),
        Err(Error::AccessDenied { .. }) => {
            elevate(enabled)?;
            confirm(enabled, our_exe)
        }
        Err(other) => Err(other),
    }
}

/// Performs the registry write in this process, with no elevation attempt.
///
/// This is what the elevated instance runs. Separated from
/// [`set_replacement`] so the elevated pass cannot recurse into another UAC
/// prompt if something has gone wrong with the token.
///
/// # Errors
///
/// [`Error::AccessDenied`] when not elevated; [`Error::Os`] otherwise.
pub fn write_replacement(enabled: bool, our_exe: &Path) -> Result<()> {
    if enabled {
        let path = our_exe.to_str().ok_or_else(|| Error::Os {
            context: format!("{} is not representable as UTF-8", our_exe.display()),
            code: 0,
        })?;

        // Quoted, because the installed path contains a space
        // (`C:\Program Files\Vitals\…`) and the loader treats the value as a
        // command line. Unquoted, Windows would try `C:\Program.exe`.
        let value = format!("\"{path}\"");

        let key = create_ifeo()?;
        write_string_value(key.raw(), DEBUGGER_VALUE, &value)
    } else {
        let Some(key) = open_ifeo(KEY_SET_VALUE)? else {
            // Nothing to remove. Off is the state that was asked for.
            return Ok(());
        };
        delete_value(key.raw(), DEBUGGER_VALUE)
    }
}

/// Re-reads the key and checks it says what the caller asked for.
fn confirm(enabled: bool, our_exe: &Path) -> Result<()> {
    let status = replacement_status()?;

    let agrees = if enabled {
        match &status {
            ReplacementStatus::ReplacedByUs { path } => paths_match(path, our_exe),
            _ => false,
        }
    } else {
        matches!(status, ReplacementStatus::NotReplaced)
    };

    if agrees {
        Ok(())
    } else {
        Err(Error::Os {
            context: format!(
                "the elevated pass reported success but the registry still reads {status:?}"
            ),
            code: 0,
        })
    }
}

/// Re-launches this executable elevated to do the write.
fn elevate(enabled: bool) -> Result<()> {
    let args = format!(
        "{SET_REPLACEMENT_ARG} {}",
        if enabled { "on" } else { "off" }
    );
    match run_elevated(&args, "the Task Manager replacement was left unchanged")? {
        0 => Ok(()),
        code => Err(Error::Os {
            context: "the elevated instance could not write the registry value".to_owned(),
            code: code.cast_signed(),
        }),
    }
}

/// Runs this executable elevated with `args`, waits for it, and returns its
/// exit code.
///
/// Shared by every "do this one thing as administrator" path, so the UAC
/// handling — a dismissed prompt is a decision, not a fault — is written
/// once. `declined` finishes the sentence "administrator approval was
/// declined, so …" for the caller's context.
///
/// `SEE_MASK_NOCLOSEPROCESS` is what makes the wait possible at all: without
/// it `ShellExecuteExW` returns no handle and there is nothing to wait on,
/// so the caller would read the result before the child had produced it.
///
/// # Errors
///
/// - [`Error::Refused`] when the user dismissed the UAC prompt.
/// - [`Error::Os`] when the shell could not start the elevated instance or
///   its exit code could not be read.
pub fn run_elevated(args: &str, declined: &str) -> Result<u32> {
    let exe = std::env::current_exe()?;
    let exe_w = wide(exe.to_str().ok_or_else(|| Error::Os {
        context: format!("{} is not representable as UTF-8", exe.display()),
        code: 0,
    })?);

    let verb = wide("runas");
    let args_w = wide(args);

    let mut info: SHELLEXECUTEINFOW = SHELLEXECUTEINFOW {
        cbSize: u32::try_from(size_of::<SHELLEXECUTEINFOW>()).unwrap_or(0),
        // `NOASYNC` for the same reason as the Properties dialog: this
        // thread is not a message pump and returns as soon as the call does.
        fMask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC,
        lpVerb: verb.as_ptr(),
        lpFile: exe_w.as_ptr(),
        lpParameters: args_w.as_ptr(),
        // The elevated child never builds a window; nothing to show.
        nShow: SW_HIDE,
        // SAFETY: every remaining field is a pointer, handle or integer for
        // which all-zero is the documented "not supplied" value.
        ..unsafe { std::mem::zeroed() }
    };

    // SAFETY: `info` is correctly sized and its three string pointers
    // reference buffers alive for the whole call.
    let ok = unsafe { ShellExecuteExW(&raw mut info) };

    if ok == 0 {
        // SAFETY: no preconditions.
        let code = unsafe { GetLastError() };

        // 1223 is what a dismissed UAC prompt returns. It is a decision, not
        // a fault, and rendering it as an error dialog would tell the user
        // something went wrong when nothing did.
        if code == ERROR_CANCELLED {
            return Err(Error::Refused(format!(
                "administrator approval was declined, so {declined}"
            )));
        }

        return Err(Error::Os {
            context: format!("ShellExecuteExW(runas, vitals {args})"),
            code: code.cast_signed(),
        });
    }

    wait_for(info.hProcess)
}

/// Waits for the elevated child and returns its exit code.
fn wait_for(process: HANDLE) -> Result<u32> {
    if process.is_null() {
        return Err(Error::Os {
            context: "the elevated instance started but returned no handle to wait on".to_owned(),
            code: 0,
        });
    }

    // SAFETY: `process` is a live handle from ShellExecuteExW, closed below
    // on every path out. Every elevated pass is one bounded operation (a
    // registry value, one process action), so there is no plausible hang to
    // bound — and a timeout would mean reading a result not yet produced.
    unsafe { WaitForSingleObject(process, INFINITE) };

    let mut code: u32 = 0;
    // SAFETY: the handle is valid and `code` is a live out-pointer.
    let read = unsafe { GetExitCodeProcess(process, &raw mut code) };
    // SAFETY: the handle came from ShellExecuteExW and is closed once.
    unsafe { CloseHandle(process) };

    if read == 0 {
        return Err(Error::Os {
            context: "GetExitCodeProcess on the elevated instance".to_owned(),
            code: 0,
        });
    }

    Ok(code)
}

/// Starts the real Task Manager, bypassing our own hook.
///
/// Note that Task Manager will immediately re-launch itself elevated, and
/// *that* launch goes through the hook and arrives as a Vitals process. The
/// app must call [`is_task_manager_elevation_hop`] at startup and, when it is
/// true, call this function again from the elevated process; on its own a
/// single call only gets the first half right.
///
/// While the replacement is on, `CreateProcess("taskmgr.exe")` launches
/// Vitals — including when Vitals is the caller, which would be an infinite
/// regress. The documented exemption is that the loader does **not** consult
/// Image File Execution Options for a process created by a debugger, so the
/// process is started with `DEBUG_ONLY_THIS_PROCESS` and detached
/// immediately.
///
/// `DebugSetProcessKillOnExit(FALSE)` before detaching is not optional: the
/// default is that a debuggee dies with its debugger, so without it Task
/// Manager would vanish the moment Vitals closed.
///
/// # Errors
///
/// [`Error::Os`] when the process cannot be created, which on a machine with
/// Task Manager disabled by policy is the expected answer.
pub fn launch_real_task_manager() -> Result<()> {
    launch_task_manager_as_debuggee()
}

/// Whether a launch that arrived with Task Manager's command line is Task
/// Manager's own elevation re-launch rather than a person pressing a key.
///
/// The discriminator is the token. Ctrl+Shift+Esc, the taskbar menu and
/// `Win+X` all start the debugger from the user's unelevated shell, so a
/// Vitals started that way is never elevated. Task Manager, by contrast,
/// starts unelevated and immediately asks the `AppInfo` service to start a
/// second copy elevated — and *that* copy is what the hook turns into us.
/// An elevated token plus `taskmgr.exe` arguments therefore has exactly one
/// cause, and the right response is to become the real Task Manager, not to
/// show a window.
///
/// Why not the parent PID: the elevated copy is spawned by `svchost`
/// (`AppInfo`), not by the first Task Manager, and the first one has exited
/// by the time we could look. Measured with `prove_taskmgr_parent`: parent
/// was `None`, token was elevated.
///
/// A user who runs their whole shell elevated (UAC off, or an admin
/// `explorer.exe`) will find Ctrl+Shift+Esc opening the real Task Manager
/// rather than Vitals. That is the safer failure: the alternative is a
/// permanent loop for everyone else.
#[must_use]
pub fn is_task_manager_elevation_hop() -> bool {
    is_elevated()
}

/// Whether this process runs with an elevated token.
#[must_use]
pub fn is_elevated() -> bool {
    crate::host::is_elevated()
}

fn launch_task_manager_as_debuggee() -> Result<()> {
    let system_root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".to_owned());
    let path = format!(r"{system_root}\System32\taskmgr.exe");

    // `CreateProcessW` may write to the command line buffer, so it is a
    // mutable vector rather than a pointer into a literal.
    let mut command = wide(&path);

    let mut startup: STARTUPINFOW = STARTUPINFOW {
        cb: u32::try_from(size_of::<STARTUPINFOW>()).unwrap_or(0),
        // SAFETY: the remaining fields are pointers and integers whose
        // all-zero value means "use the defaults".
        ..unsafe { std::mem::zeroed() }
    };
    // SAFETY: every field is a handle or integer; zero is the documented
    // "not yet filled in" state for an out-parameter.
    let mut process: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };

    // SAFETY: `command` is a live, NUL-terminated, writable UTF-16 buffer;
    // both structs are correctly sized and live for the call.
    let created = unsafe {
        CreateProcessW(
            std::ptr::null(),
            command.as_mut_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            0,
            DEBUG_ONLY_THIS_PROCESS,
            std::ptr::null(),
            std::ptr::null(),
            &raw mut startup,
            &raw mut process,
        )
    };

    if created == 0 {
        // SAFETY: no preconditions.
        let code = unsafe { GetLastError() };
        return Err(Error::Os {
            context: format!("CreateProcessW({path}) as a debuggee"),
            code: code.cast_signed(),
        });
    }

    detach(process.dwProcessId);

    // SAFETY: both handles came from CreateProcessW and are closed once. The
    // debuggee keeps running; these are our references to it, not its life.
    unsafe {
        CloseHandle(process.hThread);
        CloseHandle(process.hProcess);
    }

    Ok(())
}

/// Drains the initial debug events, then detaches without killing the child.
///
/// A debuggee is suspended at its first debug event until the debugger
/// continues it. Detaching without draining leaves Task Manager frozen
/// before it has drawn anything, which looks exactly like a crash.
fn detach(pid: u32) {
    // SAFETY: no preconditions; `FALSE` is the documented value for "leave
    // the debuggee running when the debugger exits".
    unsafe { DebugSetProcessKillOnExit(0) };

    // Bounded: the loader raises a create-process event and one module load
    // per image almost immediately. Ten iterations with a short timeout is
    // far more than that and cannot wedge the caller's thread if the
    // process dies before it starts.
    for _ in 0..10 {
        // SAFETY: `event` is a live out-parameter for the duration of the
        // call; the API fills it or returns zero.
        let mut event: DEBUG_EVENT = unsafe { std::mem::zeroed() };

        // SAFETY: as above.
        let got = unsafe { WaitForDebugEventEx(&raw mut event, 100) };
        if got == 0 {
            break;
        }

        // SAFETY: the identifiers come from the event we were just handed.
        unsafe {
            ContinueDebugEvent(event.dwProcessId, event.dwThreadId, DBG_CONTINUE);
        }

        // The first create-process event means the image is loaded and
        // running; there is nothing further we need to see.
        if event.dwDebugEventCode == CREATE_PROCESS_DEBUG_EVENT {
            break;
        }
    }

    // SAFETY: no preconditions. A failure here means we were not attached,
    // which is already the state we want.
    unsafe { DebugActiveProcessStop(pid) };
}

/// A registry key that closes itself.
#[derive(Debug)]
struct Key(HKEY);

impl Key {
    const fn raw(&self) -> HKEY {
        self.0
    }
}

impl Drop for Key {
    fn drop(&mut self) {
        // SAFETY: the handle came from a successful open/create and is
        // closed exactly once; `Key` is neither `Copy` nor `Clone`.
        unsafe { RegCloseKey(self.0) };
    }
}

/// Opens the IFEO key, or `None` when it does not exist.
///
/// `KEY_WOW64_64KEY` because `taskmgr.exe` is the 64-bit image and the
/// redirected `WOW6432Node` copy of this key governs a 32-bit `taskmgr.exe`
/// that does not exist on any supported Windows.
fn open_ifeo(access: u32) -> Result<Option<Key>> {
    let path = wide(IFEO_TASKMGR);
    let mut handle: HKEY = std::ptr::null_mut();

    // SAFETY: `path` is NUL-terminated and outlives the call; `handle` is a
    // valid out-pointer; the hive handle is a documented sentinel.
    let status = unsafe {
        RegOpenKeyExW(
            HKEY_LOCAL_MACHINE,
            path.as_ptr(),
            0,
            access | KEY_WOW64_64KEY,
            &raw mut handle,
        )
    };

    match status {
        ERROR_SUCCESS => Ok(Some(Key(handle))),
        ERROR_FILE_NOT_FOUND => Ok(None),
        ERROR_ACCESS_DENIED => Err(Error::AccessDenied {
            operation: "open the Task Manager image options key".to_owned(),
        }),
        code => Err(Error::Os {
            context: "RegOpenKeyExW(Image File Execution Options\\taskmgr.exe)".to_owned(),
            code: code.cast_signed(),
        }),
    }
}

/// Opens the IFEO key for writing, creating it if absent.
fn create_ifeo() -> Result<Key> {
    let path = wide(IFEO_TASKMGR);
    let mut handle: HKEY = std::ptr::null_mut();

    // SAFETY: `path` is NUL-terminated; `handle` is a valid out-pointer; a
    // null security descriptor means "inherit the parent key's", which for
    // IFEO is administrators-write, everyone-read — exactly right.
    let status = unsafe {
        RegCreateKeyExW(
            HKEY_LOCAL_MACHINE,
            path.as_ptr(),
            0,
            std::ptr::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE | KEY_READ | KEY_WOW64_64KEY,
            std::ptr::null(),
            &raw mut handle,
            std::ptr::null_mut(),
        )
    };

    match status {
        ERROR_SUCCESS => Ok(Key(handle)),
        ERROR_ACCESS_DENIED => Err(Error::AccessDenied {
            operation: "write the Task Manager image options key".to_owned(),
        }),
        code => Err(Error::Os {
            context: "RegCreateKeyExW(Image File Execution Options\\taskmgr.exe)".to_owned(),
            code: code.cast_signed(),
        }),
    }
}

/// Reads a `REG_SZ` value, or `None` when it is absent.
fn read_string_value(key: HKEY, name: &str) -> Result<Option<String>> {
    let name_w = wide(name);
    let mut kind: u32 = 0;
    let mut size: u32 = 0;

    // Two-call idiom: a null data pointer asks only for the size.
    // SAFETY: `name_w` is NUL-terminated; both out-pointers are live.
    let sized = unsafe {
        RegQueryValueExW(
            key,
            name_w.as_ptr(),
            std::ptr::null(),
            &raw mut kind,
            std::ptr::null_mut(),
            &raw mut size,
        )
    };

    match sized {
        ERROR_SUCCESS => {}
        ERROR_FILE_NOT_FOUND => return Ok(None),
        code => {
            return Err(Error::Os {
                context: format!("RegQueryValueExW({name}) size"),
                code: code.cast_signed(),
            });
        }
    }

    if size == 0 {
        return Ok(None);
    }

    let mut buffer = vec![0_u8; size as usize];

    // SAFETY: the buffer is sized by the call above and `size` still holds
    // its capacity in bytes.
    let read = unsafe {
        RegQueryValueExW(
            key,
            name_w.as_ptr(),
            std::ptr::null(),
            &raw mut kind,
            buffer.as_mut_ptr(),
            &raw mut size,
        )
    };

    if read != ERROR_SUCCESS {
        return Err(Error::Os {
            context: format!("RegQueryValueExW({name})"),
            code: read.cast_signed(),
        });
    }

    Ok(Some(decode_utf16(&buffer)))
}

/// Writes a `REG_SZ` value.
fn write_string_value(key: HKEY, name: &str, value: &str) -> Result<()> {
    let name_w = wide(name);
    let value_w = wide(value);

    // The byte count includes the terminator: a REG_SZ written without it is
    // read back by other tools as a string of unbounded length.
    let bytes = std::mem::size_of_val(value_w.as_slice());

    // SAFETY: both buffers are live for the call; `bytes` is their exact
    // size including the NUL, which is what the API documents.
    let status = unsafe {
        RegSetValueExW(
            key,
            name_w.as_ptr(),
            0,
            REG_SZ,
            value_w.as_ptr().cast::<u8>(),
            u32::try_from(bytes).unwrap_or(0),
        )
    };

    match status {
        ERROR_SUCCESS => Ok(()),
        ERROR_ACCESS_DENIED => Err(Error::AccessDenied {
            operation: format!("set the {name} value"),
        }),
        code => Err(Error::Os {
            context: format!("RegSetValueExW({name})"),
            code: code.cast_signed(),
        }),
    }
}

/// Deletes a value, treating "already absent" as success.
fn delete_value(key: HKEY, name: &str) -> Result<()> {
    let name_w = wide(name);

    // SAFETY: `name_w` is NUL-terminated and `key` is a live handle opened
    // with KEY_SET_VALUE.
    let status = unsafe { RegDeleteValueW(key, name_w.as_ptr()) };

    match status {
        ERROR_SUCCESS | ERROR_FILE_NOT_FOUND => Ok(()),
        ERROR_ACCESS_DENIED => Err(Error::AccessDenied {
            operation: format!("remove the {name} value"),
        }),
        code => Err(Error::Os {
            context: format!("RegDeleteValueW({name})"),
            code: code.cast_signed(),
        }),
    }
}

/// UTF-16, NUL-terminated.
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

/// Decodes a `REG_SZ` byte buffer, stopping at the first terminator.
fn decode_utf16(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u16::from_le_bytes(*pair))
        .take_while(|unit| *unit != 0)
        .collect();

    String::from_utf16_lossy(&units)
}

/// Strips the quotes the value is written with.
fn unquote(value: &str) -> &str {
    value.trim().trim_matches('"').trim()
}

/// Whether a registered debugger path is a Vitals executable.
///
/// By file name, not by full path: an upgrade that moves the install
/// directory, or a portable copy run from elsewhere, is still *ours*, and
/// treating it as a foreign tool would leave the user unable to turn the
/// setting off from inside the app.
fn is_vitals_executable(path: &str) -> bool {
    Path::new(path).file_name().is_some_and(|name| {
        let name = name.to_string_lossy().to_ascii_lowercase();
        name == "vitals-desktop.exe" || name == "vitals.exe"
    })
}

/// Case-insensitive path comparison, quotes already stripped.
fn paths_match(registered: &str, ours: &Path) -> bool {
    ours.to_str().is_some_and(|ours| {
        registered.eq_ignore_ascii_case(ours)
            // An installed path and `current_exe()` can differ by separator
            // normalisation, so fall back to the file name — which is what
            // `is_vitals_executable` already accepted.
            || is_vitals_executable(registered)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reading_the_key_never_fails_on_an_ordinary_machine() {
        // Read access to IFEO is granted to everyone; a failure here means
        // the path or the flags are wrong, not that the machine is unusual.
        let status = replacement_status().expect("IFEO is readable unelevated");
        // Any of the three is a legitimate answer depending on what is
        // installed; the assertion is that we got one at all.
        assert!(matches!(
            status,
            ReplacementStatus::NotReplaced
                | ReplacementStatus::ReplacedByUs { .. }
                | ReplacementStatus::ReplacedByOther { .. }
        ));
    }

    #[test]
    fn the_hop_check_is_the_token_and_nothing_else() {
        // Under an ordinary `cargo test` the token is not elevated, so this
        // is `false` and the app would open. Run from an elevated shell both
        // sides flip together. What must never happen is the two disagreeing:
        // a `true` on a normal launch means every Ctrl+Shift+Esc hands off to
        // taskmgr.exe and Vitals can never open.
        assert_eq!(is_task_manager_elevation_hop(), is_elevated());
    }

    #[test]
    fn a_foreign_debugger_is_recognised_as_foreign() {
        assert!(!is_vitals_executable(r"C:\Tools\procexp64.exe"));
        assert!(!is_vitals_executable(r"C:\Tools\SystemInformer.exe"));
    }

    #[test]
    fn our_own_executable_is_recognised_wherever_it_was_installed() {
        // The check must survive an upgrade that moves the directory, or the
        // switch becomes impossible to turn off from inside the app.
        assert!(is_vitals_executable(
            r"C:\Program Files\Vitals\vitals-desktop.exe"
        ));
        assert!(is_vitals_executable(r"D:\portable\VITALS-DESKTOP.EXE"));
    }

    #[test]
    fn the_stored_value_is_unquoted_before_it_is_compared() {
        // Written quoted because the install path contains a space; every
        // comparison downstream would fail against the quotes.
        assert_eq!(
            unquote("\"C:\\Program Files\\Vitals\\vitals-desktop.exe\""),
            r"C:\Program Files\Vitals\vitals-desktop.exe"
        );
        assert_eq!(unquote("  \"\"  "), "");
    }

    #[test]
    fn a_reg_sz_buffer_stops_at_its_terminator() {
        // The size reported by the registry includes the NUL, and trailing
        // garbage after it is not part of the value.
        let mut bytes = Vec::new();
        for unit in "abc".encode_utf16().chain(Some(0)).chain(Some(0x0041)) {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        assert_eq!(decode_utf16(&bytes), "abc");
    }

    #[test]
    fn an_empty_debugger_value_is_not_a_replacement() {
        // Some uninstallers blank the value rather than deleting it; treating
        // that as "replaced by other" would permanently block the setting.
        assert_eq!(unquote("\"\""), "");
    }
}
