//! How often a scheduled task runs, read from its trigger definitions.
//!
//! ## Why this is more than "logon or boot"
//!
//! The startup list only admits tasks with a logon or boot trigger, but such
//! a task frequently runs far more often than that: an updater that starts
//! at sign-in and then repeats every five minutes is a very different cost
//! from one that runs once per session. [`TaskTrigger`](super::TaskTrigger)
//! deliberately collapses everything to "startup or not"; this keeps the
//! detail the user needs to judge the entry.
//!
//! ## Why each trigger is read on its own
//!
//! A whole-document search for `<Interval>` or `<Enabled>` reads whichever
//! trigger happens to come first. Each trigger carries its own `Enabled` and
//! its own `Repetition`, so a disabled trigger's five-minute repetition would
//! be reported against a task that in fact runs once at boot. Every trigger
//! block is therefore cut out and parsed separately.

/// The calendar part of a schedule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalendarKind {
    /// `<ScheduleByDay>`. `None` when `DaysInterval` is absent or unreadable;
    /// the UI then says "daily" without a number rather than inventing one.
    Daily { days_interval: Option<u32> },
    /// `<ScheduleByWeek>`, with `WeeksInterval` under the same rule.
    Weekly { weeks_interval: Option<u32> },
    /// `<ScheduleByMonth>` or `<ScheduleByMonthDayOfWeek>`.
    Monthly,
}

impl CalendarKind {
    /// Approximate period in days, used only to keep the most frequent of
    /// several calendar triggers — the one that says most about the cost.
    fn period_days(self) -> u32 {
        match self {
            Self::Daily { days_interval } => days_interval.unwrap_or(1),
            Self::Weekly { weeks_interval } => weeks_interval.unwrap_or(1).saturating_mul(7),
            Self::Monthly => 30,
        }
    }
}

/// A trigger that is neither logon, boot, repetition nor calendar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OtherTrigger {
    /// `<IdleTrigger>`.
    Idle,
    /// `<EventTrigger>`, and `<WnfStateChangeTrigger>` — a Windows
    /// Notification Facility state change is a system event in every sense
    /// the user cares about, and dropping it would leave a task that runs
    /// with no schedule shown at all.
    Event,
    /// A `<TimeTrigger>` without repetition, or a `<CalendarTrigger>` with
    /// no `ScheduleBy*` block: it fires at one moment and not again.
    Once,
    /// `<SessionStateChangeTrigger>` — lock, unlock, connect, disconnect.
    SessionChange,
    /// `<RegistrationTrigger>` — when the task is created or updated.
    Registration,
}

/// Everything a task's enabled triggers say about when it runs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TaskSchedule {
    /// An enabled `<LogonTrigger>` exists.
    pub at_logon: bool,
    /// An enabled `<BootTrigger>` exists.
    pub at_boot: bool,
    /// The shortest `Repetition/Interval` across enabled triggers, in seconds.
    ///
    /// `None` when nothing repeats **or** when the interval could not be
    /// parsed. Never zero: zero would read as "continuously", which is the
    /// most alarming possible claim to make about a value we did not read.
    pub repeat_every_secs: Option<u64>,
    /// The most frequent calendar schedule, if any trigger has one.
    pub calendar: Option<CalendarKind>,
    /// The remaining kinds, each listed once, in document order.
    pub other: Vec<OtherTrigger>,
}

impl TaskSchedule {
    /// Whether anything at all was read.
    ///
    /// A task whose triggers are all disabled has an empty schedule; the UI
    /// shows nothing for it rather than a phrase that implies it runs.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        !self.at_logon
            && !self.at_boot
            && self.repeat_every_secs.is_none()
            && self.calendar.is_none()
            && self.other.is_empty()
    }
}

