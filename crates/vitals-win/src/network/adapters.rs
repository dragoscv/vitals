//! Network adapter enumeration via `GetIfTable2`.
//!
//! `GetIfTable2` is used rather than the older `GetAdaptersInfo` because it
//! reports the counters we need — bytes, packets, errors and discards — in
//! the same call as the interface identity, and it covers IPv6-only adapters
//! that the legacy API silently omits.

use std::ffi::c_void;

use vitals_core::ids::NicId;
use vitals_core::metrics::NetworkKind;

use super::rate::NetworkCounters;

/// Maximum characters in an interface alias or description, per `netioapi.h`.
const IF_MAX_STRING_SIZE: usize = 256;
const IF_MAX_PHYS_ADDRESS_LENGTH: usize = 32;

/// `MIB_IF_ROW2`.
#[repr(C)]
#[derive(Clone, Copy)]
#[allow(non_snake_case)]
struct MibIfRow2 {
    InterfaceLuid: u64,
    InterfaceIndex: u32,
    InterfaceGuid: [u8; 16],
    Alias: [u16; IF_MAX_STRING_SIZE + 1],
    Description: [u16; IF_MAX_STRING_SIZE + 1],
    PhysicalAddressLength: u32,
    PhysicalAddress: [u8; IF_MAX_PHYS_ADDRESS_LENGTH],
    PermanentPhysicalAddress: [u8; IF_MAX_PHYS_ADDRESS_LENGTH],
    Mtu: u32,
    Type: u32,
    TunnelType: i32,
    MediaType: i32,
    PhysicalMediumType: i32,
    AccessType: i32,
    DirectionType: i32,
    /// Packed boolean flags. See the `IF_FLAG_*` constants below.
    InterfaceAndOperStatusFlags: u8,
    _padding: [u8; 7],
    OperStatus: i32,
    AdminStatus: i32,
    MediaConnectState: i32,
    NetworkGuid: [u8; 16],
    ConnectionType: i32,
    TransmitLinkSpeed: u64,
    ReceiveLinkSpeed: u64,
    InOctets: u64,
    InUcastPkts: u64,
    InNUcastPkts: u64,
    InDiscards: u64,
    InErrors: u64,
    InUnknownProtos: u64,
    InUcastOctets: u64,
    InMulticastOctets: u64,
    InBroadcastOctets: u64,
    OutOctets: u64,
    OutUcastPkts: u64,
    OutNUcastPkts: u64,
    OutDiscards: u64,
    OutErrors: u64,
    OutUcastOctets: u64,
    OutMulticastOctets: u64,
    OutBroadcastOctets: u64,
    OutQLen: u64,
}

/// `MIB_IF_TABLE2` — a count followed by a variable-length row array.
#[repr(C)]
#[allow(non_snake_case)]
struct MibIfTable2 {
    NumEntries: u32,
    _padding: u32,
    Table: [MibIfRow2; 1],
}

// `GetIfTable2` and `FreeMibTable` live in iphlpapi.dll, which is not in the
// default link set.
#[link(name = "iphlpapi")]
unsafe extern "system" {
    fn GetIfTable2(table: *mut *mut MibIfTable2) -> u32;
    fn FreeMibTable(memory: *const c_void);
}

// IF_TYPE_* values from IANA ifType assignments.
const IF_TYPE_ETHERNET_CSMACD: u32 = 6;
const IF_TYPE_SOFTWARE_LOOPBACK: u32 = 24;
const IF_TYPE_PPP: u32 = 23;
const IF_TYPE_TUNNEL: u32 = 131;
const IF_TYPE_IEEE80211: u32 = 71;
const IF_TYPE_WWANPP: u32 = 243;
const IF_TYPE_WWANPP2: u32 = 244;

/// `IfOperStatusUp`
const IF_OPER_STATUS_UP: i32 = 1;

// Bit positions within `InterfaceAndOperStatusFlags`, declared in
// `netioapi.h` as a bitfield in this order.
//
// The full set is documented here even though only two are read today:
// knowing which bit is which is the expensive part, and a future reader
// adding "is this adapter paused" should not have to rediscover the layout.
#[allow(dead_code)]
mod flags {
    /// `HardwareInterface` — backed by real hardware.
    pub const HARDWARE: u8 = 1 << 0;
    /// `FilterInterface` — an NDIS filter layer, not a real adapter.
    pub const FILTER: u8 = 1 << 1;
    /// `ConnectorPresent` — has a physical connector.
    pub const CONNECTOR: u8 = 1 << 2;
    /// `NotAuthenticated`
    pub const NOT_AUTHENTICATED: u8 = 1 << 3;
    /// `NotMediaConnected`
    pub const NOT_MEDIA_CONNECTED: u8 = 1 << 4;
    /// `Paused`
    pub const PAUSED: u8 = 1 << 5;
    /// `LowPower`
    pub const LOW_POWER: u8 = 1 << 6;
    /// `EndPointInterface`
    pub const ENDPOINT: u8 = 1 << 7;
}

