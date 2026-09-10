//! `mDNS` advertisement, so the phone finds the desktop without typing an IP.
//!
//! Discovery is a convenience wrapped around the QR pairing flow in
//! [`crate::lan`]: the QR still carries the token, but a client that has
//! already paired can find the machine again after a DHCP lease change
//! instead of asking the user to rescan.
//!
//! **The advertisement exists only while the HTTP server is listening.** It is
//! created by the same call that binds the socket and torn down by the same
//! call that closes it. Nothing about this machine is broadcast when remote
//! access is off — that is the product promise, not an implementation detail,
//! so [`Advertisement::shutdown`] unregisters the record *and* stops the
//! responder thread rather than merely letting the record expire.
//!
//! Advertised on one address, not all of them. `lan.rs` exists because a
//! machine with Hyper-V, WSL or Docker has several addresses and only one of
//! them is reachable from a phone; announcing the others would put
//! unreachable `A` records in every neighbour's cache.

use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;

use mdns_sd::{IfKind, ServiceDaemon, ServiceInfo};

/// The service type clients browse for.
///
/// `_vitals` is not registered with IANA. That is normal for a local-only
/// service and is why the `TXT` record carries a model version: a future
/// incompatible wire format is distinguished by that, not by a new type.
pub const SERVICE_TYPE: &str = "_vitals._tcp.local.";

/// `TXT` key holding [`vitals_core::MODEL_VERSION`].
///
/// A client reads this *before* connecting, so an old phone can say "this
/// desktop speaks a newer protocol" instead of failing on the first frame.
pub const TXT_MODEL_VERSION: &str = "model";

/// `TXT` key holding the human-readable application version.
pub const TXT_APP_VERSION: &str = "version";

/// The standard `mDNS` port. Tests use another one to stay off the wire the
/// operating system's own responder is using.
const MDNS_PORT: u16 = 5353;

/// How long to wait for the responder to confirm the goodbye packet.
///
/// Short on purpose: this runs on the UI's stop path, and a client that misses
/// the goodbye simply ages the record out. Blocking the user's "off" switch
/// for longer than this would be worse than a stale cache entry.
const UNREGISTER_TIMEOUT: Duration = Duration::from_millis(500);

/// A live `mDNS` registration.
///
/// Dropping this does **not** stop the advertisement — call
/// [`Advertisement::shutdown`]. The same convention as
/// [`crate::ServeHandle::stop`], and for the same reason: a silent teardown on
/// drop means an unrelated refactor that moves the handle can switch broadcast
/// off (or, worse, leave it on) without anything in the diff saying so.
pub struct Advertisement {
    /// `None` once shut down, so a second call is a no-op rather than an
    /// error against a dead daemon.
    daemon: Option<ServiceDaemon>,
    fullname: String,
}

// Hand-written: `ServiceDaemon` is not `Debug`, and the workspace warns on a
// public type that is not. The registration state is what a log line needs.
impl std::fmt::Debug for Advertisement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Advertisement")
            .field("fullname", &self.fullname)
            .field("registered", &self.daemon.is_some())
            .finish()
    }
}

impl Advertisement {
    /// The registered instance name, e.g. `desk._vitals._tcp.local.`.
    #[must_use]
    pub fn fullname(&self) -> &str {
        &self.fullname
    }

    /// Withdraws the record and stops the responder thread.
    ///
    /// Sends a goodbye packet (`TTL` 0) so neighbours drop the entry
    /// immediately rather than keeping a dead host for the remaining TTL, then
    /// shuts the daemon down so no socket is left bound. Idempotent.
    pub fn shutdown(&mut self) {
        let Some(daemon) = self.daemon.take() else {
            return;
        };

        match daemon.unregister(&self.fullname) {
            // Waiting matters: `shutdown` below closes the sockets, and a
            // goodbye still queued at that point is never transmitted.
            Ok(status) => {
                if let Err(error) = status.recv_timeout(UNREGISTER_TIMEOUT) {
                    tracing::warn!(%error, "mDNS goodbye was not confirmed");
                }
            }
            Err(error) => tracing::warn!(%error, "could not unregister the mDNS service"),
        }

        if let Err(error) = daemon.shutdown() {
            tracing::warn!(%error, "could not stop the mDNS responder");
        }
    }
}

/// Starts advertising the local API on one address.
///
/// `address` is the interface the user chose, or the best guess from
/// [`crate::interfaces`]; `port` is the port the HTTP server actually bound,
/// which is not necessarily the one that was requested.
///
/// Callers must treat failure as non-fatal — discovery is a nicety and the
/// server is perfectly usable through the QR code without it.
///
/// # Errors
///
/// Returns the underlying `mdns-sd` error if the responder cannot be created
/// (typically multicast being unavailable) or the service description is
/// rejected.
pub fn advertise(
    address: Ipv4Addr,
    port: u16,
    app_version: &str,
) -> Result<Advertisement, mdns_sd::Error> {
    advertise_on(MDNS_PORT, address, port, app_version)
}

