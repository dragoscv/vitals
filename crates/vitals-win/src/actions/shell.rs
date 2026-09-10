//! Shell integrations: reveal a file in Explorer, show its Properties.
//!
//! Both of these hand a path to the Windows shell, so both validate the path
//! first. `ShellExecuteExW` on a path that does not exist raises its *own*
//! error dialog on top of the app — a UI we do not own, cannot localise and
//! cannot dismiss — so a missing file is refused here with a real error the
//! caller can render itself. The same check rejects a directory: "Open file
//! location" on a folder would select the folder inside its parent, which is
//! not what the caller asked for, and Properties on a directory is a
//! different dialog with different meaning.

use std::path::Path;

use vitals_core::error::{Error, Result};
use vitals_core::ids::ProcessKey;
use windows_sys::Win32::Foundation::S_OK;
use windows_sys::Win32::System::Threading::{
    PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows_sys::Win32::UI::Shell::Common::ITEMIDLIST;
use windows_sys::Win32::UI::Shell::{
    ILCreateFromPathW, ILFree, SEE_MASK_INVOKEIDLIST, SEE_MASK_NOASYNC, SHELLEXECUTEINFOW,
    SHOpenFolderAndSelectItems, ShellExecuteExW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

use super::process::{ProcessHandle, verify_identity};

/// The full path of a process's executable.
///
/// The sampler carries only the file name — the path needs a handle per
/// process, which is the per-tick cost the enumeration exists to avoid — so
/// this is asked for once, for the selected row, and only by the two shell
/// actions below that cannot work without it.
///
/// Returns `Ok(None)` when the process denies `PROCESS_QUERY_LIMITED_INFORMATION`.
/// Access denied is not an error here for the same reason it is not in
/// `efficiency_mode`: it is the expected answer for every protected process,
/// and the UI's job is to disable the button, not to show a failure.
///
/// # Errors
///
/// - [`Error::NotFound`] when the process has exited or its PID was reused.
/// - [`Error::Os`] when the kernel refuses to report the name for a process
///   it did let us open.
pub fn executable_path(key: ProcessKey) -> Result<Option<String>> {
    let handle = match ProcessHandle::open(key.pid, PROCESS_QUERY_LIMITED_INFORMATION) {
        Ok(handle) => handle,
        Err(Error::AccessDenied { .. }) => return Ok(None),
        Err(other) => return Err(other),
    };
    verify_identity(&handle, key)?;

    // `MAX_PATH` is a lie on modern Windows with long paths enabled, so the
    // buffer is the extended limit. A heap vector rather than a stack array:
    // clippy's `large_stack_arrays` is right that 64 KiB does not belong on
    // the Tauri command thread's stack.
    let mut buffer = vec![0_u16; 32_768];
    let mut len = u32::try_from(buffer.len()).unwrap_or(u32::MAX);

    // SAFETY: the handle is valid; `buffer` is live for `len` UTF-16 units,
    // and the kernel writes `len` back as the count it filled.
    let ok = unsafe {
        QueryFullProcessImageNameW(
            handle.raw(),
            PROCESS_NAME_WIN32,
            buffer.as_mut_ptr(),
            &raw mut len,
        )
    };

    if ok == 0 {
        // SAFETY: no preconditions.
        let code = unsafe { windows_sys::Win32::Foundation::GetLastError() };
        return Err(Error::Os {
            context: format!("QueryFullProcessImageNameW({})", key.pid.get()),
            code: code.cast_signed(),
        });
    }

    let filled = buffer.get(..len as usize).unwrap_or(&[]);
    Ok(Some(String::from_utf16_lossy(filled)))
}

/// Opens Explorer with the file selected.
///
/// `SHOpenFolderAndSelectItems` rather than `explorer.exe /select,"path"`:
/// the subprocess form re-parses the path through a command line, so a path
/// containing a comma or a quote selects the wrong thing or nothing at all,
/// and it spawns a process we then cannot report a failure from.
///
/// # Errors
///
/// - [`Error::NotFound`] when the path does not exist or is not a file.
/// - [`Error::Os`] when the shell refuses to open the folder.
pub fn open_file_location(path: &Path) -> Result<()> {
    let wide = validated_wide(path)?;

    // SAFETY: `wide` is a live, NUL-terminated UTF-16 buffer for the duration
    // of the call. A null return means the path could not be parsed into an
    // ID list, which is checked below.
    let pidl = unsafe { ILCreateFromPathW(wide.as_ptr()) };
    if pidl.is_null() {
        return Err(Error::NotFound(format!(
            "the shell could not resolve {}",
            path.display()
        )));
    }

    // Selecting the item itself with a count of zero: passing the file's own
    // ID list as the *folder* and no children tells the shell to open the
    // parent and select it. Splitting the path ourselves would get the
    // parent wrong for namespace locations that are not real directories.
    // SAFETY: `pidl` is a valid ID list from `ILCreateFromPathW`, freed
    // below on every path out.
    let hr = unsafe { SHOpenFolderAndSelectItems(pidl, 0, std::ptr::null(), 0) };

    // SAFETY: `pidl` came from `ILCreateFromPathW` and is freed exactly once.
    unsafe { ILFree(pidl.cast_const().cast::<ITEMIDLIST>()) };

    if hr == S_OK {
        Ok(())
    } else {
        Err(Error::Os {
            context: format!("SHOpenFolderAndSelectItems({})", path.display()),
            code: hr,
        })
    }
}

/// Shows the shell's Properties dialog for a file.
///
/// # Errors
///
/// - [`Error::NotFound`] when the path does not exist or is not a file.
/// - [`Error::Os`] when the shell refuses to show the dialog.
pub fn show_file_properties(path: &Path) -> Result<()> {
    let wide = validated_wide(path)?;
    let verb: Vec<u16> = "properties".encode_utf16().chain(Some(0)).collect();

    // `SEE_MASK_INVOKEIDLIST` is what makes the "properties" verb work at
    // all: it is a dynamic verb contributed by shell extensions rather than
    // a statically registered one, so without the ID list the call fails
    // with "no association".
    //
    // `SEE_MASK_NOASYNC` because the Tauri command thread is not a message
    // pump and returns the moment this call does. Without it the shell does
    // its work on a worker thread that our thread's return can outlive, and
    // the dialog flashes up and vanishes.
    let mut info: SHELLEXECUTEINFOW = SHELLEXECUTEINFOW {
        cbSize: u32::try_from(size_of::<SHELLEXECUTEINFOW>()).unwrap_or(0),
        fMask: SEE_MASK_INVOKEIDLIST | SEE_MASK_NOASYNC,
        lpVerb: verb.as_ptr(),
        lpFile: wide.as_ptr(),
        nShow: SW_SHOWNORMAL,
        // SAFETY: every remaining field is a pointer, handle or integer for
        // which all-zero is the documented "not supplied" value.
        ..unsafe { std::mem::zeroed() }
    };

    // SAFETY: `info` is a correctly sized SHELLEXECUTEINFOW whose two string
    // pointers reference buffers alive for the whole call.
    let ok = unsafe { ShellExecuteExW(&raw mut info) };

    if ok == 0 {
        // SAFETY: no preconditions.
        let code = unsafe { windows_sys::Win32::Foundation::GetLastError() };
        return Err(Error::Os {
            context: format!("ShellExecuteExW(properties, {})", path.display()),
            code: code.cast_signed(),
        });
    }

    Ok(())
}

/// Rejects anything that is not an existing file, then widens it.
///
/// The check and the conversion are one function because doing them
/// separately invites a caller that converts and forgets to check — and the
/// consequence of that is the shell's own unlocalised error dialog.
fn validated_wide(path: &Path) -> Result<Vec<u16>> {
    if !path.is_file() {
        return Err(Error::NotFound(format!(
            "{} is not a file that exists",
            path.display()
        )));
    }

    let text = path.to_str().ok_or_else(|| {
        Error::NotFound(format!("{} is not representable as UTF-8", path.display()))
    })?;

    Ok(text.encode_utf16().chain(Some(0)).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_that_does_not_exist_is_refused_before_the_shell_sees_it() {
        // The shell would put its own error dialog on screen, which we cannot
        // localise or dismiss. Both entry points must fail first.
        let missing = Path::new(r"C:\this\does\not\exist\vitals-test.exe");
        assert!(matches!(
            open_file_location(missing),
            Err(Error::NotFound(_))
        ));
        assert!(matches!(
            show_file_properties(missing),
            Err(Error::NotFound(_))
        ));
    }

    #[test]
    fn a_directory_is_refused_rather_than_treated_as_a_file() {
        // "Open file location" on a directory would select it inside its
        // parent, which is a different answer from the one asked for.
        let dir = std::env::temp_dir();
        assert!(
            dir.is_dir(),
            "temp dir must exist for this test to mean anything"
        );
        assert!(matches!(open_file_location(&dir), Err(Error::NotFound(_))));
    }

    #[test]
    fn a_real_file_passes_validation_and_widens_with_a_terminator() {
        let exe = std::env::current_exe().expect("the test binary is a real file");
        let wide = validated_wide(&exe).expect("a real file must validate");
        assert_eq!(wide.last(), Some(&0), "the shell requires NUL termination");
        assert!(wide.len() > 1);
    }

    #[test]
    fn our_own_executable_path_resolves_to_the_running_binary() {
        // The one process guaranteed to grant us a handle. If this reports
        // None or a different file, the shell actions can never be enabled.
        let pid = std::process::id();
        let key = crate::process::ProcessEnumerator::new()
            .enumerate()
            .expect("enumerate")
            .into_iter()
            .find(|p| p.key.pid.get() == pid)
            .map(|p| p.key)
            .expect("our own process is in the list");
        let path = executable_path(key)
            .expect("querying our own process must not fail")
            .expect("our own process must not deny us a handle");
        let expected = std::env::current_exe().expect("current exe");
        assert_eq!(
            Path::new(&path).file_name(),
            expected.file_name(),
            "got {path}, expected {}",
            expected.display()
        );
    }
}
