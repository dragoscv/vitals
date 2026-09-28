//! The lag watchdog, from the app's side: its settings file, its logon
//! entry, and the "Test sound" button (ADR-0032).
//!
//! The watchdog is its own process so that it keeps watching while Vitals is
//! closed. The app never talks to it directly: it writes
//! `%APPDATA%\Vitals\watchdog.json`, which the watchdog re-reads within a
//! second, and it owns the `HKCU\…\Run` value that starts the watchdog at
//! logon. The installer puts `vitals-watchdog.exe` in `watchdog\` beside
//! the app; the first launch after install registers and starts it, so a
//! fresh install is protected without anyone opening Settings.

use serde::{Deserialize, Serialize};

use crate::commands::CommandError;

type CommandResult<T> = std::result::Result<T, CommandError>;

/// Mirrors `apps/watchdog/src/config.rs`. Unknown fields are ignored on the
/// other side, so either can gain a field first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchdogConfig {
    pub enabled: bool,
    /// `relaxed` | `normal` | `sensitive`.
    pub sensitivity: String,
    pub sound: WatchdogSound,
    pub volume: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind", content = "value")]
pub enum WatchdogSound {
    Default,
    Silent,
    System(String),
    File(String),
}

/// What the Settings panel shows about the watchdog itself.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchdogStatus {
    /// This build ships the watchdog (always true for an install; a dev
    /// build without `cargo build -p vitals-watchdog` has none).
    pub available: bool,
    /// Windows starts it at logon.
    pub registered: bool,
    /// It is running now.
    pub running: bool,
}

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_VALUE: &str = "VitalsWatchdog";
const EXE: &str = "vitals-watchdog.exe";
const SENSITIVITIES: [&str; 3] = ["relaxed", "normal", "sensitive"];

fn config_path() -> Option<std::path::PathBuf> {
    std::env::var_os("APPDATA").map(|d| {
        std::path::PathBuf::from(d)
            .join("Vitals")
            .join("watchdog.json")
    })
}

/// Rejects what the watchdog would not understand, before it is written.
fn validate(config: &WatchdogConfig) -> CommandResult<()> {
    if !SENSITIVITIES.contains(&config.sensitivity.as_str()) {
        return Err(CommandError::Refused {
            message: format!("unknown sensitivity {:?}", config.sensitivity),
        });
    }
    if config.volume > 100 {
        return Err(CommandError::Refused {
            message: "volume is a percentage".into(),
        });
    }
    if let WatchdogSound::File(path) = &config.sound
        && !std::path::Path::new(path).is_file()
    {
        return Err(CommandError::Refused {
            message: format!("{path} is not a file"),
        });
    }
    Ok(())
}

/// Writes the settings file atomically: a temporary file then a rename, so
/// the watchdog never reads half of it.
fn write_config(config: &WatchdogConfig) -> CommandResult<()> {
    let path = config_path().ok_or_else(|| CommandError::Internal {
        message: "APPDATA is not set".into(),
    })?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(io)?;
    }
    let text = serde_json::to_string_pretty(config).map_err(|e| CommandError::Internal {
        message: e.to_string(),
    })?;
    let temp = path.with_extension("json.tmp");
    std::fs::write(&temp, text).map_err(io)?;
    std::fs::rename(&temp, &path).map_err(io)
}

#[allow(clippy::needless_pass_by_value)]
fn io(error: std::io::Error) -> CommandError {
    CommandError::Internal {
        message: error.to_string(),
    }
}

/// The watchdog shipped with this build.
///
/// `watchdog\` beside the app for an install (bundled as a resource by
/// `scripts/bundle-watchdog.ps1`; Tauri's resource dir on Windows is the
/// install dir); in a dev build the cargo target directory beside the app.
#[cfg(windows)]
fn bundled_exe() -> Option<std::path::PathBuf> {
    let dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    [dir.join("watchdog").join(EXE), dir.join(EXE)]
        .into_iter()
        .find(|p| p.is_file())
}

#[cfg(windows)]
fn registered_command() -> Option<String> {
    let out = std::process::Command::new("reg")
        .args(["query", &format!(r"HKCU\{RUN_KEY}"), "/v", RUN_VALUE])
        .creation_flags_hidden()
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .find(|l| l.trim_start().starts_with(RUN_VALUE))
        .and_then(|l| l.split("REG_SZ").nth(1))
        .map(|v| v.trim().to_owned())
}