/// [`advertise`], with the multicast port as a seam.
///
/// Tests announce on a private port so they neither collide with the operating
/// system's responder nor put a real `_vitals` record on the office network.
fn advertise_on(
    mdns_port: u16,
    address: Ipv4Addr,
    port: u16,
    app_version: &str,
) -> Result<Advertisement, mdns_sd::Error> {
    let daemon = ServiceDaemon::new_with_port(mdns_port)?;

    // Requirement, not an optimisation: announce on the chosen adapter only.
    // Disabling everything first means a new adapter appearing later (a VPN
    // coming up, a phone tethering) does not silently widen the broadcast.
    daemon.disable_interface(IfKind::All)?;
    daemon.enable_interface(IfKind::Addr(IpAddr::V4(address)))?;

    let instance = instance_name();
    let mut info = ServiceInfo::new(
        SERVICE_TYPE,
        &instance,
        &format!("{instance}.local."),
        // The string form of the address is the one `AsIpAddrs` documents for
        // every address type, so it cannot silently resolve to a slice impl.
        address.to_string().as_str(),
        port,
        &[
            (TXT_MODEL_VERSION, vitals_core::MODEL_VERSION.to_string()),
            (TXT_APP_VERSION, app_version.to_owned()),
        ][..],
    )?;
    // Belt and braces with `enable_interface` above: this also constrains the
    // addresses the daemon would auto-fill if `addr_auto` is ever turned on.
    info.set_interfaces(vec![IfKind::Addr(IpAddr::V4(address))]);

    let fullname = info.get_fullname().to_owned();
    daemon.register(info)?;

    Ok(Advertisement {
        daemon: Some(daemon),
        fullname,
    })
}

/// The instance name neighbours see, derived from the machine name.
///
/// Read from the environment rather than through a `hostname` crate: Windows
/// always sets `COMPUTERNAME`, and the fallback chain below covers the two
/// non-Windows cases without adding a dependency for one string.
///
/// Sanitised because a machine name may contain characters that are legal in
/// `NetBIOS` and awkward in a DNS label; a dot in particular would create a
/// spurious extra level in the name.
fn instance_name() -> String {
    let raw = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .ok()
        .or_else(|| {
            std::fs::read_to_string("/etc/hostname")
                .ok()
                .map(|s| s.trim().to_owned())
        })
        .unwrap_or_default();

    sanitise_label(&raw)
}