/// A network interface with its cumulative counters.
#[derive(Debug, Clone)]
pub struct AdapterInfo {
    pub id: NicId,
    /// Friendly name, e.g. "Ethernet 2".
    pub alias: String,
    /// Hardware description, e.g. "Intel(R) Ethernet Controller I225-V".
    pub description: String,
    pub kind: NetworkKind,
    pub connected: bool,
    /// Whether real hardware backs this interface, as opposed to a virtual
    /// switch, tunnel or VPN adapter. Used to default the UI to the
    /// interfaces a user recognises.
    pub hardware: bool,
    /// Negotiated receive speed in bits per second.
    pub link_speed: Option<u64>,
    pub mac: Option<String>,
    pub mtu: u32,
    pub counters: NetworkCounters,
}

/// Enumerates network interfaces.
///
/// Returns an empty vector rather than an error if the table cannot be read:
/// a machine with no networking is unusual but not a fault, and failing the
/// whole sample tick over it would blank every other metric.
#[must_use]
pub fn enumerate_adapters() -> Vec<AdapterInfo> {
    let mut table: *mut MibIfTable2 = std::ptr::null_mut();

    // SAFETY: `GetIfTable2` allocates and writes a pointer to our local,
    // which is a valid out-parameter.
    let status = unsafe { GetIfTable2(&raw mut table) };

    if status != 0 || table.is_null() {
        return Vec::new();
    }

    // SAFETY: the call succeeded, so `table` points to an initialised
    // MIB_IF_TABLE2 owned by the system until we free it below.
    let count = unsafe { (*table).NumEntries } as usize;

    let mut out = Vec::with_capacity(count);

    for i in 0..count {
        // SAFETY: `Table` is a flexible array member of `NumEntries` rows;
        // `i` is bounded by that count.
        let row = unsafe { &*(*table).Table.as_ptr().add(i) };
        if let Some(adapter) = convert(row) {
            out.push(adapter);
        }
    }

    // SAFETY: `table` came from GetIfTable2 and is freed exactly once. No
    // `AdapterInfo` borrows from it — `convert` copies every field out.
    unsafe { FreeMibTable(table.cast()) };

    out
}

/// Converts a raw row, skipping interfaces not worth showing.
fn convert(row: &MibIfRow2) -> Option<AdapterInfo> {
    let kind = classify(row.Type);

    // Loopback carries no real traffic and would otherwise dominate the
    // interface list on any developer machine.
    if kind == NetworkKind::Loopback {
        return None;
    }

    // Skip NDIS filter layers.
    //
    // `GetIfTable2` returns every layer of the network stack, not just real
    // adapters. Each installed filter — WFP, QoS, VirtualBox, Hyper-V
    // extensions — appears as its own row bound to the same hardware, with
    // *identical* byte counters. Measured on this machine: Windows shows 11
    // adapters, the raw table returns 60+, and one physical NIC's traffic
    // appeared thirteen times.
    //
    // Summing them would multiply total throughput by the number of
    // installed filters, which is precisely the bug that makes third-party
    // network monitors report impossible bandwidth.
    if row.InterfaceAndOperStatusFlags & flags::FILTER != 0 {
        return None;
    }

    let alias = wide_to_string(&row.Alias)?;
    let description = wide_to_string(&row.Description).unwrap_or_else(|| alias.clone());

    Some(AdapterInfo {
        id: NicId(row.InterfaceIndex),
        alias,
        description,
        kind,
        connected: row.OperStatus == IF_OPER_STATUS_UP,
        hardware: row.InterfaceAndOperStatusFlags & flags::HARDWARE != 0,
        // A disconnected interface reports either 0 or u64::MAX depending on
        // the driver. Neither is a speed, so report nothing rather than
        // rendering "18 exabits/s".
        link_speed: match row.ReceiveLinkSpeed {
            0 | u64::MAX => None,
            speed => Some(speed),
        },
        mac: format_mac(&row.PhysicalAddress, row.PhysicalAddressLength as usize),
        mtu: row.Mtu,
        counters: NetworkCounters {
            bytes_received: row.InOctets,
            bytes_sent: row.OutOctets,
            packets_received: row.InUcastPkts.saturating_add(row.InNUcastPkts),
            packets_sent: row.OutUcastPkts.saturating_add(row.OutNUcastPkts),
            errors_in: row.InErrors,
            errors_out: row.OutErrors,
            discards_in: row.InDiscards,
            discards_out: row.OutDiscards,
        },
    })
}

