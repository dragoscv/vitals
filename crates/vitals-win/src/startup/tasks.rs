//! Scheduled tasks with a logon or boot trigger.
//!
//! ## Why these belong in a startup list
//!
//! Task Manager's Startup tab does not show scheduled tasks at all. A task
//! with a `LogonTrigger` runs at every logon exactly like a `Run` value does,
//! and is therefore where anything that does not wish to be found in the
//! Startup tab puts itself. Every serious autostart auditor —
//! Sysinternals Autoruns included — enumerates them for this reason.
//!
//! ## Why not the obvious two routes
//!
//! Measured on this machine, unelevated, and both fail *silently* — which is
//! why they are documented here rather than merely abandoned:
//!
//! - **`%SystemRoot%\System32\Tasks`**, where the definitions genuinely live
//!   as XML mirroring the folder tree. The directory is ACL'd: enumerating it
//!   as a standard user returns **zero entries and no error**. Not an access
//!   failure that could be reported — an empty listing indistinguishable from
//!   a machine with no tasks.
//! - **`…\Schedule\TaskCache\{Tree,Logon,Boot}`**, the registry index. The
//!   base key opens; every sub-key enumeration returns nothing, for the same
//!   reason and with the same invisible failure.
//!
//! The supported route is the Task Scheduler COM API, which does work
//! unelevated (221 tasks here). It needs `Win32_System_TaskScheduler`, a COM
//! apartment and a dozen interface round-trips per task.
//!
//! `schtasks.exe /query /xml ONE` is that same COM API, already hosted: one
//! process, **177 ms**, all 221 definitions concatenated, no elevation. It is
//! a stopgap and should be replaced by direct `ITaskService` calls when the
//! COM feature is enabled — spawning a process per refresh is not something
//! to keep. It is used now because the alternative is shipping a tab that
//! reports zero tasks and looks correct while doing so.

use std::path::PathBuf;
use std::process::Command;

use super::{StartupEntry, StartupSource, StartupState};

/// What causes a task to run.
///
/// Only the two that matter for startup are distinguished; everything else
/// collapses to [`Other`](TaskTrigger::Other) because a task that runs at
/// 3 a.m. on Tuesdays is not a startup item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskTrigger {
    /// `<LogonTrigger>` — runs when a user signs in.
    Logon,
    /// `<BootTrigger>` — runs when the machine starts, before any logon.
    Boot,
    /// Time, event, idle, registration and the rest.
    Other,
}

impl TaskTrigger {
    /// Whether this trigger makes the task a startup item.
    #[must_use]
    pub const fn is_startup(self) -> bool {
        matches!(self, Self::Logon | Self::Boot)
    }
}

/// A scheduled task as read from its definition.
#[derive(Debug, Clone)]
pub struct TaskInfo {
    /// Full task path, e.g. `\Microsoft\Windows\Defrag\ScheduledDefrag`.
    pub path: String,
    /// Every trigger the task declares.
    ///
    /// A task commonly has several — a boot trigger *and* a daily one is the
    /// standard updater pattern. Reducing this to a single trigger, as a
    /// naive reader does, loses the boot trigger whenever it is not first.
    pub triggers: Vec<TaskTrigger>,
    /// The first action's command, when the task runs an executable.
    ///
    /// `None` for a COM-handler or e-mail task, which have no command line.
    /// Not defaulted to an empty string, which would render as a blank
    /// command that looks measured.
    pub command: Option<String>,
    /// Arguments to that command, if any.
    pub arguments: Option<String>,
    /// Whether the task will run.
    pub state: StartupState,
    /// The account it runs as, e.g. `SYSTEM` or a SID.
    pub principal: Option<String>,
}

impl TaskInfo {
    /// Whether any trigger fires at logon or boot.
    #[must_use]
    pub fn runs_at_startup(&self) -> bool {
        self.triggers.iter().any(|t| t.is_startup())
    }
}