#[cfg(windows)]
fn running() -> bool {
    use vitals_win::process::ProcessEnumerator;
    ProcessEnumerator::new().enumerate().is_ok_and(|all| {
        all.iter().any(|p| {
            p.name
                .as_deref()
                .is_some_and(|n| n.eq_ignore_ascii_case(EXE))
        })
    })
}

/// Hides the console window a child `reg`/`vitals-watchdog` would flash.
#[cfg(windows)]
trait Hidden {
    fn creation_flags_hidden(&mut self) -> &mut Self;
}

#[cfg(windows)]
impl Hidden for std::process::Command {
    fn creation_flags_hidden(&mut self) -> &mut Self {
        use std::os::windows::process::CommandExt;
        /// `CREATE_NO_WINDOW`.
        const NO_WINDOW: u32 = 0x0800_0000;
        self.creation_flags(NO_WINDOW)
    }
}

/// Registers the bundled watchdog at logon and starts it.
///
/// Registers the path it is installed at, so an update (which replaces the
/// file in place) needs nothing more. `--install` would copy it to
/// `%LOCALAPPDATA%` instead; that is for a watchdog run from a build, and
/// an installed one must stay where the uninstaller can remove it.
#[cfg(windows)]
fn enable() -> CommandResult<()> {
    let exe = bundled_exe().ok_or_else(|| CommandError::Unsupported {
        message: "this build does not include the watchdog".into(),
    })?;
    let command = format!("\"{}\"", exe.display());
    let status = std::process::Command::new("reg")
        .args([
            "add",
            &format!(r"HKCU\{RUN_KEY}"),
            "/v",
            RUN_VALUE,
            "/t",
            "REG_SZ",
            "/d",
            &command,
            "/f",
        ])
        .creation_flags_hidden()
        .status()
        .map_err(io)?;
    if !status.success() {
        return Err(CommandError::Internal {
            message: "could not write the logon entry".into(),
        });
    }
    if !running() {
        std::process::Command::new(&exe)
            .creation_flags_hidden()
            .spawn()
            .map_err(io)?;
    }
    Ok(())
}

/// Removes the logon entry. The running watchdog keeps going until logoff:
/// its own settings (`enabled: false`) already stop it proposing anything,
/// and ending it from here would need the same care as ending any process.
#[cfg(windows)]
fn disable() -> CommandResult<()> {
    if registered_command().is_none() {
        return Ok(());
    }
    let status = std::process::Command::new("reg")
        .args(["delete", &format!(r"HKCU\{RUN_KEY}"), "/v", RUN_VALUE, "/f"])
        .creation_flags_hidden()
        .status()
        .map_err(io)?;
    if status.success() {
        Ok(())
    } else {
        Err(CommandError::Internal {
            message: "could not remove the logon entry".into(),
        })
    }
}

/// The watchdog's state, for the Settings panel.
#[tauri::command]
#[cfg(windows)]
pub async fn get_watchdog_status() -> CommandResult<WatchdogStatus> {
    tauri::async_runtime::spawn_blocking(|| WatchdogStatus {
        available: bundled_exe().is_some(),
        registered: registered_command().is_some(),
        running: running(),
    })
    .await
    .map_err(|e| CommandError::Internal {
        message: e.to_string(),
    })
}

/// Saves the settings and makes the logon entry match `enabled`.
///
/// One command, so the switch in Settings and what Windows starts at logon
/// cannot disagree.
#[tauri::command]
#[cfg(windows)]
pub async fn set_watchdog_config(config: WatchdogConfig) -> CommandResult<WatchdogStatus> {
    validate(&config)?;
    write_config(&config)?;
    let enabled = config.enabled;
    tauri::async_runtime::spawn_blocking(move || if enabled { enable() } else { disable() })
        .await
        .map_err(|e| CommandError::Internal {
            message: e.to_string(),
        })??;
    get_watchdog_status().await
}