/// Reads the schedule from a single task definition.
#[must_use]
pub fn parse_schedule(task_xml: &str) -> TaskSchedule {
    let mut schedule = TaskSchedule::default();

    // Only the `<Triggers>` block: a `<Settings>` element contains an
    // `<IdleSettings>` whose children must not be mistaken for a trigger, and
    // a description can contain anything.
    let Some(triggers) = inner(task_xml, "Triggers") else {
        return schedule;
    };

    for (name, body) in trigger_blocks(triggers) {
        let disabled =
            inner(body, "Enabled").is_some_and(|value| value.trim().eq_ignore_ascii_case("false"));
        if disabled {
            continue;
        }

        let repeat = inner(body, "Repetition")
            .and_then(|repetition| inner(repetition, "Interval"))
            .and_then(parse_iso_duration)
            // `PT0S` is the schema's way of writing "no repetition", not an
            // interval of zero; see `repeat_every_secs`.
            .filter(|&secs| secs > 0);
        if let Some(secs) = repeat {
            schedule.repeat_every_secs = Some(
                schedule
                    .repeat_every_secs
                    .map_or(secs, |current| current.min(secs)),
            );
        }

        let other = match name {
            "LogonTrigger" => {
                schedule.at_logon = true;
                None
            }
            "BootTrigger" => {
                schedule.at_boot = true;
                None
            }
            "CalendarTrigger" => match calendar_kind(body) {
                Some(kind) => {
                    let keep = schedule
                        .calendar
                        .is_none_or(|current| kind.period_days() < current.period_days());
                    if keep {
                        schedule.calendar = Some(kind);
                    }
                    None
                }
                None => Some(OtherTrigger::Once),
            },
            // A repeating time trigger is fully described by its interval;
            // adding "once" beside "every 10 min" would contradict it.
            "TimeTrigger" if repeat.is_none() => Some(OtherTrigger::Once),
            "IdleTrigger" => Some(OtherTrigger::Idle),
            "EventTrigger" | "WnfStateChangeTrigger" => Some(OtherTrigger::Event),
            "SessionStateChangeTrigger" => Some(OtherTrigger::SessionChange),
            "RegistrationTrigger" => Some(OtherTrigger::Registration),
            _ => None,
        };

        if let Some(kind) = other
            && !schedule.other.contains(&kind)
        {
            schedule.other.push(kind);
        }
    }

    schedule
}

/// The calendar schedule of a `<CalendarTrigger>` body, if it has one.
fn calendar_kind(body: &str) -> Option<CalendarKind> {
    if let Some(by_day) =
        inner(body, "ScheduleByDay").or_else(|| empty_element(body, "ScheduleByDay"))
    {
        return Some(CalendarKind::Daily {
            days_interval: positive(inner(by_day, "DaysInterval")),
        });
    }
    if let Some(by_week) =
        inner(body, "ScheduleByWeek").or_else(|| empty_element(body, "ScheduleByWeek"))
    {
        return Some(CalendarKind::Weekly {
            weeks_interval: positive(inner(by_week, "WeeksInterval")),
        });
    }
    let monthly = ["ScheduleByMonth", "ScheduleByMonthDayOfWeek"]
        .iter()
        .any(|name| has_element(body, name));
    monthly.then_some(CalendarKind::Monthly)
}

/// A positive integer, or `None` — zero is not a valid interval and must not
/// be shown as one.
fn positive(text: Option<&str>) -> Option<u32> {
    text.and_then(|t| t.trim().parse::<u32>().ok())
        .filter(|&n| n > 0)
}

/// Parses an ISO 8601 duration as used by Task Scheduler, into seconds.
///
/// Accepts `P[nW][nD][T[nH][nM][nS]]`. Years and months are refused: they
/// have no fixed length, and the scheduler never writes them for an
/// interval. Anything malformed — including a bare `P` or `PT` — is `None`,
/// never zero.
#[must_use]
pub fn parse_iso_duration(text: &str) -> Option<u64> {
    let rest = text.trim().strip_prefix('P')?;
    let (date, time) = match rest.split_once('T') {
        Some((date, time)) => {
            // `P1DT` names a time part and gives none; the schema rejects it.
            if time.is_empty() {
                return None;
            }
            (date, Some(time))
        }
        None => (rest, None),
    };

    let mut total: u64 = 0;
    let mut components = 0;

    for (part, units) in [
        (Some(date), &[('W', 604_800_u64), ('D', 86_400)][..]),
        (time, &[('H', 3_600_u64), ('M', 60), ('S', 1)][..]),
    ] {
        let Some(mut part) = part else { continue };
        let mut next_unit = 0;
        while !part.is_empty() {
            let digits = part.find(|c: char| !c.is_ascii_digit())?;
            if digits == 0 {
                return None;
            }
            let value: u64 = part[..digits].parse().ok()?;
            let unit = part[digits..].chars().next()?;
            // Units must appear in order and at most once (`PT5M1H` is not a
            // duration), which a lookup from the current position enforces.
            let offset = units[next_unit..].iter().position(|&(u, _)| u == unit)?;
            let (_, scale) = units[next_unit + offset];
            next_unit += offset + 1;
            total = total.checked_add(value.checked_mul(scale)?)?;
            components += 1;
            part = &part[digits + unit.len_utf8()..];
        }
    }

    (components > 0).then_some(total)
}

