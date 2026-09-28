//! What a notification button asks for.
//!
//! The button's argument is the only thing that comes back from the toast,
//! possibly minutes later, so it carries the full [`ProcessKey`]: a bare PID
//! could by then belong to something else entirely.

use vitals_core::ids::{Pid, ProcessKey};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// End the process (and, for a tree target, everything below it).
    End(ProcessKey),
    /// Lower it (and everything below it) to below-normal priority.
    Lower(ProcessKey),
    /// Stop proposing this image name for a while.
    Ignore(ProcessKey),
}

impl Action {
    #[must_use]
    pub fn encode(self) -> String {
        let (verb, key) = match self {
            Self::End(k) => ("end", k),
            Self::Lower(k) => ("lower", k),
            Self::Ignore(k) => ("ignore", k),
        };
        format!("{verb}:{}:{}", key.pid.get(), key.start_time)
    }

    /// `None` for anything this process did not write, including a click on
    /// the body of the toast, which carries an empty argument.
    #[must_use]
    pub fn decode(arg: &str) -> Option<Self> {
        let mut parts = arg.split(':');
        let verb = parts.next()?;
        let pid: u32 = parts.next()?.parse().ok()?;
        let start: u64 = parts.next()?.parse().ok()?;
        if parts.next().is_some() {
            return None;
        }
        let key = ProcessKey::new(Pid(pid), start);
        match verb {
            "end" => Some(Self::End(key)),
            "lower" => Some(Self::Lower(key)),
            "ignore" => Some(Self::Ignore(key)),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_action_survives_the_round_trip_through_the_toast() {
        let key = ProcessKey::new(Pid(64_068), 134_050_000_000_000_000);
        for action in [Action::End(key), Action::Lower(key), Action::Ignore(key)] {
            assert_eq!(Action::decode(&action.encode()), Some(action));
        }
    }

    #[test]
    fn anything_malformed_is_refused_rather_than_guessed() {
        for bad in [
            "",
            "end",
            "end:1",
            "end:x:1",
            "end:1:2:3",
            "kill:1:2",
            "END:1:2",
        ] {
            assert_eq!(Action::decode(bad), None, "{bad:?}");
        }
    }
}