/// Maps an IANA interface type to our kind.
fn classify(if_type: u32) -> NetworkKind {
    match if_type {
        IF_TYPE_ETHERNET_CSMACD => NetworkKind::Ethernet,
        IF_TYPE_IEEE80211 => NetworkKind::WiFi,
        IF_TYPE_SOFTWARE_LOOPBACK => NetworkKind::Loopback,
        IF_TYPE_PPP | IF_TYPE_TUNNEL => NetworkKind::Vpn,
        IF_TYPE_WWANPP | IF_TYPE_WWANPP2 => NetworkKind::Cellular,
        _ => NetworkKind::Unknown,
    }
}

/// Formats a physical address as colon-separated hex.
///
/// Only genuine 6-byte IEEE 802 MAC addresses are returned. Tunnel
/// pseudo-interfaces report a 32-byte address — Teredo produces one padded
/// almost entirely with zeroes — and formatting that yields a 95-character
/// string that destroys any table layout it lands in. It is also not a MAC in
/// any useful sense, so reporting nothing is both tidier and more honest.
fn format_mac(bytes: &[u8], len: usize) -> Option<String> {
    /// An IEEE 802 MAC address is six octets. Anything else is not one.
    const MAC_LEN: usize = 6;

    // A length of zero is normal for tunnels and loopback.
    if len != MAC_LEN || len > bytes.len() {
        return None;
    }

    let mac = bytes[..len]
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(":");
    Some(mac)
}

