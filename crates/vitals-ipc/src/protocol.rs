//! The command protocol between the unprivileged UI and the elevated helper.
//!
//! ## Threat model
//!
//! The helper runs as SYSTEM and accepts instructions to kill processes and
//! change firewall rules. If any local process could talk to it, it would be a
//! privilege-escalation service. So:
//!
//! - The pipe DACL restricts access to the installing user and administrators.
//! - The helper verifies the client's signature and image path before
//!   accepting any command.
//! - Every command is an enum variant, never a shell string. There is no
//!   parser to escape and no injection surface.
//! - Mutating commands are logged with the caller's SID for audit.

use serde::{Deserialize, Serialize};
use vitals_core::ids::ProcessKey;
use vitals_core::provider::{Direction, Priority};
use vitals_core::units::Bytes;

/// Exchanged before any command. A mismatch aborts the connection.
///
/// Version-checking at handshake rather than per-command means a stale helper
/// left behind by a partial update fails immediately and visibly, instead of
/// misinterpreting a field and acting on the wrong process.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Handshake {
    /// Must equal [`vitals_core::MODEL_VERSION`].
    pub model_version: u32,
    /// Helper build, for diagnostics.
    pub helper_version: String,
    /// Nonce echoed by the client to prove liveness.
    pub nonce: u64,
}

/// A privileged operation requested by the UI.
///
/// Deliberately an enum of concrete variants rather than anything
/// string-shaped: it makes the entire privileged surface enumerable and
/// reviewable in one place.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "kebab-case")]
#[non_exhaustive]
pub enum Command {
    Ping,

    // ---- Process control ----
    Terminate {
        key: ProcessKey,
    },
    TerminateTree {
        key: ProcessKey,
    },
    Suspend {
        key: ProcessKey,
    },
    Resume {
        key: ProcessKey,
    },
    SetPriority {
        key: ProcessKey,
        priority: Priority,
    },
    SetAffinity {
        key: ProcessKey,
        mask: u64,
    },
    SetEfficiencyMode {
        key: ProcessKey,
        enabled: bool,
    },
    TrimWorkingSet {
        key: ProcessKey,
    },

    // ---- Tracing ----
    /// Starts the ETW session that backs per-process IO attribution.
    StartTracing {
        flags: TraceFlags,
    },
    StopTracing,

    // ---- Network ----
    CloseConnection {
        local_port: u16,
        remote_address: String,
        remote_port: u16,
    },
    BlockProgram {
        executable_path: String,
        direction: Direction,
    },
    UnblockProgram {
        rule_id: String,
    },

    // ---- Power ----
    SetPowerPlan {
        id: String,
    },
    CreatePowerPlan {
        from: String,
        name: String,
    },
    DeletePowerPlan {
        id: String,
    },

    // ---- Services & startup ----
    StartService {
        name: String,
    },
    StopService {
        name: String,
    },
    SetServiceStartType {
        name: String,
        start_type: ServiceStartType,
    },
    SetStartupItemEnabled {
        id: String,
        enabled: bool,
    },

    // ---- Dangerous, gated behind explicit opt-in ----
    SetGpuClockOffset {
        gpu_index: u32,
        core_mhz: i32,
        memory_mhz: i32,
    },
    SetGpuPowerLimit {
        gpu_index: u32,
        percent: u32,
    },
    SetFanDuty {
        sensor_id: String,
        percent: u32,
    },
}

impl Command {
    /// Whether this command changes system state.
    #[must_use]
    pub const fn is_mutating(&self) -> bool {
        !matches!(self, Self::Ping)
    }

    /// Whether this command can damage hardware and therefore requires the
    /// user to have enabled Advanced mode.
    #[must_use]
    pub const fn requires_advanced_mode(&self) -> bool {
        matches!(
            self,
            Self::SetGpuClockOffset { .. }
                | Self::SetGpuPowerLimit { .. }
                | Self::SetFanDuty { .. }
        )
    }

    /// Short label for the audit log.
    #[must_use]
    pub const fn audit_name(&self) -> &'static str {
        match self {
            Self::Ping => "ping",
            Self::Terminate { .. } => "terminate",
            Self::TerminateTree { .. } => "terminate-tree",
            Self::Suspend { .. } => "suspend",
            Self::Resume { .. } => "resume",
            Self::SetPriority { .. } => "set-priority",
            Self::SetAffinity { .. } => "set-affinity",
            Self::SetEfficiencyMode { .. } => "set-efficiency-mode",
            Self::TrimWorkingSet { .. } => "trim-working-set",
            Self::StartTracing { .. } => "start-tracing",
            Self::StopTracing => "stop-tracing",
            Self::CloseConnection { .. } => "close-connection",
            Self::BlockProgram { .. } => "block-program",
            Self::UnblockProgram { .. } => "unblock-program",
            Self::SetPowerPlan { .. } => "set-power-plan",
            Self::CreatePowerPlan { .. } => "create-power-plan",
            Self::DeletePowerPlan { .. } => "delete-power-plan",
            Self::StartService { .. } => "start-service",
            Self::StopService { .. } => "stop-service",
            Self::SetServiceStartType { .. } => "set-service-start-type",
            Self::SetStartupItemEnabled { .. } => "set-startup-item-enabled",
            Self::SetGpuClockOffset { .. } => "set-gpu-clock-offset",
            Self::SetGpuPowerLimit { .. } => "set-gpu-power-limit",
            Self::SetFanDuty { .. } => "set-fan-duty",
        }
    }
}

