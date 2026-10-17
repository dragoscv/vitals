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

/// Reloads the main window when its `WebView2` renderer process dies.
///
/// The renderer is a separate process; when it is killed (by the user's own
/// task manager, by the OS reclaiming memory, by a GPU driver reset) the
/// window stays up and shows nothing, while the sampler, tray and alerts run
/// on behind it. A blank window in a monitor reads as "Vitals is broken",
/// and the only fix was to quit from the tray and relaunch. A reload brings
/// it back in a second with the same backend state.
///
/// Only a render-process *exit* reloads. An unresponsive renderer may be
/// mid-way through real work and a reload would throw that away; the
/// browser process exiting means the webview itself is gone and cannot be
/// reloaded. Both are logged.
#[cfg(windows)]
pub fn reload_on_renderer_exit(app: &tauri::AppHandle) {
    use tauri::Manager;
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let result = window.with_webview(|platform| {
        if let Err(error) = renderer::attach(&platform.controller()) {
            tracing::warn!(%error, "could not watch the webview's renderer process");
        }
    });
    if let Err(error) = result {
        tracing::warn!(%error, "could not reach the webview to watch its renderer");
    }
}

#[cfg(windows)]
mod renderer {
    use std::time::{Duration, Instant};

    use webview2_com::Microsoft::Web::WebView2::Win32::{
        COREWEBVIEW2_PROCESS_FAILED_KIND, COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED,
        ICoreWebView2Controller,
    };
    use webview2_com::ProcessFailedEventHandler;

    pub(super) fn attach(controller: &ICoreWebView2Controller) -> webview2_com::Result<()> {
        // SAFETY: a live controller handed over by Tauri on the UI thread,
        // which is the thread WebView2 requires for both calls.
        let webview = unsafe { controller.CoreWebView2() }?;
        let mut budget = ReloadBudget::default();
        let handler = ProcessFailedEventHandler::create(Box::new(move |sender, args| {
            let (Some(sender), Some(args)) = (sender, args) else {
                return Ok(());
            };
            // Not a real kind: if the read fails, `?` returns before use.
            let mut kind = COREWEBVIEW2_PROCESS_FAILED_KIND(-1);
            // SAFETY: `args` is valid for the duration of the callback and
            // `kind` is a live out-parameter.
            unsafe { args.ProcessFailedKind(&raw mut kind) }?;
            if kind != COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED {
                tracing::warn!(kind = kind.0, "a webview process failed; not reloading");
                return Ok(());
            }
            if budget.allow(Instant::now()) {
                tracing::warn!("the webview renderer exited; reloading the window");
                // SAFETY: the sender is the live webview this event is for,
                // on its UI thread.
                unsafe { sender.Reload() }?;
            } else {
                tracing::warn!("the webview renderer keeps exiting; leaving it stopped");
            }
            Ok(())
        }));
        let mut token = 0_i64;
        // SAFETY: as above; the token is a live out-parameter. The handler
        // stays registered for the life of the webview, so the token is
        // never needed to remove it.
        unsafe { webview.add_ProcessFailed(&handler, &raw mut token) }?;
        Ok(())
    }

    /// At most [`ReloadBudget::MAX`] reloads per [`ReloadBudget::WINDOW`].
    ///
    /// A renderer that dies on load — a bad driver, a page that crashes
    /// it — would otherwise reload forever, a crash loop burning a core
    /// behind a flickering window.
    #[derive(Debug, Default)]
    pub(super) struct ReloadBudget {
        recent: Vec<Instant>,
    }

    impl ReloadBudget {
        const MAX: usize = 3;
        const WINDOW: Duration = Duration::from_secs(60);

        pub(super) fn allow(&mut self, now: Instant) -> bool {
            self.recent
                .retain(|at| now.duration_since(*at) < Self::WINDOW);
            if self.recent.len() >= Self::MAX {
                return false;
            }
            self.recent.push(now);
            true
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn a_renderer_that_keeps_dying_is_reloaded_three_times_a_minute_and_no_more() {
            let start = Instant::now();
            let mut budget = ReloadBudget::default();
            assert!(budget.allow(start));
            assert!(budget.allow(start + Duration::from_secs(1)));
            assert!(budget.allow(start + Duration::from_secs(2)));
            assert!(!budget.allow(start + Duration::from_secs(3)));
            // A minute after the first reload exactly one slot has freed
            // (the reloads at 1 s and 2 s are still inside the window).
            assert!(budget.allow(start + Duration::from_millis(60_500)));
            assert!(!budget.allow(start + Duration::from_millis(60_600)));
        }
    }
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
