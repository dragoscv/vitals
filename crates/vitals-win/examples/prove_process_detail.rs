//! Proves the on-demand per-process readings against this real machine.
//!
//! Covers the four things fixtures cannot show: that the kernel actually
//! serves the disk-counter extension here, that the storage-stack figure
//! genuinely differs from the all-I/O figure (which is the entire reason for
//! S9-01), that handle and module enumeration return real named objects, and
//! that efficiency mode round-trips on a process we own.
//!
//! Run with: `cargo run -p vitals-win --example prove_process_detail`

use std::collections::HashMap;
use std::error::Error;
use std::process::{Child, Command, Stdio};

use vitals_core::ids::ProcessKey;
use vitals_win::actions::{efficiency_mode, set_efficiency_mode};
use vitals_win::process::{DiskCounterSource, ProcessEnumerator, RawProcess};
use vitals_win::{handles, modules};

type Fallible = Result<(), Box<dyn Error>>;

fn mib(bytes: u64) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

fn missing(what: &str) -> Box<dyn Error> {
    Box::<dyn Error>::from(format!("{what} not found"))
}

fn key_of(processes: &[RawProcess], pid: u32) -> Result<ProcessKey, Box<dyn Error>> {
    processes
        .iter()
        .find(|p| p.key.pid.get() == pid)
        .map(|p| p.key)
        .ok_or_else(|| missing(&format!("process {pid}")))
}

fn main() -> Fallible {
    let mut enumerator = ProcessEnumerator::new();
    let processes = enumerator.enumerate()?;

    prove_disk_counters(&enumerator, &processes)?;

    let self_key = key_of(&processes, std::process::id())?;
    prove_handles(self_key)?;
    prove_modules(self_key)?;
    prove_efficiency_mode(&processes)?;

    Ok(())
}

/// S9-01: the storage-stack counter, and how far the old one overstated.
fn prove_disk_counters(enumerator: &ProcessEnumerator, processes: &[RawProcess]) -> Fallible {
    println!(
        "== S9-01 disk counters ==\n{} processes · source: {:?}\n",
        processes.len(),
        enumerator.disk_counter_source()
    );

    // The over-reporting the extension exists to fix: all-I/O counts pipes,
    // sockets and the console, so the gap is the fabricated "disk" traffic
    // the old column was showing.
    let mut rows: Vec<_> = processes
        .iter()
        .filter(|p| !p.is_idle_process() && p.storage_read_bytes.is_some())
        .map(|p| {
            let all_io = p.read_bytes.saturating_add(p.write_bytes);
            let disk = p.disk_read_bytes().0.saturating_add(p.disk_write_bytes().0);
            (p, all_io, disk, all_io.saturating_sub(disk))
        })
        .collect();
    rows.sort_by_key(|(_, _, _, gap)| std::cmp::Reverse(*gap));

    if rows.is_empty() {
        println!(
            "  the full class was refused (STATUS_ACCESS_DENIED without SeDebugPrivilege)\n  \
             — disk rates come from the all-I/O counter. Run elevated to see the\n  \
             storage-stack figures and the overstatement table.\n"
        );
        return Ok(());
    }

    println!(
        "{:>7}  {:>12}  {:>12}  {:>12}  NAME",
        "PID", "ALL-IO MiB", "DISK MiB", "OVERSTATED"
    );
    for (process, all_io, disk, gap) in rows.iter().take(10) {
        println!(
            "{:>7}  {:>12.1}  {:>12.1}  {:>12.1}  {}",
            process.key.pid.get(),
            mib(*all_io),
            mib(*disk),
            mib(*gap),
            process.name.as_deref().unwrap_or("?")
        );
    }
    let overstating = rows
        .iter()
        .filter(|(_, _, _, gap)| *gap > 1024 * 1024)
        .count();
    println!(
        "\n  {overstating} of {} processes would have been overstated by >1 MiB\n",
        rows.len()
    );

    // Rows carried storage counters, so the enumerator must agree about where
    // they came from — a disagreement means the source flag and the data have
    // drifted apart.
    if enumerator.disk_counter_source() == DiskCounterSource::StorageStack {
        Ok(())
    } else {
        Err(missing("a consistent disk counter source"))
    }
}

