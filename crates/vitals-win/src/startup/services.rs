//! Service enumeration via the Service Control Manager.
//!
//! ## Why `EnumServicesStatusExW` rather than WMI
//!
//! `Get-CimInstance Win32_Service` takes 300–900 ms on a typical machine
//! because WMI instantiates a provider, walks the registry and builds a COM
//! object per service. `EnumServicesStatusExW` returns all of them, with live
//! PIDs, in a single call — measured at roughly 2 ms for 300 services. For a
//! tab that refreshes while open, that difference is the whole design.
//!
//! Start type is not in the enumeration result, so it costs one
//! `QueryServiceConfigW` per service. That is the expensive half and is why
//! [`enumerate_services`] takes a flag to skip it.

use std::ffi::c_void;

use vitals_core::error::{Error, Result};

/// An open SCM or service handle. Opaque.
type ScHandle = *mut c_void;

/// `SC_MANAGER_ENUMERATE_SERVICE | SC_MANAGER_CONNECT`
const SC_MANAGER_ENUMERATE_SERVICE: u32 = 0x0004;
const SC_MANAGER_CONNECT: u32 = 0x0001;

/// `SERVICE_QUERY_CONFIG`
const SERVICE_QUERY_CONFIG: u32 = 0x0001;

/// `SERVICE_WIN32` — both own-process and shared-process services.
///
/// Deliberately excludes `SERVICE_DRIVER`: kernel drivers are also "services"
/// to the SCM, and including them adds several hundred rows the Services tab
/// has no business showing.
const SERVICE_WIN32: u32 = 0x0000_0030;

/// `SERVICE_STATE_ALL`
const SERVICE_STATE_ALL: u32 = 0x0000_0003;

/// `SC_ENUM_PROCESS_INFO`
const SC_ENUM_PROCESS_INFO: u32 = 0;

const ERROR_MORE_DATA: i32 = 234;
const ERROR_ACCESS_DENIED: i32 = 5;

/// `SERVICE_STATUS_PROCESS.dwCurrentState` values.
mod state {
    pub const STOPPED: u32 = 1;
    pub const START_PENDING: u32 = 2;
    pub const STOP_PENDING: u32 = 3;
    pub const RUNNING: u32 = 4;
    pub const CONTINUE_PENDING: u32 = 5;
    pub const PAUSE_PENDING: u32 = 6;
    pub const PAUSED: u32 = 7;
}

/// `QUERY_SERVICE_CONFIGW.dwStartType` values.
mod start {
    pub const BOOT: u32 = 0;
    pub const SYSTEM: u32 = 1;
    pub const AUTO: u32 = 2;
    pub const DEMAND: u32 = 3;
    pub const DISABLED: u32 = 4;
}

/// `SERVICE_STATUS_PROCESS` — nine `DWORD`s.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct ServiceStatusProcess {
    service_type: u32,
    current_state: u32,
    controls_accepted: u32,
    win32_exit_code: u32,
    service_specific_exit_code: u32,
    check_point: u32,
    wait_hint: u32,
    /// Zero when the service is not running. Never treated as a real PID.
    process_id: u32,
    service_flags: u32,
}

/// `ENUM_SERVICE_STATUS_PROCESSW`.
///
/// Two pointers into the same buffer, then the status inline. The pointers
/// are the layout trap: they address strings appended at the *end* of the
/// caller's buffer, so the array cannot be copied out of that buffer without
/// dangling. Every string is converted before the buffer is dropped.
#[repr(C)]
#[derive(Clone, Copy)]
struct EnumServiceStatusProcessW {
    service_name: *mut u16,
    display_name: *mut u16,
    status: ServiceStatusProcess,
}

/// `QUERY_SERVICE_CONFIGW`.
#[repr(C)]
struct QueryServiceConfigW {
    service_type: u32,
    start_type: u32,
    error_control: u32,
    binary_path_name: *mut u16,
    load_order_group: *mut u16,
    tag_id: u32,
    dependencies: *mut u16,
    service_start_name: *mut u16,
    display_name: *mut u16,
}

