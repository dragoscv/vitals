//! CPU package temperature, hottest core and package power from the
//! optional `vitals-sensors` service.
//!
//! The app never opens the driver. The service (crate `vitals-sensors`, run
//! as `LocalSystem`) owns `PawnIO` and publishes one JSON line per pipe
//! connection; this module is the reader, plus the pieces the Devices screen
//! needs to offer the install: whether `PawnIO` is present, the pinned
//! download, and the elevated run of the bundled helper.
//!
//! # Cheap enough for the sampling tick
//!
//! Unlike WMI, a pipe read is a local open and a 150-byte read — tens of
//! microseconds — so the 1 Hz sampler may call [`latest`]. When the service
//! is absent the open fails immediately, and the next attempt is deferred by
//! [`ABSENT_BACKOFF`] so a machine without it pays nothing measurable.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use vitals_core::error::{Error, Result};
use vitals_sensors::{
    HELPER_EXE, PAWNIO_SETUP_SHA256, PAWNIO_SETUP_URL, PAWNIO_UNINSTALL_KEY, Reading, helper_args,
    pawnio_version_ok, read_pipe, sha256_hex,
};

/// A reading is reused for this long; the service refreshes once a second.
const FRESH: Duration = Duration::from_millis(900);
/// After the pipe was not there, how long before asking again.
const ABSENT_BACKOFF: Duration = Duration::from_secs(10);

/// What the service measured. Each figure is independently optional.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct CpuSensors {
    pub package_celsius: Option<f32>,
    pub hottest_core_celsius: Option<f32>,
    pub package_watts: Option<f32>,
}

impl CpuSensors {
    fn from_reading(r: &Reading) -> Option<Self> {
        if !r.ok {
            return None;
        }
        // A reading outside physical range is a decoding fault, not a fact
        // about the CPU. Dropped rather than clamped: a clamped 0 °C or
        // 150 °C would be believed.
        let temp = |c: Option<f32>| c.filter(|c| c.is_finite() && (-20.0..=150.0).contains(c));
        let watts = r
            .package_w
            .filter(|w| w.is_finite() && (0.0..=2000.0).contains(w));
        Some(Self {
            package_celsius: temp(r.package_c),
            hottest_core_celsius: temp(r.hottest_core_c),
            package_watts: watts,
        })
    }

    /// The single "CPU temperature": the package sensor, else the hottest
    /// core (AMD reports only Tctl, Intel parts without a package sensor
    /// only cores).
    #[must_use]
    pub fn temperature(&self) -> Option<f32> {
        self.package_celsius.or(self.hottest_core_celsius)
    }
}

#[derive(Debug)]
struct Cache {
    at: Instant,
    value: Option<CpuSensors>,
    absent: bool,
}

static CACHE: Mutex<Option<Cache>> = Mutex::new(None);

/// The latest service reading, or `None` when the service is not installed,
/// not running, or cannot read this CPU.
#[must_use]
pub fn latest() -> Option<CpuSensors> {
    let mut guard = CACHE.lock().ok()?;
    if let Some(c) = guard.as_ref() {
        let ttl = if c.absent { ABSENT_BACKOFF } else { FRESH };
        if c.at.elapsed() < ttl {
            return c.value;
        }
    }
    let answer = read_pipe(Duration::ZERO);
    let absent = answer.is_err();
    let value = answer.ok().as_ref().and_then(CpuSensors::from_reading);
    *guard = Some(Cache {
        at: Instant::now(),
        value,
        absent,
    });
    value
}

/// Drops the cache, so a reading appears the moment an install finishes
/// instead of up to [`ABSENT_BACKOFF`] later.
pub fn forget() {
    if let Ok(mut g) = CACHE.lock() {
        *g = None;
    }
}

/// Everything the Devices screen needs to decide between a reading and an
/// install button.
#[derive(Debug, Clone, PartialEq)]
pub struct ServiceStatus {
    /// The service answered with a successful reading.
    pub running: bool,
    /// Why not, when it did not: the service's own error (unsupported CPU,
    /// driver missing) or the pipe error (not installed).
    pub error: Option<String>,
    /// The service is installed — it answered at all, even with an error.
    pub installed: bool,
    pub pawnio_installed: bool,
    pub reading: Option<CpuSensors>,
}

/// Asks the service once, with a short wait for a busy pipe.
#[must_use]
pub fn status() -> ServiceStatus {
    let answer = read_pipe(Duration::from_millis(300));
    let pawnio_installed = pawnio_version_ok(pawnio_version().as_deref());
    match answer {
        Ok(r) => ServiceStatus {
            running: r.ok,
            error: r.error.clone(),
            installed: true,
            pawnio_installed,
            reading: CpuSensors::from_reading(&r),
        },
        Err(e) => ServiceStatus {
            running: false,
            error: Some(e),
            installed: false,
            pawnio_installed,
            reading: None,
        },
    }
}

/// `DisplayVersion` of the `PawnIO` uninstall entry, if present.
#[must_use]
pub fn pawnio_version() -> Option<String> {
    use windows::Win32::System::Registry::{HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RegGetValueW};
    use windows::core::HSTRING;

    let mut buf = [0u16; 128];
    let mut len = u32::try_from(buf.len() * 2).unwrap_or(0);
    // SAFETY: the buffer and its byte length agree.
    let rc = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            &HSTRING::from(PAWNIO_UNINSTALL_KEY),
            &HSTRING::from("DisplayVersion"),
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr().cast()),
            Some(&raw mut len),
        )
    };
    if rc.is_err() {
        return None;
    }
    let n = (len as usize / 2).min(buf.len());
    Some(
        String::from_utf16_lossy(&buf[..n])
            .trim_end_matches('\0')
            .trim()
            .to_owned(),
    )
}