/// Yields `(name, body)` for every `<XxxTrigger>` element at any depth of
/// the given text, including self-closing ones (with an empty body).
fn trigger_blocks(xml: &str) -> Vec<(&str, &str)> {
    let mut out = Vec::new();
    let mut rest = xml;

    while let Some(open) = rest.find('<') {
        let after = &rest[open + 1..];
        let name_len = after
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .unwrap_or(after.len());
        let name = &after[..name_len];

        if name.is_empty() || !name.ends_with("Trigger") {
            rest = after;
            continue;
        }

        let Some(tag_end) = after.find('>') else {
            break;
        };
        let tag = &after[..tag_end];
        if tag.ends_with('/') {
            out.push((name, ""));
            rest = &after[tag_end + 1..];
            continue;
        }

        let body_start = tag_end + 1;
        let close = format!("</{name}>");
        let Some(body_len) = after[body_start..].find(&close) else {
            // An unterminated trigger is corrupt; reading its body to the end
            // of the document would attribute later triggers' settings to it.
            break;
        };
        out.push((name, &after[body_start..body_start + body_len]));
        rest = &after[body_start + body_len + close.len()..];
    }

    out
}

/// The raw text between `<name ...>` and `</name>`, first occurrence.
///
/// Attributes on the opening tag are allowed (`<Triggers xmlns="…">` is
/// possible); a self-closing element has no body and yields `None`.
fn inner<'a>(xml: &'a str, name: &str) -> Option<&'a str> {
    let start = open_tag_end(xml, name)?;
    if xml[..start].ends_with("/>") {
        return None;
    }
    let close = format!("</{name}>");
    let len = xml[start..].find(&close)?;
    Some(&xml[start..start + len])
}

/// `Some("")` for a self-closing `<name/>`, so `<ScheduleByDay/>` still
/// counts as a daily schedule with its interval left unread.
fn empty_element<'a>(xml: &'a str, name: &str) -> Option<&'a str> {
    let end = open_tag_end(xml, name)?;
    xml[..end].ends_with("/>").then_some("")
}

fn has_element(xml: &str, name: &str) -> bool {
    open_tag_end(xml, name).is_some()
}

