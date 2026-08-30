//! Grouping connections by owning process.
//!
//! The raw table on a desktop machine runs to several hundred rows, most of
//! them `TIME_WAIT` teardown noise. What makes the section readable is the
//! per-process roll-up: "Chrome: 47 connections, 12 remote hosts, 3 of them
//! public".

use std::collections::{BTreeSet, HashMap};
use std::net::IpAddr;

use vitals_core::ids::Pid;

use super::classify::AddressScope;
use super::types::{Connection, Transport};

/// A per-process summary of network activity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessConnections {
    pub pid: Pid,
    /// Total rows, including listeners and teardown states.
    pub total: usize,
    pub tcp: usize,
    pub udp: usize,
    /// TCP rows where data can currently flow.
    pub active: usize,
    /// TCP rows awaiting inbound connections.
    pub listening: usize,
    /// Distinct peer addresses, so a process holding twenty sockets to one
    /// CDN edge is not mistaken for one talking to twenty hosts.
    pub remote_hosts: BTreeSet<IpAddr>,
    /// Rows whose peer is on the public internet.
    pub public: usize,
}

impl ProcessConnections {
    fn new(pid: Pid) -> Self {
        Self {
            pid,
            total: 0,
            tcp: 0,
            udp: 0,
            active: 0,
            listening: 0,
            remote_hosts: BTreeSet::new(),
            public: 0,
        }
    }

    fn add(&mut self, conn: &Connection) {
        self.total += 1;

        match conn.transport {
            Transport::Tcp { state, .. } => {
                self.tcp += 1;
                if state.is_active() {
                    self.active += 1;
                }
                if state.is_listening() {
                    self.listening += 1;
                }
            }
            Transport::Udp => self.udp += 1,
        }

        // A peer is only counted as a distinct host if it is a real one; a
        // listener's 0.0.0.0 peer is a placeholder, not a machine somewhere.
        if let Some(ip) = conn.remote_ip()
            && conn.scope() != AddressScope::Unspecified
        {
            self.remote_hosts.insert(ip);
        }

        if conn.scope() == AddressScope::Public {
            self.public += 1;
        }
    }

    /// How many distinct peers this process is in contact with.
    #[must_use]
    pub fn distinct_remote_hosts(&self) -> usize {
        self.remote_hosts.len()
    }
}

/// Groups connections by owning process.
///
/// The result is sorted by total connection count descending, then by PID,
/// so the output is stable across ticks and the busiest process leads.
#[must_use]
pub fn group_by_process(connections: &[Connection]) -> Vec<ProcessConnections> {
    let mut by_pid: HashMap<Pid, ProcessConnections> = HashMap::new();

    for conn in connections {
        by_pid
            .entry(conn.pid)
            .or_insert_with(|| ProcessConnections::new(conn.pid))
            .add(conn);
    }

    let mut out: Vec<_> = by_pid.into_values().collect();
    out.sort_unstable_by(|a, b| b.total.cmp(&a.total).then(a.pid.cmp(&b.pid)));
    out
}

/// Counts rows by how far their traffic travels.
///
/// Feeds a summary strip above the table — "18 public, 40 LAN, 212 local".
#[must_use]
pub fn count_by_scope(connections: &[Connection]) -> HashMap<AddressScope, usize> {
    let mut counts = HashMap::new();
    for conn in connections {
        *counts.entry(conn.scope()).or_insert(0) += 1;
    }
    counts
}

#[cfg(test)]
mod tests {
    use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};

    use super::*;
    use crate::connections::types::TcpState;

    fn addr(a: [u8; 4], port: u16) -> SocketAddr {
        SocketAddr::V4(SocketAddrV4::new(
            Ipv4Addr::new(a[0], a[1], a[2], a[3]),
            port,
        ))
    }

    fn tcp(pid: u32, remote: SocketAddr, state: TcpState) -> Connection {
        Connection {
            local: addr([192, 168, 1, 10], 51_000),
            transport: Transport::Tcp { remote, state },
            pid: Pid(pid),
            local_scope_id: None,
            remote_scope_id: None,
        }
    }

    fn udp(pid: u32) -> Connection {
        Connection {
            local: addr([0, 0, 0, 0], 5353),
            transport: Transport::Udp,
            pid: Pid(pid),
            local_scope_id: None,
            remote_scope_id: None,
        }
    }

    #[test]
    fn groups_by_pid_and_counts_distinct_peers() {
        let conns = vec![
            tcp(100, addr([140, 82, 121, 4], 443), TcpState::Established),
            // Same peer twice — a second socket to one CDN edge is not a
            // second host.
            tcp(100, addr([140, 82, 121, 4], 443), TcpState::Established),
            tcp(100, addr([1, 1, 1, 1], 853), TcpState::TimeWait),
            udp(100),
            tcp(200, addr([10, 0, 0, 5], 445), TcpState::Established),
        ];

        let groups = group_by_process(&conns);
        assert_eq!(groups.len(), 2, "two distinct PIDs must yield two groups");

        let first = &groups[0];
        assert_eq!(first.pid, Pid(100), "the busiest process must sort first");
        assert_eq!(first.total, 4);
        assert_eq!(first.tcp, 3);
        assert_eq!(first.udp, 1);
        assert_eq!(
            first.active, 2,
            "TIME_WAIT is teardown, not an active conversation"
        );
        assert_eq!(
            first.distinct_remote_hosts(),
            2,
            "two sockets to the same peer must count as one remote host"
        );
        assert_eq!(first.public, 3, "all three TCP peers are public addresses");

        let second = &groups[1];
        assert_eq!(second.pid, Pid(200));
        assert_eq!(
            second.public, 0,
            "a LAN peer is not public internet traffic"
        );
    }

    #[test]
    fn listeners_do_not_count_as_remote_hosts() {
        let conns = vec![tcp(300, addr([0, 0, 0, 0], 0), TcpState::Listen)];

        let groups = group_by_process(&conns);
        let group = &groups[0];

        assert_eq!(group.listening, 1);
        assert_eq!(
            group.distinct_remote_hosts(),
            0,
            "a listener's 0.0.0.0 placeholder peer is not a machine on the network"
        );
        assert_eq!(group.active, 0);
    }

    #[test]
    fn empty_input_yields_empty_output() {
        assert!(
            group_by_process(&[]).is_empty(),
            "no connections must produce no groups, not a placeholder row"
        );
        assert!(count_by_scope(&[]).is_empty());
    }

    #[test]
    fn scope_counts_split_local_from_internet() {
        let conns = vec![
            tcp(1, addr([127, 0, 0, 1], 5432), TcpState::Established),
            tcp(1, addr([8, 8, 8, 8], 53), TcpState::Established),
            tcp(2, addr([192, 168, 1, 1], 80), TcpState::Established),
        ];

        let counts = count_by_scope(&conns);
        assert_eq!(counts.get(&AddressScope::Loopback), Some(&1));
        assert_eq!(counts.get(&AddressScope::Public), Some(&1));
        assert_eq!(counts.get(&AddressScope::Private), Some(&1));
        assert_eq!(
            counts.get(&AddressScope::Cgnat),
            None,
            "a scope with no rows must be absent rather than reported as zero"
        );
    }
}
