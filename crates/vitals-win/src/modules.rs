//! Modules (executable and DLLs) mapped into a process, on demand only.
//!
//! `EnumProcessModulesEx` reads the target's loader data by probing its
//! address space, so the cost scales with the module count — several hundred
//! for a browser — and every call opens a handle. That is fine for a selected
//! row and unacceptable once a second across the whole machine, so nothing
//! here is on the sampler's path.
//!
//! ## `ERROR_PARTIAL_COPY`
//!
//! The documented and very common failure. The loader is mid-way through
//! mapping or unmapping a module, so part of the list the kernel copied for
//! us is no longer valid and the whole call fails. It is transient by
//! definition, which is why one retry is worth making — and why a *second*
//! retry is not: a process that is still loading after the first will still
//! be loading after the third, and the honest answer is then an error rather
//! than a list we know is incomplete.

use std::path::Path;

use vitals_core::error::{Error, Result};
use vitals_core::ids::ProcessKey;
use vitals_core::process::ModuleInfo;
use windows_sys::Win32::Foundation::{ERROR_PARTIAL_COPY, FALSE, HMODULE};
use windows_sys::Win32::System::ProcessStatus::{
    K32EnumProcessModulesEx, K32GetModuleFileNameExW, K32GetModuleInformation, LIST_MODULES_ALL,
    MODULEINFO,
};
use windows_sys::Win32::System::Threading::{PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_VM_READ};

use crate::actions::process::{ProcessHandle, verify_identity};

/// Lists the modules mapped into a process.
///
/// `LIST_MODULES_ALL`, so a 32-bit process on a 64-bit OS reports its 32-bit
/// modules rather than the empty list the default filter would give.
///
/// Version strings are not read: that means opening each file and parsing its
/// version resource, which is neither free nor available for a module whose
/// backing file has been deleted or replaced since it was mapped. The path is
/// returned instead, which is what a caller needs to fetch a version lazily
/// for the one row a user asks about.
///
/// # Errors
///
/// - [`Error::NotFound`] when the process has exited or its PID was reused.
/// - [`Error::AccessDenied`] when the process denies `PROCESS_VM_READ`.
/// - [`Error::Os`] with `ERROR_PARTIAL_COPY` when the loader was still busy
///   after a retry. Returned rather than a short list: a partial module list
///   is indistinguishable from a complete one and would have a user
///   concluding a DLL is not loaded when it is.
pub fn for_process(key: ProcessKey) -> Result<Vec<ModuleInfo>> {
    let handle = ProcessHandle::open(key.pid, PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ)?;
    verify_identity(&handle, key)?;

    let modules = enumerate(&handle)?;

    // The enumeration is not atomic with the identity check. A process that
    // exited during it would otherwise contribute whatever the loader left
    // behind, under a key that no longer means anything.
    verify_identity(&handle, key)?;

    Ok(modules
        .into_iter()
        .filter_map(|module| describe(&handle, module))
        .collect())
}

/// Reads the raw module handle list, retrying once on a transient failure.
fn enumerate(handle: &ProcessHandle) -> Result<Vec<HMODULE>> {
    match enumerate_once(handle) {
        Err(Error::Os { code, .. }) if code == ERROR_PARTIAL_COPY.cast_signed() => {
            enumerate_once(handle)
        }
        other => other,
    }
}

fn enumerate_once(handle: &ProcessHandle) -> Result<Vec<HMODULE>> {
    // Sized for a typical process; grown once if the target is a browser.
    let mut modules: Vec<HMODULE> = vec![std::ptr::null_mut(); 256];

    loop {
        let mut needed: u32 = 0;
        let capacity = u32::try_from(std::mem::size_of_val(modules.as_slice())).unwrap_or(u32::MAX);

        // SAFETY: the handle is valid and carries PROCESS_VM_READ; the
        // pointer addresses `capacity` bytes of a Vec we own.
        let ok = unsafe {
            K32EnumProcessModulesEx(
                handle.raw(),
                modules.as_mut_ptr(),
                capacity,
                &raw mut needed,
                LIST_MODULES_ALL,
            )
        };

        if ok == FALSE {
            // SAFETY: no preconditions.
            let code = unsafe { windows_sys::Win32::Foundation::GetLastError() };
            return Err(match code {
                windows_sys::Win32::Foundation::ERROR_ACCESS_DENIED => Error::AccessDenied {
                    operation: "read the module list".into(),
                },
                other => Error::Os {
                    context: "K32EnumProcessModulesEx".into(),
                    code: other.cast_signed(),
                },
            });
        }

        let required = needed as usize / size_of::<HMODULE>();
        if required <= modules.len() {
            modules.truncate(required);
            return Ok(modules);
        }

        // The loader can map more between the two calls, so leave headroom
        // rather than sizing exactly and looping again.
        modules.resize(required + required / 4 + 8, std::ptr::null_mut());
    }
}

