//! User sessions Tauri commands.
//!
//! Bridges the Windows backend to the webview via explicit DTOs. The frontend
//! types are generated from these structs, so changing a field here is a wire
//! contract change.

use serde::Serialize;

use crate::commands::CommandError;

type CommandResult<T> = std::result::Result<T, CommandError>;

/// A logon session DTO.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogonSessionDto {
    pub session_id: u32,
    pub user_name: Option<String>,
    pub domain: Option<String>,
    pub client_name: Option<String>,
    /// Translation key, not display text.
    pub state: &'static str,
    /// Unix timestamp in seconds, or null if unavailable.
    ///
    /// The Terminal Services API does not reliably expose logon time, and
    /// deriving it from the oldest process in the session is fragile. Reporting
    /// `null` is the honest answer when it cannot be obtained.
    pub logon_time: Option<u64>,
    pub is_services: bool,
}

/// Per-session resource rollup.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRollupDto {
    pub session_id: u32,
    pub process_count: usize,
    /// Sum of CPU percentages of all processes in this session.
    pub cpu_percent: f64,
    /// Sum of private bytes of all processes in this session.
    pub memory_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsersSnapshot {
    pub sessions: Vec<LogonSessionDto>,
    pub rollups: Vec<SessionRollupDto>,
}

/// Enumerates logon sessions.
///
/// Cheap enough to poll from the Users screen at the standard 1 Hz cadence.
/// Session state changes when a user disconnects or reconnects, not on a
/// sub-second interval, so faster polling would just produce the same result.
#[tauri::command]
#[cfg(windows)]
pub fn get_users() -> CommandResult<UsersSnapshot> {
    use vitals_win::users;

    // For now, return sessions with empty rollups. The rollup requires process
    // samples, and integrating that is deferred to a follow-up when the
    // sampler's output is made available to this command.
    let sessions = users::collect()?;

    Ok(UsersSnapshot {
        sessions: sessions
            .iter()
            .map(|session| LogonSessionDto {
                session_id: session.session_id,
                user_name: session.user_name.clone(),
                domain: session.domain.clone(),
                client_name: session.client_name.clone(),
                state: session_state(session.state),
                logon_time: session.logon_time.and_then(|time| {
                    time.duration_since(std::time::UNIX_EPOCH)
                        .ok()
                        .map(|d| d.as_secs())
                }),
                is_services: session.is_services(),
            })
            .collect(),
        rollups: Vec::new(),
    })
}

/// Maps session state to a translation key.
///
/// The same reasoning as `service_state` in `inventory.rs`: the raw value
/// inside `Unknown` is dropped because it is diagnostic detail, not something
/// the UI can present to the user in a translatable way.
#[cfg(windows)]
const fn session_state(state: vitals_win::users::SessionState) -> &'static str {
    use vitals_win::users::SessionState as S;
    match state {
        S::Active => "active",
        S::Connected => "connected",
        S::Disconnected => "disconnected",
        S::Idle => "idle",
        S::Listen => "listen",
        S::Shadow => "shadow",
        S::Unknown(_) => "unknown",
    }
}

/// Non-Windows fallback.
#[tauri::command]
#[cfg(not(windows))]
pub fn get_users() -> CommandResult<UsersSnapshot> {
    Ok(UsersSnapshot {
        sessions: Vec::new(),
        rollups: Vec::new(),
    })
}