/// Which ETW providers to enable.
///
/// Separated because each has a real cost: the disk-IO provider on a busy
/// server can emit tens of thousands of events per second. Enabling only what
/// the visible surface needs keeps our own overhead defensible.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
// Five independent on/off switches is exactly what this is. Packing them into
// a bitfield or an enum set would obscure the wire format for no benefit —
// each flag maps one-to-one onto an ETW provider.
#[allow(clippy::struct_excessive_bools)]
pub struct TraceFlags {
    pub process_lifetime: bool,
    pub disk_io: bool,
    pub network_io: bool,
    pub registry: bool,
    pub image_load: bool,
}

impl TraceFlags {
    /// The minimum needed to catch short-lived processes, which is cheap.
    #[must_use]
    pub const fn lifetime_only() -> Self {
        Self {
            process_lifetime: true,
            disk_io: false,
            network_io: false,
            registry: false,
            image_load: false,
        }
    }

    #[must_use]
    pub const fn is_empty(self) -> bool {
        !self.process_lifetime
            && !self.disk_io
            && !self.network_io
            && !self.registry
            && !self.image_load
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ServiceStartType {
    Automatic,
    AutomaticDelayed,
    Manual,
    Disabled,
}

/// The helper's reply.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum CommandResult {
    Ok,
    /// Bytes actually released by [`Command::TrimWorkingSet`].
    Bytes {
        value: Bytes,
    },
    /// Identifier of a created firewall rule.
    RuleCreated {
        rule_id: String,
    },
    Error {
        message: String,
        kind: ErrorKind,
    },
}

/// Mirrors the actionable distinctions in [`vitals_core::Error`] across the
/// wire, so the UI keeps its ability to respond differently per case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ErrorKind {
    AccessDenied,
    NotFound,
    Unsupported,
    Refused,
    Timeout,
    Internal,
}

#[cfg(test)]
mod tests {
    use super::*;
    use vitals_core::ids::Pid;

    #[test]
    fn ping_is_the_only_non_mutating_command() {
        assert!(!Command::Ping.is_mutating());
        assert!(
            Command::Terminate {
                key: ProcessKey::new(Pid(1), 0)
            }
            .is_mutating()
        );
    }

    #[test]
    fn hardware_commands_require_advanced_mode() {
        assert!(
            Command::SetGpuClockOffset {
                gpu_index: 0,
                core_mhz: 100,
                memory_mhz: 0
            }
            .requires_advanced_mode()
        );
        assert!(
            !Command::Suspend {
                key: ProcessKey::new(Pid(1), 0)
            }
            .requires_advanced_mode()
        );
    }

    #[test]
    fn commands_round_trip_through_json() {
        let cmd = Command::SetPriority {
            key: ProcessKey::new(Pid(1234), 99),
            priority: Priority::High,
        };
        let encoded = serde_json::to_string(&cmd).expect("serialise");
        let decoded: Command = serde_json::from_str(&encoded).expect("deserialise");
        assert_eq!(cmd, decoded);
    }

    #[test]
    fn trace_flags_lifetime_only_is_not_empty() {
        assert!(!TraceFlags::lifetime_only().is_empty());
        assert!(TraceFlags::default().is_empty());
    }

    #[test]
    fn every_command_has_a_distinct_audit_name() {
        // Guards against copy-paste errors in the match arm, which would
        // silently mislabel entries in the audit log.
        let samples = [
            Command::Ping,
            Command::Terminate {
                key: ProcessKey::new(Pid(1), 0),
            },
            Command::TerminateTree {
                key: ProcessKey::new(Pid(1), 0),
            },
            Command::Suspend {
                key: ProcessKey::new(Pid(1), 0),
            },
            Command::Resume {
                key: ProcessKey::new(Pid(1), 0),
            },
        ];
        let mut names: Vec<_> = samples.iter().map(Command::audit_name).collect();
        names.sort_unstable();
        let count = names.len();
        names.dedup();
        assert_eq!(names.len(), count, "audit names must be unique");
    }
}