/// Keeps a hostname to characters that are safe in a DNS label.
fn sanitise_label(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '-'
            }
        })
        // A label is capped at 63 bytes; every character kept above is one
        // byte, so a character count is a byte count here.
        .take(63)
        .collect();

    let trimmed = cleaned.trim_matches('-');
    if trimmed.is_empty() {
        // An unnamed machine still needs a resolvable label, and an empty one
        // makes `register` reject the whole service.
        "vitals".to_owned()
    } else {
        trimmed.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mdns_sd::ServiceEvent;

    /// Browsing on loopback: no LAN, no router, and nothing this test does
    /// escapes the machine.
    const TEST_ADDRESS: Ipv4Addr = Ipv4Addr::LOCALHOST;

    /// Private multicast ports, one per test. The system responder owns 5353,
    /// and a test that announced there would advertise a fake desktop to the
    /// office. One port each rather than one shared: tests run concurrently,
    /// they all publish the same instance name, and on a shared port the
    /// lifecycle test saw the TXT test's responder and failed for the wrong
    /// reason.
    const LIFECYCLE_MDNS_PORT: u16 = 5454;
    const TXT_MDNS_PORT: u16 = 5455;
    const IDEMPOTENCE_MDNS_PORT: u16 = 5456;

    /// Generous: registration probes for conflicts before announcing, which
    /// takes several hundred milliseconds by design (RFC 6762 §8.1).
    const DISCOVERY_TIMEOUT: Duration = Duration::from_secs(5);

    /// Folds events into `present` until `stop` accepts it, or `window` ends.
    ///
    /// `present` is `&mut` on purpose: the lifecycle test threads one set
    /// through both halves. Starting the second half from an empty set would
    /// make "nothing is advertised" trivially true the instant it was called,
    /// which is precisely how an earlier version of this test passed against
    /// a responder that had not been shut down at all.
    fn watch_until(
        events: &mdns_sd::Receiver<ServiceEvent>,
        present: &mut Vec<String>,
        window: Duration,
        stop: impl Fn(&[String]) -> bool,
    ) {
        if stop(present) {
            return;
        }
        let deadline = std::time::Instant::now() + window;
        while let Some(remaining) = deadline.checked_duration_since(std::time::Instant::now()) {
            match events.recv_timeout(remaining) {
                Ok(ServiceEvent::ServiceResolved(service)) => {
                    if !present.contains(&service.fullname) {
                        present.push(service.fullname.clone());
                    }
                }
                Ok(ServiceEvent::ServiceRemoved(_, fullname)) => {
                    present.retain(|f| f != &fullname);
                }
                Ok(_) => {}
                Err(_) => break,
            }
            if stop(present) {
                break;
            }
        }
    }

    #[test]
    fn the_advertisement_stops_when_the_server_stops() {
        // The product promise: nothing broadcasts while remote access is off.
        // Proven against a real responder on the wire, not by inspecting our
        // own state — an `Option` set to `None` would say nothing about what
        // is still being transmitted.
        //
        // ONE browser spans the whole lifecycle, deliberately. A fresh browser
        // after the shutdown was tried and is worthless: presence is proved by
        // the responder's unsolicited announcement (a push), whereas a later
        // query needs the responder to *answer* (a pull), which a
        // loopback-scoped responder never hears. A mutant that leaked the
        // responder passed that version. Both halves must observe pushes, and
        // the goodbye packet is the push that proves teardown.
        let browser = ServiceDaemon::new_with_port(LIFECYCLE_MDNS_PORT).expect("a browsing daemon");
        let events = browser.browse(SERVICE_TYPE).expect("browsing starts");

        let mut advert = advertise_on(LIFECYCLE_MDNS_PORT, TEST_ADDRESS, 7331, "0.0.0-test")
            .expect("advertising");
        let wanted = advert.fullname().to_owned();

        let mut seen: Vec<String> = Vec::new();
        watch_until(&events, &mut seen, DISCOVERY_TIMEOUT, |s| {
            s.contains(&wanted)
        });
        assert!(
            seen.contains(&wanted),
            "the running server was not discoverable: saw {seen:?}, wanted {wanted}"
        );

        advert.shutdown();

        // The goodbye must arrive and REMOVE the instance found above. If
        // teardown were skipped no `ServiceRemoved` is ever sent, the set stays
        // populated, and this fails when the window expires — which is exactly
        // the failure we want.
        watch_until(&events, &mut seen, DISCOVERY_TIMEOUT, <[String]>::is_empty);
        let _ = browser.shutdown();
        assert!(
            seen.is_empty(),
            "still advertising after the server stopped: {seen:?}"
        );
    }

    #[test]
    fn the_txt_record_carries_the_model_and_app_versions() {
        // A client uses these to refuse an incompatible desktop before it
        // opens a socket, so a missing key is a silent compatibility failure.
        let mut advert =
            advertise_on(TXT_MDNS_PORT, TEST_ADDRESS, 7331, "9.9.9").expect("advertising");

        let browser = ServiceDaemon::new_with_port(TXT_MDNS_PORT).expect("a browsing daemon");
        let events = browser.browse(SERVICE_TYPE).expect("browsing starts");

        let deadline = std::time::Instant::now() + DISCOVERY_TIMEOUT;
        let mut resolved = None;
        while let Some(remaining) = deadline.checked_duration_since(std::time::Instant::now()) {
            match events.recv_timeout(remaining) {
                Ok(ServiceEvent::ServiceResolved(service)) => {
                    resolved = Some(service);
                    break;
                }
                Ok(_) => {}
                Err(_) => break,
            }
        }
        let _ = browser.shutdown();

        let service = resolved.expect("the service resolves");
        assert_eq!(
            service.port, 7331,
            "the advertised port must be the bound one"
        );
        assert_eq!(
            service.get_property_val_str(TXT_MODEL_VERSION),
            Some(vitals_core::MODEL_VERSION.to_string().as_str())
        );
        assert_eq!(service.get_property_val_str(TXT_APP_VERSION), Some("9.9.9"));

        advert.shutdown();
    }

    #[test]
    fn shutting_down_twice_is_harmless() {
        // `stop_lan_server` is reachable twice (the UI switch, then app exit),
        // and the second call must not talk to a dead daemon.
        let mut advert = advertise_on(IDEMPOTENCE_MDNS_PORT, TEST_ADDRESS, 7331, "0.0.0-test")
            .expect("advertising");
        advert.shutdown();
        advert.shutdown();
    }

    #[test]
    fn a_machine_name_becomes_a_legal_dns_label() {
        // Windows machine names admit characters a DNS label does not; a dot
        // would silently add a level to the name and break resolution.
        assert_eq!(sanitise_label("DESK-01"), "DESK-01");
        assert_eq!(sanitise_label("my desk"), "my-desk");
        assert_eq!(sanitise_label("host.example"), "host-example");
        // Leading and trailing separators are stripped, not just replaced.
        assert_eq!(sanitise_label("_lab_"), "lab");
        assert_eq!(sanitise_label(""), "vitals");
        assert_eq!(sanitise_label("???"), "vitals");
        assert!(sanitise_label(&"a".repeat(200)).len() <= 63);
    }

    #[test]
    fn the_service_type_is_the_one_clients_browse_for() {
        // `register` rejects anything not ending in `._tcp.local.`, and a
        // typo here would only surface as "nothing is ever discovered".
        assert!(SERVICE_TYPE.ends_with("._tcp.local."));
        assert_eq!(SERVICE_TYPE, "_vitals._tcp.local.");
    }
}
