//! The real `vitals mcp`, spoken to over real pipes.
//!
//! The crate's unit tests drive the protocol through in-memory streams and a
//! fixture machine. None of them run the binary, so a panic at startup, a
//! greeting printed to standard output or a tool wired to the wrong reading
//! would pass every one of them. These spawn the real executable and perform
//! the handshake an agent performs.
//!
//! Windows-only for the same reason `ps --json` is: the CLI's direct sampler
//! exists nowhere else, so `vitals mcp` cannot start on a machine with no
//! desktop app to attach to.

#![cfg(windows)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fmt::Write as _;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};

use serde_json::Value;

/// Runs one session: sends `messages`, closes standard input, and returns the
/// replies in order.
///
/// Closing standard input is what ends the session — that is the transport's
/// own rule, so a server that only stopped on a signal would hang here rather
/// than in someone's agent.
fn session(messages: &str) -> Vec<Value> {
    let mut child: Child = Command::new(env!("CARGO_BIN_EXE_vitals"))
        .arg("mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        // Not inherited: the CLI writes its "source: …" note to standard
        // error, and a test should not depend on a console being there.
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn vitals mcp");

    child
        .stdin
        .as_mut()
        .expect("a pipe was asked for")
        .write_all(messages.as_bytes())
        .expect("write the session");
    drop(child.stdin.take());

    let stdout = child.stdout.take().expect("a pipe was asked for");
    let replies: Vec<Value> = BufReader::new(stdout)
        .lines()
        .map(|line| {
            let line = line.expect("read a line");
            serde_json::from_str(&line).unwrap_or_else(|error| {
                panic!("stdout carried something that is not one MCP message ({error}): {line}")
            })
        })
        .collect();

    let status = child.wait().expect("wait for the server");
    assert!(status.success(), "vitals mcp exited with {status:?}");
    replies
}

#[test]
fn the_real_binary_completes_the_handshake_and_answers_a_summary_call() {
    let replies = session(concat!(
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","clientInfo":{"name":"vitals-test"}}}"#,
        "\n",
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        "\n",
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
        "\n",
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"summary","arguments":{"top":3}}}"#,
        "\n",
    ));

    // Three requests, three replies: the notification is answered with
    // nothing, which is the rule an agent's client relies on.
    assert_eq!(replies.len(), 3, "{replies:#?}");
    assert_eq!(replies[0]["id"], 1);
    assert_eq!(replies[0]["result"]["serverInfo"]["name"], "vitals");
    assert!(replies[0]["result"]["instructions"].is_string());

    assert_eq!(replies[1]["id"], 2);
    assert_eq!(
        replies[1]["result"]["tools"]
            .as_array()
            .expect("an array")
            .len(),
        6
    );

    assert_eq!(replies[2]["id"], 3);
    assert_eq!(
        replies[2]["result"]["isError"], false,
        "a running machine must answer a summary: {:#?}",
        replies[2]
    );
    let payload = &replies[2]["result"]["structuredContent"];
    // The test's own process is running, so a real read of a real machine
    // cannot report none of them — a zero here means the numbers came from
    // nowhere.
    assert!(
        payload["processCount"].as_u64().unwrap_or(0) >= 1,
        "a machine that is running this test has processes: {payload}"
    );
    assert!(payload["system"]["cpu"].is_object(), "{payload}");
}

