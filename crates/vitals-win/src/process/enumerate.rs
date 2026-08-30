//! Process enumeration.

use std::mem::size_of;

use vitals_core::error::{Error, Result};
use vitals_core::ids::{Pid, ProcessKey};

use super::raw::{
    NtQuerySystemInformation, STATUS_INFO_LENGTH_MISMATCH, SYSTEM_PROCESS_INFORMATION,
    SystemProcessInformation,
};

/// A process as read from the kernel, before any rate computation.
///
/// Owned and plain, so the unsafe buffer walk is confined to this module and
/// nothing downstream holds a pointer into kernel-shaped memory.
#[derive(Debug, Clone)]
pub struct RawProcess {
    pub key: ProcessKey,
    pub parent: Option<Pid>,
    pub name: Option<String>,
    pub session_id: u32,
    pub base_priority: i32,
    pub thread_count: u32,
    pub handle_count: u32,
    /// Cumulative kernel time, 100ns units.
    pub kernel_time: u64,
    /// Cumulative user time, 100ns units.
    pub user_time: u64,
    pub create_time: i64,
    /// Private bytes — memory freed if the process exited.
    pub private_bytes: u64,
    pub working_set: u64,
    pub peak_working_set: u64,
    pub virtual_size: u64,
    pub page_faults: u32,
    /// Faults that hit the disk. The real memory-pressure signal.
    pub hard_faults: u32,
    pub read_bytes: u64,
    pub write_bytes: u64,
    pub other_bytes: u64,
    pub read_ops: u64,
    pub write_ops: u64,
}

impl RawProcess {
    /// Total CPU time consumed since start, in 100ns units.
    #[must_use]
    pub const fn cpu_time(&self) -> u64 {
        self.kernel_time.saturating_add(self.user_time)
    }

    /// Whether this is the System Idle Process.
    ///
    /// PID 0 accumulates CPU time for every idle cycle on every core, so
    /// including it makes the machine look permanently pegged. Every tool in
    /// this space hides it, and so must we.
    #[must_use]
    pub const fn is_idle_process(&self) -> bool {
        self.key.pid.get() == 0
    }
}

/// Enumerates processes, reusing its buffer between calls.
///
/// Hold one instance for the lifetime of the sampler: the buffer grows to fit
/// the machine and then stops allocating, which matters when this runs once a
/// second forever.
#[derive(Debug)]
pub struct ProcessEnumerator {
    buffer: Vec<u8>,
}

impl Default for ProcessEnumerator {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcessEnumerator {
    /// Initial buffer size.
    ///
    /// 512 KiB holds roughly 1200 processes with their threads, which covers
    /// the overwhelming majority of machines in one call. Starting smaller
    /// just guarantees a resize on the first sample.
    const INITIAL_CAPACITY: usize = 512 * 1024;

    /// Guards against an unbounded growth loop if the kernel keeps reporting
    /// a mismatch. 64 MiB is far beyond any real machine.
    const MAX_CAPACITY: usize = 64 * 1024 * 1024;

    #[must_use]
    pub fn new() -> Self {
        Self {
            buffer: vec![0_u8; Self::INITIAL_CAPACITY],
        }
    }

    /// Reads the current process list.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Os`] if the native call fails for a reason other than
    /// an undersized buffer, which is handled internally by growing.
    pub fn enumerate(&mut self) -> Result<Vec<RawProcess>> {
        self.fill_buffer()?;

        // SAFETY: `fill_buffer` returned Ok, so the kernel has written a
        // valid, self-consistent chain of SYSTEM_PROCESS_INFORMATION entries
        // into `self.buffer`, and `walk` bounds every read by the buffer
        // length as it follows NextEntryOffset.
        Ok(unsafe { walk(&self.buffer) })
    }

