//! Logon session representation.

use std::time::SystemTime;

/// Windows session state, mapped from `WTS_CONNECTSTATE_CLASS`.
///
/// The raw enum has twelve values but five are reserved and undocumented. This
/// type carries only the seven Windows actually uses, plus an explicit unknown
/// for any value Windows returns that this build cannot name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    /// Console session, the local keyboard and display.
    Active,
    /// Connected but not receiving input: the physical console is locked or a
    /// remote session is minimised.
    Connected,
    /// The user disconnected but the session is still running.
    Disconnected,
    /// Idle terminal, not yet assigned to a user.
    Idle,
    /// Listening for an inbound connection.
    Listen,
    /// A shadow session, mirroring another.
    Shadow,
    /// Windows returned a state this build does not recognise. Rather than
    /// silently mapping it to a plausible guess, report it as unknown so the
    /// failure is visible.
    Unknown(u32),
}

impl SessionState {
    /// Maps the raw `WTS_CONNECTSTATE_CLASS` value.
    ///
    /// The defined constants are:
    /// - `WTSActive = 0`
    /// - `WTSConnected = 1`
    /// - `WTSConnectQuery = 2`
    /// - `WTSShadow = 3`
    /// - `WTSDisconnected = 4`
    /// - `WTSIdle = 5`
    /// - `WTSListen = 6`
    /// - `WTSReset = 7`
    /// - `WTSDown = 8`
    /// - `WTSInit = 9`
    ///
    /// Values 2, 7, 8, 9 are reserved in the documentation and should never
    /// appear, but if they do this function maps them to `Unknown` rather than
    /// making up an answer.
    #[must_use]
    pub const fn from_raw(raw: u32) -> Self {
        match raw {
            0 => Self::Active,
            1 => Self::Connected,
            3 => Self::Shadow,
            4 => Self::Disconnected,
            5 => Self::Idle,
            6 => Self::Listen,
            other => Self::Unknown(other),
        }
    }

    /// Whether this session is an interactive user.
    ///
    /// Session 0 (Services) and listening sessions are not users — they should
    /// be classified rather than hidden, so the absence is legible.
    #[must_use]
    pub const fn is_interactive(&self) -> bool {
        matches!(
            self,
            Self::Active | Self::Connected | Self::Disconnected | Self::Shadow
        )
    }
}

/// A Windows logon session.
///
/// Corresponds to what appears in Task Manager's Users tab: the account, the
/// session type, and whether the session is currently active or disconnected.
#[derive(Debug, Clone)]
pub struct LogonSession {
    pub session_id: u32,
    pub user_name: Option<String>,
    pub domain: Option<String>,
    /// The client machine name for a remote session.
    pub client_name: Option<String>,
    pub state: SessionState,
    /// When the user logged on.
    ///
    /// This is legitimately unavailable on some configurations: the Terminal
    /// Services API does not expose it directly, and deriving it from the
    /// session's oldest process is fragile. `None` is the honest answer when
    /// it cannot be obtained.
    pub logon_time: Option<SystemTime>,
}

impl LogonSession {
    /// The display name: DOMAIN\User, or User if there is no domain, or the
    /// session id alone if the user name could not be read.
    ///
    /// Session 0 is the Services session and typically has no user. Listening
    /// sessions are idle connection slots. Both return a fallback label rather
    /// than an empty string.
    #[must_use]
    pub fn display_name(&self) -> String {
        match (&self.domain, &self.user_name) {
            (Some(domain), Some(user)) if !domain.is_empty() => format!("{domain}\\{user}"),
            (_, Some(user)) if !user.is_empty() => user.clone(),
            _ => {
                if self.session_id == 0 {
                    "Services".to_owned()
                } else if matches!(self.state, SessionState::Listen) {
                    format!("Listener (session {})", self.session_id)
                } else {
                    format!("Session {}", self.session_id)
                }
            }
        }
    }

    /// Whether this session is session 0, the non-interactive Services session.
    #[must_use]
    pub const fn is_services(&self) -> bool {
        self.session_id == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_state_mapping() {
        // Every documented value maps correctly.
        assert_eq!(SessionState::from_raw(0), SessionState::Active);
        assert_eq!(SessionState::from_raw(1), SessionState::Connected);
        assert_eq!(SessionState::from_raw(3), SessionState::Shadow);
        assert_eq!(SessionState::from_raw(4), SessionState::Disconnected);
        assert_eq!(SessionState::from_raw(5), SessionState::Idle);
        assert_eq!(SessionState::from_raw(6), SessionState::Listen);
    }

    #[test]
    fn reserved_values_map_to_unknown() {
        // The reserved values that should never appear.
        assert_eq!(SessionState::from_raw(2), SessionState::Unknown(2));
        assert_eq!(SessionState::from_raw(7), SessionState::Unknown(7));
        assert_eq!(SessionState::from_raw(8), SessionState::Unknown(8));
        assert_eq!(SessionState::from_raw(9), SessionState::Unknown(9));
        assert_eq!(SessionState::from_raw(999), SessionState::Unknown(999));
    }

    #[test]
    fn interactive_classification() {
        // User sessions are interactive.
        assert!(SessionState::Active.is_interactive());
        assert!(SessionState::Connected.is_interactive());
        assert!(SessionState::Disconnected.is_interactive());
        assert!(SessionState::Shadow.is_interactive());

        // System sessions are not.
        assert!(!SessionState::Idle.is_interactive());
        assert!(!SessionState::Listen.is_interactive());
        assert!(!SessionState::Unknown(2).is_interactive());
    }

    #[test]
    fn display_name_with_domain() {
        let session = LogonSession {
            session_id: 1,
            user_name: Some("Alice".to_owned()),
            domain: Some("CORP".to_owned()),
            client_name: None,
            state: SessionState::Active,
            logon_time: None,
        };
        assert_eq!(session.display_name(), "CORP\\Alice");
    }

    #[test]
    fn display_name_without_domain() {
        let session = LogonSession {
            session_id: 1,
            user_name: Some("Bob".to_owned()),
            domain: None,
            client_name: None,
            state: SessionState::Active,
            logon_time: None,
        };
        assert_eq!(session.display_name(), "Bob");
    }

    #[test]
    fn display_name_for_session_zero() {
        let session = LogonSession {
            session_id: 0,
            user_name: None,
            domain: None,
            client_name: None,
            state: SessionState::Disconnected,
            logon_time: None,
        };
        assert_eq!(session.display_name(), "Services");
    }

    #[test]
    fn display_name_for_listener() {
        let session = LogonSession {
            session_id: 65536,
            user_name: None,
            domain: None,
            client_name: None,
            state: SessionState::Listen,
            logon_time: None,
        };
        assert_eq!(session.display_name(), "Listener (session 65536)");
    }

    #[test]
    fn display_name_fallback() {
        let session = LogonSession {
            session_id: 3,
            user_name: None,
            domain: None,
            client_name: None,
            state: SessionState::Disconnected,
            logon_time: None,
        };
        assert_eq!(session.display_name(), "Session 3");
    }

    #[test]
    fn services_session_classification() {
        let services = LogonSession {
            session_id: 0,
            user_name: None,
            domain: None,
            client_name: None,
            state: SessionState::Disconnected,
            logon_time: None,
        };
        assert!(services.is_services());

        let user = LogonSession {
            session_id: 1,
            user_name: Some("User".to_owned()),
            domain: None,
            client_name: None,
            state: SessionState::Active,
            logon_time: None,
        };
        assert!(!user.is_services());
    }
}
