//! The `StartupApproved` records that Task Manager writes.
//!
//! ## Why this matters more than it looks
//!
//! Disabling an item in Task Manager's Startup tab does **not** delete the
//! `Run` value or the shortcut. It leaves them exactly where they were and
//! records the decision in a parallel tree:
//!
//! ```text
//! HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run
//! HKLM\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run32
//! ...\StartupApproved\StartupFolder
//! ```
//!
//! A tool that reads only the `Run` keys therefore lists every item the user
//! has already switched off as "enabled". That is not a cosmetic error: it is
//! the single number the user is trying to reduce, reported wrongly, and it
//! is why several startup managers disagree with Task Manager.

use super::registry::{Hive, KEY_WOW64_64KEY, RegKey};
use super::{StartupSource, StartupState};

/// The `StartupApproved` sub-key that governs a given source.
///
/// Note that the folder entries share one key regardless of whether the
/// shortcut is per-user or all-users; the two are distinguished by which
/// hive the record lives in, not by the sub-key name.
pub(crate) const RUN: &str =
    r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";
pub(crate) const RUN32: &str =
    r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run32";
pub(crate) const FOLDER: &str =
    r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\StartupFolder";

/// Interprets a `StartupApproved` blob.
///
/// The value is `REG_BINARY`, normally twelve bytes: a state byte, three
/// bytes of padding, then a `FILETIME` recording when the user toggled it.
///
/// Only the low bit of byte zero decides the outcome. Observed states are
/// `02`/`06` for enabled and `03`/`07` for disabled — the upper bits vary by
/// who wrote the record (Explorer versus the Settings app) and carry no
/// meaning for us. Testing for equality with `02` therefore reports a `06`
/// entry as disabled, which is a real bug in more than one startup manager.
///
/// Returns `None` for an empty blob: absent data is not a decision, and the
/// caller must fall back to "enabled by default" only because the item's mere
/// presence in `Run` says so — not because we measured anything.
#[must_use]
pub fn interpret(blob: &[u8]) -> Option<StartupState> {
    let flags = *blob.first()?;

    if flags & 1 == 1 {
        Some(StartupState::Disabled)
    } else {
        Some(StartupState::Enabled)
    }
}

/// The moment the user last toggled the entry, as a raw `FILETIME`.
///
/// Returns `None` for a blob too short to contain one, and for a zero
/// timestamp — Explorer writes zeroes when it enables an item, so a zero is
/// "never explicitly toggled", not "toggled at the start of 1601".
#[must_use]
pub fn toggled_at(blob: &[u8]) -> Option<u64> {
    let bytes: [u8; 8] = blob.get(4..12)?.try_into().ok()?;
    match u64::from_le_bytes(bytes) {
        0 => None,
        ticks => Some(ticks),
    }
}

/// A loaded snapshot of every `StartupApproved` record.
///
/// Read once per sweep rather than re-opening the key for each entry: a
/// machine with forty startup items would otherwise open and close forty
/// registry keys to answer one boolean each.
#[derive(Debug, Default)]
pub struct ApprovalIndex {
    machine_run: Vec<(String, StartupState)>,
    machine_run32: Vec<(String, StartupState)>,
    user_run: Vec<(String, StartupState)>,
    machine_folder: Vec<(String, StartupState)>,
    user_folder: Vec<(String, StartupState)>,
}

impl ApprovalIndex {
    /// Loads every approval record available to the current user.
    ///
    /// Never fails: an absent `StartupApproved` key simply means nothing has
    /// been toggled on this machine, which is the state of a fresh install.
    #[must_use]
    pub fn load() -> Self {
        Self {
            machine_run: read(Hive::LocalMachine, RUN, KEY_WOW64_64KEY),
            // Run32 is a real sub-key name, not a redirection artefact: the
            // 64-bit hive holds the approvals for 32-bit Run entries under
            // this separate name. Reading it through the 32-bit view as
            // well would find nothing.
            machine_run32: read(Hive::LocalMachine, RUN32, KEY_WOW64_64KEY),
            user_run: read(Hive::CurrentUser, RUN, 0),
            machine_folder: read(Hive::LocalMachine, FOLDER, KEY_WOW64_64KEY),
            user_folder: read(Hive::CurrentUser, FOLDER, 0),
        }
    }

