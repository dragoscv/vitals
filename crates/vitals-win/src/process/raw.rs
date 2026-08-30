//! Raw NT structures for process enumeration.
//!
//! `SYSTEM_PROCESS_INFORMATION` is a variable-length linked structure: each
//! entry is followed by its thread array, and `next_entry_offset` walks to
//! the following process. It returns every process, its threads, its memory
//! counters and its IO counters in a **single** syscall.
//!
//! The documented alternative — `CreateToolhelp32Snapshot`, then
//! `OpenProcess` + `GetProcessTimes` + `GetProcessMemoryInfo` +
//! `GetProcessIoCounters` + `CloseHandle` per process — is five syscalls and
//! a handle open per process per tick. At 400 processes and 1 Hz that is
//! 2000 syscalls a second against one, and it silently fails on any process
//! we cannot open, which includes most system processes.
//!
//! Layouts are declared here rather than taken from a crate because they live
//! behind `Wdk` feature gates whose availability has moved between releases.
//! These structures have been stable since Windows XP.

#![allow(non_snake_case)]

use std::ffi::c_void;

/// `SystemProcessInformation`
pub const SYSTEM_PROCESS_INFORMATION: i32 = 5;

/// `SystemExtendedProcessInformation`
pub const SYSTEM_EXTENDED_PROCESS_INFORMATION: i32 = 57;

/// `STATUS_INFO_LENGTH_MISMATCH` — the buffer was too small.
///
/// NTSTATUS is a signed 32-bit value where the top bit marks an error, so
/// every failure code reinterprets as negative. That is the encoding, not an
/// accident, which is why the sign flip here is explicit.
pub const STATUS_INFO_LENGTH_MISMATCH: i32 = 0xC000_0004_u32.cast_signed();

/// A counted UTF-16 string. `Length` is in **bytes**, not characters.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct UnicodeString {
    pub Length: u16,
    pub MaximumLength: u16,
    pub Buffer: *mut u16,
}

impl UnicodeString {
    /// Copies the string out as a Rust `String`.
    ///
    /// Returns `None` when the buffer is null, which is normal: the idle
    /// process and some protected processes report no image name.
    ///
    /// # Safety
    ///
    /// `Buffer` must point to at least `Length` bytes that remain valid for
    /// the duration of the call. In practice this means calling it only
    /// while the enumeration buffer is still alive.
    #[must_use]
    pub unsafe fn to_string_lossy(self) -> Option<String> {
        if self.Buffer.is_null() || self.Length == 0 {
            return None;
        }
        // Length is in bytes; UTF-16 code units are two bytes each.
        let len = (self.Length / 2) as usize;
        // SAFETY: guaranteed by the caller's contract.
        let slice = unsafe { std::slice::from_raw_parts(self.Buffer, len) };
        Some(String::from_utf16_lossy(slice))
    }
}

/// Per-thread information following each process entry.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SystemThreadInformation {
    pub KernelTime: i64,
    pub UserTime: i64,
    pub CreateTime: i64,
    pub WaitTime: u32,
    pub StartAddress: *mut c_void,
    pub ClientId: ClientId,
    pub Priority: i32,
    pub BasePriority: i32,
    pub ContextSwitches: u32,
    pub ThreadState: u32,
    pub WaitReason: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ClientId {
    pub UniqueProcess: isize,
    pub UniqueThread: isize,
}

/// One process entry.
///
/// Field order is load-bearing — it mirrors the kernel's layout exactly. Do
/// not reorder, and add nothing that is not in the real structure.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SystemProcessInformation {
    /// Byte offset to the next entry, or 0 at the end of the list.
    pub NextEntryOffset: u32,
    pub NumberOfThreads: u32,
    pub WorkingSetPrivateSize: i64,
    pub HardFaultCount: u32,
    pub NumberOfThreadsHighWatermark: u32,
    pub CycleTime: u64,
    /// Creation time, in 100ns intervals since 1601. Used as the
    /// disambiguator in `ProcessKey`.
    pub CreateTime: i64,
    pub UserTime: i64,
    pub KernelTime: i64,
    pub ImageName: UnicodeString,
    pub BasePriority: i32,
    pub UniqueProcessId: isize,
    pub InheritedFromUniqueProcessId: isize,
    pub HandleCount: u32,
    pub SessionId: u32,
    pub UniqueProcessKey: usize,
    pub PeakVirtualSize: usize,
    pub VirtualSize: usize,
    pub PageFaultCount: u32,
    pub PeakWorkingSetSize: usize,
    pub WorkingSetSize: usize,
    pub QuotaPeakPagedPoolUsage: usize,
    pub QuotaPagedPoolUsage: usize,
    pub QuotaPeakNonPagedPoolUsage: usize,
    pub QuotaNonPagedPoolUsage: usize,
    /// Private bytes — the honest "how much memory is this using" figure.
    pub PagefileUsage: usize,
    pub PeakPagefileUsage: usize,
    pub PrivatePageCount: usize,
    pub ReadOperationCount: i64,
    pub WriteOperationCount: i64,
    pub OtherOperationCount: i64,
    pub ReadTransferCount: i64,
    pub WriteTransferCount: i64,
    pub OtherTransferCount: i64,
}

unsafe extern "system" {
    pub fn NtQuerySystemInformation(
        SystemInformationClass: i32,
        SystemInformation: *mut c_void,
        SystemInformationLength: u32,
        ReturnLength: *mut u32,
    ) -> i32;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::{align_of, size_of};

    #[test]
    fn process_information_has_the_kernel_expected_size() {
        // The kernel walks this structure by byte offset. A layout mismatch
        // does not fail loudly — it silently reads the wrong fields and
        // produces plausible nonsense, which is far worse.
        //
        // 64-bit: 0x100 bytes. Pinned so an accidental field edit is caught
        // here rather than by a user reporting impossible memory figures.
        #[cfg(target_pointer_width = "64")]
        assert_eq!(size_of::<SystemProcessInformation>(), 0x100);

        assert_eq!(
            align_of::<SystemProcessInformation>(),
            align_of::<usize>().max(8)
        );
    }

    #[test]
    fn thread_information_has_the_expected_size() {
        #[cfg(target_pointer_width = "64")]
        assert_eq!(size_of::<SystemThreadInformation>(), 0x50);
    }

    #[test]
    fn unicode_string_is_three_machine_words_or_fewer() {
        assert_eq!(size_of::<UnicodeString>(), size_of::<usize>() * 2);
    }

    #[test]
    fn null_unicode_string_yields_none_rather_than_panicking() {
        // The System Idle Process reports exactly this.
        let empty = UnicodeString {
            Length: 0,
            MaximumLength: 0,
            Buffer: std::ptr::null_mut(),
        };
        // SAFETY: a null buffer is the case under test.
        assert!(unsafe { empty.to_string_lossy() }.is_none());
    }

    #[test]
    fn unicode_string_length_is_interpreted_as_bytes() {
        // Treating Length as a character count would double every name and
        // read past the buffer.
        let mut utf16: Vec<u16> = "chrome.exe".encode_utf16().collect();
        let s = UnicodeString {
            Length: u16::try_from(utf16.len() * 2).expect("fits"),
            MaximumLength: u16::try_from(utf16.len() * 2).expect("fits"),
            Buffer: utf16.as_mut_ptr(),
        };
        // SAFETY: `utf16` outlives the call and holds Length/2 code units.
        assert_eq!(
            unsafe { s.to_string_lossy() }.as_deref(),
            Some("chrome.exe")
        );
    }
}
