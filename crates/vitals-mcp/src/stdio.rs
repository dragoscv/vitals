//! The transport an agent launches: one JSON message per line, over the
//! process's own standard input and output.
//!
//! Which stream matters. **Standard output carries the protocol and nothing
//! else** — an MCP client parses every line it reads there, so a stray
//! greeting, a progress note or a `tracing` line would be read as a malformed
//! message. Diagnostics belong on standard error, and the CLI writes its
//! "sampling directly" note there for exactly this reason.
//!
//! The framing is a newline after each message, with no `Content-Length`
//! header: that is what MCP's `stdio` transport specifies, and it is not the
//! same as LSP's.

use std::io::{BufRead, Write};

use crate::host::McpHost;
use crate::server::Server;

/// Serves this process's standard input and output until either ends.
///
/// # Errors
/// A read or write failure on the process's own streams. A closed pipe is not
/// one: an agent that quits closes standard input, which ends the loop
/// normally.
pub fn serve<H: McpHost>(host: H) -> std::io::Result<()> {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let server = Server::new(host);
    serve_streams(&server, stdin.lock(), stdout.lock())
}

/// The loop, over any pair of streams.
///
/// Split out so a test can drive a whole session through in-memory pipes: a
/// transport that has only ever been exercised through the real `stdin` is a
/// transport whose framing and flushing nobody has checked.
///
/// # Errors
/// A read or write failure on either stream.
pub fn serve_streams<H: McpHost, R: BufRead, W: Write>(
    server: &Server<H>,
    mut input: R,
    mut output: W,
) -> std::io::Result<()> {
    let mut line = String::new();
    loop {
        line.clear();
        // A message longer than the line buffer is still one message: the
        // reader grows, and `read_line` returns only at a newline or at EOF.
        if input.read_line(&mut line)? == 0 {
            return Ok(());
        }
        let Some(reply) = server.handle(&line) else {
            continue;
        };
        writeln!(output, "{reply}")?;
        // Flushed per message: an agent is waiting on this one, and a
        // buffered reply looks exactly like a server that has hung.
        output.flush()?;
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::test_host::TestHost;

    fn session(messages: &str) -> Vec<serde_json::Value> {
        let mut output = Vec::new();
        serve_streams(
            &Server::new(TestHost::new()),
            std::io::Cursor::new(messages),
            &mut output,
        )
        .expect("an in-memory session cannot fail");
        let text = String::from_utf8(output).expect("the transport writes UTF-8");
        text.lines()
            .map(|line| serde_json::from_str(line).expect("every line is one JSON message"))
            .collect()
    }

    #[test]
    fn a_whole_session_over_streams_answers_each_request_on_its_own_line() {
        let replies = session(concat!(
            "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\"}\n",
            "{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n",
            "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/list\"}\n",
            "{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"tools/call\",",
            "\"params\":{\"name\":\"summary\"}}\n",
        ));
        // Three requests, three replies: the notification produced none.
        assert_eq!(replies.len(), 3);
        assert_eq!(replies[0]["id"], 1);
        assert_eq!(replies[1]["id"], 2);
        assert_eq!(replies[2]["id"], 3);
        assert_eq!(replies[1]["result"]["tools"].as_array().unwrap().len(), 6);
        assert_eq!(replies[2]["result"]["isError"], false);
    }

    #[test]
    fn a_blank_line_between_messages_is_ignored_rather_than_answered() {
        let replies = session("\n\n{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\n\n");
        assert_eq!(replies.len(), 1);
        assert_eq!(replies[0]["id"], 1);
    }

    #[test]
    fn an_unparseable_line_is_answered_and_the_session_carries_on() {
        // A client that sends one bad message must not lose the connection:
        // the next request still gets its answer.
        let replies = session("garbage\n{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"ping\"}\n");
        assert_eq!(replies.len(), 2);
        assert_eq!(replies[0]["error"]["code"], crate::jsonrpc::PARSE_ERROR);
        assert_eq!(replies[1]["id"], 2);
    }

    #[test]
    fn a_message_with_no_trailing_newline_at_end_of_input_is_still_answered() {
        // The last line of a piped script has no newline more often than one
        // would think, and dropping it would look like a hang.
        let replies = session("{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}");
        assert_eq!(replies.len(), 1);
        assert_eq!(replies[0]["id"], 1);
    }

    #[test]
    fn nothing_at_all_on_the_input_ends_the_server_quietly() {
        let mut output = Vec::new();
        serve_streams(
            &Server::new(TestHost::new()),
            std::io::Cursor::new(""),
            &mut output,
        )
        .expect("immediate EOF is not an error");
        assert!(output.is_empty(), "nothing was read, so nothing is written");
    }
}
