//! Process actions on macOS, behind the same command names as Windows.
//!
//! The frontend's `invoke` calls and the drift check see one set of commands;
//! only the bodies differ. macOS has no UAC, no critical-process flag and no
//! protected processes, so the risk plan is simpler: launchd and the kernel
//! are forbidden, root-owned processes are disruptive (the kernel will refuse
//! an unprivileged signal anyway), everything else is safe.

use serde::Deserialize;
use vitals_core::ids::{Pid, ProcessKey};
use vitals_core::provider::Priority;

use crate::commands::{ActionPlan, ActionRisk, CommandError};

type CommandResult<T> = std::result::Result<T, CommandError>;

fn plan(pid: u32, protected: bool) -> ActionPlan {
    let risk = if pid <= 1 {
        ActionRisk::Forbidden
    } else if protected {
        ActionRisk::Disruptive
    } else {
        ActionRisk::Safe
    };
    let consequence = match risk {
        ActionRisk::Safe => "process.confirm.safe",
        ActionRisk::Disruptive => "process.confirm.disruptive",
        ActionRisk::Critical => "process.confirm.critical",
        ActionRisk::Forbidden => "process.confirm.forbidden",
    };
    ActionPlan {
        risk,
        consequence: consequence.to_owned(),
        needs_confirmation: !matches!(risk, ActionRisk::Safe),
        // No elevation path exists in the macOS build.
        elevation_might_help: false,
    }
}

/// See the Windows implementation in `commands.rs`.
#[tauri::command]
// Tauri hands owned arguments; `name` is unused on macOS (no static list).
#[allow(clippy::needless_pass_by_value)]
pub fn plan_terminate_process(pid: u32, name: Option<String>, protected: bool) -> ActionPlan {
    let _ = name;
    plan(pid, protected)
}

/// See the Windows implementation in `commands.rs`.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn plan_suspend_process(pid: u32, name: Option<String>, protected: bool) -> ActionPlan {
    let _ = name;
    plan(pid, protected)
}

/// Ends a process (`SIGTERM`), after re-checking its identity.
#[tauri::command]
pub fn terminate_process(pid: u32, start_time: u64, confirmed: Option<bool>) -> CommandResult<()> {
    let _ = confirmed;
    vitals_macos::terminate(ProcessKey::new(Pid(pid), start_time))?;
    Ok(())
}

/// Stops a process (`SIGSTOP`).
#[tauri::command]
pub fn suspend_process(pid: u32, start_time: u64, confirmed: Option<bool>) -> CommandResult<()> {
    let _ = confirmed;
    vitals_macos::suspend(ProcessKey::new(Pid(pid), start_time))?;
    Ok(())
}

/// Continues a stopped process (`SIGCONT`).
#[tauri::command]
pub fn resume_process(pid: u32, start_time: u64) -> CommandResult<()> {
    vitals_macos::resume(ProcessKey::new(Pid(pid), start_time))?;
    Ok(())
}

/// Scheduling priority as the webview names it (same wire strings as on
/// Windows).
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PriorityDto {
    Idle,
    BelowNormal,
    Normal,
    AboveNormal,
    High,
    Realtime,
}

impl From<PriorityDto> for Priority {
    fn from(value: PriorityDto) -> Self {
        match value {
            PriorityDto::Idle => Self::Idle,
            PriorityDto::BelowNormal => Self::BelowNormal,
            PriorityDto::Normal => Self::Normal,
            PriorityDto::AboveNormal => Self::AboveNormal,
            PriorityDto::High => Self::High,
            PriorityDto::Realtime => Self::Realtime,
        }
    }
}

/// Changes a process's nice value.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn set_process_priority(pid: u32, start_time: u64, priority: PriorityDto) -> CommandResult<()> {
    vitals_macos::set_priority(ProcessKey::new(Pid(pid), start_time), priority.into())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launchd_is_forbidden_and_a_root_process_needs_confirmation() {
        assert!(matches!(plan(1, false).risk, ActionRisk::Forbidden));
        let root = plan(400, true);
        assert!(matches!(root.risk, ActionRisk::Disruptive));
        assert!(root.needs_confirmation);
        assert!(!plan(4000, false).needs_confirmation);
    }
}
