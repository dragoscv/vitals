//! Whether Windows Firewall would let another device reach a program that is
//! listening on a port.
//!
//! ## Why this exists
//!
//! `vitals serve` binds the port, prints a pairing URL, and can then be
//! unreachable from the network because nothing created an inbound rule for
//! it: the installer creates one for `vitals-desktop.exe`, and the CLI is a
//! different program. Measured on 2026-11-19 (S24-22): listening on
//! `0.0.0.0:7331`, `200` on loopback, `HTTP 000` from a second host. A person
//! reads that as "pairing is broken", not "this binary has no rule".
//!
//! ## Why the registry and not `netsh`
//!
//! Windows keeps every rule in one registry key, one value per rule, in a
//! stable and language-independent form:
//!
//! ```text
//! v2.33|Action=Allow|Active=TRUE|Dir=In|Protocol=6|Profile=Private|LPort=7331|App=…|Name=…|
//! ```
//!
//! `netsh advfirewall` and `Get-NetFirewallRule` both print *localised*
//! labels, so a parser over their output would have to guess at the language.
//! The stored form does not.
//!
//! ## What this deliberately does not do
//!
//! It does not create a rule — that needs elevation, and a server that
//! escalates in order to be reachable is a worse trade than a warning. It also
//! does not model `Profile`: a rule that names any profile is treated as
//! admitting a connection. The answer is used for a hint, so a wrong "yes"
//! leaves things exactly as they were, while a wrong "no" costs one extra
//! line of output.

use std::path::Path;

use crate::startup::registry::{Hive, KEY_READ, KEY_WOW64_64KEY, RegKey, decode_string};

/// Where Windows stores the rules, one value per rule.
const RULES_KEY: &str =
    r"SYSTEM\CurrentControlSet\Services\SharedAccess\Parameters\FirewallPolicy\FirewallRules";

/// One stored rule, as far as reachability needs it.
///
/// Four independent booleans rather than a state machine: they are four
/// separate fields in the stored form (`Action`, `Dir`, `Active`, `Protocol`),
/// each of which the firewall sets on its own. Collapsing them into an enum
/// would invent an ordering the format does not have.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Rule {
    /// `Action=Allow`.
    pub allow: bool,
    /// `Dir=In`.
    pub inbound: bool,
    /// `Active=TRUE`. `FALSE` is a rule the user switched off.
    pub enabled: bool,
    /// `Protocol=6`, or no `Protocol` field at all, which means any protocol.
    pub tcp: bool,
    /// `App=…`, exactly as stored — a `%VAR%` reference is left alone.
    pub app: Option<String>,
    /// `LPort=…`.
    pub ports: Ports,
}

/// What a rule says about local ports.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Ports {
    /// No usable `LPort`: every port, which is how an app-only rule reads.
    #[default]
    Any,
    /// The ports it lists.
    Listed(Vec<u16>),
}

/// Parses one stored rule.
///
/// Unknown fields are ignored rather than rejected: Microsoft adds new ones
/// between releases (`TTK2_22`, `Defer`, `Svc`), and a rule this version does
/// not understand is still a rule.
#[must_use]
pub fn parse_rule(text: &str) -> Rule {
    let mut rule = Rule {
        tcp: true,
        ..Rule::default()
    };
    for field in text.split('|') {
        let Some((key, value)) = field.split_once('=') else {
            continue;
        };
        match key {
            "Action" => rule.allow = value.eq_ignore_ascii_case("Allow"),
            "Dir" => rule.inbound = value.eq_ignore_ascii_case("In"),
            "Active" => rule.enabled = value.eq_ignore_ascii_case("TRUE"),
            "Protocol" => rule.tcp = value == "6",
            "App" => rule.app = Some(value.to_owned()),
            "LPort" => rule.ports = parse_ports(value),
            _ => {}
        }
    }
    rule
}

/// `LPort` is a single port, a comma-separated list, or `Any`.
///
/// Anything unparseable is treated as `Any`, which is the direction that keeps
/// this a hint: a rule naming a range this does not model still stops a
/// spurious warning.
fn parse_ports(value: &str) -> Ports {
    if value.eq_ignore_ascii_case("Any") {
        return Ports::Any;
    }
    let listed: Vec<u16> = value
        .split(',')
        .filter_map(|p| p.trim().parse().ok())
        .collect();
    if listed.is_empty() {
        Ports::Any
    } else {
        Ports::Listed(listed)
    }
}

/// Whether a rule admits a remote connection to `program` on `port`.
///
/// A rule names a program, a port, or both. Either one admits the connection:
/// a port rule is open to whatever listens on it, so a rule written for
/// another program on the right port still answers this question.
#[must_use]
pub fn admits(rule: &Rule, program: &str, port: u16) -> bool {
    if !(rule.allow && rule.inbound && rule.enabled && rule.tcp) {
        return false;
    }
    let named_here = rule
        .app
        .as_deref()
        .is_some_and(|app| app.eq_ignore_ascii_case(program));
    let port_open = matches!(&rule.ports, Ports::Listed(listed) if listed.contains(&port));
    named_here || port_open
}

/// Every rule this machine stores, parsed.
///
/// `None` when the key cannot be read — a machine where the firewall has never
/// run, or a policy that denies read access to it. Exposed as well as
/// [`inbound_allowed`] so a prover can say how many rules it actually saw: an
/// empty list and a list where nothing matches are different facts, and only
/// the first means the reader is broken.
#[must_use]
pub fn rules() -> Option<Vec<Rule>> {
    let key = RegKey::open(Hive::LocalMachine, RULES_KEY, KEY_READ | KEY_WOW64_64KEY).ok()?;
    let mut parsed = Vec::new();
    for (_name, kind, data) in key.values() {
        if let Some(text) = decode_string(kind, &data) {
            parsed.push(parse_rule(&text));
        }
    }
    Some(parsed)
}