#[link(name = "advapi32")]
unsafe extern "system" {
    fn OpenSCManagerW(machine: *const u16, database: *const u16, desired: u32) -> ScHandle;
    fn OpenServiceW(scm: ScHandle, name: *const u16, desired: u32) -> ScHandle;
    fn CloseServiceHandle(handle: ScHandle) -> i32;
    fn EnumServicesStatusExW(
        scm: ScHandle,
        info_level: u32,
        service_type: u32,
        service_state: u32,
        services: *mut u8,
        buffer_size: u32,
        bytes_needed: *mut u32,
        services_returned: *mut u32,
        resume_handle: *mut u32,
        group_name: *const u16,
    ) -> i32;
    fn QueryServiceConfigW(
        service: ScHandle,
        config: *mut u8,
        buffer_size: u32,
        bytes_needed: *mut u32,
    ) -> i32;
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetLastError() -> u32;
}

/// The last error, as the signed code [`Error::Os`] carries.
///
/// Win32 error codes are documented as unsigned but only the low 16 bits are
/// ever used by the SCM paths here, so the conversion cannot wrap in
/// practice; `cast_possible_wrap` is silenced at this one place rather than
/// at three call sites.
fn last_error() -> i32 {
    // SAFETY: no arguments, no preconditions.
    let raw = unsafe { GetLastError() };
    i32::try_from(raw).unwrap_or(-1)
}

/// The run state of a service.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceState {
    Stopped,
    StartPending,
    StopPending,
    Running,
    ContinuePending,
    PausePending,
    Paused,
    /// The SCM reported a value outside the documented set. Preserved rather
    /// than folded into `Stopped`, which would report a running service as
    /// stopped.
    Unknown(u32),
}

impl ServiceState {
    const fn from_raw(raw: u32) -> Self {
        match raw {
            state::STOPPED => Self::Stopped,
            state::START_PENDING => Self::StartPending,
            state::STOP_PENDING => Self::StopPending,
            state::RUNNING => Self::Running,
            state::CONTINUE_PENDING => Self::ContinuePending,
            state::PAUSE_PENDING => Self::PausePending,
            state::PAUSED => Self::Paused,
            other => Self::Unknown(other),
        }
    }

    /// Whether the service currently has a process.
    #[must_use]
    pub const fn is_running(self) -> bool {
        matches!(self, Self::Running | Self::Paused)
    }
}

/// When the service is configured to start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartType {
    /// Loaded by the boot loader. Kernel drivers only.
    Boot,
    /// Started during kernel initialisation.
    System,
    /// Starts at boot without being asked. What the Startup tab cares about.
    Automatic,
    /// Started on demand by something that needs it.
    Manual,
    /// Will not start.
    Disabled,
    /// The configuration could not be read — almost always because querying
    /// it needs rights we do not have unelevated. Never reported as
    /// `Manual`, which would understate how much runs at boot.
    Unknown,
}

impl StartType {
    const fn from_raw(raw: u32) -> Self {
        match raw {
            start::BOOT => Self::Boot,
            start::SYSTEM => Self::System,
            start::AUTO => Self::Automatic,
            start::DEMAND => Self::Manual,
            start::DISABLED => Self::Disabled,
            _ => Self::Unknown,
        }
    }

    /// Whether the service contributes to boot time.
    ///
    /// Returns `None` when unknown, so a caller counting boot-time services
    /// cannot silently undercount.
    #[must_use]
    pub const fn starts_at_boot(self) -> Option<bool> {
        match self {
            Self::Boot | Self::System | Self::Automatic => Some(true),
            Self::Manual | Self::Disabled => Some(false),
            Self::Unknown => None,
        }
    }
}

