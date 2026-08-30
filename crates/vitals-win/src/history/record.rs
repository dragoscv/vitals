//! App history record types.
//!
//! These are the accumulated totals per executable, starting from whenever
//! Vitals first ran after being installed. Unlike Task Manager's "App history"
//! tab (which reads Windows' SRUM database), this is our own accumulation and
//! starts empty on a fresh install.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

/// Accumulated resource usage for a single executable.
///
/// Every counter is a running total from the moment Vitals first saw this
/// executable. Counters never decrease — if a process exits and starts again,
/// its next session adds to the previous total.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppHistoryRecord {
    /// Normalised executable path, used as the primary key.
    pub executable: PathBuf,
    /// Display name extracted from the path. Derived, not persisted.
    #[serde(skip)]
    pub name: String,
    /// Total CPU seconds consumed across all sessions.
    pub cpu_seconds: f64,
    /// Bytes read from disk across all sessions.
    pub disk_read_bytes: u64,
    /// Bytes written to disk across all sessions.
    pub disk_write_bytes: u64,
    /// Highest private bytes observed in any session.
    pub peak_private_bytes: u64,
    /// When Vitals first observed this executable.
    #[serde(with = "system_time_as_millis")]
    pub first_seen: SystemTime,
    /// When Vitals last observed this executable running.
    #[serde(with = "system_time_as_millis")]
    pub last_seen: SystemTime,
    /// Count of distinct sessions. A session ends when the process exits; a
    /// restart increments this.
    pub sessions: u32,
}

impl AppHistoryRecord {
    /// Creates a new record for an executable just observed for the first time.
    #[must_use]
    pub fn new(executable: PathBuf, now: SystemTime) -> Self {
        let name = Self::display_name(&executable);
        Self {
            executable,
            name,
            cpu_seconds: 0.0,
            disk_read_bytes: 0,
            disk_write_bytes: 0,
            peak_private_bytes: 0,
            first_seen: now,
            last_seen: now,
            sessions: 0,
        }
    }

    /// Extracts a human-readable name from the executable path.
    fn display_name(path: &Path) -> String {
        path.file_name()
            .and_then(|n| n.to_str())
            .map_or_else(|| path.display().to_string(), String::from)
    }

    /// Ensures the display name is populated after deserialisation.
    pub(crate) fn hydrate(&mut self) {
        if self.name.is_empty() {
            self.name = Self::display_name(&self.executable);
        }
    }
}

/// `SystemTime` as milliseconds since `UNIX_EPOCH`, for stable JSON.
mod system_time_as_millis {
    use std::time::{SystemTime, UNIX_EPOCH};

    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    // `SystemTime` is small enough that clippy would rather it were passed by
    // value, but serde fixes this signature: `serialize_with` always hands the
    // field by reference. The lint cannot be satisfied without breaking the
    // contract it is attached to.
    #[allow(clippy::trivially_copy_pass_by_ref)]
    pub fn serialize<S>(time: &SystemTime, s: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let millis = time
            .duration_since(UNIX_EPOCH)
            .map_err(serde::ser::Error::custom)?
            .as_millis();
        millis.serialize(s)
    }

    pub fn deserialize<'de, D>(d: D) -> Result<SystemTime, D::Error>
    where
        D: Deserializer<'de>,
    {
        let millis = u64::deserialize(d)?;
        Ok(UNIX_EPOCH + std::time::Duration::from_millis(millis))
    }
}

/// Normalises an executable path to a canonical identity.
///
/// Paths on Windows are case-insensitive, and the same program can appear with
/// different drive letter casing or path separators. Normalising prevents
/// double-counting the same executable under two spellings.
pub fn normalise_executable(path: &Path) -> PathBuf {
    // Drive letters and paths are case-insensitive on Windows. Converting to
    // lowercase ensures `C:\Foo.exe` and `c:\foo.exe` collapse to one key.
    let lower = path.display().to_string().to_lowercase();
    PathBuf::from(lower.replace('/', "\\"))
}

/// The complete history, keyed by normalised executable path.
pub type AppHistory = BTreeMap<PathBuf, AppHistoryRecord>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalise_collapses_case_variants() {
        let a = PathBuf::from(r"C:\Windows\System32\cmd.exe");
        let b = PathBuf::from(r"c:\windows\system32\CMD.EXE");

        assert_eq!(normalise_executable(&a), normalise_executable(&b));
    }

    #[test]
    fn normalise_unifies_separators() {
        let a = PathBuf::from(r"C:\Program Files\App\bin.exe");
        let b = PathBuf::from("C:/Program Files/App/bin.exe");

        assert_eq!(normalise_executable(&a), normalise_executable(&b));
    }

    #[test]
    fn display_name_extracts_filename() {
        let path = PathBuf::from(r"C:\Windows\System32\notepad.exe");
        let name = AppHistoryRecord::display_name(&path);
        assert_eq!(name, "notepad.exe");
    }

    #[test]
    fn display_name_handles_bare_name() {
        let path = PathBuf::from("explorer.exe");
        let name = AppHistoryRecord::display_name(&path);
        assert_eq!(name, "explorer.exe");
    }

    #[test]
    fn record_serialisation_round_trips() {
        let now = SystemTime::now();
        let record = AppHistoryRecord {
            executable: PathBuf::from(r"C:\test.exe"),
            name: "test.exe".into(),
            cpu_seconds: 123.45,
            disk_read_bytes: 1024,
            disk_write_bytes: 2048,
            peak_private_bytes: 4096,
            first_seen: now,
            last_seen: now,
            sessions: 5,
        };

        let json = serde_json::to_string(&record).expect("test fixture is well-formed");
        let mut decoded: AppHistoryRecord =
            serde_json::from_str(&json).expect("test fixture is well-formed");
        decoded.hydrate();

        assert_eq!(decoded.executable, record.executable);
        assert_eq!(decoded.name, "test.exe");
        assert!((decoded.cpu_seconds - record.cpu_seconds).abs() < 0.01);
        assert_eq!(decoded.disk_read_bytes, record.disk_read_bytes);
        assert_eq!(decoded.sessions, record.sessions);
    }
}
