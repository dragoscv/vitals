//! Pure classification helpers for connection endpoints.
//!
//! These are deliberately free of any Windows dependency so they can be
//! tested on their own. The interesting one is [`classify_address`]: the
//! question a user actually asks of a connection list is not "what is the
//! numeric address" but "is this thing talking to the internet, or to
//! itself?". Everything else in the section is supporting detail.

use std::fmt;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// Where the far end of a connection lives, relative to the user's machine.
///
/// Ordered from most local to least, so a UI can sort by "how far away is
/// this" and get a sensible result for free.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AddressScope {
    /// `0.0.0.0` or `::` — a wildcard bind, not a real peer.
    Unspecified,
    /// `127.0.0.0/8` or `::1`. Traffic that never leaves the machine.
    Loopback,
    /// `169.254.0.0/16` or `fe80::/10`. Same physical link only, and for
    /// IPv6 meaningless without the scope identifier.
    LinkLocal,
    /// RFC 1918 or RFC 4193 unique-local. The home or office LAN.
    Private,
    /// RFC 6598 `100.64.0.0/10`, carrier-grade NAT. Not the LAN and not
    /// reachable from the public internet either — worth naming separately
    /// because users on mobile tethering see a lot of it and it is not a
    /// sign of anything untoward.
    Cgnat,
    /// Multicast or broadcast. Discovery chatter — mDNS, SSDP, LLMNR.
    Multicast,
    /// Routable on the public internet. The category that matters.
    Public,
}

impl AddressScope {
    /// Whether traffic to this address leaves the machine.
    #[must_use]
    pub const fn leaves_machine(self) -> bool {
        !matches!(self, Self::Loopback | Self::Unspecified)
    }

    /// Whether traffic to this address leaves the local network.
    ///
    /// [`Self::Cgnat`] counts as leaving: the packets do go upstream, they
    /// simply terminate at the carrier rather than on the open internet.
    #[must_use]
    pub const fn leaves_network(self) -> bool {
        matches!(self, Self::Public | Self::Cgnat)
    }

    /// A short label suitable for a table cell.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Unspecified => "unspecified",
            Self::Loopback => "loopback",
            Self::LinkLocal => "link-local",
            Self::Private => "private",
            Self::Cgnat => "carrier NAT",
            Self::Multicast => "multicast",
            Self::Public => "public",
        }
    }
}

impl fmt::Display for AddressScope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// Classifies an address by how far the traffic travels.
#[must_use]
pub fn classify_address(addr: IpAddr) -> AddressScope {
    match addr {
        IpAddr::V4(v4) => classify_v4(v4),
        IpAddr::V6(v6) => classify_v6(v6),
    }
}

fn classify_v4(addr: Ipv4Addr) -> AddressScope {
    let [a, b, _, _] = addr.octets();

    // Order matters: the unspecified address is technically inside no
    // special-purpose block that `is_private` recognises, but it is checked
    // first because a wildcard bind is not a peer at all.
    if addr.is_unspecified() {
        return AddressScope::Unspecified;
    }
    if addr.is_loopback() {
        return AddressScope::Loopback;
    }
    if addr.is_link_local() {
        return AddressScope::LinkLocal;
    }
    if addr.is_multicast() || addr.is_broadcast() {
        return AddressScope::Multicast;
    }
    // RFC 6598: 100.64.0.0/10. `is_private` does not cover this, and
    // std's `is_shared` is still unstable, so it is spelt out.
    if a == 100 && (64..128).contains(&b) {
        return AddressScope::Cgnat;
    }
    if addr.is_private() {
        return AddressScope::Private;
    }
    AddressScope::Public
}

fn classify_v6(addr: Ipv6Addr) -> AddressScope {
    if addr.is_unspecified() {
        return AddressScope::Unspecified;
    }
    if addr.is_loopback() {
        return AddressScope::Loopback;
    }
    if addr.is_multicast() {
        return AddressScope::Multicast;
    }

    // An IPv4-mapped address (::ffff:a.b.c.d) is a v4 peer wearing a v6
    // hat. Classifying it as "public" because the v6 prefix is unfamiliar
    // would mislabel loopback and LAN traffic on every dual-stack socket.
    if let Some(v4) = addr.to_ipv4_mapped() {
        return classify_v4(v4);
    }

    let segments = addr.segments();

    // fe80::/10 — link-local unicast.
    if segments[0] & 0xffc0 == 0xfe80 {
        return AddressScope::LinkLocal;
    }
    // fc00::/7 — unique local (RFC 4193), the IPv6 answer to RFC 1918.
    if segments[0] & 0xfe00 == 0xfc00 {
        return AddressScope::Private;
    }

    AddressScope::Public
}

