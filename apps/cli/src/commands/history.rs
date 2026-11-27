//! `vitals history` — machine-wide samples from the desktop's store.
//!
//! The store belongs to the desktop app, so a machine the CLI samples
//! directly has none to give and says so (exit code 5) rather than inventing
//! a flat line. Point the CLI at the app (`--attach`, or let it attach by
//! itself) to read its history.

use anyhow::Result;

use crate::exit;
use crate::render;
use crate::source::Source;

/// # Errors
/// The source keeps no history, or an attached host refused the request.
pub fn run(source: &Source, seconds: u32, json: bool) -> Result<()> {
    let samples = source.history(seconds).map_err(exit::not_found)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&samples)?);
    } else {
        println!("{}", render::history(&samples));
        if samples.is_empty() {
            eprintln!("no history was recorded in the last {seconds} seconds");
        }
    }
    Ok(())
}
