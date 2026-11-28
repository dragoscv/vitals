//! ISO-8601 → Unix milliseconds, without a date crate.
//!
//! agentq (PowerShell `ToString('o')`), Copilot, Codex and Claude all write
//! RFC 3339 timestamps, with 0–7 fractional digits and either `Z` or an
//! offset. That is the whole grammar this needs; a calendar crate for it
//! would be the largest dependency in the crate.

/// Parses `YYYY-MM-DDTHH:MM:SS[.frac][Z|±HH:MM]`. No offset means UTC.
#[must_use]
pub fn parse_iso_ms(s: &str) -> Option<u64> {
    let bytes = s.trim().as_bytes();
    if bytes.len() < 19 || bytes[4] != b'-' || bytes[7] != b'-' || !matches!(bytes[10], b'T' | b' ')
    {
        return None;
    }
    let num = |from: usize, len: usize| -> Option<i64> {
        let mut value = 0i64;
        for &digit in bytes.get(from..from + len)? {
            if !digit.is_ascii_digit() {
                return None;
            }
            value = value * 10 + i64::from(digit - b'0');
        }
        Some(value)
    };
    let (year, month, day) = (num(0, 4)?, num(5, 2)?, num(8, 2)?);
    let (hour, minute, sec) = (num(11, 2)?, num(14, 2)?, num(17, 2)?);
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || sec > 60
    {
        return None;
    }
    let mut pos = 19;
    let mut millis = 0i64;
    if bytes.get(pos) == Some(&b'.') {
        pos += 1;
        let start = pos;
        while bytes.get(pos).is_some_and(u8::is_ascii_digit) {
            pos += 1;
        }
        for (place, &digit) in bytes[start..pos].iter().take(3).enumerate() {
            millis += i64::from(digit - b'0') * [100, 10, 1][place];
        }
    }
    let offset_min = match bytes.get(pos) {
        None | Some(b'Z' | b'z') => 0,
        Some(&sign @ (b'+' | b'-')) => {
            let off_hours = num(pos + 1, 2)?;
            let off_minutes = if bytes.get(pos + 3) == Some(&b':') {
                num(pos + 4, 2)?
            } else {
                num(pos + 3, 2)?
            };
            let total = off_hours * 60 + off_minutes;
            if sign == b'+' { total } else { -total }
        }
        Some(_) => return None,
    };
    let days = days_from_civil(year, month, day);
    let secs = days * 86_400 + hour * 3600 + minute * 60 + sec - offset_min * 60;
    let ms = secs * 1000 + millis;
    u64::try_from(ms).ok()
}

/// Howard Hinnant's days-from-civil: days since 1970-01-01.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Unix milliseconds as an RFC 3339 UTC stamp, e.g. `2026-11-28T00:35:34.883Z`.
///
/// The inverse of [`parse_iso_ms`], and the exact form `track.mjs` writes with
/// `new Date().toISOString()` (three fractional digits, `Z`). A task this page
/// claims therefore carries a stamp the CLI and every other reader parse back
/// to the same millisecond — two spellings of "now" in one store is the drift
/// this pairing exists to prevent.
#[must_use]
pub fn iso_from_ms(ms: u64) -> String {
    let total_secs = (ms / 1000).cast_signed();
    let millis = ms % 1000;
    let days = total_secs.div_euclid(86_400);
    let tod = total_secs.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let (hour, minute, second) = (tod / 3600, (tod % 3600) / 60, tod % 60);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{millis:03}Z")
}

/// Howard Hinnant's `civil_from_days`: days since 1970-01-01 to a proleptic
/// Gregorian date. Pure integer arithmetic, so the reverse of
/// [`days_from_civil`] needs no date crate either.
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// Current wall clock in Unix milliseconds.
#[must_use]
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

/// A file's mtime in Unix milliseconds.
#[must_use]
pub fn mtime_ms(meta: &std::fs::Metadata) -> Option<u64> {
    let t = meta.modified().ok()?;
    let d = t.duration_since(std::time::UNIX_EPOCH).ok()?;
    u64::try_from(d.as_millis()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn powershell_round_trip_format_with_seven_fraction_digits_parses_to_the_millisecond() {
        assert_eq!(
            parse_iso_ms("2026-10-17T03:29:31.1701992Z"),
            Some(1_792_207_771_170)
        );
    }

    #[test]
    fn an_offset_is_applied_so_local_and_utc_spellings_agree() {
        assert_eq!(
            parse_iso_ms("2026-10-17T06:01:11.3625626+03:00"),
            parse_iso_ms("2026-10-17T03:01:11.362Z")
        );
    }

    #[test]
    fn no_fraction_and_no_zone_is_utc() {
        assert_eq!(parse_iso_ms("1970-01-01T00:00:01"), Some(1000));
    }

    #[test]
    fn garbage_and_pre_epoch_dates_are_none_not_zero() {
        assert_eq!(parse_iso_ms("10/17/2026 03:28:47"), None);
        assert_eq!(parse_iso_ms(""), None);
        assert_eq!(parse_iso_ms("1969-12-31T23:59:59Z"), None);
        assert_eq!(parse_iso_ms("2026-13-01T00:00:00Z"), None);
    }

    #[test]
    fn a_stamp_we_write_parses_back_to_the_same_millisecond() {
        // The two directions must agree, or a task this app claims carries a
        // time the CLI disagrees with by a fraction of a second.
        for ms in [
            0,
            1000,
            1_792_207_771_170,
            1_800_000_000_000,
            2_051_222_400_000,
        ] {
            assert_eq!(
                parse_iso_ms(&iso_from_ms(ms)),
                Some(ms),
                "round trip lost a millisecond at {ms}"
            );
        }
    }

    #[test]
    fn the_written_stamp_is_the_three_fraction_digit_form_the_cli_writes() {
        // `new Date().toISOString()` in track.mjs, byte for byte.
        assert_eq!(iso_from_ms(1_792_207_771_170), "2026-10-17T03:29:31.170Z");
        assert_eq!(iso_from_ms(0), "1970-01-01T00:00:00.000Z");
    }
}
