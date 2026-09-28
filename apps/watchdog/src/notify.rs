//! Windows toast notifications with action buttons.
//!
//! An unpackaged exe can raise toasts once its `AppUserModelID` has a
//! `DisplayName` under `HKCU\Software\Classes\AppUserModelId` — no
//! installer, no Start-menu shortcut, no administrator. The button's
//! argument comes back through the `Activated` event while this process is
//! alive, which it always is: it is the watchdog.

use std::sync::mpsc::Sender;

use tauri_winrt_notification::{Duration, Scenario, Sound, Toast};
use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ, RegCloseKey,
    RegCreateKeyExW, RegSetValueExW,
};

use crate::action::Action;
use crate::config;
use crate::strings::xml_attr;

pub const APP_ID: &str = "app.vitals.watchdog";
const DISPLAY_NAME: &str = "Vitals Watchdog";

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Writes one `REG_SZ` under `HKCU\{path}`.
pub fn set_user_string(path: &str, name: &str, value: &str) -> bool {
    let path_w = wide(path);
    let name_w = wide(name);
    let value_w = wide(value);
    let mut key: HKEY = std::ptr::null_mut();
    // SAFETY: valid wide strings; `key` is written on success.
    let created = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            path_w.as_ptr(),
            0,
            std::ptr::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            std::ptr::null(),
            &raw mut key,
            std::ptr::null_mut(),
        )
    };
    if created != 0 {
        return false;
    }
    let bytes = u32::try_from(value_w.len() * 2).unwrap_or(0);
    // SAFETY: `key` is open for writing; the data is `bytes` long.
    let written = unsafe {
        RegSetValueExW(
            key,
            name_w.as_ptr(),
            0,
            REG_SZ,
            value_w.as_ptr().cast(),
            bytes,
        )
    };
    // SAFETY: opened above, closed once.
    unsafe { RegCloseKey(key) };
    written == 0
}

/// Registers the `AppUserModelID` so Windows shows our toasts.
pub fn register() -> bool {
    let path = format!(r"Software\Classes\AppUserModelId\{APP_ID}");
    set_user_string(&path, "DisplayName", DISPLAY_NAME)
}

/// A button: label and the action it sends back.
#[derive(Debug, Clone)]
pub struct Button {
    pub label: String,
    pub action: Action,
}

/// The crate's sound for a configured one.
///
/// The crate writes the `<audio>` element itself and offers no raw form, so
/// each configured sound is mapped onto its enum. Every system sound is
/// played once (`Single`), never looped: under the alarm scenario a missing
/// `<audio>` means the alarm's looping sound, so `Default` must be explicit.
fn toast_sound(sound: &config::Sound) -> Option<Sound> {
    use tauri_winrt_notification::LoopableSound;
    match sound {
        config::Sound::Silent | config::Sound::File(_) => None,
        config::Sound::Default => Some(Sound::Reminder),
        // A name this build does not offer (a newer app wrote it, or a hand
        // edit): the default chime, not silence.
        config::Sound::System(name) if !config::SYSTEM_SOUNDS.contains(&name.as_str()) => {
            Some(Sound::Reminder)
        }
        config::Sound::System(name) => Some(match name.as_str() {
            "IM" => Sound::IM,
            "Mail" => Sound::Mail,
            "Reminder" => Sound::Reminder,
            "SMS" => Sound::SMS,
            other => other
                .strip_prefix("Looping.")
                .and_then(|n| n.parse::<LoopableSound>().ok())
                .map_or(Sound::Reminder, Sound::Single),
        }),
    }
}

/// Shows a proposal that stays on screen until answered, with the user's
/// chosen sound.
///
/// `Alarm`, not the default or `Reminder`: with Do Not Disturb on (it was,
/// on the machine this was tested on) Windows shows banners only for
/// priority apps and alarms, and the second test proposal went silently to
/// the notification centre — useless in a freeze. An ordinary toast also
/// slides away after seven seconds.
pub fn propose(
    title: &str,
    lines: [&str; 2],
    buttons: &[Button],
    sound: &config::Sound,
    volume: u8,
    tx: Sender<Action>,
) {
    let mut toast = Toast::new(APP_ID)
        .title(title)
        .text1(lines[0])
        .text2(lines[1])
        .scenario(Scenario::Alarm)
        .sound(toast_sound(sound))
        .duration(Duration::Long);
    for b in buttons {
        toast = toast.add_button(&xml_attr(&b.label), &b.action.encode());
    }
    let toast = toast.on_activated(move |arg| {
        if let Some(action) = arg.as_deref().and_then(Action::decode) {
            let _ = tx.send(action);
        }
        Ok(())
    });
    if let Err(error) = toast.show() {
        tracing::warn!(%error, "proposal toast failed");
        return;
    }
    if let Some(path) = sound.file() {
        crate::win::play_file(path, volume);
    }
}

/// Shows a short result.
pub fn inform(title: &str) {
    if let Err(error) = Toast::new(APP_ID)
        .title(title)
        .duration(Duration::Short)
        .show()
    {
        tracing::warn!(%error, "result toast failed");
    }
}

/// A sample toast with the given sound, for the Settings "Test" button.
///
/// Returns why it could not, so the Test button in Settings reports a file
/// the watchdog cannot open instead of exiting 0 in silence. A file plays
/// on this thread, to the end or the ten-second cap.
pub fn sample(title: &str, sound: &config::Sound, volume: u8) -> anyhow::Result<()> {
    Toast::new(APP_ID)
        .title(title)
        .sound(toast_sound(sound))
        .duration(Duration::Short)
        .show()
        .map_err(|e| anyhow::anyhow!("the notification could not be shown: {e}"))?;
    if let Some(path) = sound.file() {
        crate::win::play_blocking(path, volume).map_err(|e| anyhow::anyhow!("{path}: {e}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn xml(sound: Option<Sound>) -> String {
        // The crate's own rendering is private; `Debug` on its enum is the
        // stable part we can compare against.
        format!("{sound:?}")
    }

    #[test]
    fn a_file_or_silence_leaves_the_toast_silent_so_nothing_plays_twice() {
        assert!(toast_sound(&config::Sound::Silent).is_none());
        assert!(toast_sound(&config::Sound::File("a.wav".into())).is_none());
    }

    #[test]
    fn every_offered_system_sound_maps_to_a_single_play_never_a_loop() {
        for name in config::SYSTEM_SOUNDS {
            let sound = toast_sound(&config::Sound::System((*name).to_owned()));
            let text = xml(sound);
            assert!(!text.contains("Loop("), "{name} -> {text}");
            if let Some(n) = name.strip_prefix("Looping.") {
                assert!(text.contains(n), "{name} -> {text}");
            }
        }
    }

    #[test]
    fn the_default_is_explicit_so_the_alarm_scenario_does_not_loop() {
        // A toast with no <audio> under scenario="alarm" plays the looping
        // alarm; the crate's `Sound::Default` emits exactly that.
        assert!(!matches!(
            toast_sound(&config::Sound::Default),
            Some(Sound::Default) | None
        ));
    }

    #[test]
    fn an_unknown_system_sound_falls_back_rather_than_going_silent() {
        assert!(toast_sound(&config::Sound::System("Looping.Nope".into())).is_some());
    }
}
