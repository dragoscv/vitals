//! One message in, at most one message out.
//!
//! The whole of the session state is the host, so there is none: no session
//! id, no negotiation to remember, nothing to expire. That is what lets the
//! same code serve a long-lived `stdio` process and a stateless HTTP POST,
//! and it is why a request that arrives before `initialize` is answered
//! normally instead of being refused — there is no state it could corrupt,
//! and an inspector that sends `tools/list` first is a debugging aid worth
//! supporting.

use serde::Deserialize;
use serde_json::{Value, json};

use crate::host::McpHost;
use crate::jsonrpc::{self, Error, Request};
use crate::protocol;
use crate::tools::{Outcome, Tool, unknown_tool_message};

/// The MCP methods this server implements.
const INITIALIZE: &str = "initialize";
const INITIALIZED: &str = "notifications/initialized";
const PING: &str = "ping";
const TOOLS_LIST: &str = "tools/list";
const TOOLS_CALL: &str = "tools/call";
const RESOURCES_LIST: &str = "resources/list";
const RESOURCES_READ: &str = "resources/read";

/// A server bound to one machine.
pub struct Server<H> {
    host: H,
}

// Written out rather than derived: a `H: Debug` bound on the whole server
// would force every host — including one holding a sampler that has no
// useful `Debug` — to implement it for no reason.
impl<H> std::fmt::Debug for Server<H> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Server").finish_non_exhaustive()
    }
}

impl<H: McpHost> Server<H> {
    #[must_use]
    pub const fn new(host: H) -> Self {
        Self { host }
    }

    /// Handles one message, returning the reply to send — or `None` when
    /// nothing should be sent, which is the case for a notification, for a
    /// blank line and for the framing whitespace between them.
    #[must_use]
    pub fn handle(&self, text: &str) -> Option<Value> {
        let text = text.trim();
        if text.is_empty() {
            return None;
        }
        let request: Request = match serde_json::from_str(text) {
            Ok(request) => request,
            // No `id` to correlate with, so the reply carries `null`: a
            // client waiting on a request deserves to learn that its own
            // syntax was what failed, rather than waiting out a timeout.
            Err(error) => {
                let error = Error::parse(format!("this is not a JSON-RPC message: {error}"));
                return Some(jsonrpc::failure(&Value::Null, &error));
            }
        };
        if request.jsonrpc != jsonrpc::VERSION {
            let error = Error::invalid_request(format!(
                "`jsonrpc` must be \"{}\", not \"{}\"",
                jsonrpc::VERSION,
                request.jsonrpc
            ));
            let id = request.reply_id();
            return Some(jsonrpc::failure(&id, &error));
        }
        // A notification is acted on and never answered. Nothing here has a
        // side effect to perform, so "acted on" means "not treated as an
        // error".
        if request.is_notification() {
            return None;
        }
        let id = request.reply_id();
        Some(match self.dispatch(&request) {
            Ok(result) => jsonrpc::success(&id, &result),
            Err(error) => jsonrpc::failure(&id, &error),
        })
    }

    /// The result for one method.
    fn dispatch(&self, request: &Request) -> Result<Value, Error> {
        match request.method.as_str() {
            INITIALIZE => Ok(protocol::initialize(
                &self.host,
                requested_version(request.params.as_ref()),
            )),
            INITIALIZED | PING => Ok(json!({})),
            TOOLS_LIST => Ok(protocol::tools_list()),
            TOOLS_CALL => self.call_tool(request.params.as_ref()),
            RESOURCES_LIST => Ok(protocol::resources_list(&self.host)),
            RESOURCES_READ => {
                let uri = read_uri(request.params.as_ref())?;
                protocol::resource_contents(&self.host, &uri)
            }
            other => Err(Error::method_not_found(other)),
        }
    }

    fn call_tool(&self, params: Option<&Value>) -> Result<Value, Error> {
        let params = params.ok_or_else(|| {
            Error::invalid_params("`tools/call` needs a `name`, and nothing was sent")
        })?;
        let call: CallTool = serde_json::from_value(params.clone())
            .map_err(|error| Error::invalid_params(format!("could not read the call: {error}")))?;
        let tool = Tool::from_name(&call.name)
            .ok_or_else(|| Error::invalid_params(unknown_tool_message(&call.name)))?;
        Ok(match tool.call(&self.host, call.arguments.as_ref())? {
            Outcome::Data(payload) => protocol::tool_result(&payload),
            Outcome::Failed(reason) => protocol::tool_failure(&reason),
        })
    }
}

/// `tools/call`'s parameters. The schema is validated by the client against
/// the tool's `inputSchema`; this is the transport-level shape only.
#[derive(Debug, Deserialize)]
struct CallTool {
    name: String,
    #[serde(default)]
    arguments: Option<Value>,
}

fn requested_version(params: Option<&Value>) -> Option<&str> {
    params?.get("protocolVersion")?.as_str()
}

