//! The six tools, and the functions that answer them.
//!
//! Six, all read-only: the summary, the process list, the sensors, the raised
//! alerts, a slice of history, and the "why is it slow" verdict. There is no
//! tool that changes anything, and `policy::is_control_name` fails a test if
//! one is ever named like it would.
//!
//! Everything here is pure apart from the [`McpHost`] call it makes: the
//! arguments are validated, the host is asked, and the answer is rendered as
//! a JSON object an agent can read. No string is ever hand-built, so the
//! wire shapes cannot drift from the types `vitals-core` exports.

use serde::Serialize;
use serde_json::{Map, Value, json};

use vitals_core::MachineSample;
use vitals_core::alerts::Alert;
use vitals_core::process::Process;
use vitals_core::remote::SensorLine;

use crate::host::McpHost;
use crate::jsonrpc::Error;
use crate::policy;

/// What a tool produced.
///
/// A malformed request is a JSON-RPC error (the caller wrote something the
/// protocol cannot carry); a tool that ran and could not answer is
/// [`Outcome::Failed`], which travels as a normal result carrying `isError`
/// so the reason reaches the agent's transcript instead of being swallowed
/// into a transport-level failure.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// Structured data, ready for `structuredContent`.
    Data(Value),
    /// Why there is no data, in one sentence.
    Failed(String),
}

/// One tool this server offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Summary,
    Processes,
    Sensors,
    Alerts,
    History,
    Diagnosis,
}

impl Tool {
    /// Every tool, in the order `tools/list` reports them. Readings first,
    /// the verdict last: an agent that reads the list top to bottom gets the
    /// cheap overview before the expensive join.
    pub const ALL: [Self; 6] = [
        Self::Summary,
        Self::Processes,
        Self::Sensors,
        Self::Alerts,
        Self::History,
        Self::Diagnosis,
    ];

    /// The name an agent calls it by. Stable: it is the string in a client's
    /// allow-list, not a label.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Summary => "summary",
            Self::Processes => "processes",
            Self::Sensors => "sensors",
            Self::Alerts => "alerts",
            Self::History => "history",
            Self::Diagnosis => "diagnosis",
        }
    }

    /// What the tool does, written for a model choosing between them: what it
    /// answers, when it is the right one, and what an empty answer means.
    #[must_use]
    pub const fn description(self) -> &'static str {
        match self {
            Self::Summary => {
                "The machine right now: CPU, memory, disk, network, GPU and temperature, \
                 plus the busiest processes and the total process count. A few kilobytes \
                 against the full list, so it is the right first call. Call it again to \
                 see whether a number is moving."
            }
            Self::Processes => {
                "The busiest processes on this machine, most CPU first, with the total \
                 number of processes so you can tell how much of the machine the list \
                 covers. A process missing from the list is not stopped — lower the \
                 ranking or ask again."
            }
            Self::Sensors => {
                "Every hardware sensor reading this machine can make: temperatures, fan \
                 speeds, voltages, power draw and battery state, each with the unit, how \
                 it was obtained, and how much to trust it. An empty list means nothing \
                 is measurable here, not that everything reads zero."
            }
            Self::Alerts => {
                "The conditions Vitals considers a problem at this moment, most serious \
                 first: sustained CPU, disk or memory load, a disk filling up or failing, \
                 a hot CPU, a worn battery, an automatic service that stopped. An empty \
                 list is the healthy answer, and each alert carries the readings behind \
                 it and how long it has held."
            }
            Self::History => {
                "Machine-wide samples covering the last N seconds, at whatever resolution \
                 the store keeps for that span. Use it for a question about the past — \
                 'has memory been climbing all morning?' — rather than the present. An \
                 empty list means nothing has been recorded, which is also what a host \
                 without a store answers."
            }
            Self::Diagnosis => {
                "One verdict answering 'why is this PC slow', naming the processes that \
                 account for it, plus everything else currently raised. A null verdict \
                 means nothing is wrong, which is a real answer. Contributors are \
                 reported with their measured share and never as a cause."
            }
        }
    }

    /// The tool as MCP's `Tool` object.
    #[must_use]
    pub fn definition(self) -> Value {
        json!({
            "name": self.name(),
            "description": self.description(),
            "inputSchema": self.input_schema(),
        })
    }

    /// A JSON Schema for the arguments, which is what a client validates a
    /// model's tool call against before sending it.
    #[must_use]
    pub fn input_schema(self) -> Value {
        match self {
            Self::Summary | Self::Processes => {
                let default = if self == Self::Summary {
                    policy::SUMMARY_DEFAULT_TOP
                } else {
                    policy::PROCESSES_DEFAULT_TOP
                };
                json!({
                    "type": "object",
                    "properties": {
                        "top": {
                            "type": "integer",
                            "minimum": 1,
                            "maximum": policy::MAX_TOP,
                            "default": default,
                            "description": "How many processes to return, busiest first.",
                        }
                    },
                    "additionalProperties": false,
                })
            }
            Self::History => json!({
                "type": "object",
                "properties": {
                    "seconds": {
                        "type": "integer",
                        "minimum": 60,
                        "maximum": policy::HISTORY_MAX_SECONDS,
                        "default": policy::HISTORY_DEFAULT_SECONDS,
                        "description": "How far back to read, in seconds.",
                    }
                },
                "additionalProperties": false,
            }),
            Self::Sensors | Self::Alerts | Self::Diagnosis => {
                json!({ "type": "object", "properties": {}, "additionalProperties": false })
            }
        }
    }

    /// The tool with this name, or `None`.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|tool| tool.name() == name)
    }

    /// Answers the tool.
    ///
    /// # Errors
    /// [`Error::INVALID_PARAMS`] when the arguments are not what the tool's
    /// schema says. A host that cannot answer is not an error — it is
    /// [`Outcome::Failed`], so the reason survives into the transcript.
    pub fn call<H: McpHost + ?Sized>(
        self,
        host: &H,
        arguments: Option<&Value>,
    ) -> Result<Outcome, Error> {
        match self {
            Self::Summary => summary(host, arguments),
            Self::Processes => processes(host, arguments),
            Self::Sensors => Ok(sensors(host)),
            Self::Alerts => Ok(alerts(host)),
            Self::History => history(host, arguments),
            Self::Diagnosis => Ok(diagnosis(host)),
        }
    }
}