/// The outcome of a task sweep, including what could not be read.
#[derive(Debug, Default)]
pub struct TaskScan {
    /// Tasks with a logon or boot trigger.
    pub startup_tasks: Vec<TaskInfo>,
    /// How many definitions existed but could not be opened.
    ///
    /// Reported rather than swallowed: "we found 12" and "we found 12 and
    /// were refused 40" are very different claims, and only one of them is
    /// honest about an unelevated process's view of `\Microsoft\Windows`.
    pub unreadable: usize,
    /// How many task definitions were seen in total.
    pub total_seen: usize,
}

/// Scans every task definition and returns those that run at startup.
///
/// Never fails as a whole: a definition that cannot be parsed increments
/// [`TaskScan::unreadable`] rather than aborting and hiding every task that
/// *was* readable.
#[must_use]
pub fn scan_tasks() -> TaskScan {
    let Some(xml) = query_all_definitions() else {
        return TaskScan::default();
    };
    parse_scan(&xml)
}

/// Runs `schtasks /query /xml ONE` and returns its output.
///
/// `ONE` concatenates every definition into a single document; the default
/// mode emits a separate XML declaration per task, which no parser accepts as
/// one document. Returns `None` if the tool is missing or fails, which is
/// distinguishable downstream from "there are no tasks" only because the
/// caller sees a zero `total_seen`.
fn query_all_definitions() -> Option<String> {
    let output = Command::new("schtasks.exe")
        .args(["/query", "/xml", "ONE"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    // Measured: schtasks writes plain UTF-8 with no BOM here, despite the
    // on-disk definitions being UTF-16LE. `decode_utf16le` detects both, so
    // it is used rather than assuming either.
    decode_utf16le(&output.stdout)
}

/// Splits a concatenated document and keeps the startup tasks.
///
/// Separated from the process spawn so the whole parse is testable against a
/// fixture, which is the half that can actually be wrong.
#[must_use]
pub fn parse_scan(document: &str) -> TaskScan {
    let mut scan = TaskScan::default();

    for block in split_tasks(document) {
        scan.total_seen += 1;

        // `<URI>` is the task's full path and is present in every definition
        // schtasks emits. Without it the task cannot be identified, let
        // alone acted on, so it is counted as unreadable rather than shown
        // with an invented name.
        let Some(path) = element(block, "URI").filter(|u| !u.trim().is_empty()) else {
            scan.unreadable += 1;
            continue;
        };

        let info = parse_task(block);
        if !info.runs_at_startup() {
            continue;
        }

        scan.startup_tasks.push(TaskInfo { path, ..info });
    }

    scan
}

/// Yields each `<Task …>…</Task>` block of a concatenated document.
///
/// A split on `</Task>` alone would also cut at `</TaskName>`; the closing
/// tag is matched whole for the same reason [`count_elements`] does.
fn split_tasks(document: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = document;

    while let Some(start) = rest.find("<Task ") {
        let after = &rest[start..];
        let Some(end) = after.find("</Task>") else {
            break;
        };
        out.push(&after[..end]);
        rest = &after[end + "</Task>".len()..];
    }

    out
}

/// Decodes a task definition, which may be UTF-16LE or UTF-8.
///
/// Both encodings occur and neither is announced reliably:
///
/// - On-disk definitions under `System32\Tasks` are UTF-16LE **with** a BOM.
/// - `schtasks /query /xml` writes UTF-8 with **no** BOM.
///
/// Assuming UTF-16 for anything BOM-less is the trap, and it cost real time
/// here: a UTF-8 document has an even byte count as often as not, so it
/// decodes "successfully" into CJK mojibake — a perfectly valid `String` in
/// which no element name is ever found. Every task then looks trigger-less
/// and the scan reports zero startup tasks with no error anywhere.
///
/// "Try UTF-8, fall back to UTF-16" does **not** work, which is worth
/// stating because it is the obvious fix and it is wrong: NUL is a perfectly
/// legal UTF-8 byte, so UTF-16LE ASCII decodes as valid UTF-8 into a string
/// of interleaved NULs. Both directions therefore "succeed" and the
/// distinction has to be made on content.
///
/// The discriminator used is the NUL byte itself. XML text contains none —
/// a NUL is not a legal XML character at all — whereas UTF-16LE ASCII is
/// half NULs by construction. So: honour a BOM, and otherwise treat the
/// presence of NULs in the first part of the buffer as UTF-16.
///
/// Returns `None` for input that is neither — an odd-length BOM-marked
/// UTF-16 buffer is truncated and corrupt, and half-decoding it would
/// produce a task with no triggers, which reads as "not a startup item"
/// rather than as an error.
#[must_use]
pub fn decode_utf16le(bytes: &[u8]) -> Option<String> {
    if let Some(body) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8(body.to_vec()).ok();
    }

    if let Some(body) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        return decode_utf16_body(body);
    }

    // Sampling a prefix rather than the whole buffer keeps this cheap on a
    // 350 KB document; a UTF-16 file has a NUL within its first few bytes.
    let probe = &bytes[..bytes.len().min(512)];
    if probe.contains(&0) {
        return decode_utf16_body(bytes);
    }

    match std::str::from_utf8(bytes) {
        Ok(text) => Some(text.to_owned()),
        Err(_) => decode_utf16_body(bytes),
    }
}

