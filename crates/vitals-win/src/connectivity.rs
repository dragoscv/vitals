//! The connectivity probe: gateway, two public resolvers, and a name lookup.
//!
//! Runs only when the user asks for it. Nothing here is on a timer and nothing
//! here samples: four ICMP echoes per target is a burst of network traffic and
//! a dashboard that did it every second would be a small denial of service
//! against the user's own router.
//!
//! ICMP first, then a TCP connect on 443. Plenty of networks drop echo
//! requests and would otherwise have the probe report a healthy link as dead;
//! a completed TCP handshake on 443 to `1.1.1.1` or `8.8.8.8` proves the same
//! thing with no privileges at all. The gateway is ICMP only — a home router
//! rarely has anything listening, so a TCP attempt there would fail even when
//! the link is perfect.

use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

use vitals_core::connectivity::{
    ConnectivityReport, INTERNET_TARGETS, PROBES_PER_TARGET, TargetProbe,
};

use windows::Win32::NetworkManagement::IpHelper::ICMP_ECHO_REPLY;

/// One echo's timeout. Long enough for a slow mobile link, short enough that
/// four of them plus the DNS lookup finish inside a couple of seconds.
const ECHO_TIMEOUT: Duration = Duration::from_millis(900);

/// How long a name lookup gets before it is called slow.
const DNS_TIMEOUT: Duration = Duration::from_secs(3);

/// A well-known name to resolve. `example.com` rather than a dotted address:
/// the point is to exercise the resolver, and a literal address would not.
const DNS_NAME: &str = "example.com";

/// One reply buffer: the reply struct, the header and the payload we sent.
/// `IcmpSendEcho` documents this as the minimum, and the buffer is declared as
/// `u64` words so it is 8-byte aligned — the struct contains a pointer, and
/// reading it through a byte array would be a misaligned access.
const REPLY_BYTES: usize = std::mem::size_of::<ICMP_ECHO_REPLY>() + 8 + 8;

/// Runs the whole probe. Blocks for up to a few seconds; the caller puts it on
/// the blocking pool.
#[must_use]
pub fn run() -> ConnectivityReport {
    let gateway = default_gateway().map(|ip| echo_target(ip.to_string(), IpAddr::V4(ip)));
    let internet = INTERNET_TARGETS
        .iter()
        .map(|target| internet_target(target))
        .collect();
    ConnectivityReport::from_parts(gateway, internet, dns_lookup_ms())
}

/// The default gateway, from the IPv4 route table.
///
/// The route with the lowest metric among the `0.0.0.0/0` entries, ignoring
/// the on-link rows whose next hop is `0.0.0.0` (those are the interface's own
/// subnet, not a gateway).
#[must_use]
pub fn default_gateway() -> Option<Ipv4Addr> {
    use windows::Win32::Foundation::NO_ERROR;
    use windows::Win32::NetworkManagement::IpHelper::{
        FreeMibTable, GetIpForwardTable2, MIB_IPFORWARD_TABLE2,
    };
    use windows::Win32::Networking::WinSock::AF_INET;

    // SAFETY: the pointer is produced by GetIpForwardTable2 and freed exactly
    // once, below, with the allocator Windows documents for it.
    unsafe {
        let mut table: *mut MIB_IPFORWARD_TABLE2 = std::ptr::null_mut();
        if GetIpForwardTable2(AF_INET, &raw mut table) != NO_ERROR || table.is_null() {
            return None;
        }
        let rows =
            std::slice::from_raw_parts((*table).Table.as_ptr(), (*table).NumEntries as usize);
        let mut best: Option<(u32, Ipv4Addr)> = None;
        for row in rows {
            if let Some(found) = gateway_from_row(row) {
                let metric = row.Metric;
                if best.is_none_or(|(m, _)| metric < m) {
                    best = Some((metric, found));
                }
            }
        }
        FreeMibTable(table.cast());
        best.map(|(_, ip)| ip)
    }
}

/// The gateway a single route row names, if it is a usable default route.
fn gateway_from_row(
    row: &windows::Win32::NetworkManagement::IpHelper::MIB_IPFORWARD_ROW2,
) -> Option<Ipv4Addr> {
    // SAFETY: a sockaddr union is read only after checking its family.
    let (destination, next_hop) = unsafe {
        let dest = &row.DestinationPrefix.Prefix;
        let hop = &row.NextHop;
        if dest.si_family != windows::Win32::Networking::WinSock::AF_INET
            || hop.si_family != windows::Win32::Networking::WinSock::AF_INET
        {
            return None;
        }
        (
            Ipv4Addr::from(u32::from_be(dest.Ipv4.sin_addr.S_un.S_addr)),
            Ipv4Addr::from(u32::from_be(hop.Ipv4.sin_addr.S_un.S_addr)),
        )
    };
    // Only the default route, and only when it actually points somewhere: the
    // on-link rows for the local subnet also have a 0.0.0.0 destination.
    (destination.is_unspecified()
        && !next_hop.is_unspecified()
        && row.DestinationPrefix.PrefixLength == 0)
        .then_some(next_hop)
}

