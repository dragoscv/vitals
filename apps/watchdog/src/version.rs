//! Notices that a new release exists, even when Vitals is not running.
//!
//! The app tells its own user about a waiting update (see `updates::prompt` in
//! the desktop crate), and for the failure that started this — a machine that
//! sat on 1.1.0 through four releases while the app ran in its tray — that is
//! enough, because the app was running the whole time.
//!
//! This covers the other case: Vitals closed, or never started, on a machine
//! where the watchdog is the only thing of ours running. It asks GitHub for the
//! latest release tag itself rather than reading a file the app wrote, because
//! the app may not have run at all since the release came out.
//!
//! Two properties this must keep, both from the watchdog's ADR:
//!
//! * it **never acts** — no download, no install, no file touched but its own
//!   timestamp; and
//! * it is **once a day**, because a logon-time process that repeated itself
//!   would be the nagging it exists to avoid.
//!
//! The installed version is this binary's own: the app and the watchdog ship
//! from one release, so `CARGO_PKG_VERSION` here *is* the installed app version
//! and no registry read is needed.

use std::path::PathBuf;
use std::time::{Duration, SystemTime};

/// How long between checks. Six hours: a release is not urgent enough to ask
/// GitHub more often, and a machine that sleeps through part of the day still
/// catches up.
pub const CHECK_EVERY: Duration = Duration::from_secs(6 * 60 * 60);

/// How long between reminders once a new release is known.
pub const REMIND_EVERY: Duration = Duration::from_secs(24 * 60 * 60);

/// The release page the request reads. The API rather than the HTML page: one
/// field, no scraping, and it is the same source the app's updater uses.
const LATEST_URL: &str = "https://api.github.com/repos/dragoscv/vitals/releases/latest";

/// The version this build is, and therefore the version installed when the two
/// came from one release.
#[must_use]
pub fn installed() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// A release tag without its `v`.
///
/// Tags are `v1.3.0` and the version in every manifest is `1.3.0`; comparing
/// them without this would make `v1.3.0` fail to parse and silently report no
/// update ever.
#[must_use]
pub fn tag_version(tag: &str) -> &str {
    tag.trim().strip_prefix('v').unwrap_or(tag.trim())
}

/// Whether `latest` is a newer release than `installed`.
///
/// Compares the numeric components, not the strings: `1.10.0` is newer than
/// `1.9.0`, and a lexical comparison would say the opposite — the classic way
/// an update check silently stops offering updates.
///
/// Anything unparseable is **not** newer. A tag this code cannot read is a
/// reason to say nothing, never to tell someone an update exists.
#[must_use]
pub fn is_newer(installed: &str, latest: &str) -> bool {
    let Some(installed) = components(installed) else {
        return false;
    };
    let Some(latest) = components(latest) else {
        return false;
    };
    // Missing components are zero, so `1.3` and `1.3.0` are the same release
    // rather than an update.
    let width = installed.len().max(latest.len());
    for i in 0..width {
        let a = installed.get(i).copied().unwrap_or(0);
        let b = latest.get(i).copied().unwrap_or(0);
        if b != a {
            return b > a;
        }
    }
    false
}

/// The numeric components of a version, ignoring any pre-release or build
/// suffix.
///
/// A suffix is dropped rather than ordered: `1.4.0-beta.1` is compared as
/// `1.4.0`, which can offer a pre-release to someone on the final `1.3.0`. That
/// is deliberate — GitHub's "latest release" excludes pre-releases and drafts —
/// and it errs towards a reminder, not towards a wrong install, because this
/// path installs nothing.
fn components(version: &str) -> Option<Vec<u64>> {
    let core = version.trim().split(['-', '+']).next().unwrap_or_default();
    if core.is_empty() {
        return None;
    }
    core.split('.')
        .map(|part| part.parse::<u64>().ok())
        .collect()
}

/// The latest release tag from GitHub, or `None` when it could not be read.
///
/// `None` for every failure — offline, rate limited, a proxy that answers HTML,
/// a changed response shape — because all of them mean the same thing here:
/// say nothing. A watchdog that toasted "could not check for updates" would be
/// reporting its own problem as the user's.
#[cfg(windows)]
#[must_use]
pub fn latest_release() -> Option<String> {
    let body = get(LATEST_URL)?;
    let json: serde_json::Value = serde_json::from_str(&body).ok()?;
    let tag = json.get("tag_name")?.as_str()?;
    Some(tag_version(tag).to_owned())
}

/// No Windows, no check.
///
/// `None` rather than a stand-in: this module is compiled and tested on the
/// Linux CI leg, where the pure comparison is what matters, and a fabricated
/// answer would be worse than none both there and on any platform the watchdog
/// does not run on.
#[cfg(not(windows))]
#[must_use]
pub fn latest_release() -> Option<String> {
    None
}