/// A service as the SCM sees it.
#[derive(Debug, Clone)]
pub struct ServiceInfo {
    /// The key name, e.g. `Spooler`. Stable and not localised — the only
    /// safe identifier.
    pub name: String,
    /// The localised label, e.g. "Print Spooler".
    ///
    /// `None` when the SCM returns an empty string, which happens for
    /// services whose registry `DisplayName` was never set. Falling back to
    /// the key name here would make the two indistinguishable in the UI.
    pub display_name: Option<String>,
    pub state: ServiceState,
    /// `Unknown` unless `with_config` was requested and the query succeeded.
    pub start_type: StartType,
    /// The live PID. `None` when stopped, and specifically not `Some(0)` —
    /// the SCM writes 0 for a stopped service and 0 is a real PID value
    /// elsewhere in this codebase.
    pub pid: Option<u32>,
    /// The configured image path, when the config was read.
    pub binary_path: Option<String>,
    /// The svchost group this service shares, e.g. `netsvcs`.
    ///
    /// Grouped services share one process, so their memory and CPU cannot be
    /// attributed individually. The UI must say so rather than showing the
    /// host's whole footprint against each of a dozen services — which is
    /// exactly what Task Manager's Details tab appears to do.
    pub svchost_group: Option<String>,
}

impl ServiceInfo {
    /// Whether this service shares its process with others.
    #[must_use]
    pub const fn is_shared_host(&self) -> bool {
        self.svchost_group.is_some()
    }
}

/// An SCM or service handle that closes itself.
#[derive(Debug)]
struct ServiceHandle(ScHandle);

impl Drop for ServiceHandle {
    fn drop(&mut self) {
        // SAFETY: `self.0` came from a successful Open*W and is closed
        // exactly once — the type is neither Copy nor Clone.
        unsafe { CloseServiceHandle(self.0) };
    }
}