/// The first directory in `candidates` holding the helper.
#[must_use]
pub fn find_helper(candidates: &[PathBuf]) -> Option<PathBuf> {
    candidates
        .iter()
        .map(|d| d.join(HELPER_EXE))
        .find(|p| p.is_file())
}

/// Downloads the pinned `PawnIO` installer into a fresh temp directory and
/// checks its hash.
///
/// The hash is checked here so a tampered or truncated download fails before
/// a UAC prompt is shown for it, and again by the elevated helper on its own
/// copy, which is the check that actually protects the elevated run.
///
/// # Errors
///
/// [`Error::Os`] when the download fails; [`Error::Refused`] when the file
/// does not match the pin.
pub fn download_pawnio() -> Result<PathBuf> {
    use windows::Win32::System::Com::Urlmon::URLDownloadToFileW;
    use windows::core::HSTRING;

    let dir = std::env::temp_dir().join(format!("vitals-pawnio-{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("PawnIO_setup.exe");
    // urlmon rather than an HTTP crate: it is already in every Windows
    // install, honours the system proxy, and this is one file fetched once
    // in the app's life. A TLS stack in the sampler crate for it would be
    // the wrong trade.
    // SAFETY: both strings outlive the call; no callback object is passed.
    unsafe {
        URLDownloadToFileW(
            None,
            &HSTRING::from(PAWNIO_SETUP_URL),
            &HSTRING::from(path.as_os_str()),
            0,
            None,
        )
    }
    .map_err(|e| Error::Os {
        context: format!("downloading {PAWNIO_SETUP_URL}: {e}"),
        code: e.code().0,
    })?;
    let bytes = std::fs::read(&path)?;
    if sha256_hex(&bytes) != PAWNIO_SETUP_SHA256 {
        let _ = std::fs::remove_dir_all(&dir);
        return Err(Error::Refused(
            "the downloaded PawnIO installer does not match the pinned hash, so it was not run"
                .to_owned(),
        ));
    }
    Ok(path)
}

/// Installs (`install = true`) or removes the service, with one UAC prompt.
///
/// Returns the helper's exit code (`vitals_sensors::exit`).
///
/// # Errors
///
/// [`Error::Refused`] when the user dismisses the UAC prompt or the download
/// fails its pin; [`Error::Os`] when the helper cannot be started.
pub fn setup(helper: &Path, install: bool) -> Result<i32> {
    let needs_driver = install && !pawnio_version_ok(pawnio_version().as_deref());
    let installer = if needs_driver {
        Some(download_pawnio()?)
    } else {
        None
    };
    let declined = if install {
        "the sensors service was not installed"
    } else {
        "the sensors service was left installed"
    };
    let result = crate::actions::run_program_elevated(
        helper,
        &helper_args(install, installer.as_deref()),
        declined,
    );
    if let Some(dir) = installer.as_deref().and_then(Path::parent) {
        let _ = std::fs::remove_dir_all(dir);
    }
    forget();
    result.map(u32::cast_signed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(package: Option<f32>, core: Option<f32>, watts: Option<f32>) -> Reading {
        let mut r = Reading::success("intel", package, core);
        r.package_w = watts;
        r
    }

    #[test]
    fn a_failed_reading_is_no_reading_at_all() {
        assert_eq!(
            CpuSensors::from_reading(&Reading::failure("no driver")),
            None
        );
    }

    #[test]
    fn out_of_range_values_are_dropped_not_clamped() {
        let s = CpuSensors::from_reading(&ok(Some(400.0), Some(61.0), Some(-3.0))).expect("ok");
        assert_eq!(s.package_celsius, None);
        assert_eq!(s.hottest_core_celsius, Some(61.0));
        assert_eq!(s.package_watts, None);
    }

    #[test]
    fn the_cpu_temperature_falls_back_to_the_hottest_core() {
        let s = CpuSensors::from_reading(&ok(None, Some(70.0), None)).expect("ok");
        assert_eq!(s.temperature(), Some(70.0));
        let s = CpuSensors::from_reading(&ok(Some(55.0), Some(70.0), None)).expect("ok");
        assert_eq!(s.temperature(), Some(55.0), "package wins when present");
    }

    #[test]
    fn an_absent_service_reports_nothing_and_is_not_installed() {
        // Holds on any machine without the service; on one with it, the
        // status must at least be internally consistent.
        let s = status();
        if s.installed {
            assert_eq!(s.running, s.reading.is_some());
        } else {
            assert!(!s.running);
            assert!(s.reading.is_none());
            assert!(s.error.is_some(), "an absence must carry its reason");
        }
    }

    #[test]
    fn find_helper_picks_the_first_directory_that_has_it() {
        let dir = std::env::temp_dir().join(format!("vitals-helper-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(dir.join(HELPER_EXE), b"x").expect("write");
        let missing = dir.join("nope");
        let found = find_helper(&[missing, dir.clone()]);
        assert_eq!(found, Some(dir.join(HELPER_EXE)));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
