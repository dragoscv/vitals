//! Named-pipe server: one JSON line per connection, read-only.
//!
//! # A refresher thread, not a read per client
//!
//! codai's service reads the registers when a client asks and caches for a
//! second. Package power changes that: it is energy over an interval, and a
//! lazily-read counter would average over whatever gap the clients happened
//! to leave — one second for the sampler, five for the Devices screen, an
//! hour after the laptop slept. A fixed one-second refresher gives every
//! reading the same meaning, and clients only ever copy the latest line.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use vitals_sensors::{PIPE_NAME, Reading};
use windows::Win32::Foundation::{CloseHandle, ERROR_PIPE_CONNECTED, HANDLE, HLOCAL, LocalFree};
use windows::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};
use windows::Win32::Storage::FileSystem::{
    FILE_FLAG_FIRST_PIPE_INSTANCE, FlushFileBuffers, PIPE_ACCESS_OUTBOUND, WriteFile,
};
use windows::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_REJECT_REMOTE_CLIENTS,
    PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
};
use windows::core::HSTRING;

use crate::sensor::Sensor;

/// SYSTEM and Administrators full; interactive and authenticated users read
/// only. Outbound-only pipe, so "read" is all a client can do anyway — the
/// DACL is the second fence, and it also keeps a network logon out.
const PIPE_SDDL: &str = "D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;GR;;;IU)(A;;GR;;;AU)";
const REFRESH: Duration = Duration::from_secs(1);
/// How long to wait before retrying a driver that failed to open.
const REOPEN: Duration = Duration::from_secs(30);
/// A client that never reads would pin a writer thread in `FlushFileBuffers`;
/// beyond this many in flight, new connections are dropped without data.
const MAX_IN_FLIGHT: usize = 16;

type Latest = Arc<Mutex<Reading>>;

#[derive(Debug)]
pub struct Server {
    stop: Arc<AtomicBool>,
}

impl Server {
    pub fn new() -> Self {
        Self {
            stop: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn stop_flag(&self) -> Arc<AtomicBool> {
        self.stop.clone()
    }

    /// Serves until the stop flag is set (and [`poke`] wakes the listener).
    pub fn run(&self) -> Result<(), String> {
        let latest: Latest = Arc::new(Mutex::new(Reading::failure("starting")));
        let refresher = spawn_refresher(latest.clone(), self.stop.clone())?;

        let mut psd = PSECURITY_DESCRIPTOR::default();
        // SAFETY: valid SDDL string; `psd` is freed with LocalFree below.
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                &HSTRING::from(PIPE_SDDL),
                SDDL_REVISION_1,
                &raw mut psd,
                None,
            )
        }
        .map_err(|e| format!("pipe security descriptor: {e}"))?;
        let sa = SECURITY_ATTRIBUTES {
            nLength: u32::try_from(size_of::<SECURITY_ATTRIBUTES>()).unwrap_or(0),
            lpSecurityDescriptor: psd.0,
            bInheritHandle: false.into(),
        };
        let in_flight = Arc::new(AtomicUsize::new(0));

        let result = self.listen(&sa, &latest, &in_flight);
        // SAFETY: allocated by ConvertStringSecurityDescriptorToSecurityDescriptorW.
        unsafe {
            LocalFree(Some(HLOCAL(psd.0)));
        }
        self.stop.store(true, Ordering::SeqCst);
        let _ = refresher.join();
        result
    }

    fn listen(
        &self,
        sa: &SECURITY_ATTRIBUTES,
        latest: &Latest,
        in_flight: &Arc<AtomicUsize>,
    ) -> Result<(), String> {
        // The first instance must be ours (anti-squatting: another process
        // cannot pre-create the name and impersonate the service). After
        // that one instance is always alive, so the name cannot be taken.
        let mut current = create_instance(sa, true)?;
        loop {
            // SAFETY: `current` is a live server pipe handle.
            if let Err(e) = unsafe { ConnectNamedPipe(current, None) }
                && e.code() != ERROR_PIPE_CONNECTED.to_hresult()
            {
                close_handle(current);
                if self.stop.load(Ordering::SeqCst) {
                    return Ok(());
                }
                std::thread::sleep(Duration::from_millis(100));
                current = create_instance(sa, false)?;
                continue;
            }
            if self.stop.load(Ordering::SeqCst) {
                close_client(current);
                return Ok(());
            }
            let next = create_instance(sa, false)?;
            serve_client(current, latest, in_flight);
            current = next;
        }
    }
}

