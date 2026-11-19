//! `/mcp` — the same read-only Model Context Protocol server, over the LAN
//! server's Streamable HTTP transport.
//!
//! One machine, two clients, one implementation: the tools, the schemas and
//! the read-only guarantee all come from [`vitals_mcp`], and this module is
//! only the adapter that turns a `POST` body into an answer. Nothing about
//! the protocol is re-implemented here, which is what makes "no control
//! tools" true on both transports at once rather than twice by hand.
//!
//! # Off unless the server is running, and never without a token
//!
//! The route lives inside the authenticated router, so it is unreachable
//! until the user turns Remote access on, and it is refused for a loopback
//! caller riding the tokenless grant — the same rule as the AI page, for a
//! related reason. An MCP client is a program that reads a sentence and
//! decides what to do next; a token makes every such program one the user
//! paired deliberately, and revocable, rather than any process that happens
//! to run on the machine. Read scope is all it needs, and a read-scope token
//! can reach nothing here that acts: there is nothing that acts.

use std::sync::OnceLock;

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde_json::json;
use vitals_core::MachineSample;
use vitals_core::alerts::Alert;
use vitals_core::diagnosis::{Diagnosis, diagnose};
use vitals_core::provider::HostInfo;
use vitals_core::remote::{SensorLine, Summary};
use vitals_mcp::{McpHost, Reading, Server};

use crate::router::Granted;
use crate::state::ApiState;

/// `POST /mcp`: one JSON-RPC message in, at most one out.
///
/// Answers `202 Accepted` with no body for a notification, which is what the
/// transport asks for, and `application/json` otherwise. Server-sent events
/// are not offered: this server holds no session and pushes nothing, so the
/// JSON answer is the whole of the conversation.
pub(crate) async fn mcp(
    State(state): State<ApiState>,
    axum::Extension(granted): axum::Extension<Granted>,
    body: axum::body::Bytes,
) -> Response {
    if !granted.has_token() {
        // The same refusal on loopback and over the network: distinguishing
        // them would tell a caller which of the two it was.
        return (
            StatusCode::FORBIDDEN,
            Json(json!({
                "error": "the MCP endpoint needs a paired token, also on loopback",
            })),
        )
            .into_response();
    }

    let server = Server::new(ApiHost::new(state));
    // A body that is not UTF-8 is not JSON either, and the lossy conversion
    // lets the JSON parser produce the error message rather than this module
    // inventing a second one.
    match server.handle(&String::from_utf8_lossy(&body)) {
        Some(reply) => (
            [(
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/json"),
            )],
            reply.to_string(),
        )
            .into_response(),
        // A notification: accepted, and answered with nothing.
        None => StatusCode::ACCEPTED.into_response(),
    }
}

/// The server's materialised frame, behind the MCP host seam.
pub(crate) struct ApiHost {
    state: ApiState,
    /// Read at most once per request, and only if something asks for the
    /// machine's name: reading it costs a platform enquiry, and a tool call
    /// never needs it.
    machine: OnceLock<String>,
}

impl ApiHost {
    #[must_use]
    pub(crate) fn new(state: ApiState) -> Self {
        Self {
            state,
            machine: OnceLock::new(),
        }
    }

    /// The machine name, or a stable placeholder when the host cannot read
    /// one. Never empty: `vitals://` is a URI a client rejects as malformed.
    fn machine_name(&self) -> &str {
        self.machine.get_or_init(|| {
            (self.state.host)()
                .map(|info| info.hostname)
                .filter(|name| !name.trim().is_empty())
                .unwrap_or_else(|| "machine".to_owned())
        })
    }

    /// The reconstructed present, or `None` before the first frame.
    ///
    /// The keyframe rather than the newest frame: a delta carries only what
    /// changed, so a process list built from one would be whatever happened
    /// to move rather than what is running.
    fn frame(&self) -> Option<std::sync::Arc<vitals_core::sample::Frame>> {
        self.state.frames.initial()
    }
}

impl std::fmt::Debug for ApiHost {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ApiHost")
            .field("machine", &self.machine.get())
            .finish_non_exhaustive()
    }
}

impl McpHost for ApiHost {
    fn machine(&self) -> &str {
        self.machine_name()
    }

    fn host_info(&self) -> Option<HostInfo> {
        (self.state.host)()
    }

    fn summary(&self, top: usize) -> Reading<Option<Summary>> {
        Ok(self.state.frames.summary(top))
    }

    fn sensors(&self) -> Reading<Vec<SensorLine>> {
        // A closure, not a fallible call: the host is responsible for its own
        // caching and a failure there is a panic the route answers 500 for.
        // Reporting `Ok` here is honest — this adapter has no way to ask.
        Ok((self.state.sensors)())
    }

    fn alerts(&self) -> Reading<Vec<Alert>> {
        Ok((self.state.alerts)())
    }

    fn history(&self, seconds: u32) -> Reading<Vec<MachineSample>> {
        Ok((self.state.history)(seconds))
    }

