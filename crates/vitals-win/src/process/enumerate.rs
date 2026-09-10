//! Process enumeration.

use std::mem::size_of;

use vitals_core::error::{Error, Result};
use vitals_core::ids::{Pid, ProcessKey};

use super::raw::{
    NtQuerySystemInformation, STATUS_INFO_LENGTH_MISMATCH, SYSTEM_FULL_PROCESS_INFORMATION,
    SYSTEM_PROCESS_INFORMATION, SystemExtendedThreadInformation, SystemProcessInformation,
    SystemProcessInformationExtension,
};

/// Which kernel counter produced a process's disk figures.
///
/// Re-exported from `vitals-core` rather than defined here: the UI has to
/// label the Disk column with this, so it crosses the IPC boundary, and two
/// enums with the same variants and a mapping function between them is one
/// more thing to forget to update.
///
/// - `StorageStack` is `PROCESS_DISK_COUNTERS` from the
///   `SystemFullProcessInformation` extension — bytes that reached a storage
///   driver, which needs `SeDebugPrivilege`.
/// - `AllIo` is `ReadTransferCount`/`WriteTransferCount` from the base
///   record: every `NtReadFile`/`NtWriteFile`, pipes and console included.
pub use vitals_core::process::DiskCounterSource;

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
    /// All-I/O transfer counts from the base record. Always present.
    ///
    /// These are *not* disk figures — see [`Self::disk_read_bytes`] — but
    /// they are the only per-process I/O number every supported build has.
    pub read_bytes: u64,
    pub write_bytes: u64,
    pub other_bytes: u64,
    pub read_ops: u64,
    pub write_ops: u64,
    /// Storage-stack bytes read, when the kernel exposed the disk-counter
    /// extension. `None` on builds without `SystemFullProcessInformation`;
    /// never a zero standing in for "not reported".
    pub storage_read_bytes: Option<u64>,
    pub storage_write_bytes: Option<u64>,
}

impl RawProcess {
    /// Total CPU time consumed since start, in 100ns units.
    #[must_use]
    pub const fn cpu_time(&self) -> u64 {
        self.kernel_time.saturating_add(self.user_time)
    }

    /// Cumulative bytes read from disk, and which counter said so.
    ///
    /// Prefers the storage-stack counter; falls back to the all-I/O figure
    /// only when the kernel did not report one. Callers differencing this
    /// across ticks get a consistent source for a given machine, because the
    /// enumerator settles on one information class and keeps it.
    #[must_use]
    pub const fn disk_read_bytes(&self) -> (u64, DiskCounterSource) {
        match self.storage_read_bytes {
            Some(bytes) => (bytes, DiskCounterSource::StorageStack),
            None => (self.read_bytes, DiskCounterSource::AllIo),
        }
    }

