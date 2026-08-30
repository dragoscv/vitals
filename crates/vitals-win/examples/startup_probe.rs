//! Prints every startup item found, for cross-validation against Windows.
//!
//! Run with `cargo run -p vitals-win --example startup_probe`.
//!
//! The counts printed at the end are meant to be compared against an
//! independent source, which is the only way to catch a silent
//! under-enumeration:
//!
//! ```powershell
//! (Get-CimInstance Win32_StartupCommand).Count
//! (Get-Service).Count
//! (Get-ScheduledTask | Where-Object {
//!     $_.Triggers.CimClass.CimClassName -match 'Logon|Boot' }).Count
//! ```
//!
//! Exact agreement is not expected and the disagreements are the interesting
//! part — see the notes printed alongside the totals.

use vitals_win::startup::{
    DisableRisk, StartupSource, StartupState, assess_entry, collect, services::StartType,
};

fn main() {
    let inventory = match collect(true) {
        Ok(inventory) => inventory,
        Err(error) => {
            eprintln!("could not collect startup items: {error}");
            std::process::exit(1);
        }
    };

    print_entries(&inventory);
    print_services(&inventory);
    print_totals(&inventory);
}

/// Prints the startup-entry table and any orphaned entries.
fn print_entries(inventory: &vitals_win::startup::StartupInventory) {
    println!("== Startup entries ==\n");
    println!(
        "{:<34} {:<20} {:<9} {:<15} COMMAND",
        "NAME", "SOURCE", "STATE", "RISK"
    );
    println!("{:-<130}", "");

    let mut entries: Vec<_> = inventory.entries.iter().collect();
    entries.sort_by_key(|e| (e.source, e.label().to_lowercase()));

    for entry in &entries {
        let state = match entry.state {
            StartupState::Enabled => "enabled",
            StartupState::Disabled => "DISABLED",
            StartupState::Unknown => "unknown",
        };

        let risk = match assess_entry(entry, None) {
            DisableRisk::Safe => "safe",
            DisableRisk::Degrades => "degrades",
            DisableRisk::SystemCritical => "SYSTEM-CRITICAL",
            DisableRisk::Forbidden => "forbidden",
        };

        // An em dash rather than a blank: a missing command is a fact worth
        // seeing, and an empty cell reads as a formatting bug.
        let command = entry.command.as_deref().unwrap_or("—");

        println!(
            "{:<34} {:<20} {:<9} {:<15} {}",
            truncate(entry.label(), 33),
            entry.source.as_str(),
            state,
            risk,
            truncate(command, 60)
        );
    }

    let missing: Vec<_> = entries
        .iter()
        .filter(|e| e.image_missing() == Some(true))
        .collect();

    if !missing.is_empty() {
        println!("\n== Orphaned entries (configured image is not on disk) ==\n");
        for entry in &missing {
            println!("  {} -> {:?}", entry.label(), entry.image_path);
        }
    }
}

/// Prints the boot-relevant services.
fn print_services(inventory: &vitals_win::startup::StartupInventory) {
    println!("\n== Services ==\n");
    println!(
        "{:<32} {:<40} {:<10} {:<10} {:<7} SVCHOST GROUP",
        "NAME", "DISPLAY NAME", "STATE", "START", "PID"
    );
    println!("{:-<120}", "");

    let mut services: Vec<_> = inventory.services.iter().collect();
    services.sort_by_key(|s| s.name.to_lowercase());

    for service in services.iter().filter(|s| {
        // Only the boot-relevant ones, or the table is 300 rows long. The
        // totals below still count everything.
        s.start_type.starts_at_boot() == Some(true)
    }) {
        println!(
            "{:<32} {:<40} {:<10} {:<10} {:<7} {}",
            truncate(&service.name, 31),
            truncate(service.display_name.as_deref().unwrap_or("—"), 39),
            format!("{:?}", service.state),
            format!("{:?}", service.start_type),
            service
                .pid
                .map_or_else(|| "—".to_owned(), |p| p.to_string()),
            service.svchost_group.as_deref().unwrap_or("—")
        );
    }
}

/// Prints the counts meant for cross-validation against PowerShell.
fn print_totals(inventory: &vitals_win::startup::StartupInventory) {
    println!("\n== Totals ==\n");

    for source in [
        StartupSource::MachineRun,
        StartupSource::MachineRun32,
        StartupSource::MachineRunOnce,
        StartupSource::MachineRunOnce32,
        StartupSource::UserRun,
        StartupSource::UserRunOnce,
        StartupSource::CommonStartupFolder,
        StartupSource::UserStartupFolder,
        StartupSource::ScheduledTask,
    ] {
        println!(
            "  {:<24} {}",
            source.as_str(),
            inventory.by_source(source).count()
        );
    }

    let no_display = inventory
        .services
        .iter()
        .filter(|s| s.display_name.is_none())
        .count();
    let unknown_config = inventory
        .services
        .iter()
        .filter(|s| s.start_type == StartType::Unknown)
        .count();

    println!("\n  entries total            {}", inventory.entries.len());
    println!("  entries enabled          {}", inventory.enabled().count());
    println!("  services total           {}", inventory.services.len());
    println!(
        "  services automatic       {}",
        inventory.automatic_services().count()
    );
    println!(
        "  services in svchost      {}",
        inventory
            .services
            .iter()
            .filter(|s| s.is_shared_host())
            .count()
    );
    println!("  services w/o display     {no_display}");
    println!("  services config unread   {unknown_config}");
    println!("  task definitions unread  {}", inventory.unreadable_tasks);

    println!(
        "\nCompare against PowerShell. Expected legitimate disagreements:\n\
         \x20 - Win32_StartupCommand omits RunOnce and scheduled tasks entirely.\n\
         \x20 - Get-Service counts drivers too when they are Win32-visible; this\n\
         \x20   list is SERVICE_WIN32 only, so it should be the smaller number.\n\
         \x20 - Get-ScheduledTask sees tasks whose definitions schtasks cannot\n\
         \x20   export unelevated."
    );
}

/// Shortens a string for column display, marking that it was cut.
fn truncate(text: &str, width: usize) -> String {
    // Counted in chars, not bytes: slicing a multi-byte path by byte index
    // panics, and startup entries genuinely contain non-ASCII names.
    if text.chars().count() <= width {
        return text.to_owned();
    }
    let kept: String = text.chars().take(width.saturating_sub(1)).collect();
    format!("{kept}…")
}
