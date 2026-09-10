//! Which address to put in the QR code.
//!
//! This is harder than it looks on a development machine. `local_ip()` picks
//! one address, and on a box with Hyper-V, WSL or Docker installed it very
//! often picks `172.x` from `vEthernet (WSL)` or `192.168.224.x` from the
//! Hyper-V Default Switch — addresses no phone can reach. A QR code pointing
//! at one of those produces "cannot connect" with nothing to debug.
//!
//! So: enumerate everything, drop what cannot be a LAN address, score what
//! remains, and let the user pick when there is genuine ambiguity. No
//! heuristic is right on every machine, and guessing silently is what makes
//! the failure confusing.

use std::net::{IpAddr, Ipv4Addr};

use serde::{Deserialize, Serialize};

/// A candidate address for the phone to connect to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Interface {
    /// The adapter name as the OS reports it, so the user can recognise it.
    pub name: String,
    pub address: Ipv4Addr,
    /// Our guess at whether this is the one a phone can reach. Higher is
    /// better; the UI preselects the highest.
    pub score: i32,
    /// True when the name matches a known virtual adapter. Kept rather than
    /// filtered out entirely: on a machine that genuinely only has a Hyper-V
    /// switch, hiding every option would leave the user with nothing.
    pub virtualised: bool,
}

/// Substrings that identify an adapter a phone will not be on.
const VIRTUAL_MARKERS: [&str; 8] = [
    "vethernet",
    "wsl",
    "hyper-v",
    "virtualbox",
    "vmware",
    "docker",
    "loopback",
    "tailscale",
];

/// Every usable IPv4 address on this machine, best guess first.
///
/// Filters to private, non-loopback, non-link-local addresses: a QR code
/// containing a public address would be inviting the internet in, and
/// 169.254.x means DHCP failed.
#[must_use]
pub fn interfaces() -> Vec<Interface> {
    let raw = local_ip_address::list_afinet_netifas().unwrap_or_default();
    let mut found: Vec<Interface> = raw
        .into_iter()
        .filter_map(|(name, ip)| match ip {
            IpAddr::V4(v4) if is_lan(v4) => Some(classify(name, v4)),
            _ => None,
        })
        .collect();

    // Stable order: score, then name, so the list does not reshuffle between
    // reads and move the option the user was about to click.
    found.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.name.cmp(&b.name)));
    found.dedup_by(|a, b| a.address == b.address);
    found
}

fn is_lan(v4: Ipv4Addr) -> bool {
    v4.is_private() && !v4.is_loopback() && !v4.is_link_local()
}

fn classify(name: String, address: Ipv4Addr) -> Interface {
    let lower = name.to_ascii_lowercase();
    let virtualised = VIRTUAL_MARKERS.iter().any(|m| lower.contains(m));

    // A virtual adapter is almost never the answer, so it sinks below
    // everything else. Among the rest, 192.168.x is the overwhelmingly common
    // home/office range and 10.x is the common corporate one; 172.16–31 is
    // most often a container bridge even on a non-virtual-looking adapter.
    let mut score = 0;
    if virtualised {
        score -= 100;
    }
    match address.octets() {
        [192, 168, ..] => score += 30,
        [10, ..] => score += 20,
        [172, second, ..] if (16..=31).contains(&second) => score += 5,
        _ => {}
    }
    if lower.contains("wi-fi") || lower.contains("wireless") || lower.contains("wlan") {
        // The phone is on Wi-Fi; so, usually, is the address it can reach.
        score += 10;
    } else if lower.contains("ethernet") {
        score += 8;
    }

    Interface {
        name,
        address,
        score,
        virtualised,
    }
}

/// The URL the QR code encodes.
///
/// The token lives in the **fragment**, which browsers never send to the
/// server. It therefore cannot appear in a request line, an access log or a
/// `Referer` header; the page reads `location.hash` and moves it into an
/// `Authorization` header. It does land in browser history, which is why
/// tokens are revocable rather than permanent.
#[must_use]
pub fn pairing_url(address: Ipv4Addr, port: u16, token: &str) -> String {
    format!("http://{address}:{port}/mobile.html#t={token}")
}

