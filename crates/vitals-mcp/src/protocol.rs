//! The MCP messages themselves: what a client sends, and what it gets back.
//!
//! Pure functions over a [`McpHost`] and a JSON value, so every shape an
//! agent sees can be asserted without a process, a socket or an agent.

use serde_json::{Value, json};

use crate::host::McpHost;
use crate::jsonrpc::Error;
use crate::tools::Tool;

/// The MCP revision this server implements.
///
/// Named rather than negotiated freely: the server answers a client that asks
/// for something else with *this* string, which is what the specification
/// says to do when the requested revision is not one the server knows. A
/// client that cannot live with it disconnects itself.
pub const PROTOCOL_VERSION: &str = "2025-06-18";

/// The name reported in `serverInfo`, and the one an agent's allow-list uses.
pub const SERVER_NAME: &str = "vitals";

/// The version of this server, taken from the crate so a release cannot
/// forget to bump it.
#[must_use]
pub const fn server_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// What an agent should know about this server before choosing a tool.
///
/// MCP has a field for this, and it is worth filling: an agent that reads it
/// knows the server is read-only, so it stops looking for a tool to end a
/// process and starts explaining what the numbers mean instead.
#[must_use]
pub fn instructions(host: &dyn McpHost) -> String {
    format!(
        "Read-only monitor for the Windows computer \"{machine}\". The tools report what \
         is happening on it: system load, the processes behind it, sensor readings, \
         alerts, the recent history and a verdict on what is slowing it down. Nothing \
         here can change the machine — there is no tool that ends, freezes or \
         re-prioritises anything, and asking for one will fail. A reading this machine \
         cannot make is absent or null, never zero.",
        machine = host.machine()
    )
}

/// The reply to `initialize`.
///
/// `requested` is the client's `protocolVersion`, if it sent one. The answer
/// is always [`PROTOCOL_VERSION`]: see its documentation for why that is the
/// specification's own recommendation rather than a shortcut.
#[must_use]
pub fn initialize(host: &dyn McpHost, requested: Option<&str>) -> Value {
    json!({
        "protocolVersion": negotiated_version(requested),
        "capabilities": {
            "tools": { "listChanged": false },
            // No subscriptions and no change notifications: the machine's
            // numbers move every second and pushing them would be a stream,
            // not a resource. An agent calls the tools when it wants them.
            "resources": { "subscribe": false, "listChanged": false },
        },
        "serverInfo": { "name": SERVER_NAME, "version": server_version() },
        "instructions": instructions(host),
    })
}

/// The revision the server will speak, given what the client asked for.
#[must_use]
pub fn negotiated_version(requested: Option<&str>) -> &'static str {
    match requested {
        // One arm on purpose: this server knows exactly one revision, so the
        // answer cannot depend on the ask. The parameter remains because the
        // reply is defined in terms of it, and a second revision would be
        // chosen here rather than in the caller.
        Some(_) | None => PROTOCOL_VERSION,
    }
}

/// The reply to `tools/list`.
#[must_use]
pub fn tools_list() -> Value {
    json!({
        "tools": Tool::ALL.iter().map(|tool| tool.definition()).collect::<Vec<Value>>(),
    })
}

/// The reply to `resources/list`: one resource per machine.
///
/// One, because a machine is the whole of what this server exposes. A
/// fictional tree of resources (`vitals://host/processes/1234`) would promise
/// a browseable surface that does not exist, and an agent that walks it gets
/// a not-found error it cannot act on.
#[must_use]
pub fn resources_list(host: &dyn McpHost) -> Value {
    let uri = resource_uri(host.machine());
    json!({
        "resources": [{
            "uri": uri,
            "name": host.machine(),
            "description": "What this agent can read from this machine, and how to read it.",
            "mimeType": "application/json",
        }],
    })
}

