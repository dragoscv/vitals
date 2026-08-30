//! Lists installed applications, for comparison against Settings > Apps.
//!
//! Run with: `cargo run -p vitals-win --example apps_probe`
//!
//! Cross-check the raw examined count against:
//!
//! ```powershell
//! Get-ItemProperty `
//!   HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*, `
//!   HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*, `
//!   HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\* |
//!   Where-Object DisplayName | Measure-Object
//! ```

use vitals_win::apps::{enumerate_installed_apps, enumerate_store_apps};

fn mib(bytes: u64) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

fn truncate(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_owned();
    }
    let kept: String = text.chars().take(width.saturating_sub(1)).collect();
    format!("{kept}…")
}

fn main() {
    let scan = enumerate_installed_apps();

    // `--names` prints one untruncated display name per line, which is what
    // the PowerShell cross-check diffs against. The formatted table below
    // elides long names and is useless for a set comparison.
    if std::env::args().any(|a| a == "--names") {
        for app in &scan.apps {
            println!("{}", app.display_name);
        }
        return;
    }

    println!(
        "{:<44}  {:<24}  {:<16}  {:>10}  {:<10}  {:<8}  {:<5}",
        "NAME", "PUBLISHER", "VERSION", "SIZE", "INSTALLED", "SOURCE", "MSI"
    );
    println!("{}", "-".repeat(130));

    for app in &scan.apps {
        let size = app
            .estimated_size
            .map_or_else(|| "—".to_owned(), |s| format!("{:.1} MB", mib(s.get())));

        let source = match app.source {
            vitals_win::apps::AppSource::MachineNative => "HKLM64",
            vitals_win::apps::AppSource::MachineWow64 => "HKLM32",
            vitals_win::apps::AppSource::UserNative => "HKCU64",
            vitals_win::apps::AppSource::UserWow64 => "HKCU32",
        };

        println!(
            "{:<44}  {:<24}  {:<16}  {:>10}  {:<10}  {:<8}  {:<5}",
            truncate(&app.display_name, 44),
            truncate(app.publisher.as_deref().unwrap_or("—"), 24),
            truncate(app.version.as_deref().unwrap_or("—"), 16),
            size,
            app.install_date
                .map_or_else(|| "—".to_owned(), vitals_win::apps::InstallDate::to_iso),
            source,
            if app.is_msi { "yes" } else { "no" },
        );
    }

    println!();
    println!("examined            {}", scan.examined);
    println!("shown               {}", scan.apps.len());
    println!("duplicates collapsed {}", scan.duplicates_collapsed);
    println!("filtered out        {}", scan.rejected_total());
    for (reason, count) in &scan.rejected {
        println!("  {reason:<22?} {count}");
    }

    let per_user = scan.apps.iter().filter(|a| a.per_user).count();
    let wow64 = scan.apps.iter().filter(|a| a.source.is_wow64()).count();
    let sized = scan
        .apps
        .iter()
        .filter(|a| a.estimated_size.is_some())
        .count();
    let dated = scan
        .apps
        .iter()
        .filter(|a| a.install_date.is_some())
        .count();
    let quiet = scan
        .apps
        .iter()
        .filter(|a| a.quiet_uninstall_string.is_some())
        .count();
    let msi = scan.apps.iter().filter(|a| a.is_msi).count();

    println!();
    println!("per-user            {per_user}");
    println!("32-bit (WOW6432Node) {wow64}");
    println!("MSI-managed         {msi}");
    println!("with a size         {sized}");
    println!("with an install date {dated}");
    println!("with a quiet uninstall {quiet}");

    match enumerate_store_apps() {
        Ok(store) => println!("\nStore/MSIX apps     {}", store.len()),
        Err(err) => println!("\nStore/MSIX apps     unavailable: {err}"),
    }
}
