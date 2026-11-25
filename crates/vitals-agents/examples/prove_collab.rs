//! Reads the real cross-agent state on this machine and prints what the
//! collaboration page would show: how many agents are present, how many
//! threads exist, and the newest thread's first line.
//!
//!     cargo run -p vitals-agents --example prove_collab
//!
//! Exits 0 even when agentq's coordination directory is absent: a machine
//! without agentq has nothing to prove, which is a fact to print, not a
//! failure. Fixtures cannot show that the real registry, message and job files
//! parse — only the machine can.

use vitals_agents::collab;

fn main() {
    let now = vitals_agents::time::now_ms();
    let Some(root) = vitals_agents::agentq::default_root() else {
        println!("no coordination directory: this environment has no user profile or HOME");
        return;
    };
    println!("coord root: {}", root.display());
    if !root.is_dir() {
        println!(
            "no coordination directory at {} — agentq has never run for this user",
            root.display()
        );
        return;
    }

    let snap = collab::read(&root, now);
    let live = snap.agents.iter().filter(|a| a.live).count();
    println!(
        "agents present: {live} of {} listed (present = last call within {} min)",
        snap.agents.len(),
        snap.liveness_min
    );
    for a in snap.agents.iter().take(8) {
        println!(
            "  {:<8} {:<9} repo={:<12} verb={:<8} calls={:?} last={}",
            if a.live { "present" } else { "idle" },
            a.client.as_deref().unwrap_or("?"),
            a.repo.as_deref().unwrap_or("—"),
            a.last_verb.as_deref().unwrap_or("—"),
            a.calls,
            a.last_purpose
                .as_deref()
                .map_or_else(|| "—".into(), |p| first_line(p, 48)),
        );
    }

    let answered = snap
        .threads
        .iter()
        .filter(|t| !t.answers.is_empty())
        .count();
    println!("threads: {} ({answered} answered)", snap.threads.len());
    if let Some(t) = snap.threads.first() {
        println!(
            "newest thread {} to {} [{:?}] from {}: {}",
            t.id,
            t.to,
            t.state.as_deref().unwrap_or("?"),
            t.from.session.as_deref().unwrap_or("?"),
            first_line(&t.message, 80),
        );
        for a in t.answers.iter().take(3) {
            println!(
                "  reply from {}: {}",
                a.session.as_deref().unwrap_or("?"),
                first_line(&a.message, 64),
            );
        }
    } else {
        println!("newest thread: none");
    }

    let running = snap.jobs.iter().filter(|j| j.state == "running").count();
    println!("jobs: {} running {running}", snap.jobs.len());
    for j in snap.jobs.iter().take(5) {
        println!(
            "  {:<8} {:<16} repo={:<12} {}",
            j.state,
            j.label.as_deref().unwrap_or(j.id.as_str()),
            j.repo.as_deref().unwrap_or("—"),
            first_line(j.command.as_deref().unwrap_or(""), 48),
        );
    }
    println!("taken at: {} ms since epoch", snap.taken_at_ms);
}

/// The first line of a free-text field, so a printed row stays one line.
fn first_line(text: &str, max: usize) -> String {
    let flat: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= max {
        return flat;
    }
    let cut: String = flat.chars().take(max).collect();
    format!("{cut}…")
}
