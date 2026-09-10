//! A minimal server-sent-events reader over a blocking byte stream.
//!
//! The server (`axum::response::sse`) emits `data: <json>` lines, a blank
//! line between events, and `: keep-alive` comments every 15 s. That is all
//! this needs to understand; `event:`/`id:`/`retry:` are accepted and
//! ignored so a future server that sets them does not break an old CLI.

use std::io::{BufRead, BufReader, Read};

/// Yields the `data` of each complete event, joined with `\n` when the
/// server split it over several `data:` lines (the spec allows that, and
/// `Event::json_data` will do it for a payload containing a newline).
#[derive(Debug)]
pub struct Events<R: Read> {
    lines: std::io::Lines<BufReader<R>>,
}

impl<R: Read> Events<R> {
    pub fn new(reader: R) -> Self {
        Self {
            lines: BufReader::new(reader).lines(),
        }
    }
}

impl<R: Read> Iterator for Events<R> {
    type Item = std::io::Result<String>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut data: Vec<String> = Vec::new();
        loop {
            let line = match self.lines.next() {
                Some(Ok(line)) => line,
                Some(Err(error)) => return Some(Err(error)),
                // Stream closed. A partially-buffered event with no blank
                // terminator is discarded: the server never got to finish it.
                None => return None,
            };
            let line = line.strip_suffix('\r').unwrap_or(&line);

            if line.is_empty() {
                if data.is_empty() {
                    // A blank line with nothing buffered — typically the one
                    // that follows a keep-alive comment. Not an event.
                    continue;
                }
                return Some(Ok(data.join("\n")));
            }
            if line.starts_with(':') {
                // Comment. The server uses these as keep-alives.
                continue;
            }
            let (field, value) = line.split_once(':').unwrap_or((line, ""));
            // The spec strips exactly one leading space from the value.
            let value = value.strip_prefix(' ').unwrap_or(value);
            if field == "data" {
                data.push(value.to_owned());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collect(input: &str) -> Vec<String> {
        Events::new(input.as_bytes())
            .map(|r| r.expect("in-memory read cannot fail"))
            .collect()
    }

    #[test]
    fn each_blank_line_terminates_one_event() {
        let events = collect("data: {\"a\":1}\n\ndata: {\"a\":2}\n\n");
        assert_eq!(events, vec!["{\"a\":1}", "{\"a\":2}"]);
    }

    #[test]
    fn keep_alive_comments_are_not_events() {
        let events = collect(": keep-alive\n\ndata: x\n\n: keep-alive\n\n");
        assert_eq!(events, vec!["x"]);
    }

    #[test]
    fn multi_line_data_is_joined_with_newlines() {
        let events = collect("data: first\ndata: second\n\n");
        assert_eq!(events, vec!["first\nsecond"]);
    }

    #[test]
    fn crlf_line_endings_and_other_fields_are_tolerated() {
        let events = collect("event: frame\r\nid: 7\r\ndata: payload\r\n\r\n");
        assert_eq!(events, vec!["payload"]);
    }

    #[test]
    fn an_event_cut_off_by_the_stream_closing_is_not_delivered() {
        // Delivering half a JSON document would fail to parse downstream
        // and look like a server bug; dropping it is the honest outcome.
        assert!(collect("data: {\"half\":").is_empty());
    }
}
