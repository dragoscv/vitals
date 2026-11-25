# 0049 — Agent collaboration is observed from the coordination directory

- **Status:** Accepted (2026-11-25)
- **Slice:** S26-13
- **Supersedes:** none

## Context

Several agents run on this machine at once, each in its own chat window, and they
coordinate through agentq: a registry of live sessions with who runs where and
what they are doing, directed messages that carry their answers, and the jobs
that hold the builds and the deploys. All of it is on disk under
`%USERPROFILE%\.codai\coord` — `agents/<session>.json`, `msgs/<8hex>.json`,
`jobs/<id>.json` — written by the CLI and the daemon, and it is the durable
record: it survives a stopped daemon, a killed chat and a reboot.

The owner asked to see that collaboration in Vitals, live, in the same product
that already reads agentq's journal for the AI page (S18, S26-11): who is present,
what they are saying to each other, what they are running.

Three sources could answer it: the coordination directory itself; the agentq
daemon's loopback RPC (a bearer token in `daemon.json`, a long-poll `wait` and no
WebSocket or SSE); or the codai database, which mirrors the queue for the cloud
side of the same programme.

## Decision

Vitals reads the coordination directory **read-only**, and never writes to it.

1. `vitals-agents` gains a reader whose only public entry returns a snapshot:
   agents present (the registry's own thirty-minute liveness rule, matching
   `agentq-msgs.ps1` so the page and the CLI cannot disagree about "live"),
   threads (one directed message plus its answers, newest activity first), and
   jobs (running always, finished for a day, running first).
2. Every field is optional and every unmeasured figure is `None`, rendered as an
   em dash — an age that cannot be computed is not "0 s ago".
3. The reader tolerates a file that is being written while it is read: a partial
   record is skipped, never an error, because the writer is another process on
   the same machine and does not lock.
4. The page polls while it is visible (four seconds, plus a one-second clock) and
   stops when it is not. Nothing is cached beyond the poll, and no watcher is
   installed on a directory another process rewrites.

The cloud half of the requirement — agents on other machines and the runners in
the devcloud, which cannot see this directory — is **not** decided here. It needs
a shared store both sides can reach (the codai database is the only candidate),
which is a separate record.

## Consequences

- No new local service, port, schema or token: the page works with agentq
  stopped, and it works when the daemon was never started.
- Vitals cannot corrupt the coordination state. Two writers to the record a
  coordination tool gates its own decisions on is how such a tool stops being
  trusted; the page observes, agentq writes.
- The page sees only this machine's agents. That limit is visible in the UI
  rather than implied, and the cloud store is the next slice.
- A missing directory renders an empty state with the path it looked in, not an
  error and not a zero.
- Rejected: the daemon RPC (data the daemon persists to those same files, behind
  a token, for a page that would then need the daemon running);
  a file watcher (more moving parts than a four-second poll of a few small JSON
  files, in a product that deliberately ships no service worker, ADR-0029);
  writing presence or messages back (a page that writes coordination state is a
  second source of truth for it).
