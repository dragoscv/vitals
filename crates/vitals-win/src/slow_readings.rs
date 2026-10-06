//! Readings too slow for the 1 Hz tick, refreshed on a background thread.
//!
//! Drive temperature and health are two IOCTLs per physical drive (a few
//! milliseconds each, more on a USB bridge that times out), and NVML loads
//! a DLL and initialises the driver. Either would take a visible bite out of
//! the 30 ms frame budget. They also change slowly: a drive's temperature
//! moves over minutes and its wear over months. So a thread refreshes them
//! every few seconds and the tick reads the last answer, which costs a
//! mutex and a clone.
//!
//! Before the first refresh lands every reading is absent — `None`, not a
//! guess — exactly as on a machine that cannot report them.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock, PoisonError};
use std::time::Duration;

use crate::hardware::{DriveReport, read_drive_reports};
use crate::sensors::{NvidiaGpu, read_nvidia_gpus};

/// How often the background thread refreshes.
const REFRESH: Duration = Duration::from_secs(5);

/// The last answers, keyed for the frame builder.
#[derive(Debug, Clone, Default)]
pub struct SlowReadings {
    /// By physical drive number.
    pub drives: HashMap<u32, DriveReport>,
    pub nvidia: Vec<NvidiaGpu>,
}

fn state() -> &'static Mutex<SlowReadings> {
    static STATE: OnceLock<Mutex<SlowReadings>> = OnceLock::new();
    STATE.get_or_init(|| {
        // The thread is started by the first reader, so a process that never
        // samples (the CLI's `ps`) never pays for it.
        // A failed spawn leaves every slow reading absent, which is what a
        // machine that cannot report them shows anyway.
        let _ = std::thread::Builder::new()
            .name("vitals-slow-readings".into())
            .spawn(|| {
                loop {
                    let next = read_now();
                    *state().lock().unwrap_or_else(PoisonError::into_inner) = next;
                    std::thread::sleep(REFRESH);
                }
            });
        Mutex::new(SlowReadings::default())
    })
}

/// Reads everything once, synchronously. For probes and tests.
#[must_use]
pub fn read_now() -> SlowReadings {
    SlowReadings {
        drives: read_drive_reports()
            .into_iter()
            .map(|report| (report.index, report))
            .collect(),
        nvidia: read_nvidia_gpus(),
    }
}

/// The most recent background answer; empty until the first one lands.
#[must_use]
pub fn latest() -> SlowReadings {
    state()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}
