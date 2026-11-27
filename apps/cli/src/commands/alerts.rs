//! `vitals alerts` — the conditions the engine has raised right now.
//!
//! The same engine the desktop and the phone read, so the list matches the
//! window's — except from a direct sampler, which starts with an empty engine
//! and so needs one sample before it can say anything at all.

use anyhow::Result;

use crate::exit;
use crate::render;
use crate::source::Source;

/// # Errors
/// The source could not be read.
pub fn run(source: &mut Source, json: bool) -> Result<()> {
    // One snapshot primes the alert engine. A direct sampler begins with an
    // empty one; an instantaneous condition (disk health, a low battery) is
    // raised on the first evaluation while a sustained one still needs its
    // window. Without this a direct run always reported none while an
    // attached desktop reported the truth.
    source.snapshot().map_err(exit::link_failure)?;
    let alerts = source.alerts().map_err(exit::link_failure)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&alerts)?);
    } else {
        println!("{}", render::alerts(&alerts));
        if alerts.is_empty() {
            eprintln!("no alerts are currently raised");
        }
    }
    Ok(())
}
