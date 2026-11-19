//! Prints what Windows Firewall says about this binary — the boundary a unit
//! test cannot reach, because the rules live in the registry of the machine
//! the code runs on.
//!
//! ```text
//! cargo run -p vitals-win --example prove_firewall
//! cargo run -p vitals-win --example prove_firewall -- 7331
//! ```
//!
//! The second question is the control: a port nothing could plausibly have a
//! rule for must answer differently from the first, or the reader is not
//! reading anything.

fn main() {
    let port: u16 = std::env::args()
        .nth(1)
        .and_then(|arg| arg.parse().ok())
        .unwrap_or(7331);
    // Nothing listens here and no rule should mention it.
    let control: u16 = 47331;

    let Ok(program) = std::env::current_exe() else {
        println!("this process has no path on disk; nothing to check");
        return;
    };
    println!("program: {}", program.display());
    let path = program.to_string_lossy().into_owned();

    let Some(rules) = vitals_win::firewall::rules() else {
        println!("UNKNOWN: the firewall rules could not be read on this machine");
        return;
    };
    println!("rules read: {}", rules.len());

    let naming = rules
        .iter()
        .filter(|rule| {
            rule.app
                .as_deref()
                .is_some_and(|app| app.eq_ignore_ascii_case(&path))
        })
        .count();
    println!("rules naming this exact program: {naming}");

    for (label, probe) in [("asked", port), ("control", control)] {
        println!("--- {label}: {probe} ---");
        let admitting = rules
            .iter()
            .filter(|rule| vitals_win::firewall::admits(rule, &path, probe))
            .count();
        println!("  rules that would admit it: {admitting}");
        match vitals_win::firewall::inbound_allowed(&program, probe) {
            Some(true) => println!("  INBOUND ALLOWED: a rule admits this program or port"),
            Some(false) => println!("  INBOUND BLOCKED: no allow rule names this program or port"),
            None => println!("  UNKNOWN: the rules could not be read on this machine"),
        }
    }

    println!(
        "\nA phone reaching this server needs the first line to be ALLOWED. If it is BLOCKED,\n\
         `vitals serve` prints the `New-NetFirewallRule` command to run as administrator."
    );
}