/// The resource URI for a machine name.
///
/// Lower-cased, with everything a URI host does not allow replaced by `-`: a
/// Windows computer name may contain underscores and spaces, and a client
/// library that parses the URI would reject those. Lower case because a URI
/// host is case-insensitive and a client is entitled to normalise it — which
/// would stop [`resource_contents`] recognising the URI it handed out.
#[must_use]
pub fn resource_uri(machine: &str) -> String {
    let mut out = String::with_capacity(machine.len() + 9);
    out.push_str("vitals://");
    let mut wrote_any = false;
    for character in machine.chars() {
        if character.is_ascii_alphanumeric() || character == '-' || character == '.' {
            out.push(character.to_ascii_lowercase());
            wrote_any = true;
        } else if wrote_any && !out.ends_with('-') {
            out.push('-');
        }
    }
    if !wrote_any {
        // An empty host name would produce the bare URI `vitals://`, which a
        // client library reads as a syntax error rather than a machine.
        out.push_str("machine");
    }
    out
}

/// The reply to `resources/read`, or the reason the URI is not ours.
///
/// # Errors
/// [`Error::RESOURCE_NOT_FOUND`] for any URI other than this machine's, with
/// the URI that does exist named in the message.
pub fn resource_contents(host: &dyn McpHost, uri: &str) -> Result<Value, Error> {
    let ours = resource_uri(host.machine());
    if !uri.eq_ignore_ascii_case(&ours) {
        return Err(Error::resource_not_found(uri, &ours));
    }
    let tools: Vec<&str> = Tool::ALL.iter().map(|tool| tool.name()).collect();
    let document = json!({
        "machine": host.machine(),
        "host": host.host_info(),
        "readOnly": true,
        "tools": tools,
        "instructions": instructions(host),
    });
    Ok(json!({
        "contents": [{
            "uri": ours,
            "mimeType": "application/json",
            "text": document.to_string(),
        }],
    }))
}

/// A tool's data, as a `CallToolResult`.
///
/// The payload goes in twice on purpose: `structuredContent` is what a client
/// can parse, and the text is what a model reads in a transcript when the
/// client does not support structured output at all. Sending only one of them
/// means some client somewhere sees an empty result.
#[must_use]
pub fn tool_result(payload: &Value) -> Value {
    json!({
        "content": [{ "type": "text", "text": payload.to_string() }],
        "structuredContent": payload,
        "isError": false,
    })
}

