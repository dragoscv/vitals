//! Error taxonomy.
//!
//! The variants exist to let the UI respond *differently* to each case, which
//! is the whole point. "Access denied" must offer to elevate;
//! "Unsupported" must hide the affordance permanently; "`NotFound`" for a
//! process that exited between render and click is not an error worth showing
//! at all. A single opaque error string would collapse all three into the same
//! useless toast.

use crate::capability::Capability;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The operation requires elevation the current process does not have.
    ///
    /// The UI turns this into an "Elevate" affordance, not a failure toast.
    #[error("access denied: {operation} requires elevation")]
    AccessDenied { operation: String },

    /// The target no longer exists — usually a process that exited between
    /// the frame being rendered and the user clicking. Benign by default.
    #[error("target no longer exists: {0}")]
    NotFound(String),

    /// This platform or this hardware cannot provide the capability at all.
    /// The UI should hide the feature rather than let the user retry.
    #[error("unsupported on this system: {0:?}")]
    Unsupported(Capability),

    /// The elevated helper service is not installed or not running.
    #[error("helper service unavailable: {reason}")]
    HelperUnavailable { reason: String },

    /// A protocol/version mismatch between the UI and the helper.
    #[error("protocol mismatch: expected model version {expected}, helper reports {actual}")]
    VersionMismatch { expected: u32, actual: u32 },

    /// An underlying OS call failed. `code` is the raw platform error.
    #[error("os error in {context}: code {code}")]
    Os { context: String, code: i32 },

    /// The operation was refused because it would destabilise the system —
    /// e.g. terminating a critical system process.
    #[error("refused: {0}")]
    Refused(String),

    #[error("timed out after {millis}ms: {operation}")]
    Timeout { operation: String, millis: u64 },

    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl Error {
    /// Whether this is expected churn rather than a real fault.
    ///
    /// Sampling a thousand processes at 1Hz means processes will always be
    /// vanishing mid-enumeration. Logging each one at error level would
    /// produce noise that hides genuine problems.
    #[must_use]
    pub const fn is_benign(&self) -> bool {
        matches!(self, Self::NotFound(_))
    }

    /// Whether retrying after elevation could plausibly succeed.
    #[must_use]
    pub const fn is_elevation_fixable(&self) -> bool {
        matches!(
            self,
            Self::AccessDenied { .. } | Self::HelperUnavailable { .. }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vanished_process_is_benign() {
        assert!(Error::NotFound("pid 1".into()).is_benign());
        assert!(!Error::Refused("critical process".into()).is_benign());
    }

    #[test]
    fn access_denied_suggests_elevation() {
        assert!(
            Error::AccessDenied {
                operation: "terminate".into()
            }
            .is_elevation_fixable()
        );
        assert!(!Error::Unsupported(Capability::GpuOverclock).is_elevation_fixable());
    }
}
