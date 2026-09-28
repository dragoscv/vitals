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

/// Shows a proposal that stays on screen until answered.
///
/// `Alarm`, not the default or `Reminder`: with Do Not Disturb on (it was,
/// on the machine this was tested on) Windows shows banners only for
/// priority apps and alarms, and the second test proposal went silently to
/// the notification centre — useless in a freeze. An ordinary toast also
/// slides away after seven seconds. The alarm's looping sound is replaced by
/// the single default chime: this is a question, not an emergency.
pub fn propose(title: &str, lines: [&str; 2], buttons: &[Button], tx: Sender<Action>) {
    let mut toast = Toast::new(APP_ID)
        .title(title)
        .text1(lines[0])
        .text2(lines[1])
        .scenario(Scenario::Alarm)
        .sound(Some(Sound::Default))
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
