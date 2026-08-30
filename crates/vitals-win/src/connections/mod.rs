//! Per-connection network enumeration.
//!
//! Distinct from [`crate::network`], which reports throughput per *adapter*.
//! This module answers a different question: which process is talking to
//! which host, right now. Task Manager cannot answer it at all, and it is
//! the basis of the "Network Connections" section.
//!
//! # What is measured
//!
//! Every TCP connection and bound UDP socket, over both IPv4 and IPv6, with
//! its owning PID — four calls to `GetExtendedTcpTable`/`GetExtendedUdpTable`
//! in total. Omitting IPv6 would hide most modern traffic, since Windows
//! prefers it whenever the route exists.
//!
//! # What is deliberately not measured
//!
//! * **Reverse DNS.** Resolving a peer's hostname blocks for seconds on a
//!   dead resolver, and the whole system sample has a 30 ms budget. Names
//!   belong in a future asynchronous enrichment layer that can populate them
//!   out of band; a synchronous lookup here would stall every other metric.
//! * **Per-connection bandwidth.** The `MIB_*_OWNER_PID` tables carry no byte
//!   counters — the only Windows source for per-socket throughput is ETW
//!   (`Microsoft-Windows-Kernel-Network`), which needs a trace session. Until
//!   that exists the field is absent rather than zero, because a plausible
//!   zero is worse than an honest gap.
//! * **Process names.** Resolving a PID to an image name is
//!   [`crate::process`]'s job; duplicating it here would double the work the
//!   sampler already does.

pub mod aggregate;
pub mod classify;
pub mod tables;
pub mod types;

pub use aggregate::{ProcessConnections, count_by_scope, group_by_process};
pub use classify::{AddressScope, classify_address, is_ephemeral_port, well_known_service};
pub use tables::{enumerate_all, tcp_v4, tcp_v6, udp_v4, udp_v6};
pub use types::{Connection, TcpState, Transport};

/// Enumerates every connection and groups it by owning process in one pass.
///
/// The common entry point for the UI section: the table needs the flat list
/// and the header needs the roll-up, and enumerating twice would double an
/// already measurable cost.
#[must_use]
pub fn snapshot() -> (Vec<Connection>, Vec<ProcessConnections>) {
    let connections = enumerate_all();
    let groups = group_by_process(&connections);
    (connections, groups)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_groups_cover_every_connection() {
        let (connections, groups) = snapshot();

        let grouped: usize = groups.iter().map(|g| g.total).sum();
        assert_eq!(
            grouped,
            connections.len(),
            "grouping must account for every row; a mismatch means rows were dropped"
        );

        assert!(
            !connections.is_empty(),
            "a running Windows machine always has sockets; an empty snapshot means enumeration failed"
        );
    }
}
