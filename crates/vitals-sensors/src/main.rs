//! `vitals-sensors.exe` — read-only CPU temperature, package power and board
//! fan speed service.
//!
//! Commands: `service` | `read` | `install [--pawnio-setup <path>]` |
//! `uninstall` | `version`.

#[cfg(windows)]
mod install;
#[cfg(windows)]
mod pawnio;
#[cfg(windows)]
mod sensor;
#[cfg(windows)]
mod server;
#[cfg(windows)]
mod service;
#[cfg(windows)]
mod superio;

const USAGE: &str =
    "usage: vitals-sensors <service|read|install [--pawnio-setup <path>]|uninstall|version>";

#[cfg(not(windows))]
fn main() {
    let _ = USAGE;
    println!("vitals-sensors: windows only");
}

#[cfg(windows)]
fn main() {
    std::process::exit(run(&std::env::args().skip(1).collect::<Vec<_>>()));
}

#[cfg(windows)]
fn run(args: &[String]) -> i32 {
    use vitals_sensors::exit;

    let fail = |f: install::Failure| {
        eprintln!("vitals-sensors: {}", f.message);
        f.code
    };
    match args.first().map(String::as_str) {
        Some("version" | "--version" | "-V") => {
            println!("vitals-sensors {}", env!("CARGO_PKG_VERSION"));
            exit::OK
        }
        Some("service") => match service::run_dispatcher() {
            Ok(()) => exit::OK,
            Err(e) => {
                eprintln!("vitals-sensors: {e}");
                exit::FAILED
            }
        },
        Some("read") => read_once(),
        Some("install") => {
            let setup = match &args[1..] {
                [] => None,
                [flag, path] if flag == "--pawnio-setup" => Some(std::path::PathBuf::from(path)),
                _ => {
                    eprintln!("{USAGE}");
                    return exit::USAGE;
                }
            };
            install::install(setup.as_deref()).map_or_else(fail, |()| exit::OK)
        }
        Some("uninstall") if args.len() == 1 => {
            install::uninstall().map_or_else(fail, |()| exit::OK)
        }
        _ => {
            eprintln!("{USAGE}");
            exit::USAGE
        }
    }
}

/// Admin console debug: open the driver directly and print two readings a
/// second apart, the second carrying package power (a rate needs two reads).
#[cfg(windows)]
fn read_once() -> i32 {
    use vitals_sensors::{Reading, exit};

    let mut r = sensor::Sensor::open().and_then(|mut s| {
        let _ = s.read()?;
        std::thread::sleep(std::time::Duration::from_secs(1));
        s.read()
    });
    if let Ok(reading) = r.as_mut() {
        match superio::SuperIo::open() {
            Ok(Some(chip)) => {
                reading.super_io = Some(chip.name().to_owned());
                match chip.fans() {
                    Ok(fans) => reading.fans = fans,
                    Err(e) => eprintln!("fans: {e}"),
                }
            }
            Ok(None) => eprintln!("fans: no supported Super-I/O chip"),
            Err(e) => eprintln!("fans: {e}"),
        }
    }
    let r = r.unwrap_or_else(Reading::failure);
    println!("{}", r.to_json());
    if r.ok {
        exit::OK
    } else if r.error.as_deref().is_some_and(|e| e.contains("0x80070005")) {
        exit::NOT_ELEVATED
    } else {
        exit::FAILED
    }
}
