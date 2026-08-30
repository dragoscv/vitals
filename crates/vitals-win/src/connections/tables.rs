//! Raw `GetExtendedTcpTable` / `GetExtendedUdpTable` enumeration.
//!
//! ## The two-call buffer protocol
//!
//! Both APIs are sized by the caller. The first call passes a null buffer and
//! a zero size and is *expected to fail* with `ERROR_INSUFFICIENT_BUFFER`,
//! writing the required byte count back through the size pointer. Only then
//! can the buffer be allocated and the call repeated. Treating the first
//! failure as a real error yields a permanently empty list.
//!
//! The size can also change between the two calls — a connection opening in
//! that window grows the table — so the second call is retried a bounded
//! number of times rather than assumed to fit.
//!
//! ## Byte order
//!
//! Addresses and ports inside these structures are in **network byte order**
//! while every other field is host order. The port fields are `DWORD`-sized
//! but only the low 16 bits are meaningful, and those 16 bits are big-endian.
//! Reading them directly turns 443 into 47873 — a plausible-looking ephemeral
//! port, which is exactly why it survives casual review. See
//! [`port_from_dword`].

use std::ffi::c_void;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};

use vitals_core::error::{Error, Result};
use vitals_core::ids::Pid;

use super::types::{Connection, TcpState, Transport};

/// `AF_INET`.
const AF_INET: u32 = 2;
/// `AF_INET6`.
const AF_INET6: u32 = 23;

/// `TCP_TABLE_OWNER_PID_ALL` — every connection, with its owning PID.
const TCP_TABLE_OWNER_PID_ALL: i32 = 5;
/// `UDP_TABLE_OWNER_PID` — every bound socket, with its owning PID.
const UDP_TABLE_OWNER_PID: i32 = 1;

const NO_ERROR: u32 = 0;
const ERROR_INSUFFICIENT_BUFFER: u32 = 122;

/// How many times the sized call is retried when the table grows between the
/// sizing call and the fetch. Three is generous: each retry is a fresh size,
/// and a table that grows three times in a row on a machine this busy would
/// keep growing however long we waited.
const MAX_SIZING_ATTEMPTS: u32 = 3;

/// `MIB_TCPROW_OWNER_PID`, from `tcpmib.h`.
#[repr(C)]
#[derive(Clone, Copy)]
#[allow(non_snake_case)]
struct MibTcpRowOwnerPid {
    dwState: u32,
    dwLocalAddr: u32,
    dwLocalPort: u32,
    dwRemoteAddr: u32,
    dwRemotePort: u32,
    dwOwningPid: u32,
}

/// `MIB_TCP6ROW_OWNER_PID`.
#[repr(C)]
#[derive(Clone, Copy)]
#[allow(non_snake_case)]
struct MibTcp6RowOwnerPid {
    ucLocalAddr: [u8; 16],
    dwLocalScopeId: u32,
    dwLocalPort: u32,
    ucRemoteAddr: [u8; 16],
    dwRemoteScopeId: u32,
    dwRemotePort: u32,
    dwState: u32,
    dwOwningPid: u32,
}

/// `MIB_UDPROW_OWNER_PID`.
#[repr(C)]
#[derive(Clone, Copy)]
#[allow(non_snake_case)]
struct MibUdpRowOwnerPid {
    dwLocalAddr: u32,
    dwLocalPort: u32,
    dwOwningPid: u32,
}

/// `MIB_UDP6ROW_OWNER_PID`.
#[repr(C)]
#[derive(Clone, Copy)]
#[allow(non_snake_case)]
struct MibUdp6RowOwnerPid {
    ucLocalAddr: [u8; 16],
    dwLocalScopeId: u32,
    dwLocalPort: u32,
    dwOwningPid: u32,
}

/// The common header of all four tables: a count followed by a flexible
/// array of rows.
#[repr(C)]
#[allow(non_snake_case)]
struct MibTable<T> {
    dwNumEntries: u32,
    table: [T; 1],
}

