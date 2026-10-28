//! The few Mach and `libproc` reads sysinfo does not expose.
//!
//! Each wrapper returns `None` on any failure. The callers turn that into a
//! `None` reading, which the UI shows as a dash; a failed syscall must never
//! surface as a zero.

use std::mem::{MaybeUninit, size_of};

// libc marks the Mach port getters deprecated in favour of the `mach2` crate.
// They are the same stable kernel ABI; pulling in a second binding crate for
// two functions would double the unsafe surface to audit for no behaviour
// change. Confined to these two wrappers.
#[allow(deprecated)]
fn host_port() -> libc::mach_port_t {
    // SAFETY: returns a send right to the host port; never fails.
    unsafe { libc::mach_host_self() }
}

#[allow(deprecated)]
fn task_port() -> libc::mach_port_t {
    // SAFETY: returns the current task's port; never fails.
    unsafe { libc::mach_task_self() }
}

/// Cumulative CPU ticks of one logical core.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CoreTicks {
    pub user: u64,
    pub system: u64,
    pub idle: u64,
    pub nice: u64,
}

#[cfg(test)]
impl CoreTicks {
    const fn total(self) -> u64 {
        self.user + self.system + self.idle + self.nice
    }
}

/// Per-core tick counters from `host_processor_info(PROCESSOR_CPU_LOAD_INFO)`.
pub fn core_ticks() -> Option<Vec<CoreTicks>> {
    let mut count: libc::natural_t = 0;
    let mut info: libc::processor_info_array_t = std::ptr::null_mut();
    let mut info_count: libc::mach_msg_type_number_t = 0;
    // SAFETY: out-pointers are valid for the duration of the call; on success
    // the kernel hands us `info_count` integers we must vm_deallocate.
    let rc = unsafe {
        libc::host_processor_info(
            host_port(),
            libc::PROCESSOR_CPU_LOAD_INFO,
            &raw mut count,
            &raw mut info,
            &raw mut info_count,
        )
    };
    if rc != libc::KERN_SUCCESS || info.is_null() {
        return None;
    }
    let states = libc::CPU_STATE_MAX as usize;
    // SAFETY: the kernel guarantees `info_count` integers at `info`.
    let raw = unsafe { std::slice::from_raw_parts(info, info_count as usize) };
    let cores = raw
        .chunks_exact(states)
        .take(count as usize)
        .map(|c| CoreTicks {
            user: u64::from(c[libc::CPU_STATE_USER as usize] as u32),
            system: u64::from(c[libc::CPU_STATE_SYSTEM as usize] as u32),
            idle: u64::from(c[libc::CPU_STATE_IDLE as usize] as u32),
            nice: u64::from(c[libc::CPU_STATE_NICE as usize] as u32),
        })
        .collect();
    // SAFETY: returning exactly the region the kernel allocated for us.
    unsafe {
        libc::vm_deallocate(
            task_port(),
            info as libc::vm_address_t,
            (info_count as usize * size_of::<libc::integer_t>()) as libc::vm_size_t,
        );
    }
    Some(cores)
}

/// Share of the interval each core spent busy and in the kernel, 0..=100.
///
/// The counters are 32-bit in the kernel and wrap; `wrapping_sub` on the
/// widened values keeps a wrapped core from reading as a 4-billion-tick spike.
pub fn core_shares(previous: &[CoreTicks], current: &[CoreTicks]) -> Vec<(f32, f32)> {
    let wrap = |now: u64, then: u64| u64::from((now as u32).wrapping_sub(then as u32));
    previous
        .iter()
        .zip(current)
        .map(|(a, b)| {
            let user = wrap(b.user, a.user) + wrap(b.nice, a.nice);
            let system = wrap(b.system, a.system);
            let idle = wrap(b.idle, a.idle);
            let total = user + system + idle;
            if total == 0 {
                return (0.0, 0.0);
            }
            let busy = (user + system) as f32 / total as f32 * 100.0;
            let kernel = system as f32 / total as f32 * 100.0;
            (busy, kernel)
        })
        .collect()
}

/// Mach VM statistics: page counts.
#[derive(Debug, Clone, Copy)]
pub struct VmStats {
    pub page_size: u64,
    pub active: u64,
    pub inactive: u64,
    pub speculative: u64,
    pub wired: u64,
    pub compressed: u64,
    pub purgeable: u64,
    pub external: u64,
    pub faults: u64,
}

