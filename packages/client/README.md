# @vitals-app/client

A typed TypeScript client for the [Vitals](https://vitals.dragoscatalin.ro)
system monitor's local and LAN API — REST, server-sent events and WebSocket.

- No runtime dependencies: it uses the platform's `fetch`, `EventSource` and
  `WebSocket`, which exist in every browser and in Node 22+. Each can be
  injected for tests or for a runtime that brings its own.
- Types are generated from the same Rust model the desktop app uses, so a
  frame here is exactly the frame the app samples.
- ESM only, with full type declarations.

```sh
pnpm add @vitals-app/client
```

(Inside the Vitals monorepo the package is named `@vitals/client`; the
`@vitals` npm scope belongs to another project.)

## Getting a server and a token

Turn on **Remote access** in Vitals' Settings and pair a device; the app
shows the address (port 7331 by default) and a bearer token once. For local
experiments without pairing, run the development server from a clone of the
repository — it serves live metrics on port 7332 with the token `dev`:

```sh
cargo run -p vitals-server --example serve_dev
```

## Usage

```ts
import { VitalsClient, isVitalsError } from '@vitals-app/client';

const client = new VitalsClient({
  baseUrl: 'http://192.168.1.20:7331',
  token: process.env.VITALS_TOKEN!,
});

// Unauthenticated: tells "wrong address" apart from "wrong token".
const health = await client.health(); // { ok, version, modelVersion }

// The most recent keyframe, or null before the first sampling tick.
const frame = await client.snapshot();
if (frame === null) console.log('starting…');
```

### One-off reads

| Method       | Route                  | Returns                                                    |
| ------------ | ---------------------- | ---------------------------------------------------------- |
| `health()`   | `GET /api/v1/health`   | `Health` — the one call that needs no token                |
| `snapshot()` | `GET /api/v1/snapshot` | `Frame \| null` (`null` before the first tick, not zeroes) |
| `host()`     | `GET /api/v1/host`     | `HostInfo \| null`                                         |
| `alerts()`   | `GET /api/v1/alerts`   | `Alert[]` — empty is the healthy answer                    |
| `metrics()`  | `GET /metrics`         | Prometheus text; `''` before the first tick                |

### Live frames over SSE

`stream()` reconnects with backoff until you call the function it returns.
Keyframes and deltas both arrive in `onFrame`.

```ts
const stop = client.stream({
  onFrame(frame) {
    /* render */
  },
  onOpen() {},
  onDrop(error) {
    /* show "reconnecting" */
  },
  onRetry(delayMs, attempt) {},
});

// later
stop();
```

A browser cannot set headers on an `EventSource`, so the token travels in
the query string, and a 401 is invisible to it. Validate the token with
`health()` and `snapshot()` before streaming.

### Frames and control over WebSocket

`connect()` takes the same handler and returns a connection that can also
send control requests. Replies are matched to requests in order. If the
socket drops while a request is in flight, the request rejects with a
`closed` error: its outcome is unknown, so read the process list again
rather than retrying blindly.

```ts
const conn = client.connect({ onFrame(frame) {} });

const reply = await conn.control({ action: 'suspend', key });
if (!reply.ok) console.warn(reply.error);

conn.close();
```

### Acting on a process

Control needs a token with **control** scope; the default pairing is
read-only. Every request carries a full `ProcessKey` (PID plus start time),
never a bare PID, so a recycled PID is never touched.

```ts
await client.control({ action: 'terminate', key });
await client.control({ action: 'set-priority', key, priority: 'below-normal' });
await client.setEfficiencyMode(key, true); // Windows 11 efficiency mode
```

Actions: `terminate`, `suspend`, `resume`, `set-priority` (`idle`,
`below-normal`, `normal`, `above-normal`, `high`, `realtime`) and
`set-efficiency-mode`. Over HTTP, `control()` resolves on the server's 204.

## Errors

Every failure is a `VitalsError`. Branch on `kind`, not on the message:

| `kind`         | Meaning                                                                     |
| -------------- | --------------------------------------------------------------------------- |
| `network`      | The server could not be reached, or the connection dropped                  |
| `unauthorised` | 401 — no token or an unknown one (the server will not say which, by design) |
| `forbidden`    | 403 — a read-only token tried to act, or Windows refused                    |
| `http`         | Any other non-2xx status; `status` and `detail` say which                   |
| `protocol`     | A 2xx reply whose body did not match the contract                           |
| `closed`       | A WebSocket request was pending when the socket closed                      |

`detail` carries the server's structured refusal (`not-found`,
`access-denied`, `unsupported`, `internal`) when there is one.

```ts
try {
  await client.control({ action: 'terminate', key });
} catch (error) {
  if (isVitalsError(error) && error.kind === 'forbidden') {
    // ask the user to pair with control scope
  } else {
    throw error;
  }
}
```

## Coverage

Covered: `health`, `snapshot`, `host`, `alerts`, `stream` (SSE), `ws`
(WebSocket), `control` and `/metrics`.

Not yet wrapped: `GET /api/v1/summary`, `GET /api/v1/history`,
`GET /api/v1/sensors` and `POST /api/v1/pair`. They are documented in
[`docs/api/openapi.yaml`](https://github.com/dragoscv/vitals/blob/main/docs/api/openapi.yaml)
and can be called with plain `fetch` and a `Bearer` header until the client
grows methods for them.

## Licence

[MIT](https://github.com/dragoscv/vitals/blob/main/LICENSE).