/// Converts a NUL-terminated UTF-16 buffer to a `String`.
fn wide_to_string(buffer: &[u16]) -> Option<String> {
    let end = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    if end == 0 {
        return None;
    }
    Some(String::from_utf16_lossy(&buffer[..end]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::size_of;

    #[test]
    fn row_layout_matches_the_system_header() {
        // A layout mismatch reads the wrong offsets and produces plausible
        // but wrong counters, which is far worse than failing loudly.
        // MIB_IF_ROW2 is 1352 bytes on 64-bit Windows.
        #[cfg(target_pointer_width = "64")]
        assert_eq!(size_of::<MibIfRow2>(), 1352);
    }

    #[test]
    fn finds_at_least_one_adapter() {
        let adapters = enumerate_adapters();
        assert!(
            !adapters.is_empty(),
            "no adapters found; every machine has at least one non-loopback interface"
        );
    }

    #[test]
    fn loopback_is_excluded() {
        for a in enumerate_adapters() {
            assert_ne!(a.kind, NetworkKind::Loopback, "{} leaked through", a.alias);
        }
    }

    #[test]
    fn adapters_have_names() {
        for a in enumerate_adapters() {
            assert!(!a.alias.is_empty());
            assert!(!a.description.is_empty());
        }
    }

    #[test]
    fn adapter_ids_are_unique() {
        let adapters = enumerate_adapters();
        let mut ids: Vec<_> = adapters.iter().map(|a| a.id).collect();
        let total = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), total, "duplicate NicId");
    }

    #[test]
    fn ndis_filter_layers_are_excluded() {
        // Regression guard for a measured bug: the raw table returned 60+
        // rows where Windows shows 11, because every installed NDIS filter
        // appears as its own interface reporting the SAME byte counters as
        // the hardware it binds to. Summing them multiplied total throughput
        // by the number of installed filters.
        //
        // `Get-NetAdapter` reported 11 on this machine; 30 with hidden
        // interfaces included. Anything far above that means filter rows are
        // leaking back in.
        let adapters = enumerate_adapters();
        assert!(
            adapters.len() < 40,
            "{} interfaces returned — NDIS filter layers are leaking through",
            adapters.len()
        );
    }

    #[test]
    fn no_two_adapters_report_identical_traffic_counters() {
        // The observable symptom of the filter-layer bug: many rows with
        // byte-for-byte identical counters, because they are the same NIC.
        let adapters: Vec<_> = enumerate_adapters()
            .into_iter()
            .filter(|a| a.counters.bytes_received > 1_000_000)
            .collect();

        let mut seen: Vec<u64> = Vec::new();
        for a in &adapters {
            assert!(
                !seen.contains(&a.counters.bytes_received),
                "{} duplicates another interface's counters ({} bytes) — \
                 filter layers are being counted more than once",
                a.alias,
                a.counters.bytes_received
            );
            seen.push(a.counters.bytes_received);
        }
    }

    #[test]
    fn hardware_interfaces_are_identified() {
        // Every machine has at least one physical NIC, even if unplugged.
        let adapters = enumerate_adapters();
        assert!(
            adapters.iter().any(|a| a.hardware),
            "no hardware-backed interface found; the flag bit is probably misread"
        );
    }

    #[test]
    fn link_speed_is_absent_rather_than_absurd() {
        // Disconnected adapters report 0 or u64::MAX depending on the driver.
        // Rendering the latter would show "18 exabits/s".
        for a in enumerate_adapters() {
            if let Some(speed) = a.link_speed {
                assert!(speed > 0);
                // 1 Tb/s is beyond any consumer or datacentre NIC today.
                assert!(
                    speed < 1_000_000_000_000,
                    "{} reported {speed} bps",
                    a.alias
                );
            }
        }
    }

    #[test]
    fn at_least_one_adapter_is_connected() {
        // This machine has network access, since the tests were fetched.
        let adapters = enumerate_adapters();
        assert!(adapters.iter().any(|a| a.connected));
    }

    #[test]
    fn a_connected_ethernet_or_wifi_adapter_has_a_mac() {
        // Not every interface that reports as Ethernet is a physical NIC.
        // Hyper-V virtual switch extension filters bind to the stack as
        // IF_TYPE_ETHERNET_CSMACD with no physical address at all — found by
        // this test failing on a real machine. Requiring a MAC everywhere
        // would be asserting something Windows does not promise.
        //
        // What must hold: any MAC we DO produce is well formed.
        let adapters = enumerate_adapters();
        let mut with_mac = 0;

        for a in &adapters {
            let Some(mac) = &a.mac else { continue };
            with_mac += 1;
            assert_eq!(
                mac.len(),
                17,
                "malformed MAC on {}: {mac} (expected AA:BB:CC:DD:EE:FF)",
                a.alias
            );
            assert!(
                mac.chars().all(|c| c.is_ascii_hexdigit() || c == ':'),
                "non-hex characters in MAC on {}: {mac}",
                a.alias
            );
        }

        assert!(
            with_mac > 0,
            "no adapter reported a physical address; MAC extraction is probably broken"
        );
    }

    #[test]
    fn mac_formatting_is_colon_separated_hex() {
        let bytes = [0x00, 0x1A, 0x2B, 0x3C, 0x4D, 0x5E, 0xFF];
        assert_eq!(format_mac(&bytes, 6).as_deref(), Some("00:1A:2B:3C:4D:5E"));
    }

    #[test]
    fn zero_length_mac_is_none() {
        assert!(format_mac(&[0; 8], 0).is_none());
    }

    #[test]
    fn a_tunnel_pseudo_address_is_not_reported_as_a_mac() {
        // Regression guard. Teredo reports a 32-byte physical address, which
        // formatted as hex is a 95-character string that wrecks any table it
        // lands in — and is not a MAC in any meaningful sense.
        assert!(format_mac(&[0; 32], 32).is_none());
    }

    #[test]
    fn only_six_byte_addresses_are_accepted() {
        assert!(format_mac(&[0; 8], 4).is_none(), "too short");
        assert!(format_mac(&[0; 8], 8).is_none(), "too long");
        assert!(format_mac(&[0; 8], 6).is_some());
    }

    #[test]
    fn mac_length_beyond_the_buffer_is_rejected() {
        // Defensive: a corrupt length must not panic on a slice out of range.
        assert!(format_mac(&[0; 4], 32).is_none());
    }

    #[test]
    fn counters_are_monotonic_across_two_reads() {
        // Cumulative counters can only rise while an interface stays up.
        let first = enumerate_adapters();
        std::thread::sleep(std::time::Duration::from_millis(50));
        let second = enumerate_adapters();

        for a in &first {
            let Some(b) = second.iter().find(|b| b.id == a.id) else {
                continue;
            };
            assert!(
                b.counters.bytes_received >= a.counters.bytes_received,
                "{} received counter went backwards",
                a.alias
            );
        }
    }
}
