//! `vitals ps` — the process list, once.

use anyhow::Result;
use vitals_core::process::Process;

use crate::render;
use crate::source::Source;

/// Applies `--filter` then `--top`, in that order: "the five busiest chrome
/// processes", not "the chrome processes among the five busiest".
#[must_use]
pub fn select(list: Vec<Process>, filter: Option<&str>, top: usize) -> Vec<Process> {
    let filtered: Vec<Process> = match filter {
        Some(needle) => {
            let needle = needle.to_lowercase();
            list.into_iter()
                .filter(|p| p.name.to_lowercase().contains(&needle))
                .collect()
        }
        None => list,
    };
    if top > 0 {
        render::top_by_cpu(filtered, top)
    } else {
        filtered
    }
}

/// # Errors
/// The source could not produce a snapshot.
pub fn run(source: &mut Source, filter: Option<&str>, top: usize, json: bool) -> Result<()> {
    let view = source.snapshot()?;
    let list = select(view.processes(), filter, top);
    if json {
        println!("{}", serde_json::to_string(&list)?);
    } else {
        println!("{}", render::processes(&list));
        if list.is_empty() {
            eprintln!("no processes matched");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use vitals_core::fixtures;

    #[test]
    fn filter_is_case_insensitive_and_applies_before_top() {
        let list = vec![
            fixtures::process("Chrome.exe", 1, 5.0),
            fixtures::process("chrome.exe", 2, 50.0),
            fixtures::process("code.exe", 3, 90.0),
        ];
        let out = select(list, Some("CHROME"), 1);
        assert_eq!(out.len(), 1);
        assert_eq!(
            out[0].key.pid.get(),
            2,
            "busiest chrome, not busiest overall"
        );
    }
}