#[link(name = "iphlpapi")]
unsafe extern "system" {
    fn GetExtendedTcpTable(
        pTcpTable: *mut c_void,
        pdwSize: *mut u32,
        bOrder: i32,
        ulAf: u32,
        TableClass: i32,
        Reserved: u32,
    ) -> u32;

    fn GetExtendedUdpTable(
        pUdpTable: *mut c_void,
        pdwSize: *mut u32,
        bOrder: i32,
        ulAf: u32,
        TableClass: i32,
        Reserved: u32,
    ) -> u32;
}

/// Extracts a port from a `DWORD` field holding a big-endian `u16`.
///
/// The high 16 bits are reserved and are *not* always zero, so they are
/// masked off before the byte swap rather than after.
#[must_use]
pub(super) const fn port_from_dword(dword: u32) -> u16 {
    u16::from_be(dword as u16)
}

/// Builds an IPv4 address from a network-order `DWORD`.
///
/// `Ipv4Addr::from(u32)` expects host order, so the raw value is taken as
/// bytes instead — the bytes are already in the wire order the address
/// literal uses.
#[must_use]
pub(super) const fn ipv4_from_dword(dword: u32) -> Ipv4Addr {
    Ipv4Addr::from_bits(u32::from_be(dword))
}

/// A buffer aligned for any of the row types.
///
/// `u64` backing gives 8-byte alignment, comfortably above the 4 bytes every
/// row requires, so the pointer casts below are always aligned.
struct TableBuffer {
    words: Vec<u64>,
}

impl TableBuffer {
    fn with_bytes(bytes: u32) -> Self {
        let words = (bytes as usize).div_ceil(size_of::<u64>()).max(1);
        Self {
            words: vec![0_u64; words],
        }
    }

    fn as_mut_ptr(&mut self) -> *mut c_void {
        self.words.as_mut_ptr().cast()
    }

    fn byte_capacity(&self) -> u32 {
        // A table larger than 4 GiB cannot be requested by the API in the
        // first place — `pdwSize` is a DWORD — so the cast cannot lose data.
        (self.words.len() * size_of::<u64>()) as u32
    }
}

/// Which of the two APIs to call.
#[derive(Clone, Copy)]
enum Protocol {
    Tcp,
    Udp,
}

impl Protocol {
    fn table_class(self) -> i32 {
        match self {
            Self::Tcp => TCP_TABLE_OWNER_PID_ALL,
            Self::Udp => UDP_TABLE_OWNER_PID,
        }
    }

    fn context(self, family: u32) -> String {
        let proto = match self {
            Self::Tcp => "GetExtendedTcpTable",
            Self::Udp => "GetExtendedUdpTable",
        };
        let af = if family == AF_INET { "IPv4" } else { "IPv6" };
        format!("{proto} ({af})")
    }
}

/// Invokes the API once with the given buffer.
///
/// # Safety
///
/// `buffer` must be null with `*size == 0`, or point to at least `*size`
/// writable, correctly aligned bytes.
unsafe fn call(protocol: Protocol, family: u32, buffer: *mut c_void, size: *mut u32) -> u32 {
    // SAFETY: the caller guarantees the buffer/size pairing. `bOrder` is a
    // plain BOOL and `Reserved` must be zero per the documentation.
    unsafe {
        match protocol {
            Protocol::Tcp => {
                GetExtendedTcpTable(buffer, size, 0, family, protocol.table_class(), 0)
            }
            Protocol::Udp => {
                GetExtendedUdpTable(buffer, size, 0, family, protocol.table_class(), 0)
            }
        }
    }
}

