//! The CLI's exit codes, and how an error becomes one.
//!
//! A script driving `vitals` needs to tell three failures apart without
//! parsing English off stderr: it could not reach a Vitals to ask (is the app
//! even running?), its request was refused (Windows protects this process),
//! or the thing it named does not exist (that PID is gone). Those get codes
//! 3, 4 and 5; anything else is 1, and clap's own usage errors stay at 2.
//!
//! The code is carried on the error itself, not decided by matching its
//! message: a message is prose that gets reworded, and a `contains("refused")`
//! test would silently start mis-classifying the day someone fixes a typo.
//! [`code_for`] walks the `anyhow` chain looking for the tag a call site
//! attached with [`link_failure`], [`control_refused`] or [`not_found`].

use std::error::Error as StdError;
use std::fmt;

/// Success.
pub const OK: i32 = 0;
/// An error that does not fit one of the specific codes below.
pub const FAILURE: i32 = 1;
/// Could not reach the app (no attach pipe, nothing at `--attach`), or this
/// machine has no sampler to read from.
pub const LINK_FAILURE: i32 = 3;
/// A control action was refused — read-only scope, a protected process, or a
/// headless `serve --control` that has no one to confirm it.
pub const CONTROL_REFUSED: i32 = 4;
/// The named thing was not found: a PID absent from the sample, or data the
/// source does not keep (history on a direct sampler).
pub const NOT_FOUND: i32 = 5;

/// An error carrying the exit code its call site chose.
///
/// Kept private: the only way to read a code back out is [`code_for`], so a
/// caller cannot construct a tag the mapping does not know about.
#[derive(Debug)]
struct Tagged {
    code: i32,
    inner: anyhow::Error,
}

impl fmt::Display for Tagged {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.inner, f)
    }
}

impl StdError for Tagged {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        self.inner.source()
    }
}

/// Attaches `code` to `error`, preserving its message and chain.
fn tag<E: Into<anyhow::Error>>(code: i32, error: E) -> anyhow::Error {
    anyhow::Error::new(Tagged {
        code,
        inner: error.into(),
    })
}

/// Could not reach the app or a source. Use with `map_err` at a discovery or
/// connection boundary.
pub fn link_failure<E: Into<anyhow::Error>>(error: E) -> anyhow::Error {
    tag(LINK_FAILURE, error)
}

/// A control action was refused.
pub fn control_refused<E: Into<anyhow::Error>>(error: E) -> anyhow::Error {
    tag(CONTROL_REFUSED, error)
}

/// The named thing was not found.
pub fn not_found<E: Into<anyhow::Error>>(error: E) -> anyhow::Error {
    tag(NOT_FOUND, error)
}

/// The exit code an error should produce.
///
/// Walks the whole `anyhow` chain, so a tagged error wrapped in `.context(…)`
/// on the way up still reports its own code rather than falling back to 1.
#[must_use]
pub fn code_for(error: &anyhow::Error) -> i32 {
    error
        .chain()
        .find_map(|cause| cause.downcast_ref::<Tagged>())
        .map_or(FAILURE, |tagged| tagged.code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tag_survives_the_context_added_on_the_way_out() {
        let error = not_found(anyhow::anyhow!("no process with PID 7"))
            .context("resolving the PID for kill");
        assert_eq!(code_for(&error), NOT_FOUND);
    }

    #[test]
    fn an_untagged_error_is_a_generic_failure_not_a_specific_code() {
        let error = anyhow::anyhow!("something unexpected");
        assert_eq!(code_for(&error), FAILURE);
        assert_eq!(code_for(&link_failure(anyhow::anyhow!("x"))), LINK_FAILURE);
        assert_eq!(
            code_for(&control_refused(anyhow::anyhow!("x"))),
            CONTROL_REFUSED
        );
    }

    #[test]
    fn the_tagged_error_still_carries_its_message() {
        let error = control_refused(anyhow::anyhow!("refused: could crash Windows"));
        assert!(format!("{error}").contains("refused"));
        // And the inner cause is reachable, so anyhow's `{:#}` chain still
        // prints it rather than a bare wrapper.
        assert_eq!(error.chain().count(), 1);
    }
}
