//! Reads the `vitals-sensors` service pipe the way the app does, repeatedly,
//! and prints what came back — the prover for the pipe boundary.
//!
//! Fixtures cannot show that the real service's line survives the real
//! pipe: the first version failed every other read with OS error 233
//! (`ERROR_PIPE_NOT_CONNECTED`), which no unit test on either side could see.
//!
//! Run with: `cargo run -p vitals-win --example prove_sensors_service`
//! (install the service first: `vitals-sensors.exe install`, elevated).

#[cfg(windows)]
fn main() {
    use std::time::{Duration, Instant};
    use vitals_win::sensors::cpu_service;

    let s = cpu_service::status();
    println!(
        "installed={} running={} pawnio={}",
        s.installed, s.running, s.pawnio_installed
    );
    if let Some(e) = &s.error {
        println!("error: {e}");
    }

    let runs = 20;
    let mut ok = 0;
    let mut failures = Vec::new();
    let started = Instant::now();
    for _ in 0..runs {
        match vitals_sensors::read_pipe(Duration::from_millis(300)) {
            Ok(r) if r.ok => ok += 1,
            Ok(r) => failures.push(r.error.unwrap_or_default()),
            Err(e) => failures.push(e),
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    println!(
        "pipe reads: {ok}/{runs} ok in {:.1} s",
        started.elapsed().as_secs_f64()
    );
    for f in failures.iter().take(3) {
        println!("  failure: {f}");
    }

    cpu_service::forget();
    let t = Instant::now();
    let latest = cpu_service::latest();
    println!(
        "latest() {:.3} ms -> {latest:?}",
        t.elapsed().as_secs_f64() * 1000.0
    );
    if let Some(c) = latest {
        println!(
            "CPU {:?} °C (package {:?}, hottest core {:?}), package {:?} W",
            c.temperature(),
            c.package_celsius,
            c.hottest_core_celsius,
            c.package_watts
        );
    }
}

#[cfg(not(windows))]
fn main() {
    println!("windows only");
}