/// The one sentence every tool uses when the sampler has not produced a frame
/// yet, so an agent is told the same thing whichever tool it reached for.
pub const NOT_SAMPLING: &str = "Vitals has not taken a first reading yet; call again in a second.";

// ── The tools ──────────────────────────────────────────────────────────

fn summary<H: McpHost + ?Sized>(host: &H, arguments: Option<&Value>) -> Result<Outcome, Error> {
    let top = policy::top(
        optional_count(arguments, "top")?,
        policy::SUMMARY_DEFAULT_TOP,
    );
    match host.summary(top) {
        Ok(Some(summary)) => Ok(payload(&summary)),
        Ok(None) => Ok(Outcome::Failed(NOT_SAMPLING.to_owned())),
        Err(why) => Ok(Outcome::Failed(why)),
    }
}

fn processes<H: McpHost + ?Sized>(host: &H, arguments: Option<&Value>) -> Result<Outcome, Error> {
    let top = policy::top(
        optional_count(arguments, "top")?,
        policy::PROCESSES_DEFAULT_TOP,
    );
    match host.summary(top) {
        Ok(Some(summary)) => Ok(payload(&ProcessList {
            processes: &summary.top,
            count: summary.process_count,
            returned: summary.top.len(),
        })),
        Ok(None) => Ok(Outcome::Failed(NOT_SAMPLING.to_owned())),
        Err(why) => Ok(Outcome::Failed(why)),
    }
}

/// Cannot fail: a sensor read either happens or has a reason, and both are
/// answers rather than a caller's mistake.
fn sensors<H: McpHost + ?Sized>(host: &H) -> Outcome {
    match host.sensors() {
        Ok(sensors) => payload(&SensorList { sensors }),
        Err(why) => Outcome::Failed(why),
    }
}

/// Cannot fail, for the same reason as [`sensors`]: an empty list is the
/// healthy answer and a refusal is a sentence.
fn alerts<H: McpHost + ?Sized>(host: &H) -> Outcome {
    match host.alerts() {
        Ok(alerts) => payload(&AlertList { alerts }),
        Err(why) => Outcome::Failed(why),
    }
}

fn history<H: McpHost + ?Sized>(host: &H, arguments: Option<&Value>) -> Result<Outcome, Error> {
    let seconds = policy::seconds(optional_u32(arguments, "seconds")?);
    match host.history(seconds) {
        // The span is echoed back, so an agent can tell an empty history from
        // a history it never asked for.
        Ok(samples) => Ok(payload(&History { seconds, samples })),
        Err(why) => Ok(Outcome::Failed(why)),
    }
}

/// Cannot fail: "nothing is wrong" and "not sampling yet" are both answers.
fn diagnosis<H: McpHost + ?Sized>(host: &H) -> Outcome {
    match host.diagnosis() {
        Ok(Some(diagnosis)) => payload(&diagnosis),
        Ok(None) => Outcome::Failed(NOT_SAMPLING.to_owned()),
        Err(why) => Outcome::Failed(why),
    }
}