/// Fetches a table into an owned buffer, honouring the two-call protocol.
///
/// # Errors
///
/// Returns [`Error::Os`] if the API fails for any reason other than needing
/// a larger buffer, or if the table keeps growing past
/// [`MAX_SIZING_ATTEMPTS`].
fn fetch(protocol: Protocol, family: u32) -> Result<TableBuffer> {
    let mut size: u32 = 0;

    // First call: deliberately undersized. ERROR_INSUFFICIENT_BUFFER here is
    // success, not failure — it is how the required size is reported.
    // SAFETY: a null buffer with a zero size is the documented sizing form.
    let status = unsafe { call(protocol, family, std::ptr::null_mut(), &raw mut size) };

    if status != ERROR_INSUFFICIENT_BUFFER && status != NO_ERROR {
        return Err(Error::Os {
            context: protocol.context(family),
            code: status.cast_signed(),
        });
    }

    for _ in 0..MAX_SIZING_ATTEMPTS {
        let mut buffer = TableBuffer::with_bytes(size);
        let mut capacity = buffer.byte_capacity();

        // SAFETY: `buffer` owns at least `capacity` writable bytes, aligned
        // to 8, and `capacity` is passed by pointer as the API requires.
        let status = unsafe { call(protocol, family, buffer.as_mut_ptr(), &raw mut capacity) };

        match status {
            NO_ERROR => return Ok(buffer),
            // The table grew between the sizing call and this one. `capacity`
            // now holds the new requirement.
            ERROR_INSUFFICIENT_BUFFER => size = capacity,
            code => {
                return Err(Error::Os {
                    context: protocol.context(family),
                    code: code.cast_signed(),
                });
            }
        }
    }

    Err(Error::Os {
        context: format!("{} kept growing", protocol.context(family)),
        code: ERROR_INSUFFICIENT_BUFFER.cast_signed(),
    })
}

/// Reads the row count and a pointer to the first row.
///
/// # Safety
///
/// `buffer` must hold a table of `T` rows written by the API — that is, it
/// must have come from a successful [`fetch`] for a table whose row type is
/// `T`. Pairing the wrong row type with a table is the failure mode this
/// module's size assertions exist to catch, and it cannot be checked here.
unsafe fn rows<T>(buffer: &TableBuffer) -> (usize, *const T) {
    let table = buffer.words.as_ptr().cast::<MibTable<T>>();

    // SAFETY: the caller guarantees `buffer` holds a `MIB_*_TABLE` whose
    // first field is the entry count, and `fetch` never returns a buffer
    // smaller than that header.
    let count = unsafe { (*table).dwNumEntries } as usize;
    // SAFETY: same guarantee; `table` is the flexible array member.
    let first = unsafe { (*table).table.as_ptr() };

    (count, first)
}

/// Enumerates IPv4 TCP connections.
///
/// # Errors
///
/// Returns [`Error::Os`] if `GetExtendedTcpTable` fails.
pub fn tcp_v4() -> Result<Vec<Connection>> {
    let buffer = fetch(Protocol::Tcp, AF_INET)?;
    // SAFETY: fetched as TCP/IPv4, so the rows are MIB_TCPROW_OWNER_PID.
    let (count, first) = unsafe { rows::<MibTcpRowOwnerPid>(&buffer) };

    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        // SAFETY: `i` is bounded by the count the API itself reported, and
        // the stride is the row size pinned by `struct_sizes_match_windows`.
        let row = unsafe { &*first.add(i) };

        out.push(Connection {
            local: SocketAddr::V4(SocketAddrV4::new(
                ipv4_from_dword(row.dwLocalAddr),
                port_from_dword(row.dwLocalPort),
            )),
            transport: Transport::Tcp {
                remote: SocketAddr::V4(SocketAddrV4::new(
                    ipv4_from_dword(row.dwRemoteAddr),
                    port_from_dword(row.dwRemotePort),
                )),
                state: TcpState::from_raw(row.dwState),
            },
            pid: Pid(row.dwOwningPid),
            local_scope_id: None,
            remote_scope_id: None,
        });
    }

    Ok(out)
}

