# 0028 — axum for the LAN server

- Status: Accepted
- Date: 2026-09-10
- Tracker: S10-02

## Context

The LAN server (ADR 0002) needs HTTP/1.1, JSON, SSE, WebSocket and a static
file route, inside a process that already runs Tauri's multi-thread tokio
runtime. Candidates: axum, actix-web, `tiny_http`, warp.

## Decision

`axum` 0.8 with default features off and only `http1`, `json`, `tokio`,
`ws` and `query` on, plus `tower-http` for `set-header`.

axum runs on the tokio runtime Tauri already owns, so it adds no second
executor. actix-web brings its own runtime. `tiny_http` cannot do
WebSockets at all. warp is axum's predecessor in the same ecosystem with a
smaller community.

## Consequences

- Extractors run before handlers, so `Json<T>` as an argument would
  validate before authorising; `control` takes `Bytes` and parses by hand
  (see `AGENTS.md`, "Authorisation before validation").
- The dependency graph grows by hyper/tower, which the Tauri updater plugin
  already pulled in transitively.
- The same router serves the desktop-hosted server, `vitals serve` and the
  `serve_dev` example, so there is one route table to test.
