//! Proves the Device Manager inventory against this machine.
//!
//! Reads the tree twice — present devices only, then including the ones
//! Windows remembers but which are disconnected — and prints what a screen
//! would receive, so the counts can be checked against PowerShell:
//! `(Get-PnpDevice -PresentOnly).Count` and `(Get-PnpDevice).Count`.
//!
//! Run with: `cargo run -p vitals-win --example prove_devices`

#[cfg(not(windows))]
fn main() {}

#[cfg(windows)]
fn main() {
    use vitals_win::devices::{DeviceStatus, problem_description, read_devices};

    let present = read_devices(false);
    for class in &present.classes {
        println!(
            "{} ({})  [{}]",
            class.description,
            class.devices.len(),
            class.name
        );
        for device in &class.devices {
            let status = match device.status {
                DeviceStatus::Ok => "ok".to_owned(),
                DeviceStatus::Disabled => "disabled".to_owned(),
                DeviceStatus::Problem { code } => {
                    format!("problem {code}: {}", problem_description(code))
                }
                DeviceStatus::NotPresent => "not present".to_owned(),
            };
            println!(
                "    {:<60} {:<10} {:<22} {} {}{}",
                device.name,
                device.enumerator.as_deref().unwrap_or("-"),
                device.driver_version.as_deref().unwrap_or("-"),
                device.driver_date.as_deref().unwrap_or("-"),
                status,
                if device.hidden { " (hidden)" } else { "" },
            );
        }
    }

    let all = read_devices(true);
    for (label, inventory) in [("present", &present), ("all", &all)] {
        let devices = || inventory.classes.iter().flat_map(|c| c.devices.iter());
        let hidden = devices().filter(|d| d.hidden).count();
        let problems = devices()
            .filter(|d| matches!(d.status, DeviceStatus::Problem { .. }))
            .count();
        let disabled = devices()
            .filter(|d| d.status == DeviceStatus::Disabled)
            .count();
        let absent = devices().filter(|d| !d.present).count();
        println!(
            "{label:<8} {} devices in {} classes, {:.1} ms; hidden {hidden}, problem {problems}, disabled {disabled}, not present {absent}",
            devices().count(),
            inventory.classes.len(),
            inventory.elapsed.as_secs_f64() * 1000.0,
        );
    }
}
