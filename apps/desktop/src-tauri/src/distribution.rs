//! How this copy of Vitals was installed: the direct installer, or the
//! Microsoft Store (MSIX).
//!
//! One binary serves both, so Store policy is applied at run time. Inside a
//! package three features are switched off, each for a concrete reason:
//!
//! * **self-update** — the Store updates packaged apps itself, and Store
//!   policy (10.2.5) forbids an app updating its own code;
//! * **the Task Manager hook** — it writes `HKLM\…\Image File Execution
//!   Options`, which MSIX virtualises, so the hook would silently do nothing;
//! * **the sensors service** — installing a SYSTEM service and a kernel driver
//!   from a packaged app is outside what the Store certifies.

use crate::commands::CommandError;

type CommandResult<T> = std::result::Result<T, CommandError>;

/// Whether this process runs inside an MSIX package. Cached: package identity
/// cannot change for the life of a process.
pub fn packaged() -> bool {
    #[cfg(windows)]
    {
        static PACKAGED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *PACKAGED.get_or_init(vitals_win::host::is_packaged)
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// What the UI needs to know to hide Store-forbidden affordances.
#[derive(Debug, Clone, Copy, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Distribution {
    /// Installed from the Microsoft Store: updates come from the Store, and
    /// the Task Manager hook and sensors service are unavailable.
    pub packaged: bool,
}

/// How this copy was installed. Desktop-only: a LAN client has no use for it.
#[tauri::command]
// A command must return `CommandResult` for one error contract across the boundary.
#[allow(clippy::unnecessary_wraps)]
pub fn get_distribution() -> CommandResult<Distribution> {
    Ok(Distribution {
        packaged: packaged(),
    })
}

/// The refusal every Store-forbidden command returns inside a package, so the
/// UI states a reason instead of a UAC prompt that leads nowhere.
pub fn store_refusal(feature: &str) -> CommandError {
    CommandError::Unsupported {
        message: format!("{feature} is not available in the Microsoft Store version"),
    }
}
