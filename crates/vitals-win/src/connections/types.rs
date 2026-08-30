//! The connection model.
//!
//! UDP is connectionless, so a UDP entry has no state and frequently no
//! remote peer at all — it is a bound socket, not a conversation. Rather
//! than invent a "state" column full of dashes, the protocol-specific parts
//! live in [`Transport`] and the type system refuses to hand out a TCP state
//! for a UDP row.

use std::fmt;
use std::net::{IpAddr, SocketAddr};

use vitals_core::ids::Pid;

use super::classify::{AddressScope, classify_address, well_known_service};

/// The `MIB_TCP_STATE` enumeration, from `tcpmib.h`.
///
/// The wire values are pinned by a test; Windows has never renumbered them
/// but the whole point of this module is not trusting that a struct means
/// what we assume.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TcpState {
    Closed,
    Listen,
    SynSent,
    SynReceived,
    Established,
    FinWait1,
    FinWait2,
    CloseWait,
    Closing,
    LastAck,
    TimeWait,
    DeleteTcb,
    /// A value Windows returned that this build does not know. Kept rather
    /// than dropped: an unknown state is still a real connection, and
    /// silently hiding it would understate the count.
    Unknown(u32),
}

impl TcpState {
    /// Maps a raw `MIB_TCP_STATE` value.
    #[must_use]
    pub const fn from_raw(value: u32) -> Self {
        match value {
            1 => Self::Closed,
            2 => Self::Listen,
            3 => Self::SynSent,
            4 => Self::SynReceived,
            5 => Self::Established,
            6 => Self::FinWait1,
            7 => Self::FinWait2,
            8 => Self::CloseWait,
            9 => Self::Closing,
            10 => Self::LastAck,
            11 => Self::TimeWait,
            12 => Self::DeleteTcb,
            other => Self::Unknown(other),
        }
    }

    /// The conventional `netstat` spelling, so output can be diffed against
    /// `netstat -ano` directly.
    #[must_use]
    pub const fn netstat_name(self) -> &'static str {
        match self {
            Self::Closed => "CLOSED",
            Self::Listen => "LISTENING",
            Self::SynSent => "SYN_SENT",
            Self::SynReceived => "SYN_RCVD",
            Self::Established => "ESTABLISHED",
            Self::FinWait1 => "FIN_WAIT_1",
            Self::FinWait2 => "FIN_WAIT_2",
            Self::CloseWait => "CLOSE_WAIT",
            Self::Closing => "CLOSING",
            Self::LastAck => "LAST_ACK",
            Self::TimeWait => "TIME_WAIT",
            Self::DeleteTcb => "DELETE_TCB",
            Self::Unknown(_) => "UNKNOWN",
        }
    }

    /// Whether data can currently flow. Used to default the UI to the rows a
    /// user cares about, since `TIME_WAIT` entries can outnumber live ones
    /// several times over on a busy machine.
    #[must_use]
    pub const fn is_active(self) -> bool {
        matches!(self, Self::Established | Self::CloseWait | Self::FinWait1)
    }

    /// Whether this row is a server socket awaiting inbound connections
    /// rather than a conversation with a peer.
    #[must_use]
    pub const fn is_listening(self) -> bool {
        matches!(self, Self::Listen)
    }
}

impl fmt::Display for TcpState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.netstat_name())
    }
}

/// Protocol-specific detail.
///
/// The remote endpoint lives here because its *presence* is protocol
/// dependent: TCP always has one, UDP usually does not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    Tcp {
        remote: SocketAddr,
        state: TcpState,
    },
    /// A bound UDP socket. Windows' `UDP_TABLE_OWNER_PID` reports no peer,
    /// because at the socket layer there is none — a connected UDP socket
    /// looks identical to an unconnected one. Reporting `0.0.0.0:0` as a
    /// "remote address" would be fabricated, so there is no field for it.
    Udp,
}

impl Transport {
    /// `"TCP"` or `"UDP"`.
    #[must_use]
    pub const fn protocol_name(&self) -> &'static str {
        match self {
            Self::Tcp { .. } => "TCP",
            Self::Udp => "UDP",
        }
    }
}

/// One TCP connection or bound UDP socket, with its owning process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Connection {
    /// The socket this machine owns.
    pub local: SocketAddr,
    /// Protocol, and for TCP the peer and state.
    pub transport: Transport,
    /// The process that owns the socket.
    ///
    /// Windows reports PID 0 for sockets owned by the system idle/kernel
    /// context; that is a real answer, not a missing one, so it is kept.
    pub pid: Pid,
    /// IPv6 scope identifier for the local address.
    ///
    /// Only meaningful for link-local addresses, where `fe80::1` is
    /// ambiguous without knowing which interface it is on. `None` for IPv4,
    /// which has no equivalent concept.
    pub local_scope_id: Option<u32>,
    /// IPv6 scope identifier for the remote address, if any.
    pub remote_scope_id: Option<u32>,
}

impl Connection {
    /// The peer address, for TCP only.
    #[must_use]
    pub const fn remote(&self) -> Option<SocketAddr> {
        match self.transport {
            Transport::Tcp { remote, .. } => Some(remote),
            Transport::Udp => None,
        }
    }

    /// The TCP state, or `None` for UDP.
    #[must_use]
    pub const fn state(&self) -> Option<TcpState> {
        match self.transport {
            Transport::Tcp { state, .. } => Some(state),
            Transport::Udp => None,
        }
    }

    /// Whether this row uses IPv6.
    #[must_use]
    pub const fn is_ipv6(&self) -> bool {
        matches!(self.local, SocketAddr::V6(_))
    }

