//! The JSON-RPC 2.0 envelope every MCP message travels in.
//!
//! Small enough to own rather than depend on: five error codes, one request
//! shape, and the rule that a request with no `id` is a **notification** that
//! must be acted on and never answered. A JSON-RPC framework would bring a
//! transport model — sessions, batch arrays, streaming — that this server
//! deliberately does not have, because it holds no session state at all.

use serde::Deserialize;
use serde_json::{Value, json};

/// Required on every message this server accepts.
///
/// Checked, not assumed: a client speaking 1.0 has an error-handling model
/// this server has never read, and answering it with 2.0 shapes would be
/// guessing on both sides at once.
pub const VERSION: &str = "2.0";

/// The body was not JSON at all.
pub const PARSE_ERROR: i32 = -32700;
/// JSON, but not a JSON-RPC 2.0 request.
pub const INVALID_REQUEST: i32 = -32600;
/// A well-formed request for a method this server does not implement.
pub const METHOD_NOT_FOUND: i32 = -32601;
/// The right method with the wrong arguments.
pub const INVALID_PARAMS: i32 = -32602;
/// A bug on this side. The client cannot fix it and should not retry.
pub const INTERNAL_ERROR: i32 = -32603;
/// MCP's own code (`-32002`) for a resource URI that does not exist.
pub const RESOURCE_NOT_FOUND: i32 = -32002;

/// One incoming message.
///
/// Unknown members are ignored, which is what lets a client send `_meta` (and
/// whatever the next revision adds) without this server having to know about
/// it.
#[derive(Debug, Clone, Deserialize)]
pub struct Request {
    pub jsonrpc: String,
    /// Absent on a notification. `"id": null` is treated the same way: MCP
    /// clients either omit the key or send something non-null, and a `null`
    /// id that was answered would produce a response nobody correlates.
    #[serde(default)]
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Option<Value>,
}

impl Request {
    /// Whether this message must not be answered.
    #[must_use]
    pub fn is_notification(&self) -> bool {
        self.id.as_ref().is_none_or(Value::is_null)
    }

    /// The `id` to put on the reply, or JSON `null` when there is none.
    ///
    /// A parse failure has no request to take an id from, and the spec says
    /// the reply carries `null` rather than being dropped: a client waiting
    /// on a request deserves to learn that its own syntax was the problem.
    #[must_use]
    pub fn reply_id(&self) -> Value {
        match &self.id {
            None | Some(Value::Null) => Value::Null,
            Some(id) => id.clone(),
        }
    }
}

/// One JSON-RPC error, as it goes on the wire.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Error {
    pub code: i32,
    pub message: String,
}

impl Error {
    #[must_use]
    pub fn parse(message: impl Into<String>) -> Self {
        Self {
            code: PARSE_ERROR,
            message: message.into(),
        }
    }

    /// The body was JSON, but not a request the spec would recognise.
    #[must_use]
    pub fn invalid_request(message: impl Into<String>) -> Self {
        Self {
            code: INVALID_REQUEST,
            message: message.into(),
        }
    }

    #[must_use]
    pub fn method_not_found(method: &str) -> Self {
        Self {
            code: METHOD_NOT_FOUND,
            message: format!("this server has no method `{method}`"),
        }
    }

    #[must_use]
    pub fn invalid_params(message: impl Into<String>) -> Self {
        Self {
            code: INVALID_PARAMS,
            message: message.into(),
        }
    }

    /// A resource URI nothing here serves. `known` is named in the message:
    /// an agent that guessed the URI is told the one that exists.
    #[must_use]
    pub fn resource_not_found(uri: &str, known: &str) -> Self {
        Self {
            code: RESOURCE_NOT_FOUND,
            message: format!("no resource `{uri}` here; this machine is `{known}`"),
        }
    }
}

/// A successful reply.
#[must_use]
pub fn success(id: &Value, result: &Value) -> Value {
    json!({ "jsonrpc": VERSION, "id": id, "result": result })
}

/// A failed reply.
#[must_use]
pub fn failure(id: &Value, error: &Error) -> Value {
    json!({
        "jsonrpc": VERSION,
        "id": id,
        "error": { "code": error.code, "message": error.message },
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn a_request_without_an_id_is_a_notification_and_must_not_be_answered() {
        let request: Request =
            serde_json::from_str(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#)
                .expect("a notification parses");
        assert!(request.is_notification());
        assert_eq!(request.reply_id(), Value::Null);
    }

    #[test]
    fn a_null_id_is_a_notification_too_because_no_client_correlates_the_reply() {
        let request: Request = serde_json::from_str(
            r#"{"jsonrpc":"2.0","id":null,"method":"notifications/initialized"}"#,
        )
        .expect("a null id parses");
        assert!(request.is_notification());
    }

    #[test]
    fn a_string_or_number_id_survives_the_round_trip_unchanged() {
        // The id is opaque to this server, and a client that sent `"a"` will
        // not correlate a reply numbered `"a"` back as 1.
        for id in ["1", "\"abc\"", "12345", "\"0\""] {
            let text = format!(r#"{{"jsonrpc":"2.0","id":{id},"method":"ping"}}"#);
            let request: Request = serde_json::from_str(&text).expect("parses");
            assert!(!request.is_notification());
            assert_eq!(
                request.reply_id(),
                serde_json::from_str::<Value>(id).unwrap()
            );
        }
    }

    #[test]
    fn the_error_codes_are_the_numbers_the_specification_names() {
        // A typo here is silent: the client reads a code it does not know and
        // reports a generic failure, and nobody ever sees why.
        assert_eq!(PARSE_ERROR, -32700);
        assert_eq!(INVALID_REQUEST, -32600);
        assert_eq!(METHOD_NOT_FOUND, -32601);
        assert_eq!(INVALID_PARAMS, -32602);
        assert_eq!(INTERNAL_ERROR, -32603);
        assert_eq!(RESOURCE_NOT_FOUND, -32002, "MCP's own not-found code");
    }

    #[test]
    fn a_failure_carries_the_id_it_was_given_and_the_code_it_meant() {
        let reply = failure(&json!(7), &Error::method_not_found("nope"));
        assert_eq!(reply["jsonrpc"], "2.0");
        assert_eq!(reply["id"], 7);
        assert_eq!(reply["error"]["code"], METHOD_NOT_FOUND);
        assert!(
            reply["error"]["message"].as_str().unwrap().contains("nope"),
            "{reply}"
        );
        assert!(reply.get("result").is_none(), "a failure has no result");
    }

    #[test]
    fn a_success_carries_no_error_member_at_all() {
        let reply = success(&json!("a"), &json!({ "ok": true }));
        assert_eq!(reply["id"], "a");
        assert_eq!(reply["result"]["ok"], true);
        assert!(reply.get("error").is_none());
    }
}
