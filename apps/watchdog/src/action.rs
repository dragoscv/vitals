//! What a notification button asks for.
//!
//! The button's argument is the only thing that comes back from the toast,
//! possibly minutes later, so it carries the full [`ProcessKey`]: a bare PID
//! could by then belong to something else entirely.

use vitals_core::ids::{Pid, ProcessKey};

/// The two parts of the session that can be restarted rather than ended.
///
/// On 2026-10-05 `explorer.exe` and `dwm.exe` hung three times while every
/// app kept running, and the only proposal was "Ignore": both are
/// session-critical, so ending them from a toast is refused. Restarting is
/// different — Windows brings both back, and the windows of every other
/// app survive — so it is offered for these two names and nothing else.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Shell {
    /// `explorer.exe`: taskbar, Start, desktop icons.
    Explorer,
    /// `dwm.exe`: draws every window. Runs as another account, so ending it
    /// needs administrator approval; Windows restarts it at once.
    Desktop,
}

impl Shell {
    /// Which shell part an image name is, if any.
    #[must_use]
    pub fn of(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "explorer.exe" => Some(Self::Explorer),
            "dwm.exe" => Some(Self::Desktop),
            _ => None,
        }
    }

    /// The image name, for finding the live process.
    #[must_use]
    pub const fn image(self) -> &'static str {
        match self {
            Self::Explorer => "explorer.exe",
            Self::Desktop => "dwm.exe",
        }
    }

    /// The `RegisterHotKey` id and virtual key of this part's hotkey.
    ///
    /// Ctrl+Alt+Shift because none of the three alone or in pairs is free:
    /// Ctrl+Alt+letter is `AltGr` on Romanian and many other layouts, so it
    /// would steal "ș" and "ț" from the person typing them.
    #[must_use]
    pub const fn hotkey(self) -> (i32, u32) {
        match self {
            // 'E' and 'D': virtual-key codes equal the upper-case ASCII.
            Self::Explorer => (1, 0x45),
            Self::Desktop => (2, 0x44),
        }
    }

    /// The part a `WM_HOTKEY` id belongs to.
    #[must_use]
    pub fn from_hotkey(id: i32) -> Option<Self> {
        [Self::Explorer, Self::Desktop]
            .into_iter()
            .find(|s| s.hotkey().0 == id)
    }

    /// How the hotkey reads to a person.
    #[must_use]
    pub const fn hotkey_text(self) -> &'static str {
        match self {
            Self::Explorer => "Ctrl+Alt+Shift+E",
            Self::Desktop => "Ctrl+Alt+Shift+D",
        }
    }
}

/// A process in `after` that was not in `before`: the shell Windows started
/// again.
///
/// Compared by key, not by name or count, because a folder window opened
/// "in a separate process" is another `explorer.exe` that survives the
/// restart and would otherwise read as the new shell.
#[must_use]
pub fn appeared(before: &[ProcessKey], after: &[ProcessKey]) -> Option<ProcessKey> {
    after.iter().copied().find(|k| !before.contains(k))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// End the process (and, for a tree target, everything below it).
    End(ProcessKey),
    /// Lower it (and everything below it) to below-normal priority.
    Lower(ProcessKey),
    /// Stop proposing this image name for a while.
    Ignore(ProcessKey),
    /// Restart a hung `explorer.exe` or `dwm.exe`.
    Restart(ProcessKey),
    /// A global hotkey was pressed. Never arrives from a toast: [`decode`]
    /// refuses it, so a toast argument cannot restart the shell unasked.
    ///
    /// [`decode`]: Self::decode
    Hotkey(Shell),
}

impl Action {
    #[must_use]
    pub fn encode(self) -> String {
        let (verb, key) = match self {
            Self::End(k) => ("end", k),
            Self::Lower(k) => ("lower", k),
            Self::Ignore(k) => ("ignore", k),
            Self::Restart(k) => ("restart", k),
            // Not a button; written only so the log reads naturally.
            Self::Hotkey(shell) => return format!("hotkey:{}", shell.image()),
        };
        format!("{verb}:{}:{}", key.pid.get(), key.start_time)
    }

    /// `None` for anything this process did not write, including a click on
    /// the body of the toast, which carries an empty argument.
    #[must_use]
    pub fn decode(arg: &str) -> Option<Self> {
        let mut parts = arg.split(':');
        let verb = parts.next()?;
        let pid: u32 = parts.next()?.parse().ok()?;
        let start: u64 = parts.next()?.parse().ok()?;
        if parts.next().is_some() {
            return None;
        }
        let key = ProcessKey::new(Pid(pid), start);
        match verb {
            "end" => Some(Self::End(key)),
            "lower" => Some(Self::Lower(key)),
            "ignore" => Some(Self::Ignore(key)),
            "restart" => Some(Self::Restart(key)),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_action_survives_the_round_trip_through_the_toast() {
        let key = ProcessKey::new(Pid(64_068), 134_050_000_000_000_000);
        for action in [
            Action::End(key),
            Action::Lower(key),
            Action::Ignore(key),
            Action::Restart(key),
        ] {
            assert_eq!(Action::decode(&action.encode()), Some(action));
        }
    }

    #[test]
    fn a_hotkey_can_never_arrive_through_a_toast_argument() {
        for shell in [Shell::Explorer, Shell::Desktop] {
            assert_eq!(Action::decode(&Action::Hotkey(shell).encode()), None);
        }
        assert_eq!(Action::decode("hotkey:1:2"), None);
    }

    #[test]
    fn anything_malformed_is_refused_rather_than_guessed() {
        for bad in [
            "",
            "end",
            "end:1",
            "end:x:1",
            "end:1:2:3",
            "kill:1:2",
            "END:1:2",
            "restart",
            "restart:1",
            "restart:1:x",
            "restart:1:2:3",
            "RESTART:1:2",
        ] {
            assert_eq!(Action::decode(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn only_explorer_and_dwm_can_be_restarted_whatever_their_case() {
        assert_eq!(Shell::of("Explorer.EXE"), Some(Shell::Explorer));
        assert_eq!(Shell::of("dwm.exe"), Some(Shell::Desktop));
        for other in ["csrss.exe", "winlogon.exe", "explorer", "sihost.exe", ""] {
            assert_eq!(Shell::of(other), None, "{other}");
        }
    }

    #[test]
    fn each_hotkey_id_maps_back_to_its_own_shell_part_and_no_other() {
        for shell in [Shell::Explorer, Shell::Desktop] {
            assert_eq!(Shell::from_hotkey(shell.hotkey().0), Some(shell));
            assert_eq!(Shell::of(shell.image()), Some(shell));
        }
        assert_ne!(Shell::Explorer.hotkey(), Shell::Desktop.hotkey());
        assert_eq!(Shell::from_hotkey(0), None);
        assert_eq!(Shell::from_hotkey(3), None);
    }

    #[test]
    fn a_folder_window_that_survives_the_restart_is_not_mistaken_for_the_new_shell() {
        let shell = ProcessKey::new(Pid(100), 1);
        let folder = ProcessKey::new(Pid(200), 2);
        let fresh = ProcessKey::new(Pid(100), 3);
        assert_eq!(appeared(&[shell, folder], &[folder]), None);
        assert_eq!(appeared(&[shell, folder], &[folder, fresh]), Some(fresh));
        assert_eq!(appeared(&[], &[fresh]), Some(fresh));
    }
}