/// The pairing QR as an SVG string, ready to drop into the settings panel.
///
/// Rendered here rather than in the webview because everything it needs — the
/// chosen interface, the bound port, the secret — is already on this side,
/// and rendering it in JavaScript would mean handing the token to the
/// frontend a second time purely to draw it.
///
/// # Errors
///
/// Fails only if the URL is too long for a QR code, which cannot happen with
/// a 43-character token and an IPv4 address.
pub fn pairing_qr_svg(url: &str) -> Result<String, qrcode::types::QrError> {
    use qrcode::render::svg;
    use qrcode::{EcLevel, QrCode};

    // Medium error correction: the code is read off a bright screen at close
    // range, so the high levels only make the modules smaller and harder.
    let code = QrCode::with_error_correction_level(url.as_bytes(), EcLevel::M)?;
    Ok(code
        .render::<svg::Color<'_>>()
        .min_dimensions(240, 240)
        // Rendered in the app's own colours by the caller via CSS; these are
        // the fallback for a bare SVG.
        .dark_color(svg::Color("#000000"))
        .light_color(svg::Color("#ffffff"))
        .build())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn score_of(name: &str, addr: [u8; 4]) -> Interface {
        classify(name.to_owned(), Ipv4Addr::from(addr))
    }

    #[test]
    fn public_and_loopback_addresses_are_not_lan() {
        assert!(!is_lan(Ipv4Addr::new(8, 8, 8, 8)));
        assert!(!is_lan(Ipv4Addr::LOCALHOST));
        // DHCP failed; nothing can reach this.
        assert!(!is_lan(Ipv4Addr::new(169, 254, 1, 1)));
        assert!(is_lan(Ipv4Addr::new(192, 168, 1, 10)));
        assert!(is_lan(Ipv4Addr::new(10, 0, 0, 5)));
    }

    #[test]
    fn a_real_wifi_adapter_outranks_the_wsl_bridge() {
        // The exact failure this module exists to prevent: on this machine
        // `local_ip()` returns the WSL address and the phone cannot reach it.
        let wifi = score_of("Wi-Fi", [192, 168, 1, 20]);
        let wsl = score_of("vEthernet (WSL (Hyper-V firewall))", [172, 30, 96, 1]);
        assert!(wifi.score > wsl.score, "{wifi:?} vs {wsl:?}");
        assert!(wsl.virtualised);
        assert!(!wifi.virtualised);
    }

    #[test]
    fn docker_and_vmware_adapters_are_recognised() {
        assert!(score_of("Docker Desktop Bridge", [172, 17, 0, 1]).virtualised);
        assert!(score_of("VMware Network Adapter VMnet8", [192, 168, 40, 1]).virtualised);
        assert!(score_of("Ethernet 2", [10, 1, 1, 1]).virtualised.eq(&false));
    }

    #[test]
    fn ordering_is_stable_for_equal_scores() {
        // A list that reshuffles between reads moves the option the user is
        // about to click.
        let mut a = [
            score_of("Ethernet 2", [10, 0, 0, 2]),
            score_of("Ethernet 1", [10, 0, 0, 1]),
        ];
        a.sort_by(|x, y| y.score.cmp(&x.score).then_with(|| x.name.cmp(&y.name)));
        assert_eq!(a[0].name, "Ethernet 1");
    }

    #[test]
    fn the_token_is_in_the_fragment_not_the_path_or_query() {
        let url = pairing_url(Ipv4Addr::new(192, 168, 1, 20), 7331, "SECRET");
        assert_eq!(url, "http://192.168.1.20:7331/mobile.html#t=SECRET");

        let (before_fragment, fragment) = url.split_once('#').expect("has a fragment");
        assert!(
            !before_fragment.contains("SECRET"),
            "a token before the # reaches the server and its logs: {url}"
        );
        assert!(fragment.contains("SECRET"));
    }

    #[test]
    fn the_qr_encodes_a_real_url() {
        let url = pairing_url(Ipv4Addr::new(192, 168, 1, 20), 7331, &"a".repeat(43));
        let svg = pairing_qr_svg(&url).expect("a 60-character URL always fits");
        assert!(
            svg.starts_with("<?xml") || svg.starts_with("<svg"),
            "{}",
            &svg[..40]
        );
        assert!(svg.contains("width=") && svg.contains("height="));
    }

    #[test]
    fn enumerating_this_machine_does_not_panic_and_is_sorted() {
        // Cannot assert on addresses — CI has different adapters — but the
        // contract that it returns something ordered and finite must hold.
        let found = interfaces();
        for pair in found.windows(2) {
            assert!(pair[0].score >= pair[1].score, "{found:?}");
        }
    }
}
