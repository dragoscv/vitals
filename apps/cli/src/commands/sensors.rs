//! `vitals sensors` — every sensor reading this machine can make.
//!
//! The same call the desktop's `/api/v1/sensors` route makes, so the two
//! surfaces cannot disagree about the hardware. A reading that could not be
//! taken is absent from the list, never present as a zero.

use anyhow::Result;

use crate::exit;
use crate::render;
use crate::source::Source;

/// # Errors
/// The source cannot make readings: an attached host refused, or this
/// platform has no sensor backend.
pub fn run(source: &Source, json: bool) -> Result<()> {
    let lines = source.sensors().map_err(exit::not_found)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&lines)?);
    } else {
        println!("{}", render::sensors(&lines));
        if lines.is_empty() {
            eprintln!("no sensor readings are available on this machine");
        }
    }
    Ok(())
}
