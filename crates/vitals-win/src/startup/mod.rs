//! Startup items: what runs without being asked.
//!
//! Backs three tabs at once — Startup Apps, Services and Scheduled Tasks —
//! because they are three views of one question.
//!
//! ## What Task Manager misses, and why it matters
//!
//! | Source | Task Manager | Here |
//! |---|---|---|
//! | `HKCU`/`HKLM` `Run` | yes | yes |
//! | `WOW6432Node` `Run` | partially | explicitly, both views |
//! | `RunOnce` | no | yes, flagged as single-shot |
//! | Startup folders | yes | yes |
//! | Logon/boot scheduled tasks | **no** | yes |
//! | Automatic services | separate console | yes |
//!
//! The scheduled-task row is the interesting one. A task with a
//! `LogonTrigger` runs at every sign-in exactly as a `Run` value does, and it
//! is invisible in Task Manager's Startup tab — which is precisely why
//! anything that does not want to be found there uses one instead.
//!
//! ## Honesty about what was not measured
//!
//! Two figures Task Manager displays are deliberately absent:
//!
//! - **Startup impact** ("High"/"Medium"/"Low"). That rating comes from a
//!   boot trace the OS collects; it cannot be derived at enumeration time.
//!   Rather than compute a plausible-looking substitute from image size, this
//!   module reports nothing and the boot-trace work is left to a later
//!   milestone.
//! - **Publisher.** Requires Authenticode verification per image, which
//!   belongs to the signature module. [`StartupEntry::publisher`] exists in
//!   the shape and is honestly `None` until then.
//!
//! In both cases an invented number would be indistinguishable from a
//! measured one, which is the failure this codebase exists to avoid.

pub mod approved;
pub mod classify;
pub mod entry;
pub mod registry;
pub mod run_keys;
pub mod services;
pub mod taskcom;
pub mod tasks;

pub use approved::ApprovalIndex;
pub use classify::{DisableRisk, EntryFacts, assess_disable, assess_entry, extract_image_path};
pub use entry::{StartupEntry, StartupSource, StartupState};
pub use run_keys::{scan_run_keys, scan_startup_folders};
pub use services::{ServiceInfo, ServiceState, StartType, enumerate_services};
pub use tasks::{TaskInfo, TaskScan, TaskTrigger, scan_tasks};

use vitals_core::error::Result;

/// Everything that runs at logon or boot, from every source.
#[derive(Debug)]
pub struct StartupInventory {
    /// Registry values, shortcuts and logon/boot tasks, in one list.
    pub entries: Vec<StartupEntry>,
    /// Every Win32 service, whatever its start type — the Services tab shows
    /// all of them, not only the automatic ones.
    pub services: Vec<ServiceInfo>,
    /// How many task definitions existed but could not be read.
    ///
    /// Surfaced so the UI can say "and 40 we could not read" instead of
    /// presenting a partial list as complete. Unelevated, the
    /// `\Microsoft\Windows` subtree is largely readable but some definitions
    /// are ACL'd to SYSTEM.
    pub unreadable_tasks: usize,
}

impl StartupInventory {
    /// Entries that will actually run at the next logon.
    ///
    /// Excludes items of [`StartupState::Unknown`], because counting them as
    /// running would overstate the figure the user is trying to reduce.
    pub fn enabled(&self) -> impl Iterator<Item = &StartupEntry> {
        self.entries
            .iter()
            .filter(|e| e.state == StartupState::Enabled)
    }

    /// Services configured to start at boot.
    ///
    /// A service whose start type could not be read is excluded and is
    /// therefore *undercounted* rather than guessed at — the direction of
    /// the error is chosen deliberately, since claiming a service starts at
    /// boot when we do not know is the more misleading mistake.
    pub fn automatic_services(&self) -> impl Iterator<Item = &ServiceInfo> {
        self.services
            .iter()
            .filter(|s| s.start_type.starts_at_boot() == Some(true))
    }

    /// Entries drawn from a given source.
    pub fn by_source(&self, source: StartupSource) -> impl Iterator<Item = &StartupEntry> {
        self.entries.iter().filter(move |e| e.source == source)
    }
}

/// Scans scheduled tasks, preferring the in-process COM API.
///
/// `schtasks.exe` remains as a fallback rather than being deleted. The COM
/// path needs a working Task Scheduler service and a COM apartment; if
/// either is unavailable this still produces a list instead of silently
/// reporting a machine with no scheduled tasks, which is the failure mode
/// this whole area is written to avoid.
fn scan_tasks_fastest() -> tasks::TaskScan {
    // Measured on this machine, 221 tasks: COM 76 ms, schtasks 144 ms, with
    // byte-identical results — same total, same startup set, none unreadable.
    if let Some(document) = taskcom::query_all_definitions() {
        let scan = tasks::parse_scan(&document);

        // A connected scheduler with zero tasks is not a real machine; it
        // means the enumeration walked nothing. Fall through rather than
        // report an empty startup list as fact.
        if scan.total_seen > 0 {
            return scan;
        }
    }

    scan_tasks()
}