/// Names a well-known port, when the number alone would be opaque.
///
/// Returns `None` for anything unrecognised rather than inventing a label:
/// an ephemeral port genuinely has no service name, and guessing one would
/// be fabricated data. The registered-port list is deliberately short — it
/// covers what a desktop user will actually see, not all 49 151 IANA
/// assignments.
#[must_use]
pub const fn well_known_service(port: u16) -> Option<&'static str> {
    Some(match port {
        20 | 21 => "FTP",
        22 => "SSH",
        23 => "Telnet",
        25 => "SMTP",
        53 => "DNS",
        67 | 68 => "DHCP",
        80 => "HTTP",
        110 => "POP3",
        123 => "NTP",
        135 => "MSRPC",
        137..=139 => "NetBIOS",
        143 => "IMAP",
        161 | 162 => "SNMP",
        389 => "LDAP",
        443 => "HTTPS",
        445 => "SMB",
        465 => "SMTPS",
        514 => "Syslog",
        587 => "SMTP submission",
        636 => "LDAPS",
        853 => "DNS over TLS",
        993 => "IMAPS",
        995 => "POP3S",
        1433 => "MS SQL Server",
        1521 => "Oracle",
        1900 => "SSDP",
        3306 => "MySQL",
        3389 => "RDP",
        3478 | 3479 => "STUN/TURN",
        5353 => "mDNS",
        5355 => "LLMNR",
        5432 => "PostgreSQL",
        5672 => "AMQP",
        6379 => "Redis",
        8080 => "HTTP (alternate)",
        8443 => "HTTPS (alternate)",
        9200 => "Elasticsearch",
        27017 => "MongoDB",
        _ => return None,
    })
}

/// Whether a port is in the ephemeral range Windows allocates for outbound
/// sockets (49152–65535, per IANA and the Windows default since Vista).
///
/// Useful for the UI to grey out the local port of an outbound connection,
/// which carries no information.
#[must_use]
pub const fn is_ephemeral_port(port: u16) -> bool {
    port >= 49152
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v4(s: &str) -> IpAddr {
        IpAddr::V4(s.parse().expect("test address must parse"))
    }

    fn v6(s: &str) -> IpAddr {
        IpAddr::V6(s.parse().expect("test address must parse"))
    }

    #[test]
    fn classifies_the_ipv4_special_ranges() {
        let cases = [
            ("0.0.0.0", AddressScope::Unspecified),
            ("127.0.0.1", AddressScope::Loopback),
            ("169.254.1.5", AddressScope::LinkLocal),
            ("10.0.0.4", AddressScope::Private),
            ("172.16.9.1", AddressScope::Private),
            ("172.32.9.1", AddressScope::Public),
            ("192.168.1.10", AddressScope::Private),
            ("100.64.0.1", AddressScope::Cgnat),
            ("100.127.255.254", AddressScope::Cgnat),
            ("100.128.0.1", AddressScope::Public),
            ("224.0.0.251", AddressScope::Multicast),
            ("8.8.8.8", AddressScope::Public),
        ];

        for (text, expected) in cases {
            assert_eq!(
                classify_address(v4(text)),
                expected,
                "{text} should classify as {expected}"
            );
        }
    }

    #[test]
    fn classifies_the_ipv6_special_ranges() {
        let cases = [
            ("::", AddressScope::Unspecified),
            ("::1", AddressScope::Loopback),
            ("fe80::1", AddressScope::LinkLocal),
            ("febf::1", AddressScope::LinkLocal),
            ("fec0::1", AddressScope::Public),
            ("fd00::1", AddressScope::Private),
            ("fc00::1", AddressScope::Private),
            ("ff02::fb", AddressScope::Multicast),
            ("2606:4700:4700::1111", AddressScope::Public),
        ];

        for (text, expected) in cases {
            assert_eq!(
                classify_address(v6(text)),
                expected,
                "{text} should classify as {expected}"
            );
        }
    }

    #[test]
    fn ipv4_mapped_addresses_keep_their_ipv4_meaning() {
        assert_eq!(
            classify_address(v6("::ffff:127.0.0.1")),
            AddressScope::Loopback,
            "a v4-mapped loopback peer must not be reported as public internet traffic"
        );
        assert_eq!(
            classify_address(v6("::ffff:192.168.0.9")),
            AddressScope::Private
        );
    }

    #[test]
    fn scope_predicates_answer_the_users_question() {
        assert!(!AddressScope::Loopback.leaves_machine());
        assert!(!AddressScope::Unspecified.leaves_machine());
        assert!(AddressScope::Private.leaves_machine());

        assert!(!AddressScope::Private.leaves_network());
        assert!(AddressScope::Public.leaves_network());
        assert!(
            AddressScope::Cgnat.leaves_network(),
            "carrier-NAT traffic does leave the local network"
        );
    }

    #[test]
    fn names_only_ports_it_actually_knows() {
        assert_eq!(well_known_service(443), Some("HTTPS"));
        assert_eq!(well_known_service(53), Some("DNS"));
        assert_eq!(well_known_service(138), Some("NetBIOS"));
        assert_eq!(
            well_known_service(51_763),
            None,
            "an ephemeral port has no service name and must not be given one"
        );
        assert_eq!(well_known_service(0), None);
    }

    #[test]
    fn ephemeral_range_matches_the_windows_default() {
        assert!(!is_ephemeral_port(49_151));
        assert!(is_ephemeral_port(49_152));
        assert!(is_ephemeral_port(65_535));
    }
}