/// Plays `sound` exactly as a proposal would, through the watchdog itself.
///
/// So the test is honest: a file the watchdog cannot open fails here too,
/// rather than playing fine in the webview and silently at 3 a.m.
#[tauri::command]
#[cfg(windows)]
pub async fn test_watchdog_sound(sound: WatchdogSound, volume: u8) -> CommandResult<()> {
    let exe = bundled_exe().ok_or_else(|| CommandError::Unsupported {
        message: "this build does not include the watchdog".into(),
    })?;
    let config = WatchdogConfig {
        enabled: true,
        sensitivity: "normal".into(),
        sound: sound.clone(),
        volume,
    };
    validate(&config)?;
    let json = serde_json::to_string(&sound).map_err(|e| CommandError::Internal {
        message: e.to_string(),
    })?;
    // Waited for, off the IPC thread, so a file the watchdog cannot play
    // reaches the panel as an error. At most the ten-second sound cap.
    let out = tauri::async_runtime::spawn_blocking(move || {
        std::process::Command::new(exe)
            .args(["--play-sound", &json])
            .creation_flags_hidden()
            .output()
    })
    .await
    .map_err(|e| CommandError::Internal {
        message: e.to_string(),
    })?
    .map_err(io)?;
    if out.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&out.stderr);
    // The first line: the watchdog prints its error, then Rust's `main`
    // prints the same one again as `Error: …`.
    let reason = stderr
        .lines()
        .next()
        .unwrap_or_default()
        .trim()
        .trim_start_matches("error: ");
    Err(CommandError::Refused {
        message: if reason.is_empty() {
            "the watchdog could not play this sound".into()
        } else {
            reason.to_owned()
        },
    })
}

/// At launch: on the first one after an install, turn the watchdog on
/// unless the user has already decided ("decided" is the settings file
/// existing — it is only ever written from the Settings panel or here).
/// On every later one, if it is enabled but not running, start it again:
/// the installer ends it before replacing the file, so without this an
/// update would leave the machine unwatched until the next sign-in.
#[cfg(windows)]
pub fn ensure_default() {
    let Some(path) = config_path() else { return };
    if bundled_exe().is_none() {
        return;
    }
    if let Ok(text) = std::fs::read_to_string(&path) {
        let enabled = serde_json::from_str::<serde_json::Value>(&text)
            .ok()
            .and_then(|v| v.get("enabled").and_then(serde_json::Value::as_bool))
            .unwrap_or(false);
        if enabled
            && !running()
            && let Err(error) = enable()
        {
            tracing::warn!(?error, "could not restart the watchdog");
        }
        return;
    }
    let config = WatchdogConfig {
        enabled: true,
        sensitivity: "normal".into(),
        sound: WatchdogSound::Default,
        volume: 80,
    };
    let result = write_config(&config).and_then(|()| enable());
    if let Err(error) = result {
        tracing::warn!(?error, "could not turn the watchdog on at first launch");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(sensitivity: &str, sound: WatchdogSound) -> WatchdogConfig {
        WatchdogConfig {
            enabled: true,
            sensitivity: sensitivity.into(),
            sound,
            volume: 80,
        }
    }

    #[test]
    fn the_file_uses_the_exact_shape_the_watchdog_parses() {
        let text = serde_json::to_string(&config(
            "sensitive",
            WatchdogSound::File(r"C:\a.mp3".into()),
        ))
        .expect("serialises");
        assert_eq!(
            text,
            r#"{"enabled":true,"sensitivity":"sensitive","sound":{"kind":"file","value":"C:\\a.mp3"},"volume":80}"#
        );
        let unit = serde_json::to_string(&WatchdogSound::Default).expect("serialises");
        assert_eq!(unit, r#"{"kind":"default"}"#);
    }

    #[test]
    fn an_unknown_sensitivity_or_a_missing_file_is_refused_before_it_is_written() {
        assert!(validate(&config("extreme", WatchdogSound::Default)).is_err());
        assert!(
            validate(&config(
                "normal",
                WatchdogSound::File(r"Z:\no\such.wav".into())
            ))
            .is_err()
        );
        assert!(validate(&config("normal", WatchdogSound::System("Mail".into()))).is_ok());
        let mut loud = config("normal", WatchdogSound::Silent);
        loud.volume = 101;
        assert!(validate(&loud).is_err());
    }
}