/// Enumerates IPv6 TCP connections.
///
/// # Errors
///
/// Returns [`Error::Os`] if `GetExtendedTcpTable` fails.
pub fn tcp_v6() -> Result<Vec<Connection>> {
    let buffer = fetch(Protocol::Tcp, AF_INET6)?;
    // SAFETY: fetched as TCP/IPv6, so the rows are MIB_TCP6ROW_OWNER_PID.
    let (count, first) = unsafe { rows::<MibTcp6RowOwnerPid>(&buffer) };

    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        // SAFETY: bounded by the API's own count, correct stride per the
        // pinned row size.
        let row = unsafe { &*first.add(i) };

        let local_port = port_from_dword(row.dwLocalPort);
        let remote_port = port_from_dword(row.dwRemotePort);

        out.push(Connection {
            local: SocketAddr::V6(SocketAddrV6::new(
                Ipv6Addr::from(row.ucLocalAddr),
                local_port,
                0,
                row.dwLocalScopeId,
            )),
            transport: Transport::Tcp {
                remote: SocketAddr::V6(SocketAddrV6::new(
                    Ipv6Addr::from(row.ucRemoteAddr),
                    remote_port,
                    0,
                    row.dwRemoteScopeId,
                )),
                state: TcpState::from_raw(row.dwState),
            },
            pid: Pid(row.dwOwningPid),
            local_scope_id: Some(row.dwLocalScopeId),
            remote_scope_id: Some(row.dwRemoteScopeId),
        });
    }

    Ok(out)
}

/// Enumerates IPv4 bound UDP sockets.
///
/// # Errors
///
/// Returns [`Error::Os`] if `GetExtendedUdpTable` fails.
pub fn udp_v4() -> Result<Vec<Connection>> {
    let buffer = fetch(Protocol::Udp, AF_INET)?;
    // SAFETY: fetched as UDP/IPv4, so the rows are MIB_UDPROW_OWNER_PID.
    let (count, first) = unsafe { rows::<MibUdpRowOwnerPid>(&buffer) };

    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        // SAFETY: bounded by the API's own count, correct stride per the
        // pinned row size.
        let row = unsafe { &*first.add(i) };

        out.push(Connection {
            local: SocketAddr::V4(SocketAddrV4::new(
                ipv4_from_dword(row.dwLocalAddr),
                port_from_dword(row.dwLocalPort),
            )),
            transport: Transport::Udp,
            pid: Pid(row.dwOwningPid),
            local_scope_id: None,
            remote_scope_id: None,
        });
    }

    Ok(out)
}

/// Enumerates IPv6 bound UDP sockets.
///
/// # Errors
///
/// Returns [`Error::Os`] if `GetExtendedUdpTable` fails.
pub fn udp_v6() -> Result<Vec<Connection>> {
    let buffer = fetch(Protocol::Udp, AF_INET6)?;
    // SAFETY: fetched as UDP/IPv6, so the rows are MIB_UDP6ROW_OWNER_PID.
    let (count, first) = unsafe { rows::<MibUdp6RowOwnerPid>(&buffer) };

    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        // SAFETY: bounded by the API's own count, correct stride per the
        // pinned row size.
        let row = unsafe { &*first.add(i) };

        out.push(Connection {
            local: SocketAddr::V6(SocketAddrV6::new(
                Ipv6Addr::from(row.ucLocalAddr),
                port_from_dword(row.dwLocalPort),
                0,
                row.dwLocalScopeId,
            )),
            transport: Transport::Udp,
            pid: Pid(row.dwOwningPid),
            local_scope_id: Some(row.dwLocalScopeId),
            remote_scope_id: None,
        });
    }

    Ok(out)
}

