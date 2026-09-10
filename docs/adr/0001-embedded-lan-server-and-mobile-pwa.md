# 0001 — Phone access via an embedded LAN HTTP server and a lean PWA

- Status: Accepted
- Date: 2026-09-10
- Tracker: D1

## Context

Users want to glance at a PC's load from a phone on the same network. The
options were a native mobile app (a second codebase and two store accounts),
a cloud relay (an account, a server we run, and the machine's telemetry
leaving the LAN), or an HTTP server inside the desktop app that serves a
small web page.

## Decision

The desktop app embeds an HTTP server (`crates/vitals-server`) that is off by
default. When the user turns it on, the app renders a QR code containing
`http://<ip>:<port>/mobile.html#t=…`. The phone opens that URL and gets a
lean PWA served from the same process.

## Consequences

- Nothing leaves the LAN, and there is no account and no relay to operate.
- The same server is the third-party API (ADR 0002), so the phone is just one
  more client — and as a new client it audited the backend: it found the
  System Idle Process in every frame and phantom GPUs at 0 %, both fixed at
  the source.
- Plain HTTP on a private address is not a secure context, which rules out a
  service worker (ADR 0029) and shapes the security model (ADR 0003).
- The desktop must stay running for the phone to work; a headless
  `vitals serve` exists for machines without a session.