    /// Cumulative bytes written to disk, and which counter said so.
    #[must_use]
    pub const fn disk_write_bytes(&self) -> (u64, DiskCounterSource) {
        match self.storage_write_bytes {
            Some(bytes) => (bytes, DiskCounterSource::StorageStack),
            None => (self.write_bytes, DiskCounterSource::AllIo),
        }
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
    /// The information class the kernel accepted.
    ///
    /// Starts at the full class and drops to the base class permanently if
    /// the kernel rejects it. Decided once rather than per call so the disk
    /// counters a caller differences across ticks always come from the same
    /// source — flipping between them would produce one enormous bogus
    /// delta at the switch.
    class: i32,
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
            class: SYSTEM_FULL_PROCESS_INFORMATION,
        }
    }

    /// Which disk counter this enumerator's processes will carry.
    ///
    /// Meaningful after the first successful [`Self::enumerate`]; before
    /// that it reports the optimistic default.
    #[must_use]
    pub const fn disk_counter_source(&self) -> DiskCounterSource {
        if self.class == SYSTEM_FULL_PROCESS_INFORMATION {
            DiskCounterSource::StorageStack
        } else {
            DiskCounterSource::AllIo
        }
    }

    /// Current buffer size in bytes.
    ///
    /// Exposed for the overhead gate, which asserts the buffer converges
    /// rather than growing on every tick.
    #[must_use]
    pub fn buffer_capacity(&self) -> usize {
        self.buffer.len()
    }

    /// Reads the current process list.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Os`] if the native call fails for a reason other than
    /// an undersized buffer, which is handled internally by growing.
    pub fn enumerate(&mut self) -> Result<Vec<RawProcess>> {
        self.fill_buffer()?;

        let with_extension = self.class == SYSTEM_FULL_PROCESS_INFORMATION;

        // SAFETY: `fill_buffer` returned Ok, so the kernel has written a
        // valid, self-consistent chain of SYSTEM_PROCESS_INFORMATION entries
        // into `self.buffer`, and `walk` bounds every read by the buffer
        // length as it follows NextEntryOffset. `with_extension` is true
        // only when the class the kernel actually filled was the full one.
        Ok(unsafe { walk(&self.buffer, with_extension) })
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
                    self.class,
                    self.buffer.as_mut_ptr().cast(),
                    capacity,
                    &raw mut returned,
                )
            };

            if status >= 0 {
                return Ok(());
            }

            if status != STATUS_INFO_LENGTH_MISMATCH {
                // Anything other than "buffer too small" from the full class
                // means this session cannot have it: STATUS_ACCESS_DENIED
                // without SeDebugPrivilege (measured — the ordinary case), or
                // STATUS_INVALID_INFO_CLASS on a build that predates it. Fall
                // back to the class every NT release serves to everyone, and
                // stay there so a caller's rate baselines keep one source.
                if self.class == SYSTEM_FULL_PROCESS_INFORMATION {
                    self.class = SYSTEM_PROCESS_INFORMATION;
                    continue;
                }
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
/// `with_extension` says the buffer was filled by the full class, so each
/// entry is followed by extended thread records and then a
/// [`SystemProcessInformationExtension`].
///
/// # Safety
///
/// `buffer` must contain a valid chain of `SYSTEM_PROCESS_INFORMATION`
/// entries as written by `NtQuerySystemInformation`.
unsafe fn walk(buffer: &[u8], with_extension: bool) -> Vec<RawProcess> {
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

        // SAFETY: the extension offset is bounds-checked inside against both
        // the buffer and this entry's own extent.
        let disk = if with_extension {
            unsafe { read_extension(buffer, offset, &entry) }
        } else {
            None
        };

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
            storage_read_bytes: disk.map(|d| d.DiskCounters.BytesRead),
            storage_write_bytes: disk.map(|d| d.DiskCounters.BytesWritten),
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

/// Reads the disk-counter extension that follows an entry's thread array.
///
/// Returns `None` rather than guessing when the extension would not fit
/// inside the entry's own extent (`NextEntryOffset`) or the buffer. That
/// can only happen if the layout assumptions here are wrong for this
/// kernel, and the honest answer then is "not measured", not a number read
/// from the neighbouring process.
///
/// # Safety
///
/// `entry` must have been read from `buffer` at `entry_offset` by the full
/// information class.
unsafe fn read_extension(
    buffer: &[u8],
    entry_offset: usize,
    entry: &SystemProcessInformation,
) -> Option<SystemProcessInformationExtension> {
    let threads = (entry.NumberOfThreads as usize)
        .checked_mul(size_of::<SystemExtendedThreadInformation>())?;
    let start = entry_offset
        .checked_add(size_of::<SystemProcessInformation>())?
        .checked_add(threads)?;
    let end = start.checked_add(size_of::<SystemProcessInformationExtension>())?;

    // The last entry has NextEntryOffset 0, so its extent is the buffer.
    let entry_end = match entry.NextEntryOffset {
        0 => buffer.len(),
        next => entry_offset.checked_add(next as usize)?.min(buffer.len()),
    };
    if end > entry_end {
        return None;
    }

    // SAFETY: `start..end` lies inside `buffer` per the checks above. Unaligned
    // for the same reason the entry read is.
    Some(unsafe {
        buffer
            .as_ptr()
            .add(start)
            .cast::<SystemProcessInformationExtension>()
            .read_unaligned()
    })
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

    #[test]
    fn storage_counters_are_present_on_every_row_or_on_none_never_a_mixture() {
        // The extension is served per call, not per process. A row with
        // `None` beside rows with `Some` would mean the extension offset
        // arithmetic ran off the end of some entries and not others — a
        // layout bug, not a per-process fact.
        let mut e = ProcessEnumerator::new();
        let processes = e.enumerate().expect("enumerate");

        let with = processes
            .iter()
            .filter(|p| p.storage_read_bytes.is_some())
            .count();
        assert!(
            with == 0 || with == processes.len(),
            "{with} of {} rows carried storage counters",
            processes.len()
        );

        // And the enumerator's declared source must match the data it gave.
        let declared = e.disk_counter_source();
        let observed = if with == 0 {
            DiskCounterSource::AllIo
        } else {
            DiskCounterSource::StorageStack
        };
        assert_eq!(declared, observed);
    }

    #[test]
    fn a_refused_full_class_falls_back_without_erroring_and_stays_fallen_back() {
        // Unelevated, the kernel refuses class 148 with STATUS_ACCESS_DENIED.
        // That is the common case for this app, so it must be silent — and
        // sticky, because flipping sources between ticks would produce one
        // enormous bogus delta.
        let mut e = ProcessEnumerator::new();
        e.enumerate()
            .expect("first enumeration must not fail over the class");
        let first = e.disk_counter_source();
        e.enumerate().expect("second");
        assert_eq!(e.disk_counter_source(), first);
    }

    #[test]
    fn disk_bytes_prefer_the_storage_counter_and_say_so() {
        // The accessor is the single place the choice is made; the sampler
        // and the history both call it. If it silently picked the all-I/O
        // number while storage was present, the Disk column would overstate
        // exactly as before with nothing failing.
        let mut p = super::super::enumerate::RawProcess {
            key: ProcessKey::new(Pid(1), 1),
            parent: None,
            name: None,
            session_id: 0,
            base_priority: 8,
            thread_count: 1,
            handle_count: 0,
            kernel_time: 0,
            user_time: 0,
            create_time: 0,
            private_bytes: 0,
            working_set: 0,
            peak_working_set: 0,
            virtual_size: 0,
            page_faults: 0,
            hard_faults: 0,
            read_bytes: 1_000,
            write_bytes: 2_000,
            other_bytes: 0,
            read_ops: 0,
            write_ops: 0,
            storage_read_bytes: None,
            storage_write_bytes: None,
        };
        assert_eq!(p.disk_read_bytes(), (1_000, DiskCounterSource::AllIo));
        assert_eq!(p.disk_write_bytes(), (2_000, DiskCounterSource::AllIo));

        p.storage_read_bytes = Some(10);
        p.storage_write_bytes = Some(20);
        assert_eq!(p.disk_read_bytes(), (10, DiskCounterSource::StorageStack));
        assert_eq!(p.disk_write_bytes(), (20, DiskCounterSource::StorageStack));
    }

    #[test]
    fn the_extension_reader_refuses_to_read_past_an_entrys_own_extent() {
        // An entry whose NextEntryOffset leaves no room for the extension
        // must yield None, not the bytes of the following process.
        let mut buffer = vec![0_u8; 4096];
        let mut entry: SystemProcessInformation =
            // SAFETY: an all-zero SYSTEM_PROCESS_INFORMATION is a valid value
            // of every field (integers and null pointers).
            unsafe { std::mem::zeroed() };
        entry.NumberOfThreads = 1;
        // Exactly one extended thread and no room for the extension.
        entry.NextEntryOffset = u32::try_from(
            size_of::<SystemProcessInformation>() + size_of::<SystemExtendedThreadInformation>(),
        )
        .expect("fits");

        // SAFETY: writing a POD struct into a buffer we own, in bounds.
        unsafe {
            buffer
                .as_mut_ptr()
                .cast::<SystemProcessInformation>()
                .write_unaligned(entry);
        }

        // SAFETY: `entry` was read from `buffer` at 0 by construction.
        assert!(unsafe { read_extension(&buffer, 0, &entry) }.is_none());

        // Widen the extent and it becomes readable.
        entry.NextEntryOffset +=
            u32::try_from(size_of::<SystemProcessInformationExtension>()).expect("fits");
        // SAFETY: as above.
        assert!(unsafe { read_extension(&buffer, 0, &entry) }.is_some());
    }
}