// ── The wire shapes ────────────────────────────────────────────────────

/// The process list, ranked by `Summary`'s one ranking rule so this tool and
/// the summary cannot disagree about which processes are busiest.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProcessList<'a> {
    processes: &'a [Process],
    /// Every process on the machine, not just those returned.
    count: u32,
    /// How many are in `processes`, so `count` and the array need not be
    /// compared by hand.
    returned: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SensorList {
    sensors: Vec<SensorLine>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AlertList {
    alerts: Vec<Alert>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct History {
    seconds: u32,
    samples: Vec<MachineSample>,
}

/// Serialises a wire shape, turning the (practically impossible) failure into
/// a tool error rather than a panic: `expect_used` is denied outside tests,
/// and a monitor that crashes because a float was NaN would be worse.
fn payload<T: Serialize>(value: &T) -> Outcome {
    match serde_json::to_value(value) {
        Ok(value) => Outcome::Data(value),
        Err(error) => Outcome::Failed(format!("this reading could not be serialised: {error}")),
    }
}

// ── Arguments ──────────────────────────────────────────────────────────

/// Whether the tool was called with an `arguments` object, `null`, or
/// nothing at all. Anything else is the caller's mistake, not an empty list.
fn arguments_map(arguments: Option<&Value>) -> Result<Option<&Map<String, Value>>, Error> {
    match arguments {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Object(map)) => Ok(Some(map)),
        Some(other) => Err(Error::invalid_params(format!(
            "`arguments` must be an object, not {other}"
        ))),
    }
}

/// A whole-number argument, or `None` when it was not given.
fn optional_count(arguments: Option<&Value>, key: &str) -> Result<Option<usize>, Error> {
    let Some(value) = lookup(arguments, key)? else {
        return Ok(None);
    };
    let Some(n) = value.as_u64() else {
        return Err(Error::invalid_params(format!(
            "`{key}` must be a whole number of at least zero, not {value}"
        )));
    };
    // Saturating rather than wrapping on a 32-bit target; `policy::top`
    // brings it inside the range from there.
    Ok(Some(usize::try_from(n).unwrap_or(policy::MAX_TOP)))
}

/// A `u32` argument, or `None` when it was not given.
fn optional_u32(arguments: Option<&Value>, key: &str) -> Result<Option<u32>, Error> {
    let Some(value) = lookup(arguments, key)? else {
        return Ok(None);
    };
    let Some(n) = value.as_u64() else {
        return Err(Error::invalid_params(format!(
            "`{key}` must be a whole number of at least zero, not {value}"
        )));
    };
    Ok(Some(u32::try_from(n).unwrap_or(u32::MAX)))
}

fn lookup<'a>(arguments: Option<&'a Value>, key: &str) -> Result<Option<&'a Value>, Error> {
    Ok(arguments_map(arguments)?.and_then(|map| map.get(key)))
}