/// Decodes BOM-less UTF-16LE bytes.
fn decode_utf16_body(body: &[u8]) -> Option<String> {
    if !body.len().is_multiple_of(2) {
        return None;
    }

    let units: Vec<u16> = body
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect();

    Some(String::from_utf16_lossy(&units))
}

/// Extracts triggers, command and principal from task XML.
///
/// Deliberately a tag scan rather than a real XML parse. The task schema is
/// fixed, the elements we need never carry attributes that change their
/// meaning, and adding an XML dependency to the platform crate for four
/// element lookups is not a trade worth making. The scan is exact about
/// element boundaries so that a string like `<LogonTrigger` inside a
/// description cannot produce a false positive.
#[must_use]
pub fn parse_task(xml: &str) -> TaskInfo {
    let mut triggers = Vec::new();

    // Counted rather than "found", because a task with a boot trigger *and*
    // a daily trigger must report both: a reader that stops at the first
    // trigger misses the boot one whenever the schedule is declared first,
    // and that is the common ordering in vendor updaters.
    if count_elements(xml, "LogonTrigger") > 0 {
        triggers.push(TaskTrigger::Logon);
    }
    if count_elements(xml, "BootTrigger") > 0 {
        triggers.push(TaskTrigger::Boot);
    }
    for other in [
        "CalendarTrigger",
        "TimeTrigger",
        "IdleTrigger",
        "EventTrigger",
        "RegistrationTrigger",
        "SessionStateChangeTrigger",
    ] {
        if count_elements(xml, other) > 0 {
            triggers.push(TaskTrigger::Other);
            break;
        }
    }

    // `<Enabled>` appears inside triggers too, so only the last one — which
    // in the schema is the task-level Settings value — would be read by a
    // naive "find the first Enabled" scan. Restrict to the Settings block.
    //
    // An absent Settings/Enabled defaults to enabled per the schema — a
    // documented default, not a guess — which is why it shares an arm with
    // an explicit `true`.
    let disabled = element(xml, "Settings")
        .as_deref()
        .and_then(|settings| element(settings, "Enabled"))
        .is_some_and(|value| value.trim().eq_ignore_ascii_case("false"));

    let state = if disabled {
        StartupState::Disabled
    } else {
        StartupState::Enabled
    };

    TaskInfo {
        path: String::new(),
        triggers,
        command: element(xml, "Command").filter(|c| !c.trim().is_empty()),
        arguments: element(xml, "Arguments").filter(|a| !a.trim().is_empty()),
        state,
        principal: element(xml, "UserId")
            .or_else(|| element(xml, "GroupId"))
            .filter(|p| !p.trim().is_empty()),
    }
}

/// The text content of the first `<name>…</name>` element.
fn element(xml: &str, name: &str) -> Option<String> {
    let open = format!("<{name}>");
    let close = format!("</{name}>");
    let start = xml.find(&open)? + open.len();
    let end = xml[start..].find(&close)? + start;
    Some(unescape(&xml[start..end]))
}