/// Starts the daily release check on its own thread.
///
/// Reports through `tx`, the same channel the action workers use, so the
/// watchdog has exactly one place that raises a notification. The version of
/// this binary is the version installed, because both come from one release.
///
/// Every failure is silent and retried on the next check: offline, rate
/// limited, a proxy answering something else. A watchdog that reported its own
/// network trouble would be telling the user about a problem they do not have.
#[cfg(windows)]
pub fn start_checking(
    tx: std::sync::mpsc::Sender<String>,
    strings: &'static crate::strings::Strings,
) {
    let _ = std::thread::Builder::new()
        .name("vitals-release-check".into())
        // A parked thread costs a stack and nothing else, and sleeping six
        // hours between checks is exactly what it does.
        .stack_size(128 * 1024)
        .spawn(move || {
            // A release is not urgent, and the first seconds after logon belong
            // to whatever the person is actually doing.
            std::thread::sleep(FIRST_CHECK_DELAY);
            loop {
                if let Some(latest) = latest_release()
                    && should_remind(&latest, SystemTime::now())
                {
                    let message = strings
                        .update_available
                        .replace("{installed}", installed())
                        .replace("{latest}", &latest);
                    // A closed channel means the loop is gone, which means this
                    // process is exiting; nothing to do but stop.
                    if tx.send(message).is_err() {
                        return;
                    }
                }
                std::thread::sleep(CHECK_EVERY);
            }
        });
}

/// How long after logon to wait before the first check.
const FIRST_CHECK_DELAY: Duration = Duration::from_secs(120);

/// Whether today's reminder has been shown, and records it when it has not.
///
/// The answer is about *releases*, not about checks: a check that finds nothing
/// must be cheap and silent, while a reminder must not repeat within a day.
pub fn should_remind(latest: &str, now: SystemTime) -> bool {
    if !is_newer(installed(), latest) {
        return false;
    }
    if !reminder_is_due(now) {
        return false;
    }
    mark_reminded(now);
    true
}

/// Pure: whether a reminder is due, given when the last one was.
///
/// A missing or unreadable timestamp means never reminded. A timestamp in the
/// future — a clock that moved backwards — is due rather than a reason to stay
/// silent until the clock catches up.
#[must_use]
pub fn reminder_due(last: Option<SystemTime>, now: SystemTime) -> bool {
    match last {
        None => true,
        Some(when) => match now.duration_since(when) {
            Ok(since) => since >= REMIND_EVERY,
            Err(_) => true,
        },
    }
}

fn reminder_path() -> PathBuf {
    // The temporary directory, like the app's own markers: written by one
    // process, read by the next, valid for a day. Nothing here is worth
    // keeping across a cleanup.
    std::env::temp_dir().join("vitals-watchdog-update-reminded")
}

/// Whether a reminder is due, read from the stamp on disk.
fn reminder_is_due(now: SystemTime) -> bool {
    let last = std::fs::read_to_string(reminder_path())
        .ok()
        .and_then(|text| text.trim().parse::<u64>().ok())
        .map(|secs| SystemTime::UNIX_EPOCH + Duration::from_secs(secs));
    reminder_due(last, now)
}

fn mark_reminded(now: SystemTime) {
    let secs = now
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    // Written before the toast is attempted: a failing notification API must
    // not be retried every check for a day.
    let _ = std::fs::write(reminder_path(), secs.to_string());
}

/// A small HTTPS GET through `WinHTTP`.
///
/// `WinHTTP` rather than an HTTP crate: it is in Windows, so the watchdog grows
/// no TLS stack, no async runtime and no dependency it would have to be
/// audited for — in a process that exists to run in the background of machines
/// that are already struggling.
#[cfg(windows)]
fn get(url: &str) -> Option<String> {
    use windows_sys::Win32::Networking::WinHttp as http;

    // WinHTTP wants the parts, not a URL.
    let rest = url.strip_prefix("https://")?;
    let (host, path) = rest.split_once('/')?;
    let verb = to_wide("GET");
    let path = to_wide(&format!("/{path}"));
    let host = to_wide(host);

    // SAFETY: every handle is checked before it is used, each is closed
    // exactly once through the guard below, and every string handed to the API
    // is NUL-terminated and outlives the call.
    unsafe {
        let session = http::WinHttpOpen(
            to_wide("VitalsWatchdog").as_ptr(),
            http::WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
            std::ptr::null(),
            std::ptr::null(),
            0,
        );
        if session.is_null() {
            return None;
        }
        let result = (|| {
            let connection = http::WinHttpConnect(session, host.as_ptr(), 443, 0);
            if connection.is_null() {
                return None;
            }
            let request = http::WinHttpOpenRequest(
                connection,
                verb.as_ptr(),
                path.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                http::WINHTTP_FLAG_SECURE,
            );
            if request.is_null() {
                http::WinHttpCloseHandle(connection);
                return None;
            }
            let sent =
                http::WinHttpSendRequest(request, std::ptr::null(), 0, std::ptr::null(), 0, 0, 0);
            let body =
                if sent != 0 && http::WinHttpReceiveResponse(request, std::ptr::null_mut()) != 0 {
                    read_body(request)
                } else {
                    None
                };
            http::WinHttpCloseHandle(request);
            http::WinHttpCloseHandle(connection);
            body
        })();
        http::WinHttpCloseHandle(session);
        result
    }
}

