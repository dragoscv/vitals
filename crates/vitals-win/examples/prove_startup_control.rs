//! Proves the Startup screen's two new behaviours against this machine.
//!
//! 1. How many entries and services "Hide Microsoft" would hide, with a
//!    sample of what stays visible, so a misclassification shows by eye.
//! 2. A real disable → re-read → enable → re-read round trip on a throwaway
//!    `HKCU\...\Run` value created for the purpose and deleted afterwards —
//!    no elevation, and nothing the user owns is touched.
//!
//! Run with: `cargo run -p vitals-win --example prove_startup_control`

#[cfg(not(windows))]
fn main() {}

#[cfg(windows)]
fn main() -> vitals_core::error::Result<()> {
    let inventory = vitals_win::startup::collect(true)?;
    let mut cache = vitals_win::startup::CompanyCache::default();
    print_entries(&inventory, &mut cache);
    print_services(&inventory, &mut cache);
    round_trip()
}

#[cfg(windows)]
fn print_entries(
    inventory: &vitals_win::startup::StartupInventory,
    cache: &mut vitals_win::startup::CompanyCache,
) {
    use vitals_win::startup::{StartupSource, is_microsoft};

    let (mut hidden, mut shown) = (Vec::new(), Vec::new());
    for entry in &inventory.entries {
        let company = entry
            .image_path
            .as_ref()
            .and_then(|p| cache.get(&p.display().to_string()));
        let microsoft = is_microsoft(company.as_deref())
            || (entry.source == StartupSource::ScheduledTask
                && entry.name.to_ascii_lowercase().starts_with(r"\microsoft\"));
        let line = format!("{} [{}]", entry.label(), company.as_deref().unwrap_or("?"));
        if microsoft {
            hidden.push(line);
        } else {
            shown.push(line);
        }
    }
    println!(
        "startup: {} Microsoft hidden, {} shown",
        hidden.len(),
        shown.len()
    );
    for line in shown.iter().take(12) {
        println!("  shown  {line}");
    }
    for line in hidden.iter().take(4) {
        println!("  hidden {line}");
    }
}

#[cfg(windows)]
fn print_services(
    inventory: &vitals_win::startup::StartupInventory,
    cache: &mut vitals_win::startup::CompanyCache,
) {
    use vitals_win::startup::{extract_image_path, is_microsoft, registry::expand_environment};

    let mut hidden = 0;
    let mut shown = Vec::new();
    for service in &inventory.services {
        let image = service
            .binary_path
            .as_deref()
            .and_then(extract_image_path)
            .map(expand_environment);
        let company = cache.for_service(&service.name, image.as_deref());
        if is_microsoft(company.as_deref()) {
            hidden += 1;
        } else {
            shown.push(format!(
                "{} [{}]",
                service.name,
                company.as_deref().unwrap_or("?")
            ));
        }
    }
    println!(
        "services: {hidden} Microsoft hidden, {} shown of {}",
        shown.len(),
        inventory.services.len()
    );
    for line in shown.iter().take(15) {
        println!("  shown  {line}");
    }
}

#[cfg(windows)]
const PROBE: &str = "VitalsProveStartupControl";

#[cfg(windows)]
fn probe_state() -> vitals_core::error::Result<Option<vitals_win::startup::StartupState>> {
    use vitals_win::startup::StartupSource;
    Ok(vitals_win::startup::collect(false)?
        .entries
        .into_iter()
        .find(|e| e.source == StartupSource::UserRun && e.name == PROBE)
        .map(|e| e.state))
}

#[cfg(windows)]
fn reg(args: &[&str]) -> bool {
    std::process::Command::new("reg")
        .args(args)
        .output()
        .is_ok_and(|out| out.status.success())
}

#[cfg(windows)]
fn round_trip() -> vitals_core::error::Result<()> {
    use vitals_win::actions::Consent;
    use vitals_win::startup::{StartupChange, StartupSource, StartupState, apply};

    const RUN: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
    const APPROVED: &str =
        r"HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";

    if !reg(&[
        "add",
        RUN,
        "/v",
        PROBE,
        "/d",
        r"C:\Windows\System32\notepad.exe",
        "/f",
    ]) {
        println!("could not create the probe value; round trip skipped");
        return Ok(());
    }

    let change = |enabled| StartupChange::SetEnabled {
        source: StartupSource::UserRun,
        name: PROBE.into(),
        enabled,
    };

    println!("\nprobe before:   {:?}", probe_state()?);
    let off = apply(&change(false), Consent::Unconfirmed).and_then(|()| probe_state());
    println!("after disable:  {off:?}");
    let on = apply(&change(true), Consent::Unconfirmed).and_then(|()| probe_state());
    println!("after enable:   {on:?}");

    // Removed whatever happened above: a prover that leaves a startup entry
    // behind on failure is worse than one that proves nothing.
    reg(&["delete", RUN, "/v", PROBE, "/f"]);
    reg(&["delete", APPROVED, "/v", PROBE, "/f"]);
    println!("probe removed:  {:?}", probe_state()?);

    let passed = matches!(off, Ok(Some(StartupState::Disabled)))
        && matches!(on, Ok(Some(StartupState::Enabled)));
    println!("\nround trip {}", if passed { "OK" } else { "FAILED" });
    if passed {
        Ok(())
    } else {
        Err(vitals_core::error::Error::Refused(
            "the disable/enable round trip did not read back as written".into(),
        ))
    }
}