fn read_uri(params: Option<&Value>) -> Result<String, Error> {
    let params = params.ok_or_else(|| {
        Error::invalid_params("`resources/read` needs a `uri`, and nothing was sent")
    })?;
    params
        .get("uri")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| Error::invalid_params(format!("`uri` must be a string in {params}")))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::test_host::TestHost;
    use serde_json::json;

    fn ask(message: &Value) -> Value {
        Server::new(TestHost::new())
            .handle(&message.to_string())
            .unwrap_or_else(|| panic!("{message} was not answered"))
    }

    fn ask_text(text: &str) -> Value {
        Server::new(TestHost::new())
            .handle(text)
            .unwrap_or_else(|| panic!("{text} was not answered"))
    }

    #[test]
    fn a_blank_line_is_framing_and_gets_no_reply_at_all() {
        let server = Server::new(TestHost::new());
        assert!(server.handle("").is_none());
        assert!(server.handle("   \n").is_none());
    }

    #[test]
    fn the_initialize_then_tools_list_then_summary_handshake_produces_three_replies() {
        // The sequence an agent performs, end to end through one server.
        let server = Server::new(TestHost::new());

        let initialized = server
            .handle(
                &json!({
                    "jsonrpc": "2.0", "id": 1, "method": "initialize",
                    "params": { "protocolVersion": "2025-06-18", "clientInfo": { "name": "test" } },
                })
                .to_string(),
            )
            .expect("initialize is answered");
        assert_eq!(initialized["id"], 1);
        assert_eq!(initialized["result"]["serverInfo"]["name"], "vitals");

        // A notification about the handshake: sent, never answered.
        assert!(
            server
                .handle(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#)
                .is_none()
        );

        let listed = server
            .handle(r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#)
            .expect("tools/list is answered");
        assert_eq!(listed["id"], 2);
        assert_eq!(listed["result"]["tools"].as_array().unwrap().len(), 6);

        let called = server
            .handle(
                &json!({
                    "jsonrpc": "2.0", "id": 3, "method": "tools/call",
                    "params": { "name": "summary", "arguments": { "top": 2 } },
                })
                .to_string(),
            )
            .expect("tools/call is answered");
        assert_eq!(called["id"], 3);
        assert_eq!(called["result"]["isError"], false);
        let payload: Value =
            serde_json::from_str(called["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(payload["processCount"], 3);
    }

    #[test]
    fn a_body_that_is_not_json_is_answered_with_a_parse_error_and_a_null_id() {
        let reply = ask_text("not json at all");
        assert_eq!(reply["id"], Value::Null);
        assert_eq!(reply["error"]["code"], jsonrpc::PARSE_ERROR);
    }

    #[test]
    fn a_message_that_is_not_json_rpc_two_point_zero_is_refused_by_version() {
        let reply = ask_text(r#"{"jsonrpc":"1.0","id":9,"method":"ping"}"#);
        assert_eq!(reply["id"], 9, "the id was readable, so it is used");
        assert_eq!(reply["error"]["code"], jsonrpc::INVALID_REQUEST);
        assert!(reply["error"]["message"].as_str().unwrap().contains("1.0"));
    }

    #[test]
    fn an_unknown_method_is_a_method_not_found_naming_it() {
        let reply = ask(&json!({ "jsonrpc": "2.0", "id": 4, "method": "processes/kill" }));
        assert_eq!(reply["error"]["code"], jsonrpc::METHOD_NOT_FOUND);
        assert!(
            reply["error"]["message"]
                .as_str()
                .unwrap()
                .contains("processes/kill")
        );
    }

    #[test]
    fn an_unknown_tool_is_refused_with_the_list_of_ones_that_exist() {
        let reply = ask(&json!({
            "jsonrpc": "2.0", "id": 5, "method": "tools/call",
            "params": { "name": "end_process", "arguments": { "pid": 1 } },
        }));
        assert_eq!(reply["error"]["code"], jsonrpc::INVALID_PARAMS);
        let message = reply["error"]["message"].as_str().unwrap();
        assert!(message.contains("end_process"));
        assert!(message.contains("diagnosis"), "{message}");
    }

    #[test]
    fn a_tool_call_with_no_parameters_is_refused_rather_than_guessed_at() {
        let reply = ask(&json!({ "jsonrpc": "2.0", "id": 6, "method": "tools/call" }));
        assert_eq!(reply["error"]["code"], jsonrpc::INVALID_PARAMS);
    }

    #[test]
    fn a_tool_that_cannot_answer_is_a_result_with_is_error_not_a_json_rpc_error() {
        // A host that cannot reach a sensor service at all, so the tool has a
        // reason to report rather than an empty list to return.
        let reply = Server::new(TestHost::without_sensors())
            .handle(
                &json!({
                    "jsonrpc": "2.0", "id": 7, "method": "tools/call",
                    "params": { "name": "sensors" },
                })
                .to_string(),
            )
            .expect("the call is answered");
        assert!(reply.get("error").is_none(), "{reply}");
        assert_eq!(reply["result"]["isError"], true);
        assert!(
            reply["result"]["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("sensor service")
        );
    }

    #[test]
    fn reading_the_resource_the_server_advertised_returns_a_document() {
        let listed = ask(&json!({ "jsonrpc": "2.0", "id": 8, "method": "resources/list" }));
        let uri = listed["result"]["resources"][0]["uri"]
            .as_str()
            .unwrap()
            .to_owned();
        let read = ask(&json!({
            "jsonrpc": "2.0", "id": 9, "method": "resources/read",
            "params": { "uri": uri },
        }));
        assert!(read["result"]["contents"][0]["text"].is_string());
    }

    #[test]
    fn reading_a_uri_that_is_not_a_string_is_the_caller_s_mistake() {
        let reply = ask(&json!({
            "jsonrpc": "2.0", "id": 10, "method": "resources/read",
            "params": { "uri": 42 },
        }));
        assert_eq!(reply["error"]["code"], jsonrpc::INVALID_PARAMS);
    }

    #[test]
    fn a_request_for_a_method_we_do_not_have_never_reaches_the_host() {
        // `logging/setLevel` is a real MCP method this server does not
        // implement; answering it with an empty result would be a lie.
        let reply = ask(&json!({ "jsonrpc": "2.0", "id": 11, "method": "logging/setLevel" }));
        assert_eq!(reply["error"]["code"], jsonrpc::METHOD_NOT_FOUND);
    }
}