/// The reason a tool name is not one of ours, for the error message.
#[must_use]
pub fn unknown_tool_message(name: &str) -> String {
    let known: Vec<&str> = Tool::ALL.iter().map(|tool| tool.name()).collect();
    format!("no tool `{name}`; this server offers {}", known.join(", "))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::test_host::TestHost;

    fn data(tool: Tool, arguments: Option<&Value>) -> Value {
        match tool.call(&TestHost::new(), arguments) {
            Ok(Outcome::Data(value)) => value,
            other => panic!("{tool:?} did not return data: {other:?}"),
        }
    }

    #[test]
    fn every_tool_declares_a_name_a_description_and_an_object_schema() {
        // A tool whose schema is not an object is rejected by clients before
        // a model ever sees it, and a missing description is a tool no model
        // chooses.
        for tool in Tool::ALL {
            let definition = tool.definition();
            assert_eq!(definition["name"], tool.name());
            assert!(
                definition["description"]
                    .as_str()
                    .is_some_and(|d| d.len() > 40),
                "{tool:?} has no useful description"
            );
            assert_eq!(definition["inputSchema"]["type"], "object");
        }
    }

    #[test]
    fn a_tool_is_found_by_exactly_the_name_it_announces() {
        for tool in Tool::ALL {
            assert_eq!(Tool::from_name(tool.name()), Some(tool));
        }
        assert_eq!(
            Tool::from_name("Summary"),
            None,
            "names are not case-folded"
        );
        assert_eq!(Tool::from_name("kill_process"), None);
    }

    #[test]
    fn an_unknown_tool_name_lists_the_ones_that_exist() {
        let message = unknown_tool_message("nope");
        for tool in Tool::ALL {
            assert!(message.contains(tool.name()), "{message}");
        }
    }

    #[test]
    fn summary_returns_the_machine_and_the_processes_the_host_ranked() {
        let value = data(Tool::Summary, None);
        assert!(value["system"]["cpu"].is_object(), "{value}");
        assert_eq!(value["processCount"], 3);
        assert_eq!(value["top"].as_array().unwrap().len(), 3);
    }

    #[test]
    fn processes_reports_the_total_so_a_short_list_is_not_mistaken_for_a_small_machine() {
        let value = data(Tool::Processes, Some(&json!({ "top": 2 })));
        assert_eq!(value["processes"].as_array().unwrap().len(), 2);
        assert_eq!(value["returned"], 2);
        assert_eq!(value["count"], 3, "the machine has three");
    }

    #[test]
    fn a_greedy_top_is_clamped_rather_than_refused() {
        let value = data(Tool::Processes, Some(&json!({ "top": 1_000_000 })));
        assert_eq!(value["processes"].as_array().unwrap().len(), 3);
        assert_eq!(value["count"], 3);
    }

    #[test]
    fn a_top_that_is_not_a_number_is_the_caller_s_mistake_and_is_named_as_such() {
        let error = Tool::Processes
            .call(&TestHost::new(), Some(&json!({ "top": "many" })))
            .expect_err("a string is not a count");
        assert_eq!(error.code, crate::jsonrpc::INVALID_PARAMS);
        assert!(error.message.contains("`top`"), "{}", error.message);
    }

    #[test]
    fn arguments_that_are_not_an_object_are_refused_before_the_host_is_asked() {
        let error = Tool::Summary
            .call(&TestHost::new(), Some(&json!("please")))
            .expect_err("a string is not an arguments object");
        assert_eq!(error.code, crate::jsonrpc::INVALID_PARAMS);
    }

    #[test]
    fn sensors_and_alerts_wrap_their_lists_so_an_empty_answer_is_still_an_object() {
        // `structuredContent` must be an object; a bare array would be
        // rejected by a client that validates it.
        let sensors = data(Tool::Sensors, None);
        assert_eq!(sensors["sensors"].as_array().unwrap().len(), 2);
        let alerts = data(Tool::Alerts, None);
        assert_eq!(alerts["alerts"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn history_echoes_the_span_it_read_so_empty_is_not_confused_with_unasked() {
        let value = data(Tool::History, Some(&json!({ "seconds": 7200 })));
        assert_eq!(value["seconds"], 7200);
        assert_eq!(value["samples"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn history_defaults_to_an_hour_and_clamps_a_silly_span() {
        assert_eq!(
            data(Tool::History, None)["seconds"],
            policy::HISTORY_DEFAULT_SECONDS
        );
        assert_eq!(
            data(Tool::History, Some(&json!({ "seconds": 99_999_999 })))["seconds"],
            policy::HISTORY_MAX_SECONDS
        );
    }

    #[test]
    fn diagnosis_carries_the_verdict_and_the_contributors_that_explain_it() {
        let value = data(Tool::Diagnosis, None);
        assert_eq!(value["verdict"]["subsystem"], "cpu");
        assert_eq!(value["verdict"]["contributors"][0]["name"], "chrome.exe");
    }

    #[test]
    fn a_host_that_has_not_sampled_yet_is_told_so_rather_than_shown_an_empty_machine() {
        let host = TestHost::not_sampling();
        for tool in [Tool::Summary, Tool::Processes, Tool::Diagnosis] {
            match tool.call(&host, None) {
                Ok(Outcome::Failed(why)) => assert_eq!(why, NOT_SAMPLING),
                other => panic!("{tool:?} answered a machine it has not seen: {other:?}"),
            }
        }
    }

    #[test]
    fn a_host_that_could_not_be_asked_says_why_instead_of_answering_nothing() {
        let why = match Tool::Sensors.call(&TestHost::without_sensors(), None) {
            Ok(Outcome::Failed(why)) => why,
            other => panic!("a host with no sensor service did not fail: {other:?}"),
        };
        assert!(
            why.contains("sensor service"),
            "the reason must name the cause: {why}"
        );
    }

    #[test]
    fn a_genuinely_empty_reading_is_a_value_and_not_a_failure() {
        // The whole of the crate's discipline in one assertion: empty is
        // `Ok`, and only "could not ask" is `Err`.
        let host = TestHost::empty();
        for tool in [Tool::Sensors, Tool::Alerts, Tool::History] {
            match tool.call(&host, None) {
                Ok(Outcome::Data(_)) => {}
                other => panic!("{tool:?} treated an empty reading as an error: {other:?}"),
            }
        }
    }
}