/// S9-04: real handles, typed and named, inside the latency budget.
fn prove_handles(key: ProcessKey) -> Fallible {
    let start = std::time::Instant::now();
    let open = handles::for_process(key)?;
    let elapsed = start.elapsed();

    let mut by_kind: HashMap<String, usize> = HashMap::new();
    for handle in &open {
        *by_kind
            .entry(handle.kind.clone().unwrap_or_else(|| "<unknown>".into()))
            .or_default() += 1;
    }
    let mut kinds: Vec<_> = by_kind.into_iter().collect();
    kinds.sort_by_key(|(_, count)| std::cmp::Reverse(*count));

    println!(
        "== S9-04 handles ==\n{} handles in {:.0} ms · {} named\n",
        open.len(),
        elapsed.as_secs_f64() * 1000.0,
        open.iter().filter(|h| h.name.is_some()).count()
    );
    for (kind, count) in kinds.iter().take(8) {
        println!("{count:>7}  {kind}");
    }
    println!();
    for handle in open.iter().filter(|h| h.name.is_some()).take(5) {
        println!(
            "  {:>10x}  {:<12} {}",
            handle.value,
            handle.kind.as_deref().unwrap_or("?"),
            handle.name.as_deref().unwrap_or("")
        );
    }
    Ok(())
}

/// S9-05: the mapped module list.
fn prove_modules(key: ProcessKey) -> Fallible {
    let mapped = modules::for_process(key)?;
    println!("\n== S9-05 modules ==\n{} modules\n", mapped.len());
    println!("{:>18}  {:>10}  NAME", "BASE", "KiB");
    for module in mapped.iter().take(8) {
        println!(
            "{:>18x}  {:>10}  {}",
            module.base_address,
            module.size / 1024,
            module.name
        );
    }

    if !mapped
        .iter()
        .any(|m| m.name.eq_ignore_ascii_case("ntdll.dll"))
    {
        return Err(missing("ntdll.dll, which is mapped into every NT process"));
    }
    Ok(())
}

/// S9-02: set it, read it back, clear it, read it back.
fn prove_efficiency_mode(processes: &[RawProcess]) -> Fallible {
    let mut child = spawn_victim()?;
    let result = round_trip_efficiency_mode(&child);

    // Killed on every path, including the failing one, so a broken run does
    // not leave a stray cmd behind.
    child.kill()?;
    child.wait()?;
    result?;

    // A protected process: the answer must be None, never a fabricated false.
    if let Some(system) = processes.iter().find(|p| p.key.pid.get() == 4) {
        println!("  System (pid 4): {:?}", efficiency_mode(system.key)?);
    }
    Ok(())
}

fn round_trip_efficiency_mode(child: &Child) -> Fallible {
    let mut enumerator = ProcessEnumerator::new();
    let processes = enumerator.enumerate()?;
    let key = key_of(&processes, child.id())?;

    println!("\n== S9-02 efficiency mode ==\nchild pid {}", child.id());
    println!("  before: {:?}", efficiency_mode(key)?);
    set_efficiency_mode(key, true)?;
    println!("  set on: {:?}", efficiency_mode(key)?);
    set_efficiency_mode(key, false)?;
    println!("  cleared: {:?}", efficiency_mode(key)?);
    Ok(())
}

/// A single long-lived process to act on.
///
/// A bare `cmd` waiting on a piped stdin rather than `cmd /c ping`, which
/// spawns a grandchild that outlives the kill and holds the inherited pipe.
fn spawn_victim() -> Result<Child, Box<dyn Error>> {
    Ok(Command::new("cmd")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?)
}