pub fn vm_stats() -> Option<VmStats> {
    let mut stats = MaybeUninit::<libc::vm_statistics64>::zeroed();
    let mut count = libc::HOST_VM_INFO64_COUNT;
    // SAFETY: `stats` has room for HOST_VM_INFO64_COUNT integers.
    let rc = unsafe {
        libc::host_statistics64(
            host_port(),
            libc::HOST_VM_INFO64,
            stats.as_mut_ptr().cast(),
            &raw mut count,
        )
    };
    if rc != libc::KERN_SUCCESS {
        return None;
    }
    // SAFETY: the call succeeded and filled the struct.
    let s = unsafe { stats.assume_init() };
    // SAFETY: sysconf has no preconditions.
    let page_size = u64::try_from(unsafe { libc::sysconf(libc::_SC_PAGESIZE) }).ok()?;
    Some(VmStats {
        page_size,
        active: u64::from(s.active_count),
        inactive: u64::from(s.inactive_count),
        speculative: u64::from(s.speculative_count),
        wired: u64::from(s.wire_count),
        compressed: u64::from(s.compressor_page_count),
        purgeable: u64::from(s.purgeable_count),
        external: u64::from(s.external_page_count),
        faults: s.faults,
    })
}

/// Swap usage from `sysctl vm.swapusage`.
pub fn swap_usage() -> Option<(u64, u64)> {
    let mut usage = MaybeUninit::<libc::xsw_usage>::zeroed();
    let mut len = size_of::<libc::xsw_usage>();
    // SAFETY: NUL-terminated name, out-buffer sized for the struct.
    let rc = unsafe {
        libc::sysctlbyname(
            c"vm.swapusage".as_ptr(),
            usage.as_mut_ptr().cast(),
            &raw mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    if rc != 0 {
        return None;
    }
    // SAFETY: sysctl succeeded.
    let u = unsafe { usage.assume_init() };
    Some((u.xsu_total, u.xsu_used))
}

/// A `sysctl` string (`machdep.cpu.brand_string`, `hw.model`, …).
pub fn sysctl_string(name: &std::ffi::CStr) -> Option<String> {
    let mut len = 0usize;
    // SAFETY: size query with a null buffer.
    if unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            std::ptr::null_mut(),
            &raw mut len,
            std::ptr::null_mut(),
            0,
        )
    } != 0
        || len == 0
    {
        return None;
    }
    let mut buf = vec![0u8; len];
    // SAFETY: buffer has `len` bytes.
    if unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            buf.as_mut_ptr().cast(),
            &raw mut len,
            std::ptr::null_mut(),
            0,
        )
    } != 0
    {
        return None;
    }
    let end = buf.iter().position(|&b| b == 0).unwrap_or(len);
    let s = String::from_utf8_lossy(&buf[..end]).trim().to_owned();
    (!s.is_empty()).then_some(s)
}

/// A numeric `sysctl` (`hw.cpufrequency_max`, `kern.boottime` aside).
pub fn sysctl_u64(name: &std::ffi::CStr) -> Option<u64> {
    let mut value = 0u64;
    let mut len = size_of::<u64>();
    // SAFETY: out-buffer of 8 bytes; 4-byte sysctls fill the low half.
    let rc = unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            (&raw mut value).cast(),
            &raw mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    if rc != 0 {
        return None;
    }
    Some(if len == 4 { value & 0xFFFF_FFFF } else { value })
}

/// A PID as the C int libproc wants; PIDs above `i32::MAX` do not exist.
fn pid_arg(pid: u32) -> Option<libc::c_int> {
    libc::c_int::try_from(pid).ok()
}

/// Per-process facts from `proc_pidinfo(PROC_PIDTASKINFO)`.
#[derive(Debug, Clone, Copy)]
pub struct TaskInfo {
    pub threads: u32,
    pub resident: u64,
}