#[test]
fn all_six_tools_answer_through_the_real_binary() {
    // The crate's unit tests drive every tool against a fixture host, where
    // the host methods exist by construction. This is the only place that
    // proves each one is wired to what the CLI can really read: a tool
    // reaching for something the CLI does not implement comes back as a
    // JSON-RPC error here and nowhere else.
    let tools = [
        "summary",
        "processes",
        "sensors",
        "alerts",
        "history",
        "diagnosis",
    ];
    let mut session_text = String::new();
    for (index, tool) in tools.iter().enumerate() {
        let id = index + 1;
        let arguments = if *tool == "history" {
            r#","arguments":{"seconds":3600}"#
        } else {
            ""
        };
        let _ = write!(
            session_text,
            r#"{{"jsonrpc":"2.0","id":{id},"method":"tools/call","params":{{"name":"{tool}"{arguments}}}}}"#
        );
        session_text.push('\n');
    }

    let replies = session(&session_text);
    assert_eq!(replies.len(), tools.len(), "{replies:#?}");

    for (reply, tool) in replies.iter().zip(tools) {
        assert!(
            reply.get("error").is_none(),
            "`{tool}` did not run: {reply}"
        );
        let text = reply["result"]["content"][0]["text"]
            .as_str()
            .expect("every tool answers with text");
        assert!(!text.trim().is_empty(), "`{tool}` answered nothing");

        // Three of the six cannot fail on a machine that is running this
        // test: it has processes, a summary is a subset of them, and a
        // diagnosis whose verdict is null is still a diagnosis.
        if matches!(tool, "summary" | "processes" | "diagnosis") {
            assert_eq!(
                reply["result"]["isError"], false,
                "`{tool}` could not read a machine that is running: {text}"
            );
        }

        // The other three may legitimately answer with a sentence instead:
        // the CLI has no sensor backend of its own and no store, and a
        // sentence saying so is the designed answer rather than a failure.
        if reply["result"]["isError"] == Value::Bool(true) {
            assert!(
                text.len() > 20,
                "`{tool}` failed without saying why: {text}"
            );
            continue;
        }

        let payload: Value = serde_json::from_str(text).unwrap_or_else(|error| {
            panic!("`{tool}` answered text that is not JSON ({error}): {text}")
        });
        match tool {
            "summary" => {
                assert!(
                    payload["processCount"].as_u64().unwrap_or(0) >= 1,
                    "{payload}"
                );
                assert!(payload["system"]["cpu"].is_object(), "{payload}");
            }
            "processes" => {
                assert!(payload["processes"].is_array(), "{payload}");
                assert!(payload["count"].as_u64().unwrap_or(0) >= 1, "{payload}");
            }
            "diagnosis" => {
                assert!(payload.get("verdict").is_some(), "{payload}");
                assert!(payload.get("also").is_some(), "{payload}");
            }
            _ => assert!(payload.is_object(), "{payload}"),
        }
    }
}

#[test]
fn the_real_binary_has_no_tool_that_acts_on_the_machine() {
    let replies = session(concat!(
        r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"end_process","arguments":{"pid":1}}}"#,
        "\n",
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"set_priority"}}"#,
        "\n",
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"kill"}}"#,
        "\n",
    ));
    assert_eq!(replies.len(), 3);
    for reply in &replies {
        assert_eq!(
            reply["error"]["code"], -32602,
            "a control verb must not be a tool: {reply}"
        );
    }
    let message = replies[0]["error"]["message"].as_str().expect("a message");
    assert!(message.contains("end_process"), "{message}");
    assert!(message.contains("diagnosis"), "{message}");
}

#[test]
fn the_resource_the_real_binary_advertises_can_be_read_back() {
    let listed = session(concat!(
        r#"{"jsonrpc":"2.0","id":1,"method":"resources/list"}"#,
        "\n",
    ));
    let uri = listed[0]["result"]["resources"][0]["uri"]
        .as_str()
        .expect("a URI")
        .to_owned();
    assert!(uri.starts_with("vitals://"), "{uri}");

    let read = session(&format!(
        "{}\n",
        serde_json::json!({
            "jsonrpc": "2.0", "id": 2, "method": "resources/read",
            "params": { "uri": uri },
        })
    ));
    let text = read[0]["result"]["contents"][0]["text"]
        .as_str()
        .expect("a document");
    let document: Value = serde_json::from_str(text).expect("the document is JSON");
    assert_eq!(document["readOnly"], true);
    assert_eq!(document["tools"].as_array().expect("tools").len(), 6);
    // The document names the machine as the platform spells it ("DRAGOS"),
    // while the URI is lower-cased because a URI host is case-insensitive.
    let named = document["machine"].as_str().expect("a name");
    assert!(
        named.eq_ignore_ascii_case(uri.trim_start_matches("vitals://")),
        "{named} and {uri} name different machines"
    );
}