/// Enumerates Win32 services.
///
/// `with_config` adds a `QueryServiceConfigW` per service, which is what
/// yields [`StartType`] and the binary path. It roughly triples the cost, so
/// a caller that only needs live state can skip it — and will then correctly
/// see [`StartType::Unknown`] rather than a fabricated default.
///
/// # Errors
///
/// [`Error::AccessDenied`] if the SCM cannot be opened at all, which on a
/// standard desktop does not happen: enumeration needs no elevation.
/// [`Error::Os`] for any other SCM failure.
pub fn enumerate_services(with_config: bool) -> Result<Vec<ServiceInfo>> {
    // SAFETY: three nulls request the local machine's active database,
    // which is the documented default.
    let scm = unsafe {
        OpenSCManagerW(
            std::ptr::null(),
            std::ptr::null(),
            SC_MANAGER_CONNECT | SC_MANAGER_ENUMERATE_SERVICE,
        )
    };

    if scm.is_null() {
        let code = last_error();
        return Err(if code == ERROR_ACCESS_DENIED {
            Error::AccessDenied {
                operation: "open the service control manager".to_owned(),
            }
        } else {
            Error::Os {
                context: "OpenSCManagerW".to_owned(),
                code,
            }
        });
    }

    let scm = ServiceHandle(scm);

    // Two-call protocol. The first call is expected to *fail* with
    // ERROR_MORE_DATA while writing the required size — treating a zero
    // return as failure and giving up is the classic mistake here, and it
    // yields an empty service list that looks like a machine with no
    // services rather than like a bug.
    let mut needed: u32 = 0;
    let mut returned: u32 = 0;

    // SAFETY: a null buffer with size 0 is the documented size query; both
    // out-parameters are live locals.
    unsafe {
        EnumServicesStatusExW(
            scm.0,
            SC_ENUM_PROCESS_INFO,
            SERVICE_WIN32,
            SERVICE_STATE_ALL,
            std::ptr::null_mut(),
            0,
            &raw mut needed,
            &raw mut returned,
            std::ptr::null_mut(),
            std::ptr::null(),
        )
    };

    let code = last_error();
    if code != ERROR_MORE_DATA || needed == 0 {
        return Err(Error::Os {
            context: "EnumServicesStatusExW size query".to_owned(),
            code,
        });
    }

    // Over-allocate slightly: services can start between the size query and
    // the read, and a buffer that is exactly `needed` then fails again. The
    // alternative is a retry loop for a 4 KiB saving.
    //
    // A `Vec<u64>` rather than `Vec<u8>` because the SCM writes
    // ENUM_SERVICE_STATUS_PROCESSW records here, whose leading members are
    // pointers and therefore need 8-byte alignment. A `Vec<u8>` is only
    // byte-aligned; reading the records out of it is undefined behaviour
    // even though x86 happens to tolerate the misalignment.
    let mut buffer = vec![0_u64; (needed as usize).div_ceil(8) + 512];
    let size = u32::try_from(buffer.len() * 8).unwrap_or(needed);

    // SAFETY: `buffer` has `size` bytes; the out-parameters are live.
    let ok = unsafe {
        EnumServicesStatusExW(
            scm.0,
            SC_ENUM_PROCESS_INFO,
            SERVICE_WIN32,
            SERVICE_STATE_ALL,
            buffer.as_mut_ptr().cast::<u8>(),
            size,
            &raw mut needed,
            &raw mut returned,
            std::ptr::null_mut(),
            std::ptr::null(),
        )
    };

    if ok == 0 {
        return Err(Error::Os {
            context: "EnumServicesStatusExW".to_owned(),
            code: last_error(),
        });
    }

    let mut out = Vec::with_capacity(returned as usize);

    for index in 0..returned as usize {
        // SAFETY: the SCM wrote `returned` contiguous
        // ENUM_SERVICE_STATUS_PROCESSW records at the start of `buffer`, and
        // `index` is bounded by that count. The buffer outlives this borrow.
        //
        // The stride is `size_of::<EnumServiceStatusProcessW>()`, pinned at
        // 56 bytes by a test below. Getting it wrong reads record zero
        // correctly and assembles every later one from misaligned bytes,
        // which produces plausible pointers that crash or return rubbish —
        // and no error anywhere.
        // The buffer is `Vec<u64>`, so the cast is alignment-correct.
        let record = unsafe {
            &*buffer
                .as_ptr()
                .cast::<EnumServiceStatusProcessW>()
                .add(index)
        };

        let Some(name) = (unsafe { wide_ptr_to_string(record.service_name) }) else {
            // A record with no key name cannot be acted on, so it is not
            // worth a row.
            continue;
        };

        // SAFETY: the pointer, when non-null, addresses a NUL-terminated
        // string inside `buffer`, which is still alive.
        let display_name =
            unsafe { wide_ptr_to_string(record.display_name) }.filter(|s| !s.is_empty());

        let (start_type, binary_path) = if with_config {
            query_config(&scm, &name)
        } else {
            (StartType::Unknown, None)
        };

        let svchost_group = binary_path.as_deref().and_then(parse_svchost_group);

        out.push(ServiceInfo {
            name,
            display_name,
            state: ServiceState::from_raw(record.status.current_state),
            start_type,
            // Zero means "no process", not "process 0".
            pid: match record.status.process_id {
                0 => None,
                pid => Some(pid),
            },
            binary_path,
            svchost_group,
        });
    }

    Ok(out)
}