pub fn task_info(pid: u32) -> Option<TaskInfo> {
    let mut info = MaybeUninit::<libc::proc_taskinfo>::zeroed();
    // Both structs are a few hundred bytes; the casts cannot truncate.
    let size = libc::c_int::try_from(size_of::<libc::proc_taskinfo>()).ok()?;
    // SAFETY: buffer sized for proc_taskinfo; the call returns bytes written.
    let n = unsafe {
        libc::proc_pidinfo(
            pid_arg(pid)?,
            libc::PROC_PIDTASKINFO,
            0,
            info.as_mut_ptr().cast(),
            size,
        )
    };
    if n != size {
        return None;
    }
    // SAFETY: fully written.
    let t = unsafe { info.assume_init() };
    Some(TaskInfo {
        threads: u32::try_from(t.pti_threadnum).unwrap_or(0),
        resident: t.pti_resident_size,
    })
}

/// Physical footprint (the "Memory" column of Activity Monitor) and cumulative
/// disk bytes, from `proc_pid_rusage(RUSAGE_INFO_V4)`.
#[derive(Debug, Clone, Copy)]
pub struct Rusage {
    pub footprint: u64,
    pub disk_read: u64,
    pub disk_written: u64,
}

pub fn rusage(pid: u32) -> Option<Rusage> {
    let mut info = MaybeUninit::<libc::rusage_info_v4>::zeroed();
    // SAFETY: buffer sized for rusage_info_v4, which the flavour requests.
    let rc = unsafe {
        libc::proc_pid_rusage(
            pid_arg(pid)?,
            libc::RUSAGE_INFO_V4,
            info.as_mut_ptr().cast(),
        )
    };
    if rc != 0 {
        return None;
    }
    // SAFETY: success fills the struct.
    let r = unsafe { info.assume_init() };
    Some(Rusage {
        footprint: r.ri_phys_footprint,
        disk_read: r.ri_diskio_bytesread,
        disk_written: r.ri_diskio_byteswritten,
    })
}

/// Process start time in microseconds since the Unix epoch, from
/// `PROC_PIDTBSDINFO`. Part of the process identity (with the PID), so a
/// recycled PID can never be mistaken for the process the user picked.
pub fn start_time_us(pid: u32) -> Option<u64> {
    let info = bsd_info(pid)?;
    Some(info.pbi_start_tvsec * 1_000_000 + info.pbi_start_tvusec)
}

/// BSD facts: parent, uid, status, nice.
pub fn bsd_info(pid: u32) -> Option<libc::proc_bsdinfo> {
    let mut info = MaybeUninit::<libc::proc_bsdinfo>::zeroed();
    let size = libc::c_int::try_from(size_of::<libc::proc_bsdinfo>()).ok()?;
    // SAFETY: buffer sized for proc_bsdinfo.
    let n = unsafe {
        libc::proc_pidinfo(
            pid_arg(pid)?,
            libc::PROC_PIDTBSDINFO,
            0,
            info.as_mut_ptr().cast(),
            size,
        )
    };
    if n != size {
        return None;
    }
    // SAFETY: fully written.
    Some(unsafe { info.assume_init() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wrapped_tick_counter_does_not_read_as_a_spike() {
        let before = [CoreTicks {
            user: u64::from(u32::MAX - 10),
            system: 0,
            idle: 0,
            nice: 0,
        }];
        let after = [CoreTicks {
            user: 10,
            system: 0,
            idle: 21,
            nice: 0,
        }];
        let shares = core_shares(&before, &after);
        let (busy, _) = shares[0];
        assert!((busy - 50.0).abs() < 0.1, "busy {busy}");
    }

    #[test]
    fn an_idle_interval_is_zero_busy_and_a_frozen_one_does_not_divide_by_zero() {
        let a = [CoreTicks::default()];
        let b = [CoreTicks {
            idle: 100,
            ..CoreTicks::default()
        }];
        assert_eq!(core_shares(&a, &b), vec![(0.0, 0.0)]);
        assert_eq!(core_shares(&a, &a), vec![(0.0, 0.0)]);
    }

    #[test]
    fn this_machine_reports_ticks_memory_and_its_own_process() {
        let ticks = core_ticks().expect("host_processor_info");
        assert!(!ticks.is_empty());
        assert!(ticks.iter().any(|t| t.total() > 0));
        let vm = vm_stats().expect("host_statistics64");
        assert!(vm.page_size >= 4096);
        let me = std::process::id();
        let task = task_info(me).expect("own task info");
        assert!(task.threads >= 1 && task.resident > 0);
        assert!(rusage(me).expect("own rusage").footprint > 0);
        assert!(start_time_us(me).expect("own start time") > 1_600_000_000_000_000);
        assert!(sysctl_string(c"kern.ostype").is_some());
    }
}
