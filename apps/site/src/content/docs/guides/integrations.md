---
title: Integrations
description: Read Vitals from scripts, dashboards and home automation — REST, SSE, WebSocket, Prometheus, Home Assistant and a TypeScript SDK.
---

Everything the phone uses is a documented HTTP API, and anything else can use it too. The full
contract is
[`docs/api/openapi.yaml`](https://github.com/dragoscv/vitals/blob/main/docs/api/openapi.yaml) and
the short version is
[`docs/api/README.md`](https://github.com/dragoscv/vitals/blob/main/docs/api/README.md).

## Two servers

- **Local API** — `http://127.0.0.1:7330`, always on while the app runs, reachable only from this
  machine. No token is needed from `127.0.0.1`. If the port is taken, the app picks another and
  writes it to `%LOCALAPPDATA%\Vitals\local-api.json`.
- **LAN server** — port `7331`, only when [remote access](/guides/remote-access/) is on. Every
  request needs a paired token.

```powershell
$d = Get-Content "$env:LOCALAPPDATA\Vitals\local-api.json" | ConvertFrom-Json
curl.exe -s "http://127.0.0.1:$($d.port)/api/v1/snapshot"
```

## REST, SSE and WebSocket

All routes live under `/api/v1/`. A snapshot is a plain `GET`; a live stream is available as
Server-Sent Events or a WebSocket. Streams begin with a complete keyframe and then send compact
deltas, so a client that joins late still starts from the full picture.

## Prometheus

`/metrics` serves the Prometheus text format. A reading the machine cannot report is an **absent
series**, not a zero, so a dashboard shows a gap rather than a false flat line. Point a scrape job
at the LAN server with the token as a bearer credential.

## Home Assistant

A ready-made package adds CPU, memory, disk, GPU and alert sensors to Home Assistant. See
[`docs/integrations/home-assistant.md`](https://github.com/dragoscv/vitals/blob/main/docs/integrations/home-assistant.md).

## TypeScript SDK

[`@vitals/client`](https://github.com/dragoscv/vitals/tree/main/packages/client) is a typed client
over REST, SSE and WebSocket, built on the same generated types the app itself uses. It lives in
the repository under `packages/client`; publishing it to npm is planned.

## Discovery

While the LAN server runs it advertises `_vitals._tcp` over mDNS, so tools on the network can find
it without a hard-coded address. The advertisement stops when the server stops.
