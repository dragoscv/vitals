//! The name an executable gives itself, for the Name column.
//!
//! The kernel's process list carries only the image file name
//! (`Code - Insiders.exe`), which is what a person types into a terminal and
//! not what they recognise. Task Manager shows the version resource's
//! `FileDescription` instead ("Visual Studio Code - Insiders"), and so does
//! this.
//!
//! Reading it costs an `OpenProcess`, a path query and a version-resource
//! read — far too much for every row every second. A process never changes
//! its image, so it is read once per process lifetime, and once per distinct
//! path across processes: fifty `chrome.exe` renderers share one read.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use vitals_core::ids::ProcessKey;

use crate::startup::version_info::file_description;

/// Resolves and remembers process descriptions.
#[derive(Debug, Default)]
pub struct DescriptionCache {
    /// Per process, including `None` for one whose path or resource cannot
    /// be read, so a protected process is asked once rather than every tick.
    by_process: HashMap<ProcessKey, Option<String>>,
    /// Per lower-cased executable path.
    by_path: HashMap<String, Option<String>>,
}

impl DescriptionCache {
    #[must_use]
    pub fn new() -> Self {
        Self {
            by_process: HashMap::with_capacity(512),
            by_path: HashMap::with_capacity(256),
        }
    }

    /// The description `key` declares, if it differs from its file name.
    ///
    /// A description that only repeats the file name ("svchost.exe") adds
    /// nothing, and one that is just whitespace is no description; both are
    /// `None` so the UI shows the file name once rather than twice.
    pub fn description(&mut self, key: ProcessKey, name: Option<&str>) -> Option<String> {
        if let Some(known) = self.by_process.get(&key) {
            return known.clone();
        }
        let resolved = crate::actions::executable_path(key)
            .ok()
            .flatten()
            .and_then(|path| {
                self.by_path
                    .entry(path.to_ascii_lowercase())
                    .or_insert_with(|| file_description(Path::new(&path)))
                    .clone()
            })
            .filter(|d| !repeats_file_name(d, name));
        self.by_process.insert(key, resolved.clone());
        resolved
    }

    /// Forgets processes that are gone, so the cache is bounded by the live
    /// process count rather than by uptime. Paths are kept: they are few and
    /// the next process from the same image reuses the read.
    pub fn retain_live(&mut self, live: &HashSet<ProcessKey>) {
        self.by_process.retain(|key, _| live.contains(key));
    }
}

/// Whether `description` says no more than the file name does.
fn repeats_file_name(description: &str, name: Option<&str>) -> bool {
    let Some(name) = name else {
        return false;
    };
    let stem = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
    description.eq_ignore_ascii_case(name) || description.eq_ignore_ascii_case(stem)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::ProcessEnumerator;

    #[test]
    fn a_description_that_only_repeats_the_file_name_is_no_description() {
        assert!(repeats_file_name("svchost.exe", Some("svchost.exe")));
        assert!(repeats_file_name("SVCHOST", Some("svchost.exe")));
        assert!(!repeats_file_name(
            "Host Process for Windows Services",
            Some("svchost.exe")
        ));
        assert!(!repeats_file_name("Anything", None));
    }

    #[test]
    fn our_own_process_is_described_once_and_then_served_from_the_cache() {
        let processes = ProcessEnumerator::new().enumerate().expect("enumerate");
        let own = processes
            .iter()
            .find(|p| p.key.pid.get() == std::process::id())
            .expect("the test process is in the list");
        let mut cache = DescriptionCache::new();
        let first = cache.description(own.key, own.name.as_deref());
        assert_eq!(cache.by_process.len(), 1);
        // Served from the cache: same answer, nothing new recorded.
        assert_eq!(cache.description(own.key, own.name.as_deref()), first);
        assert_eq!(cache.by_process.len(), 1);
    }

    #[test]
    fn explorer_is_called_by_the_name_people_know_it_by() {
        // The premise of the feature, against the real machine: a process
        // the user recognises by its app name, not its file name.
        let processes = ProcessEnumerator::new().enumerate().expect("enumerate");
        let Some(explorer) = processes
            .iter()
            .find(|p| p.name.as_deref() == Some("explorer.exe"))
        else {
            return; // A CI runner with no interactive shell.
        };
        let description =
            DescriptionCache::new().description(explorer.key, explorer.name.as_deref());
        assert!(
            description
                .as_deref()
                .is_some_and(|d| d.to_ascii_lowercase().contains("explorer")),
            "got {description:?}"
        );
    }
}
