//! A local log file and a panic hook, so a crash leaves something to attach
//! to a bug report.
//!
//! The release profile uses `panic = "abort"` (ADR 0026): a panic ends the
//! process at once, with no unwinding and no console to print to. Without
//! this module a beta tester whose Vitals vanished had nothing to send.
//!
//! **Nothing here leaves the machine.** The files live under the app's data
//! directory, are overwritten rather than accumulated, and are only ever read
//! by a person who opens them. Vitals has no crash reporter and this is not
//! one.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// The log stops growing here. A monitor that runs for weeks must not fill a
/// disk with its own diary; the newest session matters, not the oldest.
const MAX_LOG_BYTES: u64 = 2 * 1024 * 1024;

/// Where the log and crash files go.
#[must_use]
pub fn log_dir() -> PathBuf {
    crate::state::data_dir().join("logs")
}

/// `vitals.log`, rotated once to `vitals.old.log` at launch when it is over
/// [`MAX_LOG_BYTES`]. Returns `None` if the directory cannot be created, in
/// which case logging goes to stderr only — never a reason to fail launch.
#[must_use]
pub fn open_log(dir: &Path) -> Option<File> {
    std::fs::create_dir_all(dir).ok()?;
    let path = dir.join("vitals.log");
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > MAX_LOG_BYTES) {
        let _ = std::fs::rename(&path, dir.join("vitals.old.log"));
    }
    OpenOptions::new().create(true).append(true).open(path).ok()
}

/// A writer for `tracing_subscriber` that appends to the log file until it
/// reaches the cap, then silently drops lines for the rest of the session.
#[derive(Debug)]
pub struct CappedLog {
    file: Mutex<Option<(File, u64)>>,
}

impl CappedLog {
    #[must_use]
    pub fn new(file: Option<File>) -> Self {
        let written = file
            .as_ref()
            .and_then(|f| f.metadata().ok())
            .map_or(0, |m| m.len());
        Self {
            file: Mutex::new(file.map(|f| (f, written))),
        }
    }

    fn write_line(&self, buf: &[u8]) {
        let Ok(mut guard) = self.file.lock() else {
            return;
        };
        if let Some((file, written)) = guard.as_mut()
            && *written + buf.len() as u64 <= MAX_LOG_BYTES * 2
            && file.write_all(buf).is_ok()
        {
            *written += buf.len() as u64;
        }
    }
}

/// The per-event handle `tracing_subscriber` asks for.
#[derive(Debug)]
pub struct CappedLogWriter<'a>(&'a CappedLog);

impl Write for CappedLogWriter<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.write_line(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for CappedLog {
    type Writer = CappedLogWriter<'a>;

    fn make_writer(&'a self) -> Self::Writer {
        CappedLogWriter(self)
    }
}

/// Installs a panic hook that writes `crash.txt` before the process aborts,
/// then runs the default hook (stderr, for a developer at a terminal).
///
/// `crash.txt` is overwritten each time: the last crash is the one worth
/// reporting, and an ever-growing file of old ones is data kept for no one.
pub fn install_panic_hook(dir: PathBuf) {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = write_crash(&dir, &crash_report(info));
        default(info);
    }));
}

fn crash_report(info: &std::panic::PanicHookInfo<'_>) -> String {
    let message = info
        .payload()
        .downcast_ref::<&str>()
        .map(|s| (*s).to_owned())
        .or_else(|| info.payload().downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "(no message)".to_owned());
    let location = info
        .location()
        .map_or_else(String::new, |l| format!("{}:{}", l.file(), l.line()));
    let thread = std::thread::current()
        .name()
        .unwrap_or("unnamed")
        .to_owned();
    let when = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let backtrace = std::backtrace::Backtrace::force_capture();
    format!(
        "Vitals {version} crashed\n\
         time (unix): {when}\n\
         os: {os} {arch}\n\
         thread: {thread}\n\
         at: {location}\n\
         message: {message}\n\n\
         {backtrace}\n",
        version = env!("CARGO_PKG_VERSION"),
        os = std::env::consts::OS,
        arch = std::env::consts::ARCH,
    )
}

fn write_crash(dir: &Path, report: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    std::fs::write(dir.join("crash.txt"), report)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("vitals-crashlog-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn an_oversized_log_is_rotated_at_launch_so_one_session_never_fills_a_disk() {
        let dir = temp_dir("rotate");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("vitals.log"),
            vec![b'x'; (MAX_LOG_BYTES + 1) as usize],
        )
        .unwrap();

        let file = open_log(&dir).unwrap();
        assert_eq!(
            file.metadata().unwrap().len(),
            0,
            "a fresh file after rotation"
        );
        assert!(dir.join("vitals.old.log").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_writer_stops_at_the_cap_instead_of_growing_forever() {
        let dir = temp_dir("cap");
        let log = CappedLog::new(open_log(&dir));
        let line = vec![b'y'; 64 * 1024];
        for _ in 0..200 {
            log.write_line(&line);
        }
        let len = std::fs::metadata(dir.join("vitals.log")).unwrap().len();
        assert!(len <= MAX_LOG_BYTES * 2, "log grew to {len} bytes");
        assert!(len > 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn only_the_last_crash_is_kept_rather_than_an_ever_growing_file() {
        let dir = temp_dir("crash");
        write_crash(&dir, "message: first\n").unwrap();
        write_crash(&dir, "message: boom\n").unwrap();
        let text = std::fs::read_to_string(dir.join("crash.txt")).unwrap();
        assert!(text.contains("boom"));
        assert!(!text.contains("first"), "{text}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