/// Probes one internet target: ICMP, then TCP 443 if echo is blocked.
fn internet_target(target: &str) -> TargetProbe {
    let Ok(ip) = target.parse::<IpAddr>() else {
        return empty_probe(target);
    };
    let icmp = echo_target(target.to_owned(), ip);
    if icmp.received > 0 {
        return icmp;
    }
    tcp_probe(target.to_owned(), ip)
}

/// Sends [`PROBES_PER_TARGET`] ICMP echoes and summarises them.
fn echo_target(target: String, ip: IpAddr) -> TargetProbe {
    use windows::Win32::NetworkManagement::IpHelper::{
        IcmpCloseHandle, IcmpCreateFile, IcmpSendEcho,
    };

    let mut sent = 0;
    let mut received = 0;
    let mut total_ms = 0.0_f64;

    // SAFETY: the handle is closed once, below; the reply buffer is a plain
    // byte array whose size is passed exactly; the request payload outlives
    // the call.
    unsafe {
        let Ok(handle) = IcmpCreateFile() else {
            return empty_probe(&target);
        };
        let payload = [0u8; 8];
        let mut reply = [0_u64; REPLY_BYTES.div_ceil(8)];
        for _ in 0..PROBES_PER_TARGET {
            let addr = match ip {
                IpAddr::V4(v4) => u32::from(v4).to_be(),
                IpAddr::V6(_) => {
                    // Only IPv4 targets are probed; an IPv6 literal here would
                    // need the v6 API and its own timeout.
                    continue;
                }
            };
            sent += 1;
            let replies = IcmpSendEcho(
                handle,
                addr,
                payload.as_ptr().cast(),
                8,
                None,
                reply.as_mut_ptr().cast(),
                (reply.len() * 8) as u32,
                ECHO_TIMEOUT.as_millis() as u32,
            );
            if replies > 0 {
                received += 1;
                // The reply carries the OS's own round trip, measured between
                // the echo going out and coming back, which is the number a
                // person expects rather than our wall clock around the call.
                let echo = &*reply.as_ptr().cast::<ICMP_ECHO_REPLY>();
                total_ms += f64::from(echo.RoundTripTime);
            }
        }
        let _ = IcmpCloseHandle(handle);
    }

    TargetProbe {
        target,
        sent,
        received,
        rtt_ms: (received > 0).then(|| total_ms / f64::from(received)),
    }
}

/// A TCP connect on 443, for networks that drop echo requests.
fn tcp_probe(target: String, ip: IpAddr) -> TargetProbe {
    let mut sent = 0;
    let mut received = 0;
    let mut total_ms = 0.0_f64;
    for _ in 0..PROBES_PER_TARGET {
        sent += 1;
        let addr = SocketAddr::new(ip, 443);
        let started = Instant::now();
        if TcpStream::connect_timeout(&addr, ECHO_TIMEOUT).is_ok() {
            received += 1;
            total_ms += started.elapsed().as_secs_f64() * 1000.0;
        }
    }
    TargetProbe {
        target,
        sent,
        received,
        rtt_ms: (received > 0).then(|| total_ms / f64::from(received)),
    }
}

/// A target with no measurements, for an address that would not parse.
fn empty_probe(target: &str) -> TargetProbe {
    TargetProbe {
        target: target.to_owned(),
        sent: 0,
        received: 0,
        rtt_ms: None,
    }
}

/// How long resolving [`DNS_NAME`] took, or `None` if it did not resolve.
#[must_use]
pub fn dns_lookup_ms() -> Option<f64> {
    let started = Instant::now();
    let outcome = format!("{DNS_NAME}:443").to_socket_addrs().ok();
    let elapsed = started.elapsed().as_secs_f64() * 1000.0;
    (outcome.is_some() && started.elapsed() <= DNS_TIMEOUT).then_some(elapsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_probe_reports_all_three_parts_and_never_panics() {
        // A unit test must pass on a machine with no network too, so nothing
        // here asserts that anything answered: only that the shape is sane and
        // that a silent target never reports a zero-latency reply.
        let report = run();
        assert_eq!(report.internet.len(), INTERNET_TARGETS.len());
        for probe in &report.internet {
            assert!(
                probe.received <= probe.sent,
                "{} cannot have more replies than probes",
                probe.target
            );
            if probe.received == 0 {
                assert_eq!(probe.rtt_ms, None, "no reply is not 0 ms");
            }
        }
    }

    #[test]
    fn an_address_that_does_not_parse_is_not_counted_as_sent() {
        let probe = internet_target("not-an-address");
        assert_eq!(probe.sent, 0);
        assert_eq!(probe.rtt_ms, None);
    }
}
