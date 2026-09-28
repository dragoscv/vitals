//! What the user chose in Settings, read from `%APPDATA%\Vitals\watchdog.json`.
//!
//! Written by the desktop app, read here. A file rather than IPC because the
//! watchdog must obey the settings while Vitals is closed, and a file is the
//! one thing both processes can see without either running. Re-read when its
//! modification time changes, so a change in Settings applies within a
//! second, without a restart.
//!
//! Every field has a default and unknown fields are ignored, so an older
//! watchdog reading a newer file (or the reverse, after a rollback) keeps
//! working instead of refusing to start.

use serde::{Deserialize, Serialize};

/// How readily a slowdown counts as one worth interrupting for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Sensitivity {
    /// Only a clear freeze: a window unanswered for half a second.
    Relaxed,
    #[default]
    Normal,
    /// Shorter stalls count too, for someone who wants to hear early.
    Sensitive,
}

/// The thresholds a sensitivity stands for.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Thresholds {
    /// A foreground window that takes this long to answer a message is
    /// stalling the person using it.
    pub window_ms: f64,
    /// A Normal-priority thread woken this late means the scheduler is
    /// starved: the cursor, input and every ordinary app are waiting too.
    pub scheduler_ms: f64,
    /// Hard page faults a second, machine-wide, with memory nearly full:
    /// the disk is standing in for RAM and everything that touches memory
    /// waits for it.
    pub hard_faults: f64,
}

impl Sensitivity {
    #[must_use]
    pub const fn thresholds(self) -> Thresholds {
        match self {
            Self::Relaxed => Thresholds {
                window_ms: 500.0,
                scheduler_ms: 100.0,
                hard_faults: 2_000.0,
            },
            Self::Normal => Thresholds {
                window_ms: 250.0,
                scheduler_ms: 50.0,
                hard_faults: 1_000.0,
            },
            Self::Sensitive => Thresholds {
                window_ms: 120.0,
                scheduler_ms: 30.0,
                hard_faults: 500.0,
            },
        }
    }
}

/// The notification sound.
///
/// The system names map to `ms-winsoundevent:` URIs, which a toast plays by
/// itself. A file cannot be: unpackaged apps may only use `ms-appx:` and
/// `ms-winsoundevent:` in toast audio (Microsoft Learn, "App notification
/// content — Audio"), so for a file the toast is silent and the watchdog
/// plays it.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind", content = "value")]
pub enum Sound {
    /// Windows' own notification sound.
    #[default]
    Default,
    Silent,
    /// A named system sound; see [`SYSTEM_SOUNDS`].
    System(String),
    /// An audio file anywhere on disk.
    File(String),
}

/// The system sounds offered, as `ms-winsoundevent:Notification.*` names.
pub const SYSTEM_SOUNDS: &[&str] = &[
    "IM",
    "Mail",
    "Reminder",
    "SMS",
    "Looping.Alarm",
    "Looping.Alarm2",
    "Looping.Alarm3",
    "Looping.Alarm4",
    "Looping.Alarm5",
    "Looping.Call",
    "Looping.Call2",
    "Looping.Call3",
];

impl Sound {
    /// The file the watchdog must play itself, if any.
    #[must_use]
    pub fn file(&self) -> Option<&str> {
        match self {
            Self::File(path) if !path.trim().is_empty() => Some(path),
            _ => None,
        }
    }
}

/// The whole settings file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Config {
    /// Off means: keep running (so turning it back on is instant) but never
    /// propose anything.
    pub enabled: bool,
    pub sensitivity: Sensitivity,
    pub sound: Sound,
    /// 0..=100, for a custom file. The toast's own sounds follow the Windows
    /// notification volume and cannot be scaled.
    pub volume: u8,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: true,
            sensitivity: Sensitivity::Normal,
            sound: Sound::Default,
            volume: 80,
        }
    }
}

impl Config {
    /// Parses the file, falling back to defaults for anything unreadable.
    ///
    /// A half-written file (the app is mid-save) or a hand edit with a typo
    /// must not stop the watchdog: the previous config stays in force if the
    /// caller keeps it, and a first read falls back to defaults.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        serde_json::from_str(text).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_field_takes_its_default_and_an_unknown_one_is_ignored() {
        let config = Config::parse(r#"{"sensitivity":"relaxed","futureThing":1}"#).expect("parses");
        assert_eq!(config.sensitivity, Sensitivity::Relaxed);
        assert!(config.enabled);
        assert_eq!(config.sound, Sound::Default);
    }

    #[test]
    fn a_broken_file_is_refused_rather_than_read_as_defaults() {
        // The caller keeps the config it had; turning a half-written save
        // into "enabled, default sound" would undo the user's choice.
        assert_eq!(Config::parse(r#"{"enabled":fal"#), None);
    }

    #[test]
    fn every_sound_shape_round_trips_through_the_file_the_app_writes() {
        for sound in [
            Sound::Default,
            Sound::Silent,
            Sound::System("Looping.Alarm2".into()),
            Sound::File(r"C:\Users\me\Music\ding.mp3".into()),
        ] {
            let config = Config {
                sound: sound.clone(),
                ..Config::default()
            };
            let text = serde_json::to_string(&config).expect("serialises");
            assert_eq!(Config::parse(&text).map(|c| c.sound), Some(sound), "{text}");
        }
    }

    #[test]
    fn the_file_uses_the_shape_the_settings_screen_sends() {
        let text = r#"{"enabled":false,"sensitivity":"sensitive","sound":{"kind":"file","value":"D:\\a.wav"},"volume":40}"#;
        let config = Config::parse(text).expect("parses");
        assert!(!config.enabled);
        assert_eq!(config.sensitivity, Sensitivity::Sensitive);
        assert_eq!(config.sound.file(), Some(r"D:\a.wav"));
        assert_eq!(config.volume, 40);
    }

    #[test]
    fn only_a_non_empty_file_path_is_played_by_the_watchdog() {
        assert_eq!(Sound::File("  ".into()).file(), None);
        assert_eq!(Sound::System("IM".into()).file(), None);
        assert_eq!(Sound::File("a.mp3".into()).file(), Some("a.mp3"));
    }

    #[test]
    fn each_sensitivity_is_stricter_than_the_one_below_it() {
        let [r, n, s] = [
            Sensitivity::Relaxed,
            Sensitivity::Normal,
            Sensitivity::Sensitive,
        ]
        .map(Sensitivity::thresholds);
        assert!(r.window_ms > n.window_ms && n.window_ms > s.window_ms);
        assert!(r.scheduler_ms > n.scheduler_ms && n.scheduler_ms > s.scheduler_ms);
        assert!(r.hard_faults > n.hard_faults && n.hard_faults > s.hard_faults);
    }
}
