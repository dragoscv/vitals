//! Reads the real agentq state and AI sessions on this machine and prints
//! what the AI page would show, plus how long each read took.
//!
//!     cargo run -p vitals-agents --example prove_agents

use std::time::Instant;

// A prover is a flat script that prints one section after another; splitting
// it into helpers would only scatter the output order across functions.
#[allow(clippy::too_many_lines)]
fn main() {
    let probe = |pid: u32| -> Option<u64> { start_ms(pid) };
    let mut reader = vitals_agents::Reader::detect();
    println!("coord root: {:?}", reader.root());

    let t = Instant::now();
    let s = reader.read(&probe);
    let first = t.elapsed();
    let t = Instant::now();
    let s2 = reader.read(&probe);
    let second = t.elapsed();

    println!("first read {first:?}, incremental read {second:?}");
    if let Some(root) = reader.root().cloned() {
        let now = vitals_agents::time::now_ms();
        let time = |name: &str, f: &dyn Fn()| {
            let t = Instant::now();
            f();
            print!(" {name}={}ms", t.elapsed().as_millis());
        };
        print!("sections:");
        time("resources", &|| {
            drop(vitals_agents::agentq::read_resources(&root, &probe, now));
        });
        time("leases", &|| {
            drop(vitals_agents::agentq::read_leases(&root, &probe, now));
        });
        time("releases", &|| {
            drop(vitals_agents::agentq::read_releases(&root, now));
        });
        time("jobs", &|| {
            drop(vitals_agents::agentq::read_jobs(&root, &probe, now));
        });
        time("registry", &|| {
            drop(vitals_agents::agentq::read_registry(&root, now));
        });
        let mut j = vitals_agents::journal::JournalTail::new();
        let t = Instant::now();
        j.poll(&root.join("journal.jsonl"), now);
        print!(" journal-cold={}ms", t.elapsed().as_millis());
        let t = Instant::now();
        j.poll(&root.join("journal.jsonl"), now);
        let _ = j.stats(now, 24);
        print!(" journal-warm+stats={}ms", t.elapsed().as_millis());
        let mut tracker =
            vitals_agents::sessions::SessionTracker::new(vitals_agents::Roots::detect());
        let t = Instant::now();
        let _ = tracker.poll(&probe, now);
        print!(" sessions-cold={}ms", t.elapsed().as_millis());
        let t = Instant::now();
        let _ = tracker.poll(&probe, now);
        println!(" sessions-warm={}ms", t.elapsed().as_millis());
    }
    println!("agentq installed: {}", s.agentq_installed);
    println!("resources: {}", s.resources.len());
    for r in s.resources.iter().take(8) {
        let held = r.tickets.iter().filter(|t| t.held).count();
        println!(
            "  {:<28} cap {:?} held {held} waiting {}",
            r.name,
            r.capacity,
            r.tickets.len() - held
        );
        for t in r.tickets.iter().take(3) {
            println!(
                "     {} {:?} {:?} {}",
                if t.held { "H" } else { "w" },
                t.liveness,
                t.lane,
                t.purpose
                    .as_deref()
                    .unwrap_or("")
                    .chars()
                    .take(60)
                    .collect::<String>()
            );
        }
    }
    let stale = s.leases.iter().filter(|l| l.stale).count();
    println!("leases: {} ({stale} stale)", s.leases.len());
    println!("releases: {}", s.releases.len());
    let running = s.jobs.iter().filter(|j| j.state == "running").count();
    let suspect = s.jobs.iter().filter(|j| j.state == "suspect").count();
    let dead = s.jobs.iter().filter(|j| j.state == "dead").count();
    println!(
        "jobs (24 h): {} running {running} suspect {suspect} dead {dead}",
        s.jobs.len()
    );
    println!(
        "marks: {:?}",
        s.marks
            .iter()
            .map(|m| format!("{}={}", m.resource, m.state))
            .collect::<Vec<_>>()
    );
    println!("registry callers (24 h): {}", s.registry.len());
    for c in s.registry.iter().take(5) {
        println!(
            "  {} client={:?} app={:?} repo={:?} calls={:?}",
            c.session, c.client, c.app, c.repo, c.calls
        );
    }
    println!(
        "cloud down: {:?}",
        s.cloud_down
            .iter()
            .map(|c| &c.workspace)
            .collect::<Vec<_>>()
    );
    println!(
        "machine cap: {:?}",
        s.machine.as_ref().map(|m| (m.cap, m.cpu, m.why.clone()))
    );
    println!(
        "journal events in window: {} (timeline {})",
        s.journal.len(),
        s2.journal.len()
    );
    let st = &s.stats;
    println!(
        "stats {}h: runs {} fail {} commits {} cloud {} wait p50 {:?} p90 {:?} hold p50 {:?} p90 {:?} wait total {:.0} min",
        st.window_hours,
        st.runs,
        st.failures,
        st.commits,
        st.cloud_runs,
        st.wait_p50_sec,
        st.wait_p90_sec,
        st.hold_p50_sec,
        st.hold_p90_sec,
        st.wait_total_sec / 60.0
    );
    for r in st.by_repo.iter().take(5) {
        println!(
            "  {:<12} runs {:>4} fail {:>3} hold {:>6.0} min wait {:>6.0} min",
            r.repo,
            r.runs,
            r.failures,
            r.hold_total_sec / 60.0,
            r.wait_total_sec / 60.0
        );
    }
    // a count, not the titles: titles are the user's own words
    println!(
        "sessions (24 h): {} ({} titled)",
        s.sessions.len(),
        s.sessions.iter().filter(|x| x.title.is_some()).count()
    );
    for x in s.sessions.iter().take(12) {
        println!(
            "  {:?} {:?} {:<20} active {:>4?} min turns {:>3?} tools {:>4?} now={:?} model={:?} pid={:?}",
            x.client,
            x.state,
            x.repo
                .as_deref()
                .unwrap_or("-")
                .chars()
                .take(20)
                .collect::<String>(),
            x.active_ms.map(|m| m / 60_000),
            x.turns,
            x.tool_calls,
            x.current_tool,
            x.model,
            x.pid
        );
    }
    let json = serde_json::to_string(&s).map_or(0, |j| j.len());
    println!("snapshot JSON: {} KB", json / 1024);

    prove_usage(&probe);
}