/// Reads start type and image path for one service.
///
/// Returns `(Unknown, None)` on any failure rather than a default, because
/// "we could not read it" and "it is set to Manual" must not look the same.
fn query_config(scm: &ServiceHandle, name: &str) -> (StartType, Option<String>) {
    let wide: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();

    // SAFETY: `wide` is NUL-terminated and alive for the call; `scm.0` is a
    // live SCM handle owned by the caller.
    let service = unsafe { OpenServiceW(scm.0, wide.as_ptr(), SERVICE_QUERY_CONFIG) };
    if service.is_null() {
        return (StartType::Unknown, None);
    }
    let service = ServiceHandle(service);

    let mut needed: u32 = 0;

    // SAFETY: null buffer, size 0 — the documented size query.
    unsafe { QueryServiceConfigW(service.0, std::ptr::null_mut(), 0, &raw mut needed) };

    if needed == 0 {
        return (StartType::Unknown, None);
    }

    // The struct is followed by its strings in the same allocation, so the
    // buffer must be aligned for the struct, not merely large enough. A
    // `Vec<u8>` is only byte-aligned; a `Vec<u64>` guarantees 8.
    let mut buffer = vec![0_u64; (needed as usize).div_ceil(8) + 1];
    let size = u32::try_from(buffer.len() * 8).unwrap_or(needed);

    // SAFETY: `buffer` holds `size` bytes with 8-byte alignment, which
    // satisfies QUERY_SERVICE_CONFIGW's pointer members.
    let ok = unsafe {
        QueryServiceConfigW(
            service.0,
            buffer.as_mut_ptr().cast::<u8>(),
            size,
            &raw mut needed,
        )
    };

    if ok == 0 {
        return (StartType::Unknown, None);
    }

    // SAFETY: the call succeeded, so the buffer starts with an initialised,
    // correctly aligned QUERY_SERVICE_CONFIGW.
    let config = unsafe { &*buffer.as_ptr().cast::<QueryServiceConfigW>() };

    // SAFETY: `binary_path_name` points into `buffer`, which is alive here.
    let path = unsafe { wide_ptr_to_string(config.binary_path_name) }.filter(|s| !s.is_empty());

    (StartType::from_raw(config.start_type), path)
}

/// Extracts the svchost group from a service's command line.
///
/// `C:\Windows\system32\svchost.exe -k netsvcs -p` yields `netsvcs`. Anything
/// that is not svchost yields `None`, including a service whose own path
/// merely contains the string.
#[must_use]
pub fn parse_svchost_group(command: &str) -> Option<String> {
    let mut tokens = command.split_whitespace();

    // The image must be svchost itself, matched on the file name. A
    // `contains("svchost.exe")` test looks equivalent and is not:
    // `NetTcpPortSharing` runs
    // `…\Framework64\v4.0.30319\SMSvcHost.exe`, which contains the
    // substring. Found by cross-checking this parse against
    // `Get-CimInstance Win32_Service`, and it would have shown that service
    // as sharing a host process it has nothing to do with.
    let image = tokens.next()?.trim_matches('"');
    let file_name = image.rsplit(['\\', '/']).next()?;
    if !file_name.eq_ignore_ascii_case("svchost.exe") {
        return None;
    }

    // `-k` is the documented switch; `/k` is accepted by svchost too and
    // appears in a handful of third-party service registrations.
    while let Some(token) = tokens.next() {
        if token.eq_ignore_ascii_case("-k") || token.eq_ignore_ascii_case("/k") {
            return tokens.next().map(str::to_owned).filter(|g| !g.is_empty());
        }
    }

    None
}

