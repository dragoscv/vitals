# 0043 — The AI page shows what a session cost, and leaves a gap where it does not know

- **Status:** accepted
- **Date:** 2026-11-17
- **Tracker:** S25-03

## Context

The AI page’s session card has shown “Tokens in / out” since the page was
built (ADR 0039), but only Claude Code sessions ever filled those two fields:
the Claude parser reads `message.usage`. Copilot CLI and VS Code Copilot Chat
sessions read as an em dash, which the owner read as “the numbers are missing”.

Copilot writes one `session.usage_record` event per model request into
`~/.copilot/session-state/<id>/events.jsonl`. Measured on this machine on
2026-11-17, the `usage` object carries, per request: `inputTokens`,
`outputTokens`, `cacheReadTokens`, `cacheWriteTokens`, `reasoningTokens`,
`cost`, `duration` (ms), `timeToFirstTokenMs`, `interTokenLatencyMs`, `model`
and `numToolCalls`. VS Code’s Copilot Chat transcripts carry **no** usage
event of any kind — those sessions genuinely have no token data to show.

## Decision

Parse `session.usage_record`, add the fields to `AgentSession`, and show them —
per session and as a page total — while keeping the crate’s rule that an
unmeasured reading is `None`, never `0`.

New `AgentSession` fields, each `Option`:

| Field             | Meaning                                              |
| ----------------- | ---------------------------------------------------- |
| `tokensCached`    | Prompt tokens served from the provider cache         |
| `tokensReasoning` | Reasoning tokens the model did not return as text    |
| `costUsd`         | What the source priced the session at, in US dollars |
| `requests`        | Model requests the source recorded                   |
| `outputMs`        | Sum of the requests’ durations, milliseconds         |
| `ttftMs`          | Average time to the first token, milliseconds        |
| `tokensPerSec`    | Output tokens per second over `outputMs`             |

`tokensPerSec` is computed **in Rust**, once, so the desktop, the phone and the
watch print the same number instead of each inventing a formula. Its definition
is output tokens over the summed request durations; it is `None` unless there
is both an output count and a strictly positive duration, so a missing
denominator gives a gap and a zero-length window never gives infinity. The page
total recomputes the rate from the summed output and duration rather than
averaging per-session rates, so one long burst does not weigh the same as one
short request.

`costUsd` distinguishes a real zero (a flat-rate plan that reports no
per-request price) from an absent price (`None`), exactly as the token counts
already distinguished a measured zero from an unknown.

## Consequences

- A Copilot CLI session shows input, output, cached and reasoning tokens, the
  request count, the output rate, time to first token and cost. A VS Code
  Copilot Chat session shows a short line saying the client does not record
  usage, rather than a wall of dashes.
- The Insights tab gains a token-usage summary: sessions that reported, total
  requests, total input and output tokens, the overall output rate and the
  total cost.
- The LAN API, the SDK and the phone carry the new fields for free because they
  are generated from one definition (ADR 0027); the phone’s agents card is
  unchanged for now and may show them later.
- Two tests pin the behaviour: one asserts the sums and the rate from two
  synthetic requests, the other asserts that a rate needs a measured duration
  and that an unknown input stays unknown.
