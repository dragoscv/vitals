---
title: Architecture
description: How Vitals is put together — one Rust sampler feeding the window, the history store, the LAN API and the CLI.
---

Vitals is a Tauri 2 application: a Rust backend and a React 19 interface rendered in WebView2. The
principle behind every part of it is that **one sampler produces one stream of frames**, and every
consumer — the window, the tray, the HUD, the phone, Prometheus, the CLI — reads that same stream,
so they can never disagree.

## The picture

```text
                        +---------------------------+
                        |   vitals-win (sampler)    |
                        |  behind vitals-core traits|
                        +-------------+-------------+
                                      |
                        frames: keyframe + deltas
                                      |
        +---------------+-------------+-------------+----------------+
        |               |                           |                |
        v               v                           v                v
+---------------+ +--------------+       +--------------------+ +--------------+
| React webview | | vitals-store |       |   vitals-server    | |  vitals-ipc  |
| (Tauri events)| | SQLite       |       |   axum HTTP        | |  named pipe  |
| window, tray, | | history +    |       | loopback :7330     | |  \\.\pipe\   |
| HUD           | | flight       |       | LAN :7331 (opt-in) | |  vitals-user |
+---------------+ | recorder     |       +---------+----------+ +------+-------+
                  +--------------+                 |                   |
                                   REST / SSE / WebSocket / metrics    |
                                                   |                   v
                                   phone, Prometheus, Home       vitals CLI
                                   Assistant, @vitals/client     (top, ps, ...)
```

## The parts

**`vitals-core`** defines the data model and the traits a platform backend implements. Every
reading that might not be available is an optional value, so "not measured" and "zero" are
different facts all the way to the screen. It builds on Linux in CI to prove no Windows detail has
leaked into it.

**`vitals-win`** is the Windows backend. A dedicated sampler thread reads Windows' own counters
and produces a frame per tick. A budget test fails the build if a sample costs too much. macOS and
Linux backends will implement the same traits.

**Frames** are a complete **keyframe** followed by **deltas** that carry only what changed. A new
consumer always receives a keyframe first, so it starts from the full picture regardless of when
it joins.

**The webview** receives frames as Tauri events and renders them with React. Commands the other
way — ending a process, changing a setting — go through typed Tauri commands.

**`vitals-store`** keeps history in SQLite with retention, and a **flight recorder**: a rolling
minute of everything, captured on request for a bug report.

**`vitals-server`** is an axum HTTP server that serves REST, Server-Sent Events, WebSocket and
Prometheus. It always listens on `127.0.0.1` for local scripts, and on the network only when remote
access is turned on. Authorisation is checked before a request body is parsed.

**`vitals-ipc`** is a per-user named pipe the CLI uses to attach to a running app and read its
frames, so one machine runs one sampler.

**`packages/protocol`** holds TypeScript types generated from the Rust model with ts-rs. The
interface, the phone page and the SDK all import them, so a change on the Rust side that breaks a
client fails to compile rather than failing at run time.

**`@vitals/client`** is the TypeScript SDK over the same API and the same generated types.

## Read further

- [`docs/architecture.md`](https://github.com/dragoscv/vitals/blob/main/docs/architecture.md) in the
  repository, the full version.
- [Architecture decision records](https://github.com/dragoscv/vitals/tree/main/docs/adr) for why
  each choice was made.