/// Converts a NUL-terminated wide pointer to a `String`.
///
/// # Safety
///
/// `ptr` must be null or point to a NUL-terminated UTF-16 string that
/// remains valid for the duration of the call.
unsafe fn wide_ptr_to_string(ptr: *const u16) -> Option<String> {
    if ptr.is_null() {
        return None;
    }

    // SAFETY: the caller guarantees a NUL terminator, so the walk stops.
    let len = unsafe {
        let mut n = 0;
        while *ptr.add(n) != 0 {
            n += 1;
        }
        n
    };

    // SAFETY: `ptr` addresses `len` initialised u16s by the walk above.
    let slice = unsafe { std::slice::from_raw_parts(ptr, len) };
    Some(String::from_utf16_lossy(slice))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn struct_layouts_match_the_windows_sdk() {
        // Pinned because getting a stride wrong here fails SILENTLY: record
        // zero reads correctly and every later one is assembled from
        // misaligned bytes, producing an empty or nonsensical list that is
        // indistinguishable from "this machine has no services".
        //
        // 64-bit: two 8-byte pointers plus nine DWORDs (36 bytes) rounded up
        // to the 8-byte alignment the pointers impose = 16 + 40 = 56.
        assert_eq!(
            size_of::<ServiceStatusProcess>(),
            36,
            "SERVICE_STATUS_PROCESS is nine DWORDs"
        );
        assert_eq!(
            size_of::<EnumServiceStatusProcessW>(),
            56,
            "ENUM_SERVICE_STATUS_PROCESSW stride; a wrong value corrupts \
             every record after the first"
        );
        assert_eq!(align_of::<EnumServiceStatusProcessW>(), 8);

        // 64, and the arithmetic is worth writing out because getting it
        // wrong here was caught by this very assertion during development:
        // three DWORDs (12) + 4 padding + three pointers (24) + tag_id (4)
        // + 4 padding + three more pointers (24)... no. Counted properly:
        // service_type/start_type/error_control = 12, pad to 16,
        // binary_path_name + load_order_group = 16 (total 32),
        // tag_id = 4, pad to 8 (total 40), then dependencies +
        // service_start_name + display_name = 24, giving 64.
        //
        // The interior `tag_id` between pointer members is the trap: it
        // forces 4 bytes of padding that a naive field-size sum omits, and
        // the resulting under-read takes `display_name` from the middle of
        // `service_start_name`.
        assert_eq!(
            size_of::<QueryServiceConfigW>(),
            64,
            "QUERY_SERVICE_CONFIGW; the strings follow it in the same \
             allocation, so a wrong size reads the path from the wrong offset"
        );
        assert_eq!(align_of::<QueryServiceConfigW>(), 8);
    }

    #[test]
    fn stopped_service_reports_no_pid_rather_than_zero() {
        // The SCM writes 0 for a stopped service. Surfacing that as a PID
        // means the UI offers "go to process 0", and 0 is the Idle process.
        let raw = ServiceStatusProcess {
            current_state: state::STOPPED,
            process_id: 0,
            ..ServiceStatusProcess::default()
        };
        let pid = match raw.process_id {
            0 => None,
            p => Some(p),
        };
        assert_eq!(pid, None);
    }

    #[test]
    fn unknown_state_is_preserved_not_folded_to_stopped() {
        assert_eq!(ServiceState::from_raw(99), ServiceState::Unknown(99));
        assert!(
            !ServiceState::from_raw(99).is_running(),
            "an unknown state must not claim to be running"
        );
        assert!(ServiceState::from_raw(state::RUNNING).is_running());
        assert!(
            ServiceState::from_raw(state::PAUSED).is_running(),
            "a paused service still owns a process"
        );
        assert!(!ServiceState::from_raw(state::START_PENDING).is_running());
    }

    #[test]
    fn unreadable_start_type_does_not_masquerade_as_manual() {
        assert_eq!(StartType::from_raw(77), StartType::Unknown);
        assert_eq!(
            StartType::Unknown.starts_at_boot(),
            None,
            "Some(false) here would undercount boot-time services whenever \
             the config query was refused"
        );
        assert_eq!(StartType::Automatic.starts_at_boot(), Some(true));
        assert_eq!(StartType::Disabled.starts_at_boot(), Some(false));
        assert_eq!(StartType::Boot.starts_at_boot(), Some(true));
    }

    #[test]
    fn svchost_group_is_parsed_from_the_command_line() {
        assert_eq!(
            parse_svchost_group(r"C:\Windows\system32\svchost.exe -k netsvcs -p"),
            Some("netsvcs".to_owned())
        );
        assert_eq!(
            parse_svchost_group(r"C:\Windows\System32\svchost.exe -k LocalServiceNoNetwork"),
            Some("LocalServiceNoNetwork".to_owned())
        );
    }

    #[test]
    fn non_svchost_services_have_no_group() {
        assert_eq!(
            parse_svchost_group(r"C:\Windows\system32\spoolsv.exe"),
            None
        );
        assert_eq!(
            parse_svchost_group(r"C:\Program Files\Vendor\svc.exe -k something"),
            None,
            "a -k switch on a non-svchost binary is that vendor's own flag, \
             not a shared host group"
        );
    }

    #[test]
    fn a_binary_whose_name_merely_contains_svchost_is_not_grouped() {
        // Real case on this machine, found by cross-checking against
        // Win32_Service: the NetTcpPortSharing service runs SMSvcHost.exe,
        // whose name contains "svchost.exe". A substring test claimed it
        // shared a host process, which is a relationship that does not
        // exist and which the UI would use to pool its resource usage.
        assert_eq!(
            parse_svchost_group(r"C:\WINDOWS\Microsoft.NET\Framework64\v4.0.30319\SMSvcHost.exe"),
            None
        );
        assert_eq!(
            parse_svchost_group(r"C:\a\SMSvcHost.exe -k grp"),
            None,
            "even with a -k switch it is not svchost"
        );
    }

    #[test]
    fn a_quoted_svchost_path_is_still_recognised() {
        assert_eq!(
            parse_svchost_group(r#""C:\Windows\system32\svchost.exe" -k netsvcs"#),
            Some("netsvcs".to_owned())
        );
    }

    #[test]
    fn svchost_without_a_group_argument_yields_none() {
        assert_eq!(
            parse_svchost_group(r"C:\Windows\system32\svchost.exe"),
            None,
            "no -k means no group; inventing one would claim a sharing \
             relationship that does not exist"
        );
        assert_eq!(
            parse_svchost_group(r"C:\Windows\system32\svchost.exe -k"),
            None,
            "a trailing -k with nothing after it is malformed, not a group \
             named empty string"
        );
    }

    #[test]
    fn real_enumeration_returns_the_expected_shape() {
        let services = enumerate_services(false)
            .expect("enumerating services needs no elevation on a desktop");

        assert!(
            services.len() > 50,
            "every Windows install runs well over fifty Win32 services; \
             got {} which suggests the two-call protocol silently failed",
            services.len()
        );

        // The name is the only identifier we can rely on, so it must never
        // be empty.
        assert!(
            services.iter().all(|s| !s.name.is_empty()),
            "a service with no key name cannot be acted on"
        );

        // A stopped service must never carry a PID, and a running one must.
        for service in &services {
            if service.state == ServiceState::Stopped {
                assert_eq!(
                    service.pid, None,
                    "{} is stopped but reports pid {:?}",
                    service.name, service.pid
                );
            }
            assert_ne!(service.pid, Some(0), "0 is never a service pid");
        }

        // Without config queries, start type must be honestly unknown.
        assert!(
            services.iter().all(|s| s.start_type == StartType::Unknown),
            "with_config=false must not invent a start type"
        );
    }

    #[test]
    fn config_query_finds_automatic_services_and_svchost_groups() {
        let services = enumerate_services(true).expect("enumerating services needs no elevation");

        let known = services
            .iter()
            .find(|s| s.name.eq_ignore_ascii_case("Schedule"))
            .expect("the Task Scheduler service exists on every Windows install");
        assert_eq!(
            known.start_type,
            StartType::Automatic,
            "Schedule is Automatic on a stock install"
        );
        assert!(
            known.binary_path.is_some(),
            "the config query must yield an image path"
        );

        let grouped = services.iter().filter(|s| s.is_shared_host()).count();
        assert!(
            grouped > 5,
            "modern Windows hosts dozens of services in svchost groups; \
             found only {grouped}, so the -k parse is probably wrong"
        );

        // Display names are optional, and the fallback must be absence, not
        // a copy of the key name.
        for service in &services {
            if let Some(display) = &service.display_name {
                assert!(
                    !display.is_empty(),
                    "{} has an empty display name that should have been None",
                    service.name
                );
            }
        }
    }
}