/// Counts occurrences of an element's opening tag.
///
/// Matches `<Name>` and `<Name ` but not `<NameSomething>`, so
/// `LogonTrigger` cannot be matched by a hypothetical `LogonTriggerEx`.
fn count_elements(xml: &str, name: &str) -> usize {
    let needle = format!("<{name}");
    let mut count = 0;
    let mut rest = xml;

    while let Some(index) = rest.find(&needle) {
        let after = &rest[index + needle.len()..];
        if matches!(
            after.chars().next(),
            Some('>' | ' ' | '\t' | '\r' | '\n' | '/')
        ) {
            count += 1;
        }
        rest = after;
    }

    count
}

/// Resolves the five predefined XML entities.
fn unescape(text: &str) -> String {
    if !text.contains('&') {
        return text.to_owned();
    }
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        // Ampersand last, or `&amp;lt;` would become `<`.
        .replace("&amp;", "&")
}

/// Converts a startup task into a generic entry.
#[must_use]
pub fn to_entry(task: &TaskInfo) -> StartupEntry {
    let command = match (&task.command, &task.arguments) {
        (Some(cmd), Some(args)) => Some(format!("{cmd} {args}")),
        (Some(cmd), None) => Some(cmd.clone()),
        _ => None,
    };

    let image_path = task
        .command
        .as_ref()
        .map(|c| PathBuf::from(super::registry::expand_environment(c.trim_matches('"'))));

    StartupEntry {
        name: task.path.clone(),
        display_name: task.path.rsplit('\\').next().map(str::to_owned),
        source: StartupSource::ScheduledTask,
        state: task.state,
        command,
        image_path,
        publisher: None,
        pid: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MULTI_TRIGGER: &str = r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.4">
  <Triggers>
    <CalendarTrigger>
      <StartBoundary>2024-01-01T03:00:00</StartBoundary>
      <Enabled>true</Enabled>
      <ScheduleByDay><DaysInterval>1</DaysInterval></ScheduleByDay>
    </CalendarTrigger>
    <BootTrigger>
      <Enabled>true</Enabled>
      <Delay>PT30S</Delay>
    </BootTrigger>
  </Triggers>
  <Principals>
    <Principal id="Author"><UserId>S-1-5-18</UserId></Principal>
  </Principals>
  <Settings>
    <Enabled>true</Enabled>
  </Settings>
  <Actions Context="Author">
    <Exec>
      <Command>C:\Program Files\Vendor\updater.exe</Command>
      <Arguments>/silent &amp; /nologo</Arguments>
    </Exec>
  </Actions>
</Task>"#;

    #[test]
    fn a_boot_trigger_after_a_schedule_is_still_found() {
        // The bug this pins: stopping at the first <*Trigger> element finds
        // only the CalendarTrigger and concludes the task is not a startup
        // item. Vendor updaters declare their schedule first almost without
        // exception, so this loses precisely the entries that matter.
        let task = parse_task(MULTI_TRIGGER);
        assert!(
            task.triggers.contains(&TaskTrigger::Boot),
            "the BootTrigger is declared second and must still be seen; got {:?}",
            task.triggers
        );
        assert!(task.triggers.contains(&TaskTrigger::Other));
        assert!(task.runs_at_startup());
    }

    #[test]
    fn command_and_arguments_are_unescaped() {
        let task = parse_task(MULTI_TRIGGER);
        assert_eq!(
            task.command.as_deref(),
            Some(r"C:\Program Files\Vendor\updater.exe")
        );
        assert_eq!(
            task.arguments.as_deref(),
            Some("/silent & /nologo"),
            "&amp; must be resolved or the command shown is not the command run"
        );
        assert_eq!(task.principal.as_deref(), Some("S-1-5-18"));
    }

    #[test]
    fn trigger_enabled_is_not_mistaken_for_task_enabled() {
        // Every trigger carries its own <Enabled>. A scan for the first one
        // reads the CalendarTrigger's value, so a task whose Settings say
        // false but whose first trigger says true is reported as enabled.
        let xml = MULTI_TRIGGER.replace(
            "<Settings>\n    <Enabled>true</Enabled>",
            "<Settings>\n    <Enabled>false</Enabled>",
        );
        assert_eq!(
            parse_task(&xml).state,
            StartupState::Disabled,
            "the Settings block is authoritative, not the first <Enabled> in \
             the document"
        );
    }

    #[test]
    fn a_task_with_no_settings_block_defaults_to_enabled() {
        let xml = "<Task><Triggers><LogonTrigger/></Triggers></Task>";
        let task = parse_task(xml);
        assert_eq!(
            task.state,
            StartupState::Enabled,
            "the schema's documented default; this is a specification fact, \
             not a guess"
        );
        assert!(task.triggers.contains(&TaskTrigger::Logon));
    }

    #[test]
    fn self_closing_trigger_elements_are_counted() {
        // Exporters emit <LogonTrigger/> for a trigger with no children.
        assert_eq!(count_elements("<a><LogonTrigger/></a>", "LogonTrigger"), 1);
        assert_eq!(
            count_elements(r#"<LogonTrigger id="x"></LogonTrigger>"#, "LogonTrigger"),
            1
        );
    }

    #[test]
    fn element_names_are_matched_whole() {
        assert_eq!(
            count_elements("<LogonTriggerExtra></LogonTriggerExtra>", "LogonTrigger"),
            0,
            "a prefix match would let an unrelated element create a phantom \
             logon trigger"
        );
    }

    #[test]
    fn a_com_handler_task_reports_no_command() {
        let xml = r"<Task><Triggers><LogonTrigger/></Triggers>
            <Actions><ComHandler><ClassId>{1234}</ClassId></ComHandler></Actions></Task>";
        let task = parse_task(xml);
        assert_eq!(
            task.command, None,
            "a COM-handler task has no command line; an empty string here \
             would render as a measured blank"
        );
        assert!(task.runs_at_startup());
    }

    #[test]
    fn an_empty_command_element_is_none_not_blank() {
        let xml = "<Task><Actions><Exec><Command>   </Command></Exec></Actions></Task>";
        assert_eq!(parse_task(xml).command, None);
    }

    #[test]
    fn utf16_definitions_decode_and_utf8_ones_survive() {
        let text = "<Task><Command>a.exe</Command></Task>";

        let mut utf16 = vec![0xFF, 0xFE];
        for unit in text.encode_utf16() {
            utf16.extend_from_slice(&unit.to_le_bytes());
        }
        assert_eq!(decode_utf16le(&utf16).as_deref(), Some(text));

        // Without a BOM. Note that these bytes ARE valid UTF-8 — NUL is a
        // legal UTF-8 byte — so a plain "try UTF-8 first" would decode them
        // into interleaved NULs and find no elements. The NUL probe is what
        // makes this resolve correctly.
        assert!(
            std::str::from_utf8(&utf16[2..]).is_ok(),
            "UTF-16LE ASCII is valid UTF-8, which is exactly why encoding \
             cannot be detected by validity alone"
        );
        assert_eq!(decode_utf16le(&utf16[2..]).as_deref(), Some(text));

        let mut utf8 = vec![0xEF, 0xBB, 0xBF];
        utf8.extend_from_slice(text.as_bytes());
        assert_eq!(decode_utf16le(&utf8).as_deref(), Some(text));
    }

    #[test]
    fn a_truncated_definition_is_rejected_rather_than_half_read() {
        assert_eq!(
            decode_utf16le(&[0xFF, 0xFE, 0x41]),
            None,
            "an odd byte count is corrupt; half-decoding yields a task with \
             no triggers, which reads as 'not a startup item' rather than as \
             an error"
        );
    }

    #[test]
    fn bomless_utf8_is_not_mangled_into_utf16() {
        // The exact bug this module hit. `schtasks /query /xml` emits UTF-8
        // with no BOM. Decoding those bytes as UTF-16 succeeds — it yields
        // valid CJK mojibake — so no error is raised, no element name is
        // ever matched, and the scan reports zero startup tasks on a machine
        // that has dozens.
        let mut xml = String::from(
            "<Tasks><Task version=\"1.2\"><RegistrationInfo><URI>\\Ab</URI>\
             </RegistrationInfo><Triggers><LogonTrigger/></Triggers></Task></Tasks>",
        );
        // An odd-length buffer would be rejected outright, which is not the
        // failure being pinned: the dangerous case is a UTF-8 document whose
        // length happens to be even, so the UTF-16 path succeeds.
        if xml.len() % 2 != 0 {
            xml.push(' ');
        }

        let decoded = decode_utf16le(xml.as_bytes()).expect("valid UTF-8");
        assert_eq!(decoded, xml);

        let scan = parse_scan(&decoded);
        assert_eq!(scan.startup_tasks.len(), 1, "decoded as UTF-16 this is 0");
    }

    #[test]
    fn ampersand_is_unescaped_last() {
        assert_eq!(
            unescape("&amp;lt;"),
            "&lt;",
            "resolving &amp; first would turn this into a literal '<'"
        );
    }

    #[test]
    fn a_concatenated_document_splits_into_its_tasks() {
        let doc = "<Tasks>\
            <Task version=\"1.4\"><RegistrationInfo><URI>\\A</URI></RegistrationInfo>\
            <Triggers><LogonTrigger/></Triggers></Task>\
            <Task version=\"1.2\"><RegistrationInfo><URI>\\B</URI></RegistrationInfo>\
            <Triggers><TimeTrigger/></Triggers></Task></Tasks>";

        let scan = parse_scan(doc);
        assert_eq!(scan.total_seen, 2, "both blocks must be seen");
        assert_eq!(
            scan.startup_tasks.len(),
            1,
            "only the logon-triggered task is a startup item"
        );
        assert_eq!(scan.startup_tasks[0].path, r"\A");
        assert_eq!(scan.unreadable, 0);
    }

    #[test]
    fn a_task_without_a_uri_is_counted_as_unreadable_not_invented() {
        let doc = "<Tasks><Task version=\"1.4\"><Triggers><LogonTrigger/></Triggers>\
            </Task></Tasks>";
        let scan = parse_scan(doc);
        assert_eq!(scan.total_seen, 1);
        assert_eq!(
            scan.unreadable, 1,
            "a task we cannot name cannot be acted on; giving it a made-up \
             path would put an unusable row in the UI"
        );
        assert!(scan.startup_tasks.is_empty());
    }

    #[test]
    fn closing_tag_matching_does_not_cut_at_a_nested_element() {
        // A naive split on "</Task" would end the block at </TaskName>,
        // truncating the definition before its Triggers and silently
        // dropping a real startup task.
        let doc = "<Tasks><Task version=\"1.4\">\
            <RegistrationInfo><URI>\\X</URI><TaskName>N</TaskName></RegistrationInfo>\
            <Triggers><BootTrigger/></Triggers></Task></Tasks>";
        let scan = parse_scan(doc);
        assert_eq!(scan.startup_tasks.len(), 1);
        assert!(scan.startup_tasks[0].triggers.contains(&TaskTrigger::Boot));
    }

    #[test]
    fn real_scan_finds_startup_tasks_and_reports_what_it_could_not_read() {
        let scan = scan_tasks();

        assert!(
            scan.total_seen > 20,
            "a stock Windows install ships well over twenty tasks; saw {}",
            scan.total_seen
        );

        assert!(
            !scan.startup_tasks.is_empty(),
            "every install has logon or boot triggered tasks; an empty list \
             here is how the ACL'd directory route failed silently"
        );

        // Every returned task must genuinely have a startup trigger,
        // otherwise the filter is not doing its job.
        for task in &scan.startup_tasks {
            assert!(
                task.runs_at_startup(),
                "{} was returned without a logon or boot trigger",
                task.path
            );
            assert!(
                task.path.starts_with('\\'),
                "{} is not a task path",
                task.path
            );
        }

        // The unreadable count must be consistent, not merely non-negative.
        assert!(
            scan.unreadable <= scan.total_seen,
            "cannot have failed to read more definitions than exist"
        );
    }
}