/// A tool that ran and could not answer.
///
/// A normal result with `isError`, not a JSON-RPC error: the call was
/// well-formed, the answer is "no, and here is why", and burying that in a
/// transport-level failure loses the sentence the agent needs.
#[must_use]
pub fn tool_failure(reason: &str) -> Value {
    json!({
        "content": [{ "type": "text", "text": reason }],
        "isError": true,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::test_host::MACHINE;
    use crate::test_host::TestHost;

    #[test]
    fn initialize_announces_both_capabilities_and_says_the_server_is_read_only() {
        let reply = initialize(&TestHost::new(), Some(PROTOCOL_VERSION));
        assert_eq!(reply["protocolVersion"], PROTOCOL_VERSION);
        assert_eq!(reply["serverInfo"]["name"], SERVER_NAME);
        assert_eq!(reply["serverInfo"]["version"], server_version());
        assert_eq!(reply["capabilities"]["tools"]["listChanged"], false);
        let instructions = reply["instructions"].as_str().unwrap();
        assert!(
            instructions.contains("Read-only"),
            "an agent must be told: {instructions}"
        );
        assert!(instructions.contains(MACHINE));
    }

    #[test]
    fn a_client_asking_for_an_unknown_revision_is_answered_with_ours() {
        // The specification's rule, and the only answer that lets a newer
        // client decide for itself whether it can continue.
        assert_eq!(negotiated_version(Some("1999-01-01")), PROTOCOL_VERSION);
        assert_eq!(negotiated_version(None), PROTOCOL_VERSION);
        assert_eq!(negotiated_version(Some(PROTOCOL_VERSION)), PROTOCOL_VERSION);
    }

    #[test]
    fn tools_list_names_every_tool_once_and_in_the_order_the_enum_declares() {
        let reply = tools_list();
        let names: Vec<&str> = reply["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap())
            .collect();
        assert_eq!(
            names,
            vec![
                "summary",
                "processes",
                "sensors",
                "alerts",
                "history",
                "diagnosis"
            ]
        );
    }

    #[test]
    fn the_machine_is_offered_as_exactly_one_resource_at_a_valid_uri() {
        let reply = resources_list(&TestHost::new());
        let resources = reply["resources"].as_array().unwrap();
        assert_eq!(resources.len(), 1);
        let uri = resources[0]["uri"].as_str().unwrap();
        assert_eq!(uri, "vitals://test-machine");
        assert!(uri.starts_with("vitals://"));
        assert_eq!(resources[0]["mimeType"], "application/json");
    }

    #[test]
    fn a_machine_name_that_is_not_a_legal_uri_host_is_made_into_one() {
        // A Windows computer name can contain an underscore, and a client
        // library that parses the URI rejects one outright.
        assert_eq!(resource_uri("DESKTOP_ABC"), "vitals://desktop-abc");
        assert_eq!(resource_uri("My Laptop"), "vitals://my-laptop");
        assert_eq!(resource_uri("PC-01.local"), "vitals://pc-01.local");
        assert_eq!(resource_uri(""), "vitals://machine");
        assert_eq!(resource_uri("__"), "vitals://machine", "nothing survived");
        assert!(
            !resource_uri("héllo").contains('é'),
            "non-ASCII must not reach a URI"
        );
    }

    #[test]
    fn reading_our_own_resource_returns_a_document_that_lists_the_tools() {
        let host = TestHost::new();
        let uri = resource_uri(host.machine());
        let reply = resource_contents(&host, &uri).unwrap();
        let contents = reply["contents"].as_array().unwrap();
        assert_eq!(contents.len(), 1);
        assert_eq!(contents[0]["mimeType"], "application/json");
        let document: Value = serde_json::from_str(contents[0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(document["machine"], MACHINE);
        assert_eq!(document["readOnly"], true);
        assert_eq!(document["tools"].as_array().unwrap().len(), Tool::ALL.len());
        assert!(document["host"]["hostname"].is_string());
    }

    #[test]
    fn reading_a_resource_we_do_not_serve_names_the_one_we_do() {
        let error = resource_contents(&TestHost::new(), "vitals://some-other-pc").unwrap_err();
        assert_eq!(error.code, crate::jsonrpc::RESOURCE_NOT_FOUND);
        assert!(
            error.message.contains("vitals://test-machine"),
            "the message must name the URI that exists: {}",
            error.message
        );
    }

    #[test]
    fn a_uri_is_matched_without_regard_to_case_because_a_client_may_normalise_it() {
        let host = TestHost::new();
        let upper = resource_uri(host.machine()).to_ascii_uppercase();
        assert!(resource_contents(&host, &upper).is_ok());
    }

    #[test]
    fn a_tool_result_carries_the_payload_as_both_text_and_structured_content() {
        let payload = json!({ "cpu": 12.5 });
        let result = tool_result(&payload);
        assert_eq!(result["isError"], false);
        assert_eq!(result["structuredContent"], payload);
        assert_eq!(result["content"][0]["type"], "text");
        let text = result["content"][0]["text"].as_str().unwrap();
        assert_eq!(serde_json::from_str::<Value>(text).unwrap(), payload);
    }

    #[test]
    fn a_tool_failure_is_a_result_with_is_error_rather_than_a_transport_failure() {
        let result = tool_failure("no sensor service is running");
        assert_eq!(result["isError"], true);
        assert_eq!(result["content"][0]["text"], "no sensor service is running");
        assert!(result.get("structuredContent").is_none());
    }
}