/// `Some(true)` when an inbound Allow rule admits this program or port,
/// `Some(false)` when the rules were read and none do, and `None` when they
/// could not be read at all.
///
/// Tri-state on purpose, like every other unmeasured value in this project: an
/// unreadable key is not evidence that the port is blocked, and a caller that
/// warned on it would be inventing a finding.
#[must_use]
pub fn inbound_allowed(program: &Path, port: u16) -> Option<bool> {
    let program = program.to_string_lossy().into_owned();
    Some(rules()?.iter().any(|rule| admits(rule, &program, port)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    /// The rule the laptop's installer-created allow entry actually holds
    /// (read from the machine on 2026-11-19), so the parser is exercised
    /// against the real shape rather than an idealised one.
    const REAL_INBOUND_APP_RULE: &str = "v2.10|Action=Allow|Active=TRUE|Dir=In|Protocol=6|Profile=Private|Profile=Public|App=C:\\users\\vladu\\appdata\\local\\programs\\vitals\\vitals-desktop.exe|Name=vitals-desktop.exe|Desc=vitals-desktop.exe|Defer=User|";

    /// The same machine's port rule for the LAN listener.
    const REAL_INBOUND_PORT_RULE: &str = "v2.33|Action=Allow|Active=TRUE|Dir=In|Protocol=6|Profile=Private|Profile=Public|LPort=7331|App=C:\\Users\\vladu\\AppData\\Local\\Programs\\Vitals\\vitals-desktop.exe|Name=Vitals LAN (dev test, removable)|";

    /// A stock Windows outbound rule, which must never answer this question.
    const REAL_OUTBOUND_RULE: &str = "v2.33|Action=Allow|Active=TRUE|Dir=Out|Protocol=6|Profile=Public|RPort=2869|RA4=LocalSubnet|RA6=LocalSubnet|App=%SystemRoot%\\system32\\svchost.exe|Svc=fdphost|Name=@FirewallAPI.dll,-32765|";

    const APP: &str = r"C:\Users\vladu\AppData\Local\Programs\Vitals\vitals.exe";

    #[test]
    fn an_inbound_allow_rule_for_the_program_admits_it_on_any_port() {
        let rule = parse_rule(REAL_INBOUND_APP_RULE);
        assert!(
            admits(
                &rule,
                r"c:\users\vladu\appdata\local\programs\vitals\vitals-desktop.exe",
                9
            ),
            "the path is compared case-insensitively, as Windows compares it"
        );
        assert!(!admits(&rule, APP, 7331));
    }

    #[test]
    fn an_inbound_allow_rule_for_the_port_admits_whatever_listens_on_it() {
        let rule = parse_rule(REAL_INBOUND_PORT_RULE);
        assert!(
            admits(&rule, APP, 7331),
            "a port rule is not program-specific"
        );
        assert!(!admits(&rule, APP, 7332), "but it is port-specific");
    }

    #[test]
    fn an_outbound_rule_never_admits_an_inbound_connection() {
        assert!(!admits(&parse_rule(REAL_OUTBOUND_RULE), APP, 7331));
        assert!(!admits(&parse_rule(REAL_OUTBOUND_RULE), APP, 2869));
    }

    #[test]
    fn a_disabled_or_blocking_rule_does_not_admit_anything() {
        let disabled = REAL_INBOUND_APP_RULE.replace("Active=TRUE", "Active=FALSE");
        assert!(!admits(&parse_rule(&disabled), APP, 7331));
        let blocked = REAL_INBOUND_PORT_RULE.replace("Action=Allow", "Action=Block");
        assert!(!admits(&parse_rule(&blocked), APP, 7331));
    }

    #[test]
    fn a_udp_rule_does_not_admit_the_tcp_listener() {
        let udp = REAL_INBOUND_PORT_RULE.replace("Protocol=6", "Protocol=17");
        assert!(!admits(&parse_rule(&udp), APP, 7331));
    }

    #[test]
    fn a_missing_protocol_field_means_any_protocol_and_still_admits() {
        let any = REAL_INBOUND_PORT_RULE.replace("Protocol=6|", "");
        assert!(admits(&parse_rule(&any), APP, 7331));
    }

    #[test]
    fn a_port_list_is_understood_and_any_means_the_rule_is_not_about_a_port() {
        let list = REAL_INBOUND_PORT_RULE.replace("LPort=7331", "LPort=7331,7332");
        assert!(admits(&parse_rule(&list), APP, 7332));
        assert!(!admits(&parse_rule(&list), APP, 7333));
        // `LPort=Any` names no port at all, so the rule is only about the
        // program beside it — which is not this one.
        let any = REAL_INBOUND_PORT_RULE.replace("LPort=7331", "LPort=Any");
        assert!(admits(
            &parse_rule(&any),
            r"C:\Users\vladu\AppData\Local\Programs\Vitals\vitals-desktop.exe",
            0
        ));
        assert!(!admits(&parse_rule(&any), APP, 7331));
    }

    #[test]
    fn an_unknown_field_is_ignored_rather_than_rejecting_the_rule() {
        let future = format!("{REAL_INBOUND_PORT_RULE}TTK3_99=something|");
        assert!(admits(&parse_rule(&future), APP, 7331));
    }

    #[test]
    fn a_rule_with_neither_app_nor_port_admits_nothing() {
        let bare = "v2.33|Action=Allow|Active=TRUE|Dir=In|Protocol=6|";
        assert!(!admits(&parse_rule(bare), APP, 7331));
    }
}