    /// The recorded state for an entry, if the user ever toggled it.
    ///
    /// `None` means no record exists. The caller must then treat the item as
    /// enabled — because it is present in a `Run` key, which is itself the
    /// evidence — rather than because this function said so.
    #[must_use]
    pub fn lookup(&self, source: StartupSource, name: &str) -> Option<StartupState> {
        let table = match source {
            StartupSource::MachineRun | StartupSource::MachineRunOnce => &self.machine_run,
            StartupSource::MachineRun32 | StartupSource::MachineRunOnce32 => &self.machine_run32,
            StartupSource::UserRun | StartupSource::UserRunOnce => &self.user_run,
            StartupSource::CommonStartupFolder => &self.machine_folder,
            StartupSource::UserStartupFolder => &self.user_folder,
            // Services and tasks carry their own enabled flag; Explorer
            // never records approvals for them.
            StartupSource::Service | StartupSource::ScheduledTask => return None,
        };

        table
            .iter()
            // Value names here are matched case-insensitively because
            // Explorer writes the shortcut's file name with whatever casing
            // the installer used, while the folder scan reports the casing
            // on disk. They differ often enough to matter.
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, state)| *state)
    }

    /// How many toggles were recorded, across all hives.
    #[must_use]
    pub fn len(&self) -> usize {
        self.machine_run.len()
            + self.machine_run32.len()
            + self.user_run.len()
            + self.machine_folder.len()
            + self.user_folder.len()
    }

    /// Whether no toggle has ever been recorded on this machine.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Reads one approval key, yielding nothing if it is absent or unreadable.
fn read(hive: Hive, path: &str, view: u32) -> Vec<(String, StartupState)> {
    let Ok(key) = RegKey::open(hive, path, view) else {
        return Vec::new();
    };

    key.values()
        .into_iter()
        .filter_map(|(name, _, data)| interpret(&data).map(|state| (name, state)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real twelve-byte record: state byte, padding, FILETIME.
    fn blob(state: u8) -> Vec<u8> {
        let mut v = vec![state, 0, 0, 0];
        v.extend_from_slice(&0x01DA_0000_0000_0000_u64.to_le_bytes());
        v
    }

    #[test]
    fn only_the_low_bit_decides_the_state() {
        // The bug this pins: comparing the whole byte against 0x02 reports
        // a 0x06 record — written by the Settings app rather than by
        // Explorer — as disabled, so the UI shows an item as off while it
        // runs at every logon.
        for enabled in [0x00, 0x02, 0x06, 0x08] {
            assert_eq!(
                interpret(&blob(enabled)),
                Some(StartupState::Enabled),
                "state byte {enabled:#04x} has its low bit clear and is enabled"
            );
        }
        for disabled in [0x01, 0x03, 0x07, 0x09] {
            assert_eq!(
                interpret(&blob(disabled)),
                Some(StartupState::Disabled),
                "state byte {disabled:#04x} has its low bit set and is disabled"
            );
        }
    }

    #[test]
    fn empty_blob_yields_no_opinion() {
        assert_eq!(
            interpret(&[]),
            None,
            "an empty value is not a decision; defaulting it to Enabled here \
             would hide the fact that we read nothing"
        );
    }

    #[test]
    fn a_single_byte_record_is_still_readable() {
        // Not all writers pad to twelve bytes. Requiring the full length
        // would drop these records entirely and report them as enabled.
        assert_eq!(interpret(&[0x03]), Some(StartupState::Disabled));
        assert_eq!(interpret(&[0x02]), Some(StartupState::Enabled));
    }

    #[test]
    fn timestamp_is_read_only_when_present_and_nonzero() {
        assert_eq!(toggled_at(&blob(0x03)), Some(0x01DA_0000_0000_0000));
        assert_eq!(
            toggled_at(&[0x02, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
            None,
            "Explorer zeroes the FILETIME when enabling; that is 'never', \
             not 1601-01-01"
        );
        assert_eq!(toggled_at(&[0x03]), None, "too short to hold a FILETIME");
    }

    #[test]
    fn services_and_tasks_have_no_approval_records() {
        let index = ApprovalIndex::default();
        assert_eq!(index.lookup(StartupSource::Service, "Spooler"), None);
        assert_eq!(index.lookup(StartupSource::ScheduledTask, r"\Foo"), None);
    }

    #[test]
    fn lookup_is_case_insensitive() {
        let index = ApprovalIndex {
            user_run: vec![("OneDrive".into(), StartupState::Disabled)],
            ..ApprovalIndex::default()
        };
        assert_eq!(
            index.lookup(StartupSource::UserRun, "onedrive"),
            Some(StartupState::Disabled),
            "Explorer and the Run key can disagree on casing; a case-sensitive \
             match would silently report a disabled item as enabled"
        );
    }

    #[test]
    fn run_once_shares_the_run_approval_table() {
        let index = ApprovalIndex {
            user_run: vec![("Setup".into(), StartupState::Disabled)],
            ..ApprovalIndex::default()
        };
        assert_eq!(
            index.lookup(StartupSource::UserRunOnce, "Setup"),
            Some(StartupState::Disabled)
        );
    }

    #[test]
    fn loading_the_real_index_does_not_panic_and_is_self_consistent() {
        let index = ApprovalIndex::load();
        // is_empty must agree with len, or a caller that guards on one and
        // indexes on the other will disagree with itself. The comparison is
        // the point of the test, so the len-zero lint is not applicable.
        #[allow(clippy::len_zero, reason = "asserting the two agree is the test")]
        let consistent = index.is_empty() == (index.len() == 0);
        assert!(consistent);
    }
}