/// Samples this machine for real and prints the resource rows the AI page
/// would draw.
///
/// A fixture cannot show that a real sampler's per-process cpu, working set
/// and disk rates survive the fold, nor which of the tool names on this
/// machine the catalogue actually recognises — the two questions worth asking
/// before believing the panel.
#[cfg(windows)]
fn prove_usage(probe: &dyn Fn(u32) -> Option<u64>) {
    let mut sampler = vitals_win::SystemSampler::new();
    let mut builder = vitals_win::FrameBuilder::new();
    // Per-process cpu is a delta between two ticks, so the first sample is
    // 0 % for every process on the machine. Throw it away and fold the
    // second, which is what the app does.
    let _ = sampler.sample();
    std::thread::sleep(std::time::Duration::from_millis(700));
    let Ok(sample) = sampler.sample() else {
        println!("usage: sampling failed, nothing to fold");
        return;
    };
    let elapsed = sample.elapsed_ms;
    let frame = builder.build(sample);
    let processes = match frame.payload {
        vitals_core::sample::FramePayload::Keyframe { processes, .. } => processes,
        vitals_core::sample::FramePayload::Delta { .. } => Vec::new(),
    };

    let now = vitals_agents::time::now_ms();
    let root = vitals_agents::agentq::default_root();
    let t = Instant::now();
    let owned = root
        .as_ref()
        .map(|r| vitals_agents::usage::read_owned(r, probe, now))
        .unwrap_or_default();
    let read = t.elapsed();

    println!(
        "usage: {} processes over {elapsed} ms, read_owned {read:?}",
        processes.len()
    );
    println!("  recorded identities: {}", owned.len());
    for o in &owned {
        println!("    pid {} start {:?} -> {:?}", o.pid, o.start_ms, o.kind);
    }

    let snapshot = vitals_agents::usage::rollup(&processes, &owned, probe, now);
    let agentq = snapshot.agentq.as_ref();
    println!(
        "  agentq row: {}",
        agentq.map_or("absent (no running job recorded)".to_string(), describe)
    );
    if snapshot.clients.is_empty() {
        println!("  clients: no row (no process on this machine matched the catalogue)");
    }
    for row in &snapshot.clients {
        println!("  client: {}", describe(row));
    }
}

#[cfg(not(windows))]
fn prove_usage(_probe: &dyn Fn(u32) -> Option<u64>) {
    println!("usage: only Windows has the process counters this folds");
}

/// Prints a row with every series named, so an absent one reads as `None`
/// and never as a zero.
fn describe(row: &vitals_core::agents::AgentUsage) -> String {
    format!(
        "{:?} processes={} cpu={:.1}% private={} ws={} disk r={}/s w={}/s net rx={:?} tx={:?} gpu={:?} vram={:?}",
        row.kind,
        row.processes,
        row.cpu.0,
        row.memory_private.0,
        row.memory_working_set.0,
        row.disk_read.0,
        row.disk_write.0,
        row.net_rx.map(|v| v.0),
        row.net_tx.map(|v| v.0),
        row.gpu.map(|v| v.0),
        row.gpu_memory.map(|v| v.0),
    )
}

#[cfg(windows)]
fn start_ms(pid: u32) -> Option<u64> {
    vitals_win::actions::process_start_ms(pid)
}

#[cfg(not(windows))]
fn start_ms(_pid: u32) -> Option<u64> {
    None
}