/// Enumerates all four tables.
///
/// Partial failure is tolerated: a machine with IPv6 disabled by policy
/// still has a perfectly good IPv4 table, and failing the whole section
/// would show the user nothing rather than most of the answer.
#[must_use]
pub fn enumerate_all() -> Vec<Connection> {
    let mut out = Vec::new();

    for rows in [tcp_v4(), tcp_v6(), udp_v4(), udp_v6()]
        .into_iter()
        .flatten()
    {
        out.extend(rows);
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn struct_sizes_match_windows() {
        // These are read via pointer arithmetic across a flexible array, so
        // a wrong size does not fail to compile — it silently reads the next
        // row at the wrong offset and produces plausible garbage addresses.
        // Values taken from tcpmib.h / udpmib.h.
        assert_eq!(
            size_of::<MibTcpRowOwnerPid>(),
            24,
            "MIB_TCPROW_OWNER_PID is 6 DWORDs; a wrong stride yields wrong IPs, not an error"
        );
        assert_eq!(
            size_of::<MibTcp6RowOwnerPid>(),
            56,
            "MIB_TCP6ROW_OWNER_PID is 2x16 bytes of address plus 6 DWORDs"
        );
        assert_eq!(
            size_of::<MibUdpRowOwnerPid>(),
            12,
            "MIB_UDPROW_OWNER_PID is 3 DWORDs"
        );
        assert_eq!(
            size_of::<MibUdp6RowOwnerPid>(),
            28,
            "MIB_UDP6ROW_OWNER_PID is 16 bytes of address plus 3 DWORDs"
        );

        // Alignment matters as much as size: the table header is a DWORD
        // count followed immediately by the rows, so a row demanding 8-byte
        // alignment would introduce padding Windows did not write.
        assert_eq!(align_of::<MibTcpRowOwnerPid>(), 4);
        assert_eq!(align_of::<MibTcp6RowOwnerPid>(), 4);
        assert_eq!(align_of::<MibUdpRowOwnerPid>(), 4);
        assert_eq!(align_of::<MibUdp6RowOwnerPid>(), 4);
    }

    #[test]
    fn table_header_puts_rows_immediately_after_the_count() {
        // If the compiler padded between the count and the first row, every
        // enumeration would be shifted by four bytes.
        assert_eq!(
            std::mem::offset_of!(MibTable<MibTcpRowOwnerPid>, table),
            4,
            "MIB_TCPTABLE_OWNER_PID rows start at offset 4, right after dwNumEntries"
        );
        assert_eq!(std::mem::offset_of!(MibTable<MibUdp6RowOwnerPid>, table), 4);
    }

    #[test]
    fn ports_are_read_as_big_endian() {
        // 443 on the wire is 0x01BB big-endian, stored as the DWORD 0xBB01.
        // Reading it host-order gives 47873, which looks exactly like a real
        // ephemeral port — the reason this needs an explicit test.
        assert_eq!(
            port_from_dword(0x0000_BB01),
            443,
            "HTTPS must decode as 443, not the byte-swapped 47873"
        );
        assert_eq!(port_from_dword(0x0000_5000), 80, "HTTP must decode as 80");
        assert_eq!(port_from_dword(0x0000_3500), 53, "DNS must decode as 53");

        // The high word is reserved and is not always zero; if it leaked
        // into the result the port would be nonsense.
        assert_eq!(
            port_from_dword(0xDEAD_BB01),
            443,
            "the reserved high word must be masked off before the byte swap"
        );

        assert_eq!(port_from_dword(0), 0);
        assert_eq!(port_from_dword(0x0000_FFFF), 65_535);
    }

    #[test]
    fn ipv4_addresses_are_read_in_wire_order() {
        // 127.0.0.1 arrives as the little-endian DWORD 0x0100007F.
        assert_eq!(
            ipv4_from_dword(0x0100_007F),
            Ipv4Addr::LOCALHOST,
            "a byte-swapped loopback address would read as 1.0.0.127"
        );
        assert_eq!(ipv4_from_dword(0), Ipv4Addr::UNSPECIFIED);
        assert_eq!(
            ipv4_from_dword(0x0A01_A8C0),
            Ipv4Addr::new(192, 168, 1, 10),
            "a typical LAN address must round-trip"
        );
    }

    #[test]
    fn ipv6_addresses_are_read_byte_for_byte() {
        // Unlike the v4 DWORD, the v6 address is already a byte array in
        // wire order, so no swap must be applied.
        let mut bytes = [0_u8; 16];
        bytes[15] = 1;
        assert_eq!(
            Ipv6Addr::from(bytes),
            Ipv6Addr::LOCALHOST,
            "::1 must not be byte-swapped the way the IPv4 DWORD is"
        );

        let link_local = [0xfe, 0x80, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x01];
        assert_eq!(
            Ipv6Addr::from(link_local),
            "fe80::1".parse::<Ipv6Addr>().expect("valid literal")
        );
    }

    #[test]
    fn an_empty_table_produces_no_rows() {
        // A table with a zero count must be read as empty rather than
        // dereferencing the flexible array member, which holds no valid row.
        let buffer = TableBuffer::with_bytes(size_of::<MibTable<MibTcpRowOwnerPid>>() as u32);
        // SAFETY: the buffer is zeroed, so dwNumEntries reads as 0 and no
        // row is dereferenced below.
        let (count, _) = unsafe { rows::<MibTcpRowOwnerPid>(&buffer) };
        assert_eq!(count, 0, "a zeroed table must enumerate as empty");
    }

    #[test]
    fn buffer_is_aligned_and_large_enough() {
        let mut buffer = TableBuffer::with_bytes(1);
        assert!(
            buffer.byte_capacity() >= 1,
            "a one-byte request must still allocate a usable buffer"
        );
        assert_eq!(
            buffer.as_mut_ptr() as usize % 8,
            0,
            "the buffer must be 8-byte aligned so every row type is aligned"
        );

        let mut big = TableBuffer::with_bytes(1_001);
        assert!(big.byte_capacity() >= 1_001);
        assert_eq!(big.as_mut_ptr() as usize % 8, 0);
    }

    #[test]
    fn tcp_v4_enumeration_agrees_with_itself() {
        // Every Windows machine has at least one TCP/IPv4 socket — the RPC
        // endpoint mapper on 135 listens from boot. An empty result here is
        // the exact symptom of a broken two-call protocol.
        let rows = tcp_v4().expect("GetExtendedTcpTable(AF_INET) must succeed on Windows");
        assert!(
            !rows.is_empty(),
            "no TCP/IPv4 rows at all almost certainly means the sizing call was mishandled"
        );

        for conn in &rows {
            assert!(
                !conn.is_ipv6(),
                "the AF_INET table must not yield IPv6 rows"
            );
            assert!(conn.state().is_some(), "every TCP row must carry a state");
        }
    }

    #[test]
    fn udp_rows_never_claim_a_peer() {
        let rows = udp_v4().expect("GetExtendedUdpTable(AF_INET) must succeed on Windows");
        for conn in &rows {
            assert_eq!(
                conn.remote(),
                None,
                "UDP is connectionless; a peer here would be fabricated"
            );
            assert_eq!(conn.state(), None);
        }
    }

    #[test]
    fn ipv6_tables_carry_scope_ids() {
        // IPv6 may be disabled by policy, in which case the table is empty
        // but the call still succeeds — that is a valid machine, not a
        // failure, so only the rows that exist are asserted on.
        let rows = tcp_v6().expect("GetExtendedTcpTable(AF_INET6) must succeed on Windows");
        for conn in &rows {
            assert!(conn.is_ipv6(), "the AF_INET6 table must yield IPv6 rows");
            assert!(
                conn.local_scope_id.is_some(),
                "an IPv6 row without a scope id makes fe80:: addresses ambiguous"
            );
        }
    }

    #[test]
    fn enumerate_all_covers_more_than_any_single_table() {
        let all = enumerate_all();
        let tcp4 = tcp_v4().unwrap_or_default();

        assert!(
            all.len() >= tcp4.len(),
            "the combined enumeration ({}) must include at least the TCP/IPv4 table ({})",
            all.len(),
            tcp4.len()
        );
        assert!(
            all.iter().any(|c| matches!(c.transport, Transport::Udp)),
            "every Windows machine has bound UDP sockets; none found suggests the UDP path is broken"
        );
    }
}
