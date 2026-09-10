//! `vitals info` — static facts about the machine plus a current summary.

use anyhow::Result;
use serde::Serialize;
use vitals_core::metrics::SystemMetrics;
use vitals_core::provider::HostInfo;

use crate::render;
use crate::source::Source;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Info<'a> {
    source: &'static str,
    /// `None` when the source has no platform backend for it.
    host: Option<&'a HostInfo>,
    system: &'a SystemMetrics,
}

/// # Errors
/// The source could not answer.
pub fn run(source: &mut Source, json: bool) -> Result<()> {
    let host = source.host()?;
    let view = source.snapshot()?;

    if json {
        let out = Info {
            source: source.kind(),
            host: host.as_ref(),
            system: &view.system,
        };
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(());
    }

    let mut rows = match &host {
        Some(info) => render::host(info),
        None => vec![("Host", render::DASH.to_owned())],
    };
    rows.extend(render::system(&view.system));
    println!("{}", render::pairs(&rows));
    Ok(())
}