/// The byte index just past the first whole-name opening tag of `name`.
fn open_tag_end(xml: &str, name: &str) -> Option<usize> {
    let needle = format!("<{name}");
    let mut offset = 0;
    while let Some(found) = xml[offset..].find(&needle) {
        let after = offset + found + needle.len();
        // Whole-name match only: `<Interval` must not match `<IntervalEx`.
        if matches!(
            xml[after..].chars().next(),
            Some('>' | '/' | ' ' | '\t' | '\r' | '\n')
        ) {
            return Some(after + xml[after..].find('>')? + 1);
        }
        offset = after;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(triggers: &str) -> String {
        format!(
            "<Task version=\"1.4\"><Triggers>{triggers}</Triggers>\
             <Settings><Enabled>true</Enabled><IdleSettings><Duration>PT10M</Duration>\
             </IdleSettings></Settings></Task>"
        )
    }

    #[test]
    fn a_logon_trigger_repeating_every_five_minutes_reports_both() {
        let schedule = parse_schedule(&task(
            "<LogonTrigger><Enabled>true</Enabled><Repetition><Interval>PT5M</Interval>\
             <StopAtDurationEnd>false</StopAtDurationEnd></Repetition></LogonTrigger>",
        ));
        assert!(schedule.at_logon);
        assert_eq!(schedule.repeat_every_secs, Some(300));
        assert!(!schedule.at_boot);
        assert!(schedule.other.is_empty(), "got {:?}", schedule.other);
    }

    #[test]
    fn a_repeating_time_trigger_beside_a_logon_trigger_supplies_the_interval_not_once() {
        let schedule = parse_schedule(&task(
            "<LogonTrigger/><TimeTrigger><StartBoundary>2024-01-01T00:00:00</StartBoundary>\
             <Repetition><Interval>PT10M</Interval></Repetition></TimeTrigger>",
        ));
        assert!(schedule.at_logon, "the self-closing logon trigger counts");
        assert_eq!(schedule.repeat_every_secs, Some(600));
        assert!(
            !schedule.other.contains(&OtherTrigger::Once),
            "a time trigger that repeats is not a one-off"
        );
    }

    #[test]
    fn the_shortest_interval_wins_when_several_triggers_repeat() {
        let schedule = parse_schedule(&task(
            "<LogonTrigger><Repetition><Interval>PT1H</Interval></Repetition></LogonTrigger>\
             <BootTrigger><Repetition><Interval>PT15M</Interval></Repetition></BootTrigger>",
        ));
        assert_eq!(schedule.repeat_every_secs, Some(900));
        assert!(schedule.at_logon && schedule.at_boot);
    }

    #[test]
    fn a_disabled_triggers_repetition_is_ignored() {
        let schedule = parse_schedule(&task(
            "<BootTrigger><Enabled>true</Enabled></BootTrigger>\
             <TimeTrigger><Enabled>false</Enabled><Repetition><Interval>PT5M</Interval>\
             </Repetition></TimeTrigger>",
        ));
        assert!(schedule.at_boot);
        assert_eq!(
            schedule.repeat_every_secs, None,
            "the five-minute repetition belongs to a trigger that never fires"
        );
        assert!(
            schedule.other.is_empty(),
            "nor is the disabled trigger a one-off"
        );
    }

    #[test]
    fn a_disabled_logon_trigger_does_not_claim_the_task_runs_at_sign_in() {
        let schedule = parse_schedule(&task(
            "<LogonTrigger><Enabled>false</Enabled></LogonTrigger>",
        ));
        assert!(!schedule.at_logon);
        assert!(schedule.is_empty());
    }

    #[test]
    fn a_calendar_trigger_every_two_days_is_daily_with_its_interval() {
        let schedule = parse_schedule(&task(
            "<BootTrigger/><CalendarTrigger><StartBoundary>2024-01-01T03:00:00</StartBoundary>\
             <ScheduleByDay><DaysInterval>2</DaysInterval></ScheduleByDay></CalendarTrigger>",
        ));
        assert_eq!(
            schedule.calendar,
            Some(CalendarKind::Daily {
                days_interval: Some(2)
            })
        );
    }

    #[test]
    fn weekly_and_monthly_calendar_triggers_are_told_apart_and_the_most_frequent_is_kept() {
        let weekly = parse_schedule(&task(
            "<CalendarTrigger><ScheduleByWeek><WeeksInterval>1</WeeksInterval>\
             <DaysOfWeek><Monday/></DaysOfWeek></ScheduleByWeek></CalendarTrigger>",
        ));
        assert_eq!(
            weekly.calendar,
            Some(CalendarKind::Weekly {
                weeks_interval: Some(1)
            })
        );

        let both = parse_schedule(&task(
            "<CalendarTrigger><ScheduleByMonth><DaysOfMonth><Day>1</Day></DaysOfMonth>\
             </ScheduleByMonth></CalendarTrigger>\
             <CalendarTrigger><ScheduleByWeek><WeeksInterval>2</WeeksInterval>\
             </ScheduleByWeek></CalendarTrigger>",
        ));
        assert_eq!(
            both.calendar,
            Some(CalendarKind::Weekly {
                weeks_interval: Some(2)
            }),
            "fortnightly runs more often than monthly, so it is the one to show"
        );
    }

    #[test]
    fn a_zero_days_interval_is_unread_not_zero() {
        let schedule = parse_schedule(&task(
            "<CalendarTrigger><ScheduleByDay><DaysInterval>0</DaysInterval></ScheduleByDay>\
             </CalendarTrigger>",
        ));
        assert_eq!(
            schedule.calendar,
            Some(CalendarKind::Daily {
                days_interval: None
            })
        );
    }

    #[test]
    fn a_malformed_interval_is_none_not_zero() {
        let schedule = parse_schedule(&task(
            "<LogonTrigger><Repetition><Interval>PTXM</Interval></Repetition></LogonTrigger>",
        ));
        assert!(schedule.at_logon);
        assert_eq!(schedule.repeat_every_secs, None);
    }

    #[test]
    fn a_zero_interval_is_treated_as_no_repetition() {
        let schedule = parse_schedule(&task(
            "<LogonTrigger><Repetition><Interval>PT0S</Interval></Repetition></LogonTrigger>",
        ));
        assert_eq!(
            schedule.repeat_every_secs, None,
            "zero would render as 'continuously', a claim nothing supports"
        );
    }

    #[test]
    fn other_trigger_kinds_are_each_listed_once() {
        let schedule = parse_schedule(&task(
            "<LogonTrigger/><IdleTrigger/><EventTrigger><Subscription>&lt;QueryList&gt;\
             &lt;/QueryList&gt;</Subscription></EventTrigger><EventTrigger/>\
             <TimeTrigger><StartBoundary>2024-01-01T00:00:00</StartBoundary></TimeTrigger>\
             <SessionStateChangeTrigger><StateChange>SessionUnlock</StateChange>\
             </SessionStateChangeTrigger><RegistrationTrigger/>",
        ));
        assert_eq!(
            schedule.other,
            vec![
                OtherTrigger::Idle,
                OtherTrigger::Event,
                OtherTrigger::Once,
                OtherTrigger::SessionChange,
                OtherTrigger::Registration,
            ]
        );
    }

    #[test]
    fn idle_settings_outside_the_triggers_block_are_not_an_idle_trigger() {
        let schedule = parse_schedule(&task("<BootTrigger/>"));
        assert!(
            schedule.other.is_empty(),
            "IdleSettings lives in Settings and says nothing about when the task runs"
        );
    }

    #[test]
    fn a_task_with_no_triggers_has_an_empty_schedule() {
        assert!(parse_schedule("<Task><Settings/></Task>").is_empty());
        assert!(parse_schedule("<Task><Triggers/></Task>").is_empty());
    }

    #[test]
    fn iso_durations_parse_to_seconds() {
        assert_eq!(parse_iso_duration("PT5M"), Some(300));
        assert_eq!(parse_iso_duration("PT1M"), Some(60));
        assert_eq!(parse_iso_duration("PT30M"), Some(1_800));
        assert_eq!(parse_iso_duration("PT1H"), Some(3_600));
        assert_eq!(parse_iso_duration("P1D"), Some(86_400));
        assert_eq!(parse_iso_duration("PT3H30M"), Some(12_600));
        assert_eq!(parse_iso_duration("P1W"), Some(604_800));
        assert_eq!(parse_iso_duration("P1DT2H"), Some(93_600));
        assert_eq!(parse_iso_duration("PT45S"), Some(45));
    }

    #[test]
    fn malformed_iso_durations_are_none_never_zero() {
        for bad in [
            "", "P", "PT", "P1DT", "PTXM", "5M", "PT5", "PT1.5S", "P1M", "P1Y", "PT5M1H", "PT5M5M",
            "PTM",
        ] {
            assert_eq!(parse_iso_duration(bad), None, "{bad:?} must not parse");
        }
    }

    #[test]
    fn an_overflowing_duration_is_none_rather_than_wrapped() {
        assert_eq!(parse_iso_duration("P99999999999999999999W"), None);
        assert_eq!(parse_iso_duration("P9999999999999999W"), None);
    }
}