    fn diagnosis(&self) -> Reading<Option<Diagnosis>> {
        let Some(frame) = self.frame() else {
            return Ok(None);
        };
        let vitals_core::sample::FramePayload::Keyframe { system, processes } = &frame.payload
        else {
            // `initial()` always renders a keyframe; a delta here would mean
            // the materialised view had been bypassed.
            return Ok(None);
        };
        Ok(Some(diagnose(&(self.state.alerts)(), system, processes)))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use vitals_core::fixtures;
    use vitals_core::sample::Frame;
    use vitals_mcp::Tool;

    fn state_with_a_frame() -> ApiState {
        let frames = crate::state::FrameSource::new();
        let mut frame = fixtures::keyframe(1, vec![fixtures::process("chrome.exe", 4242, 71.0)]);
        // The machine's own count has to match the list it serves.
        if let vitals_core::sample::FramePayload::Keyframe { system, .. } = &mut frame.payload {
            system.cpu.process_count = Some(1);
        }
        frames.publish(std::sync::Arc::new(frame));
        ApiState {
            frames,
            tokens: std::sync::Arc::new(crate::state::ServerLock::new(
                crate::auth::TokenSet::default(),
            )),
            controller: std::sync::Arc::new(crate::control::NoControl),
            assets: None,
            host: std::sync::Arc::new(|| None),
            alerts: std::sync::Arc::new(Vec::new),
            history: std::sync::Arc::new(|_| Vec::new()),
            sensors: std::sync::Arc::new(Vec::new),
            connectivity: std::sync::Arc::new(|| {
                vitals_core::connectivity::ConnectivityReport::from_parts(None, Vec::new(), None)
            }),
            version: "0.0.0-test".to_owned(),
            loopback_scope: None,
            pairing: None,
            agents: None,
        }
    }

    #[test]
    fn a_host_that_cannot_name_the_machine_still_gets_a_valid_resource_uri() {
        // The alternative is the bare `vitals://`, which a client library
        // rejects as malformed rather than reading as a machine.
        let host = ApiHost::new(state_with_a_frame());
        assert_eq!(host.machine(), "machine");
        assert_eq!(
            vitals_mcp::protocol::resource_uri(host.machine()),
            "vitals://machine"
        );
    }

    #[test]
    fn the_machine_name_is_read_once_however_often_it_is_asked_for() {
        // A platform enquiry per question would put a WMI round trip behind
        // every `resources/list`.
        use std::sync::atomic::{AtomicUsize, Ordering};
        let calls = std::sync::Arc::new(AtomicUsize::new(0));
        let counter = std::sync::Arc::clone(&calls);
        let mut state = state_with_a_frame();
        state.host = std::sync::Arc::new(move || {
            counter.fetch_add(1, Ordering::SeqCst);
            None
        });
        let host = ApiHost::new(state);
        assert_eq!(host.machine(), "machine");
        assert_eq!(host.machine(), "machine");
        assert_eq!(host.machine(), "machine");
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn the_adapter_offers_exactly_the_crates_read_only_tools() {
        // The one thing this adapter could get wrong on its own: exposing a
        // different set from the stdio transport.
        let server = Server::new(ApiHost::new(state_with_a_frame()));
        let listed = server
            .handle(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#)
            .expect("tools/list is answered");
        let names: Vec<String> = listed["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap().to_owned())
            .collect();
        let expected: Vec<String> = Tool::ALL
            .iter()
            .map(|tool| tool.name().to_owned())
            .collect();
        assert_eq!(names, expected);
    }

    #[test]
    fn a_summary_over_http_describes_the_materialised_frame() {
        let server = Server::new(ApiHost::new(state_with_a_frame()));
        let reply = server
            .handle(r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"summary"}}"#)
            .expect("the call is answered");
        assert_eq!(reply["result"]["isError"], false);
        assert_eq!(reply["result"]["structuredContent"]["processCount"], 1);
    }

    #[test]
    fn no_control_tool_can_be_reached_through_the_http_adapter() {
        let server = Server::new(ApiHost::new(state_with_a_frame()));
        for name in ["end_process", "kill", "suspend", "set_priority"] {
            let reply = server
                .handle(&format!(
                    r#"{{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{{"name":"{name}"}}}}"#
                ))
                .expect("the call is answered");
            assert_eq!(
                reply["error"]["code"], -32602,
                "`{name}` must not be a tool: {reply}"
            );
        }
    }

    #[test]
    fn a_server_that_has_not_sampled_yet_answers_the_tools_rather_than_an_empty_machine() {
        let mut state = state_with_a_frame();
        state.frames = crate::state::FrameSource::new();
        let server = Server::new(ApiHost::new(state));
        let reply = server
            .handle(r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"summary"}}"#)
            .expect("the call is answered");
        assert_eq!(reply["result"]["isError"], true);
    }

    #[test]
    fn the_frame_used_for_a_diagnosis_is_a_keyframe_with_a_process_list() {
        let host = ApiHost::new(state_with_a_frame());
        let frame: std::sync::Arc<Frame> = host.frame().unwrap();
        assert!(matches!(
            frame.payload,
            vitals_core::sample::FramePayload::Keyframe { .. }
        ));
        assert!(host.diagnosis().unwrap().is_some());
    }
}