/// Reads a response whole, as UTF-8. `None` on a short or invalid read.
#[cfg(windows)]
fn read_body(request: *mut core::ffi::c_void) -> Option<String> {
    use windows_sys::Win32::Networking::WinHttp as http;

    let mut out: Vec<u8> = Vec::new();
    let mut buffer = [0u8; 4096];
    loop {
        let mut read: u32 = 0;
        // SAFETY: the buffer is a live local of exactly the length passed, and
        // `read` is written by the call.
        let ok = unsafe {
            http::WinHttpReadData(
                request,
                buffer.as_mut_ptr().cast(),
                buffer.len() as u32,
                &raw mut read,
            )
        };
        if ok == 0 {
            return None;
        }
        if read == 0 {
            break;
        }
        out.extend_from_slice(&buffer[..read as usize]);
        // A response this large is not one of ours; the API's own limit is far
        // below it and a release payload is a few kilobytes.
        if out.len() > 256 * 1024 {
            return None;
        }
    }
    String::from_utf8(out).ok()
}

/// A NUL-terminated UTF-16 copy of `text`, for the W-suffixed API.
#[cfg(windows)]
fn to_wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ten_reads_as_newer_than_a_nine_though_it_sorts_before_it() {
        // The bug this exists for: `"1.10.0" < "1.9.0"` as strings, so a
        // lexical check would stop offering updates at every tenth minor.
        assert!(is_newer("1.9.0", "1.10.0"));
        assert!(!is_newer("1.10.0", "1.9.0"));
    }

    #[test]
    fn a_tag_is_compared_without_its_v() {
        assert_eq!(tag_version("v1.3.0"), "1.3.0");
        assert_eq!(tag_version("1.3.0"), "1.3.0");
        assert!(is_newer("1.2.0", tag_version("v1.3.0")));
    }

    #[test]
    fn the_same_version_is_not_an_update() {
        assert!(!is_newer("1.2.0", "1.2.0"));
        // Missing components are zero, not a difference.
        assert!(!is_newer("1.3", "1.3.0"));
        assert!(!is_newer("1.3.0", "1.3"));
    }

    #[test]
    fn a_major_or_minor_bump_is_an_update_in_either_direction_of_the_test() {
        assert!(is_newer("1.2.0", "2.0.0"));
        assert!(is_newer("1.2.0", "1.3.0"));
        assert!(is_newer("1.2.0", "1.2.1"));
        assert!(!is_newer("2.0.0", "1.99.0"));
    }

    #[test]
    fn something_unparseable_never_reports_an_update() {
        // A tag this cannot read is a reason to say nothing, never to claim a
        // newer release.
        assert!(!is_newer("1.2.0", "nightly"));
        assert!(!is_newer("1.2.0", ""));
        assert!(!is_newer("", "1.3.0"));
        assert!(!is_newer("1.2.0", "1.x.0"));
    }

    #[test]
    fn a_prerelease_suffix_does_not_stop_the_numbers_being_read() {
        assert_eq!(components("1.4.0-beta.1"), Some(vec![1, 4, 0]));
        assert!(is_newer("1.3.0", "1.4.0-beta.1"));
    }

    #[test]
    fn a_reminder_waits_a_day_and_then_comes_back() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        assert!(reminder_due(None, now), "never reminded");
        assert!(!reminder_due(Some(now), now + Duration::from_secs(60)));
        assert!(reminder_due(Some(now), now + REMIND_EVERY));
    }

    #[test]
    fn a_clock_that_moved_backwards_reminds_rather_than_falling_silent() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        let future = now + Duration::from_secs(100_000);
        assert!(reminder_due(Some(future), now));
    }

    #[test]
    fn a_newer_release_alone_does_not_write_the_reminder_stamp() {
        // `should_remind` is the only writer, and it refuses when there is
        // nothing newer — so a check that finds nothing leaves no state.
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        assert!(reminder_due(None, now));
        // Same release as this build: nothing to say, whatever the clock says.
        assert!(!should_remind(installed(), now));
    }

    /// The one test that needs the network, so it is ignored by default: CI
    /// runners are offline and a check that failed there would be switched off
    /// rather than read.
    ///
    /// Run it by hand after touching the `WinHTTP` call, because that FFI is
    /// easy to get subtly wrong — a wrong flag or a wrong port type returns a
    /// null handle and this is the only thing that would notice:
    ///
    /// ```text
    /// cargo test -p vitals-watchdog -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore = "needs the network; run by hand after changing the HTTP call"]
    fn the_live_check_reads_a_version_from_github() {
        let latest = latest_release().expect("GitHub answered with a tag");
        println!("installed {} / latest {latest}", installed());
        assert!(
            !latest.is_empty() && latest.chars().next().is_some_and(|c| c.is_ascii_digit()),
            "a tag was read but does not look like a version: {latest}"
        );
    }
}