    /// Fills the buffer, growing until the kernel stops complaining.
    fn fill_buffer(&mut self) -> Result<()> {
        loop {
            let mut returned: u32 = 0;
            let capacity = u32::try_from(self.buffer.len()).unwrap_or(u32::MAX);

            // SAFETY: the pointer is valid for `capacity` bytes because it
            // comes from a Vec of exactly that length, and we tell the kernel
            // the true size so it cannot overrun.
            let status = unsafe {
                NtQuerySystemInformation(
                    SYSTEM_PROCESS_INFORMATION,
                    self.buffer.as_mut_ptr().cast(),
                    capacity,
                    &raw mut returned,
                )
            };

            if status >= 0 {
                return Ok(());
            }

            if status != STATUS_INFO_LENGTH_MISMATCH {
                return Err(Error::Os {
                    context: "NtQuerySystemInformation(SystemProcessInformation)".into(),
                    code: status,
                });
            }

            // The process list can grow between the size query and the read,
            // so `returned` is a floor rather than an answer. Growing by 1.5x
            // beyond it converges in one more iteration in practice while
            // avoiding the doubling that wastes tens of megabytes.
            let needed = (returned as usize).max(self.buffer.len());
            let next = needed.saturating_add(needed / 2).max(self.buffer.len() * 2);

            if next > Self::MAX_CAPACITY {
                return Err(Error::Os {
                    context: format!(
                        "process list exceeded {} MiB",
                        Self::MAX_CAPACITY / (1024 * 1024)
                    ),
                    code: status,
                });
            }

            self.buffer.resize(next, 0);
        }
    }
}

/// Walks the kernel's linked entries, copying each into an owned struct.
///
/// # Safety
///
/// `buffer` must contain a valid chain of `SYSTEM_PROCESS_INFORMATION`
/// entries as written by `NtQuerySystemInformation`.
unsafe fn walk(buffer: &[u8]) -> Vec<RawProcess> {
    // Typical machines run 200-500 processes; pre-sizing avoids a handful of
    // reallocations on every single tick.
    let mut out = Vec::with_capacity(512);
    let mut offset = 0_usize;

    loop {
        // Bounds check before every read. The kernel is trusted, but a
        // truncated or corrupted buffer must not become an out-of-bounds
        // read in a process that runs continuously.
        if offset
            .checked_add(size_of::<SystemProcessInformation>())
            .is_none_or(|end| end > buffer.len())
        {
            break;
        }

        // SAFETY: the bounds check above guarantees the read is in range.
        // `read_unaligned` because the kernel packs entries to 8-byte
        // boundaries that do not always match the struct's alignment.
        let entry = unsafe {
            buffer
                .as_ptr()
                .add(offset)
                .cast::<SystemProcessInformation>()
                .read_unaligned()
        };

        let pid = entry.UniqueProcessId as u32;

        // SAFETY: `ImageName.Buffer` points into `buffer`, which outlives
        // this call, and `Length` is the kernel's own byte count.
        let name = unsafe { entry.ImageName.to_string_lossy() };

        out.push(RawProcess {
            key: ProcessKey::new(Pid(pid), entry.CreateTime as u64),
            // PID 0 as a parent means "no parent" — the kernel uses it for
            // orphans as well as for the idle process itself.
            parent: match entry.InheritedFromUniqueProcessId as u32 {
                0 => None,
                parent => Some(Pid(parent)),
            },
            name,
            session_id: entry.SessionId,
            base_priority: entry.BasePriority,
            thread_count: entry.NumberOfThreads,
            handle_count: entry.HandleCount,
            kernel_time: entry.KernelTime as u64,
            user_time: entry.UserTime as u64,
            create_time: entry.CreateTime,
            private_bytes: entry.PagefileUsage as u64,
            working_set: entry.WorkingSetSize as u64,
            peak_working_set: entry.PeakWorkingSetSize as u64,
            virtual_size: entry.VirtualSize as u64,
            page_faults: entry.PageFaultCount,
            hard_faults: entry.HardFaultCount,
            read_bytes: entry.ReadTransferCount as u64,
            write_bytes: entry.WriteTransferCount as u64,
            other_bytes: entry.OtherTransferCount as u64,
            read_ops: entry.ReadOperationCount as u64,
            write_ops: entry.WriteOperationCount as u64,
        });

        // Zero terminates the list. Anything that would not advance the
        // cursor must also stop us, or a malformed buffer becomes an
        // infinite loop in a process the user cannot kill from our own UI.
        if entry.NextEntryOffset == 0 {
            break;
        }
        let Some(next) = offset.checked_add(entry.NextEntryOffset as usize) else {
            break;
        };
        if next <= offset {
            break;
        }
        offset = next;
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_a_plausible_number_of_processes() {
        let mut e = ProcessEnumerator::new();
        let processes = e.enumerate().expect("enumeration should succeed");

        // Any running Windows machine has far more than a handful.
        assert!(
            processes.len() > 20,
            "only found {} processes",
            processes.len()
        );
        assert!(processes.len() < 100_000);
    }

    #[test]
    fn finds_our_own_process() {
        let mut e = ProcessEnumerator::new();
        let processes = e.enumerate().expect("enumerate");
        let me = std::process::id();

        assert!(
            processes.iter().any(|p| p.key.pid.get() == me),
            "the enumerator did not find the process running the test (pid {me})"
        );
    }

    #[test]
    fn finds_the_system_idle_process() {
        let mut e = ProcessEnumerator::new();
        let processes = e.enumerate().expect("enumerate");
        assert!(processes.iter().any(RawProcess::is_idle_process));
    }

    #[test]
    fn process_names_are_decoded() {
        let mut e = ProcessEnumerator::new();
        let processes = e.enumerate().expect("enumerate");

        // The session manager exists on every Windows machine.
        assert!(
            processes.iter().any(|p| p
                .name
                .as_deref()
                .is_some_and(|n| n.eq_ignore_ascii_case("smss.exe"))),
            "smss.exe was not found; UnicodeString decoding is probably wrong"
        );
    }

    #[test]
    fn names_are_not_mangled_by_a_length_misinterpretation() {
        // Reading Length as characters rather than bytes produces names with
        // trailing garbage. Every name should be printable and reasonably
        // short.
        let mut e = ProcessEnumerator::new();
        for p in e.enumerate().expect("enumerate") {
            if let Some(name) = &p.name {
                assert!(!name.is_empty());
                assert!(name.len() < 260, "implausible process name: {name:?}");
                assert!(
                    !name.contains('\u{0}'),
                    "embedded NUL in {name:?} means the length was misread"
                );
            }
        }
    }

    #[test]
    fn keys_are_unique() {
        // If two processes shared a key, the UI would collapse them into one
        // row and act on the wrong one.
        let mut e = ProcessEnumerator::new();
        let processes = e.enumerate().expect("enumerate");

        let mut keys: Vec<_> = processes.iter().map(|p| p.key).collect();
        let total = keys.len();
        keys.sort_unstable_by_key(|k| (k.pid.get(), k.start_time));
        keys.dedup();

        assert_eq!(
            keys.len(),
            total,
            "duplicate ProcessKey in a single snapshot"
        );
    }

    #[test]
    fn our_own_memory_figures_are_plausible() {
        let mut e = ProcessEnumerator::new();
        let processes = e.enumerate().expect("enumerate");
        let me = processes
            .iter()
            .find(|p| p.key.pid.get() == std::process::id())
            .expect("our own process");

        // A test binary uses more than 1 MiB and less than 8 GiB. Wildly
        // outside that means the field offsets are wrong.
        assert!(
            me.private_bytes > 1024 * 1024,
            "private bytes: {}",
            me.private_bytes
        );
        assert!(me.private_bytes < 8 * 1024 * 1024 * 1024);
        assert!(me.working_set > 0);
        assert!(me.thread_count >= 1);
    }

    #[test]
    fn parent_of_the_idle_process_is_none() {
        let mut e = ProcessEnumerator::new();
        let processes = e.enumerate().expect("enumerate");
        let idle = processes
            .iter()
            .find(|p| p.is_idle_process())
            .expect("idle process");
        assert!(idle.parent.is_none());
    }

    #[test]
    fn repeated_enumeration_reuses_the_buffer() {
        // The buffer must converge, not grow every tick.
        let mut e = ProcessEnumerator::new();
        e.enumerate().expect("first");
        let after_first = e.buffer.len();
        for _ in 0..5 {
            e.enumerate().expect("subsequent");
        }
        assert_eq!(
            e.buffer.len(),
            after_first,
            "buffer grew across identical calls"
        );
    }

    #[test]
    fn a_tiny_buffer_grows_instead_of_failing() {
        // Simulates a machine with far more processes than the default fits.
        let mut e = ProcessEnumerator::new();
        e.buffer = vec![0_u8; 64];

        let processes = e.enumerate().expect("should grow and succeed");
        assert!(processes.len() > 20);
        assert!(e.buffer.len() > 64);
    }

    #[test]
    fn cpu_time_sums_kernel_and_user() {
        let mut e = ProcessEnumerator::new();
        let processes = e.enumerate().expect("enumerate");
        for p in &processes {
            assert_eq!(p.cpu_time(), p.kernel_time + p.user_time);
        }
    }
}
