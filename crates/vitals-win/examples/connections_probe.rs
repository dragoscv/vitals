//! Lists every TCP connection and bound UDP socket with its owning process.
//!
//! Cross-check against `netstat -ano` and `Get-NetTCPConnection`.
//!
//! Run with: `cargo run -p vitals-win --example connections_probe`

use std::collections::HashMap;
use std::time::Instant;

use vitals_core::ids::Pid;
use vitals_win::connections::{
    Connection, count_by_scope, group_by_process, tcp_v4, tcp_v6, udp_v4, udp_v6,
};
use vitals_win::process::ProcessEnumerator;

fn process_names() -> HashMap<Pid, String> {
    let mut enumerator = ProcessEnumerator::new();
    match enumerator.enumerate() {
        Ok(processes) => processes
            .into_iter()
            .filter_map(|p| p.name.map(|n| (p.key.pid, n)))
            .collect(),
        Err(err) => {
            eprintln!("warning: could not resolve process names: {err}");
            HashMap::new()
        }
    }
}

fn endpoint(conn: &Connection) -> String {
    match conn.remote() {
        Some(remote) => remote.to_string(),
        // UDP has no peer. A dash is honest; "0.0.0.0:0" would imply one.
        None => "—".into(),
    }
}

fn main() {
    // Time each table separately: the sampler has a 30ms total budget and
    // needs to know which of the four is worth making optional.
    let t0 = Instant::now();
    let tcp4 = tcp_v4();
    let tcp4_ms = t0.elapsed().as_secs_f64() * 1000.0;

    let t1 = Instant::now();
    let tcp6 = tcp_v6();
    let tcp6_ms = t1.elapsed().as_secs_f64() * 1000.0;

    let t2 = Instant::now();
    let udp4 = udp_v4();
    let udp4_ms = t2.elapsed().as_secs_f64() * 1000.0;

    let t3 = Instant::now();
    let udp6 = udp_v6();
    let udp6_ms = t3.elapsed().as_secs_f64() * 1000.0;

    let total_ms = t0.elapsed().as_secs_f64() * 1000.0;

    let tcp4 = tcp4.unwrap_or_else(|e| {
        eprintln!("TCP/IPv4 failed: {e}");
        Vec::new()
    });
    let tcp6 = tcp6.unwrap_or_else(|e| {
        eprintln!("TCP/IPv6 failed: {e}");
        Vec::new()
    });
    let udp4 = udp4.unwrap_or_else(|e| {
        eprintln!("UDP/IPv4 failed: {e}");
        Vec::new()
    });
    let udp6 = udp6.unwrap_or_else(|e| {
        eprintln!("UDP/IPv6 failed: {e}");
        Vec::new()
    });

    let mut all = Vec::new();
    all.extend(tcp4.iter().copied());
    all.extend(tcp6.iter().copied());
    all.extend(udp4.iter().copied());
    all.extend(udp6.iter().copied());

    let names = process_names();

    print_table(&all, &names);
    print_state_breakdown(&tcp4, &tcp6);
    print_scopes(&all);
    print_busiest(&all, &names);
    print_byte_order_check(&tcp4, &tcp6);

    println!("\n--- counts (cross-check against netstat -ano) ---");
    println!("  TCP IPv4 : {:>5}   ({tcp4_ms:.2}ms)", tcp4.len());
    println!("  TCP IPv6 : {:>5}   ({tcp6_ms:.2}ms)", tcp6.len());
    println!("  UDP IPv4 : {:>5}   ({udp4_ms:.2}ms)", udp4.len());
    println!("  UDP IPv6 : {:>5}   ({udp6_ms:.2}ms)", udp6.len());
    println!("  TOTAL    : {:>5}   ({total_ms:.2}ms)", all.len());
}

fn name_of(names: &HashMap<Pid, String>, pid: Pid) -> String {
    let mut name = names.get(&pid).cloned().unwrap_or_else(|| "?".to_string());
    name.truncate(24);
    name
}

fn print_table(all: &[Connection], names: &HashMap<Pid, String>) {
    println!(
        "{:>6}  {:<24}  {:<4}  {:<47}  {:<47}  {:<12}  {:<12}  SERVICE",
        "PID", "PROCESS", "PROT", "LOCAL", "REMOTE", "STATE", "SCOPE"
    );
    println!("{}", "-".repeat(180));

    // Public traffic first — it is what the user is looking for.
    let mut sorted = all.to_vec();
    sorted.sort_by_key(|c| (std::cmp::Reverse(c.scope()), c.pid, c.local.port()));

    for conn in &sorted {
        println!(
            "{:>6}  {:<24}  {:<4}  {:<47}  {:<47}  {:<12}  {:<12}  {}",
            conn.pid.get(),
            name_of(names, conn.pid),
            conn.transport.protocol_name(),
            conn.local.to_string(),
            endpoint(conn),
            conn.state().map_or("—", |s| s.netstat_name()),
            conn.scope().label(),
            conn.service_name().unwrap_or("—"),
        );
    }
}

fn print_state_breakdown(tcp4: &[Connection], tcp6: &[Connection]) {
    println!("\n--- TCP state breakdown (compare with Get-NetTCPConnection) ---");

    let mut states: HashMap<&'static str, usize> = HashMap::new();
    for state in tcp4.iter().chain(tcp6.iter()).filter_map(Connection::state) {
        *states.entry(state.netstat_name()).or_insert(0) += 1;
    }

    let mut states: Vec<_> = states.into_iter().collect();
    states.sort_unstable();
    for (state, count) in states {
        println!("  {state:<14} {count:>5}");
    }
}

fn print_scopes(all: &[Connection]) {
    println!("\n--- by scope ---");

    let mut scopes: Vec<_> = count_by_scope(all).into_iter().collect();
    scopes.sort_unstable_by_key(|(scope, _)| *scope);
    for (scope, count) in scopes {
        println!("  {:<14} {count:>5}", scope.label());
    }
}

fn print_busiest(all: &[Connection], names: &HashMap<Pid, String>) {
    println!("\n--- busiest processes ---");
    println!(
        "{:>6}  {:<24}  {:>5}  {:>5}  {:>5}  {:>7}  {:>9}  PUBLIC",
        "PID", "PROCESS", "TOTAL", "TCP", "UDP", "ACTIVE", "REM HOSTS"
    );

    for group in group_by_process(all).iter().take(20) {
        println!(
            "{:>6}  {:<24}  {:>5}  {:>5}  {:>5}  {:>7}  {:>9}  {:>6}",
            group.pid.get(),
            name_of(names, group.pid),
            group.total,
            group.tcp,
            group.udp,
            group.active,
            group.distinct_remote_hosts(),
            group.public,
        );
    }
}

/// Proves the ports were not read host-order: 443 byte-swapped is 47873,
/// which looks like a perfectly ordinary ephemeral port.
fn print_byte_order_check(tcp4: &[Connection], tcp6: &[Connection]) {
    println!("\n--- byte-order sanity check ---");

    let count = |port: u16| {
        tcp4.iter()
            .chain(tcp6.iter())
            .filter(|c| c.remote().is_some_and(|r| r.port() == port))
            .count()
    };

    println!("  port 443 rows  : {}", count(443));
    println!("  port 47873 rows: {} (expected 0)", count(47_873));
}
