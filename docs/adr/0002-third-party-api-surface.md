# 0002 — Third-party API: REST + SSE + WebSocket + Prometheus + loopback + SDK

- Status: Accepted
- Date: 2026-09-10 (loopback replaced the named pipe the same day)
- Tracker: D2

## Context

Scripts, dashboards, Home Assistant and the CLI all want the same readings.
Each has a preferred transport: a poll, a push stream, a scrape, a local
attach. Offering one transport would make every other consumer write glue.

## Decision

One server, several views of the same frame:

- `GET /api/v1/snapshot` — the current materialised keyframe.
- `GET /api/v1/stream` — SSE, one frame per tick.
- `GET /api/v1/ws` — WebSocket, same payload.
- `GET /metrics` — Prometheus exposition, hand-written rather than via a
  registry crate: there is one snapshot and no mutable registry to keep in
  step.
- `GET /api/v1/host`, `GET /api/v1/alerts`, `POST /api/v1/control`.
- A **loopback** listener on `127.0.0.1:7330` so the CLI can attach to the
  running app without a token. This replaced the named-pipe plan (S4-11): one
  wire format for phone, CLI and scripts, and nothing new in `vitals-ipc`.
- `@vitals/client`, a typed TypeScript SDK over REST, SSE and WebSocket,
  built on the generated protocol types.
- `docs/api/openapi.yaml`, with a drift test over its route list.

## Consequences

- Every consumer sees identical numbers; a fix in the sampler reaches all of
  them.
- A client joining mid-stream must receive a materialised keyframe, not the
  newest delta — `FrameSource` applies deltas onto the keyframe, exits before
  changes, exactly as `metrics.ts` does.
- The OpenAPI document's prose has no drift test; changing a status code or
  an ordering means grepping the docs.
