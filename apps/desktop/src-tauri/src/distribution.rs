//! How this copy of Vitals was installed: the direct installer, the
//! Microsoft Store (MSIX) or the Mac App Store.
//!
//! On Windows one binary serves both, so Store policy is applied at run time.
//! The Mac App Store build is a separate, sandboxed build (Cargo feature
//! `app-store`, ADR 0041), so there it is decided at compile time. Inside a
//! store three features are switched off, each for a concrete reason:
//!
//! * **self-update** — the Store updates packaged apps itself, and Store
//!   policy (10.2.5) forbids an app updating its own code;
//! * **the Task Manager hook** — it writes `HKLM\…\Image File Execution
//!   Options`, which MSIX virtualises, so the hook would silently do nothing;
//! * **the sensors service** — installing a SYSTEM service and a kernel driver
//!   from a packaged app is outside what the Store certifies.

use crate::commands::CommandError;

type CommandResult<T> = std::result::Result<T, CommandError>;

/// Whether this process runs inside a store package: an MSIX package on
/// Windows (cached: package identity cannot change for the life of a
/// process), or the sandboxed Mac App Store build.
pub fn packaged() -> bool {
    #[cfg(windows)]
    {
        static PACKAGED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *PACKAGED.get_or_init(vitals_win::host::is_packaged)
    }
    #[cfg(target_os = "macos")]
    {
        cfg!(feature = "app-store")
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        false
    }
}

/// Which store a packaged copy came from, for the words the UI shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Store {
    Microsoft,
    MacAppStore,
}

pub fn store() -> Option<Store> {
    if !packaged() {
        None
    } else if cfg!(target_os = "macos") {
        Some(Store::MacAppStore)
    } else {
        Some(Store::Microsoft)
    }
}

/// What the UI needs to know to hide Store-forbidden affordances.
#[derive(Debug, Clone, Copy, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Distribution {
    /// Installed from a store: updates come from the store, and the Task
    /// Manager hook, sensors service and watchdog are unavailable.
    pub packaged: bool,
    /// Which store, when packaged.
    pub store: Option<Store>,
}

/// How this copy was installed. Desktop-only: a LAN client has no use for it.
#[tauri::command]
// A command must return `CommandResult` for one error contract across the boundary.
#[allow(clippy::unnecessary_wraps)]
pub fn get_distribution() -> CommandResult<Distribution> {
    Ok(Distribution {
        packaged: packaged(),
        store: store(),
    })
}

/// The refusal every Store-forbidden command returns inside a package, so the
/// UI states a reason instead of a UAC prompt that leads nowhere.
pub fn store_refusal(feature: &str) -> CommandError {
    CommandError::Unsupported {
        message: match store() {
            Some(Store::MacAppStore) => {
                format!("{feature} is not available in the Mac App Store version")
            }
            _ => format!("{feature} is not available in the Microsoft Store version"),
        },
    }
}