/// Reads the registers once a second into `latest`, reopening the driver on
/// a slow schedule when it fails.
fn spawn_refresher(
    latest: Latest,
    stop: Arc<AtomicBool>,
) -> Result<std::thread::JoinHandle<()>, String> {
    std::thread::Builder::new()
        .name("sensor-refresh".into())
        .spawn(move || {
            let mut sensor: Option<Sensor> = None;
            let mut last_open: Option<Instant> = None;
            let mut open_error = String::from("not opened yet");
            while !stop.load(Ordering::SeqCst) {
                if sensor.is_none() && last_open.is_none_or(|t| t.elapsed() >= REOPEN) {
                    last_open = Some(Instant::now());
                    match Sensor::open() {
                        Ok(s) => sensor = Some(s),
                        Err(e) => open_error = e,
                    }
                }
                let reading = match sensor.as_mut().map(Sensor::read) {
                    Some(Ok(r)) => r,
                    Some(Err(e)) => {
                        // Drop the device; it is reopened on the slow schedule.
                        sensor = None;
                        open_error.clone_from(&e);
                        Reading::failure(e)
                    }
                    None => Reading::failure(open_error.clone()),
                };
                if let Ok(mut g) = latest.lock() {
                    *g = reading;
                }
                std::thread::sleep(REFRESH);
            }
        })
        .map_err(|e| format!("spawn refresher: {e}"))
}

fn create_instance(sa: &SECURITY_ATTRIBUTES, first: bool) -> Result<HANDLE, String> {
    let mut mode = PIPE_ACCESS_OUTBOUND;
    if first {
        mode |= FILE_FLAG_FIRST_PIPE_INSTANCE;
    }
    // SAFETY: `sa` points to a live security descriptor.
    let h = unsafe {
        CreateNamedPipeW(
            &HSTRING::from(PIPE_NAME),
            mode,
            PIPE_TYPE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            PIPE_UNLIMITED_INSTANCES,
            4096,
            0,
            0,
            Some(&raw const *sa),
        )
    };
    if h.is_invalid() {
        return Err(format!(
            "CreateNamedPipeW {PIPE_NAME}: {}",
            windows::core::Error::from_thread()
        ));
    }
    Ok(h)
}

fn close_handle(h: HANDLE) {
    // SAFETY: `h` is a pipe handle owned by the caller, closed once.
    unsafe {
        let _ = CloseHandle(h);
    }
}

fn close_client(h: HANDLE) {
    // SAFETY: `h` is a connected server pipe handle owned by the caller.
    unsafe {
        let _ = DisconnectNamedPipe(h);
    }
    close_handle(h);
}

fn serve_client(h: HANDLE, latest: &Latest, in_flight: &Arc<AtomicUsize>) {
    if in_flight.fetch_add(1, Ordering::SeqCst) >= MAX_IN_FLIGHT {
        in_flight.fetch_sub(1, Ordering::SeqCst);
        close_client(h);
        return;
    }
    // A HANDLE is a raw pointer and not `Send`; it is moved as an integer
    // and owned by the spawned thread from here on.
    let raw = h.0 as usize;
    let mut line = latest.lock().map_or_else(
        |_| Reading::failure("internal: cache poisoned").to_json(),
        |r| r.to_json(),
    );
    line.push('\n');
    let counter = in_flight.clone();
    let spawned = std::thread::Builder::new()
        .name("pipe-client".into())
        .spawn(move || {
            let h = HANDLE(raw as *mut _);
            let mut written = 0u32;
            // SAFETY: `h` is a connected outbound pipe; the buffer is valid.
            unsafe {
                if WriteFile(h, Some(line.as_bytes()), Some(&raw mut written), None).is_ok() {
                    let _ = FlushFileBuffers(h);
                }
            }
            close_client(h);
            counter.fetch_sub(1, Ordering::SeqCst);
        });
    if spawned.is_err() {
        in_flight.fetch_sub(1, Ordering::SeqCst);
        close_client(h);
    }
}

/// Connects to our own pipe so a blocked `ConnectNamedPipe` returns and the
/// listener sees the stop flag.
pub fn poke() {
    for _ in 0..20 {
        if std::fs::File::open(PIPE_NAME).is_ok() {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}