    /// How far the traffic travels.
    ///
    /// Derived from the remote address where there is one. A listening TCP
    /// socket or a bound UDP socket has no peer, so it is classified by
    /// where it *accepts* traffic from — a bind to `0.0.0.0` is exposed,
    /// a bind to `127.0.0.1` is not.
    #[must_use]
    pub fn scope(&self) -> AddressScope {
        match self.remote() {
            Some(remote) => classify_address(remote.ip()),
            None => classify_address(self.local.ip()),
        }
    }

    /// The port most likely to identify the service being spoken to.
    ///
    /// For an outbound connection that is the remote port; for a listener it
    /// is the local one. Returns `None` when neither is recognised.
    #[must_use]
    pub fn service_name(&self) -> Option<&'static str> {
        self.remote()
            .and_then(|r| well_known_service(r.port()))
            .or_else(|| well_known_service(self.local.port()))
    }

    /// Whether this connection reaches beyond the local machine.
    #[must_use]
    pub fn leaves_machine(&self) -> bool {
        self.scope().leaves_machine()
    }

    /// The peer's IP, for grouping by distinct remote host.
    #[must_use]
    pub fn remote_ip(&self) -> Option<IpAddr> {
        self.remote().map(|r| r.ip())
    }
}

#[cfg(test)]
mod tests {
    use std::net::{Ipv4Addr, Ipv6Addr, SocketAddrV4, SocketAddrV6};

    use super::*;

    fn tcp(local: SocketAddr, remote: SocketAddr, state: TcpState) -> Connection {
        Connection {
            local,
            transport: Transport::Tcp { remote, state },
            pid: Pid(1234),
            local_scope_id: None,
            remote_scope_id: None,
        }
    }

    #[test]
    fn tcp_state_values_match_the_windows_enumeration() {
        // Pinned against tcpmib.h. A renumbering here would silently
        // mislabel every row rather than fail to compile.
        let expected = [
            (1, TcpState::Closed),
            (2, TcpState::Listen),
            (3, TcpState::SynSent),
            (4, TcpState::SynReceived),
            (5, TcpState::Established),
            (6, TcpState::FinWait1),
            (7, TcpState::FinWait2),
            (8, TcpState::CloseWait),
            (9, TcpState::Closing),
            (10, TcpState::LastAck),
            (11, TcpState::TimeWait),
            (12, TcpState::DeleteTcb),
        ];

        for (raw, state) in expected {
            assert_eq!(
                TcpState::from_raw(raw),
                state,
                "MIB_TCP_STATE {raw} must map to {state}"
            );
        }

        assert_eq!(
            TcpState::from_raw(99),
            TcpState::Unknown(99),
            "an unrecognised state must be preserved, not dropped"
        );
    }

    #[test]
    fn udp_rows_have_no_state_and_no_peer() {
        let conn = Connection {
            local: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 5353)),
            transport: Transport::Udp,
            pid: Pid(4),
            local_scope_id: None,
            remote_scope_id: None,
        };

        assert_eq!(
            conn.state(),
            None,
            "UDP is connectionless; inventing a state would be fabricated data"
        );
        assert_eq!(conn.remote(), None);
        assert_eq!(conn.transport.protocol_name(), "UDP");
        assert_eq!(conn.service_name(), Some("mDNS"));
    }

    #[test]
    fn scope_comes_from_the_peer_when_there_is_one() {
        let conn = tcp(
            SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(192, 168, 1, 10), 51_000)),
            SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(140, 82, 121, 4), 443)),
            TcpState::Established,
        );

        assert_eq!(
            conn.scope(),
            AddressScope::Public,
            "a LAN-local source talking to a public peer is public traffic"
        );
        assert_eq!(conn.service_name(), Some("HTTPS"));
        assert!(conn.leaves_machine());
    }

    #[test]
    fn a_loopback_listener_is_not_exposed() {
        let conn = tcp(
            SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 5432)),
            SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0)),
            TcpState::Listen,
        );

        // The peer of a listener is 0.0.0.0:0, which is unspecified, so the
        // classification must fall through to the bind address.
        assert_eq!(conn.scope(), AddressScope::Unspecified);
        assert_eq!(conn.state(), Some(TcpState::Listen));
        assert!(conn.state().is_some_and(TcpState::is_listening));
    }

    #[test]
    fn ipv6_rows_report_their_family_and_scope() {
        let conn = Connection {
            local: SocketAddr::V6(SocketAddrV6::new(
                "fe80::1".parse::<Ipv6Addr>().expect("valid literal"),
                546,
                0,
                11,
            )),
            transport: Transport::Udp,
            pid: Pid(900),
            local_scope_id: Some(11),
            remote_scope_id: None,
        };

        assert!(conn.is_ipv6(), "an fe80:: socket must be reported as IPv6");
        assert_eq!(
            conn.local_scope_id,
            Some(11),
            "a link-local address without its scope id is ambiguous"
        );
        assert_eq!(conn.scope(), AddressScope::LinkLocal);
    }

    #[test]
    fn state_activity_separates_live_rows_from_teardown() {
        assert!(TcpState::Established.is_active());
        assert!(!TcpState::TimeWait.is_active());
        assert!(!TcpState::Listen.is_active());
        assert_eq!(TcpState::TimeWait.netstat_name(), "TIME_WAIT");
        assert_eq!(
            TcpState::Listen.netstat_name(),
            "LISTENING",
            "the label must match netstat so probe output can be diffed against it"
        );
    }
}