/// Resolves one module handle to its path and extent.
///
/// Returns `None` for a module that has been unloaded since the list was
/// taken. That is normal at any moment in a running process, and a row with
/// a fabricated name and a zero size would be worse than an absent one.
fn describe(handle: &ProcessHandle, module: HMODULE) -> Option<ModuleInfo> {
    // Heap, not stack: `MAX_PATH` is not the limit here — a long-path-aware
    // process can map a module from a 32k-character path, and 64 KiB on the
    // stack of whatever thread the caller happens to be on is how a deep
    // recursion elsewhere becomes a stack overflow in this function.
    let mut path = vec![0_u16; 32_768];

    // SAFETY: the handle is valid, `module` came from the enumeration above,
    // and the buffer length passed is its true element count.
    let written = unsafe {
        K32GetModuleFileNameExW(
            handle.raw(),
            module,
            path.as_mut_ptr(),
            u32::try_from(path.len()).unwrap_or(u32::MAX),
        )
    };

    if written == 0 {
        return None;
    }

    let full = String::from_utf16_lossy(&path[..written as usize]);
    let name = Path::new(&full)
        .file_name()
        .map_or_else(|| full.clone(), |n| n.to_string_lossy().into_owned());

    let mut info = MODULEINFO {
        lpBaseOfDll: std::ptr::null_mut(),
        SizeOfImage: 0,
        EntryPoint: std::ptr::null_mut(),
    };

    // SAFETY: as above; `info` is a live MODULEINFO of the size passed.
    let ok = unsafe {
        K32GetModuleInformation(
            handle.raw(),
            module,
            &raw mut info,
            u32::try_from(size_of::<MODULEINFO>()).unwrap_or(24),
        )
    };

    // Base address and size come from the module handle itself when
    // GetModuleInformation loses the race — the handle *is* the base address
    // on Windows, so that half is never guessed. Size genuinely is unknown,
    // and 0 is the value the caller sees; there is no Option in the wire type
    // for it because a mapped module always has a size and this is the
    // vanishingly rare unload race.
    let size = if ok == FALSE {
        0
    } else {
        u64::from(info.SizeOfImage)
    };

    Some(ModuleInfo {
        name,
        path: Some(full),
        base_address: module as u64,
        size,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn spawn_victim() -> std::process::Child {
        Command::new("cmd")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("spawn a test process")
    }

    fn key_for(pid: u32) -> ProcessKey {
        let mut enumerator = crate::process::ProcessEnumerator::new();
        let found = enumerator
            .enumerate()
            .expect("enumerate")
            .into_iter()
            .find(|p| p.key.pid.get() == pid);

        match found {
            Some(process) => process.key,
            None => panic!("process {pid} not found"),
        }
    }

    #[test]
    fn every_process_has_ntdll_mapped_so_its_absence_means_the_list_is_wrong() {
        // SAFETY: no preconditions.
        let key = key_for(unsafe { windows_sys::Win32::System::Threading::GetCurrentProcessId() });
        let modules = for_process(key).expect("enumerate our own modules");

        assert!(
            modules
                .iter()
                .any(|m| m.name.eq_ignore_ascii_case("ntdll.dll")),
            "ntdll.dll is mapped into every NT process; got {:?}",
            modules.iter().map(|m| &m.name).collect::<Vec<_>>()
        );
    }

    #[test]
    fn the_first_module_is_the_executable_itself() {
        // The loader lists the image before its imports; the UI relies on it
        // to show "which binary is this" without a second query.
        // SAFETY: no preconditions.
        let key = key_for(unsafe { windows_sys::Win32::System::Threading::GetCurrentProcessId() });
        let modules = for_process(key).expect("enumerate");
        let first = modules.first().expect("at least one module");

        assert!(
            Path::new(&first.name)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("exe")),
            "expected the executable first, got {}",
            first.name
        );
    }

    #[test]
    fn every_module_reports_a_non_zero_base_address() {
        // A zero base would be a null module handle surviving the filter,
        // which means the enumeration size arithmetic is wrong.
        // SAFETY: no preconditions.
        let key = key_for(unsafe { windows_sys::Win32::System::Threading::GetCurrentProcessId() });
        for module in for_process(key).expect("enumerate") {
            assert_ne!(module.base_address, 0, "{} had a null base", module.name);
        }
    }

    #[test]
    fn an_exited_process_yields_an_error_rather_than_a_partial_list() {
        let mut child = spawn_victim();
        let key = key_for(child.id());
        child.kill().expect("kill");
        child.wait().expect("wait");

        let result = for_process(key);
        assert!(
            result.is_err(),
            "a dead process must not produce a module list, got {:?} rows",
            result.map(|r| r.len())
        );
    }

    #[test]
    fn a_recycled_pid_is_rejected_by_the_start_time_rather_than_listed() {
        // SAFETY: no preconditions.
        let live = unsafe { windows_sys::Win32::System::Threading::GetCurrentProcessId() };
        let mut key = key_for(live);
        key.start_time = key.start_time.wrapping_sub(1_000_000);

        assert!(matches!(for_process(key), Err(Error::NotFound(_))));
    }

    #[test]
    fn the_system_process_is_denied_rather_than_reported_as_having_no_modules() {
        // An empty Vec here would claim the kernel has no image mapped.
        let key = key_for(4);
        match for_process(key) {
            Err(_) => {}
            Ok(modules) => assert!(
                !modules.is_empty(),
                "System reported zero modules — that is a fabricated answer"
            ),
        }
    }
}