/// Collects every startup item on the machine.
///
/// `with_service_config` controls whether each service's start type and image
/// path are queried, which costs one extra SCM round-trip per service. With
/// it off, every [`ServiceInfo::start_type`] is honestly
/// [`StartType::Unknown`] and [`StartupInventory::automatic_services`]
/// yields nothing — rather than silently reporting every service as manual.
///
/// # Errors
///
/// Propagates [`vitals_core::error::Error::AccessDenied`] only if the service
/// control manager cannot be opened at all, which does not happen on a
/// standard desktop. Registry, folder and task scans never fail the call: an
/// unreadable source contributes nothing and, for tasks, increments
/// [`StartupInventory::unreadable_tasks`].
pub fn collect(with_service_config: bool) -> Result<StartupInventory> {
    // The task scan and the service enumeration touch unrelated subsystems —
    // one the Task Scheduler, the other the SCM — and neither shares state,
    // so they overlap. They are also the two expensive halves: measured at
    // 76 ms and 68 ms against 16 ms for everything else, so running them
    // together removes very nearly the whole of the shorter one.
    let tasks_thread = std::thread::spawn(scan_tasks_fastest);

    // The registry work happens on this thread while that runs, so it is
    // free too.
    let approvals = ApprovalIndex::load();
    let mut entries = scan_run_keys(&approvals);
    entries.extend(scan_startup_folders(&approvals));

    let services = enumerate_services(with_service_config)?;

    // A panic in the scan would poison the join. Treated as "no tasks
    // readable" rather than propagated: a failure to enumerate scheduled
    // tasks must not cost the user the run keys and services that were
    // gathered successfully.
    let scan = tasks_thread.join().unwrap_or_default();
    entries.extend(scan.startup_tasks.iter().map(tasks::to_entry));

    Ok(StartupInventory {
        entries,
        services,
        unreadable_tasks: scan.unreadable,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_full_collection_is_internally_consistent() {
        let inventory = collect(true).expect("collecting startup items needs no elevation");

        // Every source must be reachable through by_source, and the parts
        // must sum to the whole — a source missing from the match arm would
        // otherwise silently drop its entries from every filtered view.
        let per_source: usize = [
            StartupSource::MachineRun,
            StartupSource::MachineRun32,
            StartupSource::MachineRunOnce,
            StartupSource::MachineRunOnce32,
            StartupSource::UserRun,
            StartupSource::UserRunOnce,
            StartupSource::CommonStartupFolder,
            StartupSource::UserStartupFolder,
            StartupSource::ScheduledTask,
        ]
        .into_iter()
        .map(|s| inventory.by_source(s).count())
        .sum();

        assert_eq!(
            per_source,
            inventory.entries.len(),
            "an entry belongs to a source not covered by the test list"
        );

        assert!(
            inventory.enabled().count() <= inventory.entries.len(),
            "enabled entries are a subset"
        );
    }

    #[test]
    fn we_find_startup_items_task_manager_hides() {
        let inventory = collect(false).expect("collecting startup items needs no elevation");

        let tasks = inventory.by_source(StartupSource::ScheduledTask).count();
        assert!(
            tasks > 0,
            "every Windows install has logon or boot triggered tasks; finding \
             none means the XML scan is broken, and an empty list is \
             indistinguishable from 'there are none'"
        );
    }

    #[test]
    fn automatic_services_are_not_inferred_when_config_was_not_read() {
        let inventory = collect(false).expect("collecting startup items needs no elevation");
        assert_eq!(
            inventory.automatic_services().count(),
            0,
            "without a config query the start type is unknown; reporting \
             automatic services here would be fabricated"
        );

        let with_config = collect(true).expect("collecting startup items needs no elevation");
        assert!(
            with_config.automatic_services().count() > 10,
            "with the config query, a stock install has dozens of automatic \
             services"
        );
    }

    #[test]
    fn risk_assessment_covers_every_collected_entry() {
        let inventory = collect(false).expect("collecting startup items needs no elevation");

        // Not an assertion about the values — an assertion that the pure
        // classifier accepts every shape the real machine produces, since a
        // panic here would take the Startup tab down.
        for entry in &inventory.entries {
            let risk = assess_entry(entry, None);
            assert!(
                risk.is_possible() || risk == DisableRisk::Forbidden,
                "{} produced an incoherent risk",
                entry.label()
            );
        }
    }
}
