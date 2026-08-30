//! Serialises and deserialises app history to JSON.
//!
//! History is stored as a single JSON file. The format is deliberately simple
//! — a JSON array of records — so it is debuggable, and so a corrupted file
//! can be hand-repaired rather than becoming total data loss.

use std::fs;
use std::io;
use std::path::Path;

use super::record::{AppHistory, AppHistoryRecord};

/// Loads history from a JSON file, returning an empty history if the file does
/// not exist or is corrupt.
///
/// A missing file is not an error: on first run, there is no history yet. A
/// corrupt file is logged but not propagated — failing to start because of a
/// corrupt local file is a support trap that teaches users to delete their
/// data folder. Better to start fresh.
pub fn load(path: &Path) -> AppHistory {
    match load_inner(path) {
        Ok(history) => history,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            // No history yet. Normal on first run.
            AppHistory::new()
        }
        Err(e) => {
            // Corrupt file or permission error. Log it but return empty rather
            // than failing the app launch. The next save will overwrite the
            // broken file.
            eprintln!("Failed to load app history from {}: {}", path.display(), e);
            AppHistory::new()
        }
    }
}

fn load_inner(path: &Path) -> io::Result<AppHistory> {
    let json = fs::read_to_string(path)?;
    let mut records: Vec<AppHistoryRecord> =
        serde_json::from_str(&json).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    // Hydrate display names, which are skipped during serialisation.
    for record in &mut records {
        record.hydrate();
    }

    Ok(records
        .into_iter()
        .map(|r| (r.executable.clone(), r))
        .collect())
}

/// Saves history to a JSON file, pretty-printed for debuggability.
///
/// The parent directory is created if it does not exist. An error writing the
/// file is propagated — a save that silently fails teaches the user that
/// clearing history does nothing, which is worse than a visible failure.
pub fn save(path: &Path, history: &AppHistory) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let records: Vec<&AppHistoryRecord> = history.values().collect();
    let json = serde_json::to_string_pretty(&records)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    fs::write(path, json)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::SystemTime;

    use super::*;

    #[test]
    fn load_nonexistent_returns_empty() {
        let path = PathBuf::from("does-not-exist-12345.json");
        let history = load(&path);
        assert!(history.is_empty());
    }

    #[test]
    fn load_corrupt_returns_empty() {
        let temp_dir = std::env::temp_dir();
        let path = temp_dir.join("vitals-test-corrupt.json");
        fs::write(&path, b"not valid json {[}").expect("test fixture is well-formed");

        let history = load(&path);
        assert!(history.is_empty());

        fs::remove_file(&path).ok();
    }

    #[test]
    fn save_and_load_round_trip() {
        let temp_dir = std::env::temp_dir();
        let path = temp_dir.join("vitals-test-roundtrip.json");

        let mut history = AppHistory::new();
        let now = SystemTime::now();

        let exe = PathBuf::from(r"C:\test.exe");
        let mut record = AppHistoryRecord::new(exe.clone(), now);
        record.cpu_seconds = 123.45;
        record.disk_read_bytes = 1024;
        record.disk_write_bytes = 2048;
        record.peak_private_bytes = 4096;
        record.sessions = 5;

        history.insert(exe.clone(), record);

        save(&path, &history).expect("test fixture is well-formed");

        let loaded = load(&path);
        assert_eq!(loaded.len(), 1);

        let restored = loaded.get(&exe).expect("test fixture is well-formed");
        assert_eq!(restored.name, "test.exe");
        assert!((restored.cpu_seconds - 123.45).abs() < 0.01);
        assert_eq!(restored.disk_read_bytes, 1024);
        assert_eq!(restored.disk_write_bytes, 2048);
        assert_eq!(restored.peak_private_bytes, 4096);
        assert_eq!(restored.sessions, 5);

        fs::remove_file(&path).ok();
    }

    #[test]
    fn save_creates_parent_directory() {
        let temp_dir = std::env::temp_dir();
        let nested = temp_dir.join("vitals-test-nested-dir");
        let path = nested.join("history.json");

        // Ensure it does not exist before the test.
        fs::remove_dir_all(&nested).ok();

        let history = AppHistory::new();
        save(&path, &history).expect("test fixture is well-formed");

        assert!(path.exists());

        fs::remove_dir_all(&nested).ok();
    }

    #[test]
    fn save_overwrites_existing_file() {
        let temp_dir = std::env::temp_dir();
        let path = temp_dir.join("vitals-test-overwrite.json");

        let mut history = AppHistory::new();
        let now = SystemTime::now();

        let exe = PathBuf::from(r"C:\first.exe");
        history.insert(exe.clone(), AppHistoryRecord::new(exe, now));

        save(&path, &history).expect("test fixture is well-formed");

        let exe2 = PathBuf::from(r"C:\second.exe");
        let mut history2 = AppHistory::new();
        history2.insert(exe2.clone(), AppHistoryRecord::new(exe2, now));

        save(&path, &history2).expect("test fixture is well-formed");

        let loaded = load(&path);
        assert_eq!(loaded.len(), 1);
        assert!(loaded.contains_key(&PathBuf::from(r"C:\second.exe")));

        fs::remove_file(&path).ok();
    }
}
