# 0050 — Agents in Codwen's cloud appear beside the agents on this machine

- **Status:** Proposed (2026-11-25)
- **Slice:** S26-19
- **Supersedes:** none; this completes the cloud half [0049](0049-agent-collaboration-reads-the-coordination-directory.md) deferred

## Context

The collaboration view (S26-13, [0049](0049-agent-collaboration-reads-the-coordination-directory.md))
shows who is present on this machine, what they are saying to each other and what they
are running, read from agentq's coordination directory. It shows only this machine, and
the record says so plainly: the agents on other machines and the runners in the devcloud
"cannot see this directory", so the cloud half "needs a shared store both sides can reach
(the codai database is the only candidate), which is a separate record." This is that
record.

The account that reaches the store already exists, and already works. Vitals signs in with
the Codwen OIDC device flow (RFC 8628, public client `codai-cli`), keeps a long-lived key
in Windows Credential Manager (`vitals/codwen-api-key`,
`crates/vitals-agents/src/cloud.rs`), mints it under the `openid agents:read` scope
(`cloud.rs:48`) and labels it `Vitals desktop` (`cloud.rs:51`) rather than the computer
name. The AI page then reads `GET /v1/sessions?v=2&limit=100` through that key on a
30-second tick (`CLOUD_EVERY`, `apps/desktop/src-tauri/src/agents.rs:136`), and
`parse_sessions` already turns each row into a host, a state and a `presence_count`. Those
rows reach the page as `AgentsSnapshot::sessions`
(`crates/vitals-core/src/agents.rs:46`), and the Cloud panel already separates them from
the local ones with `sessions.filter((s) => s.host !== 'local')`
(`apps/desktop/src/features/agents/ActivityPanels.tsx`), including the signed-out copy
"Sign in to Codwen to see agents running in the cloud"
(`apps/desktop/src/features/agents/strings.ts`). What the collaboration view is missing is
therefore not a way to reach the cloud — that path is signed, cached and in use — but a
decision about what a cloud agent's row _means_ in that view, and how it sits beside the
local rows without the two being read as one list.

Two facts force the answer's shape. First, **the two sources have two owners**: the
coordination directory is written by agentq on this machine, the session roster by the
Codwen gateway, and neither is authoritative for the other. A row that exists in one and
not the other is not a conflict to reconcile but two different facts. Second, **the cloud's
record of an agent is a stamp, not a socket**: `/v1/sessions` carries `last_event_at`, a
`presence_count` and a `lease_expires_at`, and nothing else about liveness. A design that
said "live connection" would be claiming an observation the client never makes.

## Decision

1. **Two sources, one view, never merged.** The collaboration view gains a cloud section
   beside the local one. Each is read from its own owner and rendered in its own section;
   no row is summed, averaged or deduplicated across the boundary. A conversation that is
   genuinely both — the AI page already recognises the same one by `session_key`, and
   `parse_sessions` drops a cloud row an `is_local` predicate accepts — is **linked**, the
   same id shown in both sections, never collapsed into a single row. _The constraint:_ the
   two records have different owners and are written to different clocks, so merging them
   would make "who is here" depend on which writer happened to be fresher, and would have
   the read client inventing an authority the platform did not give it. Rule 3 of the
   inbound contract (`docs/integrations/codwen-project-health.md`) states the same thing
   from the platform's side: two distinct things, never merged.

2. **"Present" is one word, and it is built from the one signal the design can observe.**
   For a cloud agent Vitals can see exactly three things, and no others: a **last-seen
   stamp** (`last_event_at`, carried on every row), a **presence count** (`presence_count`,
   how many connections the gateway counts), and a **lease** (`lease_expires_at`, held while
   the agent works). It cannot see a **live session** — `/v1/sessions` is a poll, not a
   connection — and it cannot see a **heartbeat**: that response carries no heartbeat
   channel, and the devcloud heartbeat is a GCS object for a _workspace_, which Vitals has
   no client for. So the view keeps the four-state vocabulary the AI page already derives —
   Working when a lease is live and idle is under two minutes, Waiting while a lease is
   live or the presence count is non-zero, Idle under six hours, Ended beyond
   (`parse_sessions`) — and it adds the single word both sections share: **present**,
   meaning the last observation is inside the same thirty-minute window agentq's own
   registry uses (`LIVE_MIN = 30`, `crates/vitals-agents/src/collab.rs`, matching
   `Get-LiveAgents` in `agentq-msgs.ps1`). _The constraint:_ one view must mean one thing by
   "here"; a second window for the cloud rows would let the same word read twice, and the
   local section would then be right only by coincidence. A row whose `last_event_at` did
   not parse is neither present nor absent — it is **unknown**, and its age renders as an em
   dash.

3. **The read path is the account Vitals already has.** The cloud roster uses the same
   device-flow key, the same `openid agents:read` scope and the same `GET /v1/sessions` the
   AI page already calls, folded into the existing 30-second `CloudCache` tick rather than a
   second subscription and a second timer. No new scope, no new credential, no second
   sign-in. The cloud is **read-only and never written**: Vitals persists no cloud state as
   local truth — a display cache with no more than the poll's own lifetime — never probes a
   project or a workspace itself, and is never a second source of truth for what the cloud
   holds. _The constraint:_ the gateway refuses the key for anything but the reads it was
   minted for and its own revocation (`cloud.rs` `check`), so widening the view must not
   widen the key; and rule 2 of the same contract forbids persisting the cloud list as
   truth.

4. **One side's failure never blanks the other, and each failure has its own word.** The
   local section keeps working when the cloud read fails, and the cloud section keeps its
   last good roster flagged as out of date when the network drops, because the two have
   different owners and different failure modes — a single error would name the wrong one.
   The three cloud failures are drawn apart deliberately:

   - **No account** — no key stored: the section shows "sign in to see the agents in your
     Codwen cloud" and a sign-in affordance, the same empty state the account card already
     carries, **never an empty roster** that would read as "no cloud agents" (rule 4 of the
     contract).
   - **Expired or revoked key** — the gateway answers 401 or 403, which `cloud.rs` `check`
     decodes as `signed-out`: the section asks for sign-in again and says the key was
     refused, which is a different sentence from a network fault.
   - **No network** — the fetch throws: the last good roster stays with an "out of date"
     mark, and the local section is untouched.

   A figure or a state that cannot be measured is **absent, never zero**: an age that did
   not parse is an em dash, and a section that cannot be read shows why rather than an empty
   list or a zeroed count. _The constraint:_ the repository's own principle — an unmeasured
   reading and a real zero are different facts (AGENTS.md) — and a cloud roster is exactly
   where the two are easiest to confuse.

5. **Polling, on the clock the page already runs.** Cloud presence rides the existing
   30-second `CloudCache` tick, fetched by Rust in the reader thread that already runs while
   the page is visible; the page itself starts nothing new. The cost is two `GET`s —
   `/v1/hosts` and `/v1/sessions` — every 30 seconds **while the page is visible and signed
   in**, and nothing when the window is hidden (the page subscribes only while visible,
   [0039](0039-ai-agents-page-and-agentq.md)) or when nobody is signed in. A streaming
   channel is rejected below, and its cost is named with it.

## Consequences

- The collaboration view answers "who is here" across machines, with the local and cloud
  halves never merged and never silently zeroed. The local half needs no agentq daemon and
  works offline; the cloud half needs the account and the network, and its section says so
  when either is missing.
- The `/v1/sessions` shape is not a new contract: the page reads fields the gateway already
  serves, so this decision adds a reader, not an endpoint. If the gateway later adds a
  per-session field the view needs, it appears in that same response rather than in a
  channel of its own.
- No writer is added anywhere: Vitals still never writes coordination state
  ([0049](0049-agent-collaboration-reads-the-coordination-directory.md)) and never writes
  cloud state (rule 1 of the contract). The roster is an observation on both sides.
- **Rejected: a streaming channel.** An SSE or WebSocket from the gateway for cloud
  presence would hold one connection per open Vitals window, need reconnect and backoff
  logic of its own, carry the key inside a long-lived socket, and add a failure mode the
  product does not otherwise have — for data whose freshness is already a stamp the server
  updates on its own schedule. The product deliberately ships no service worker
  ([0029](0029-no-service-worker.md)) and derives presence from stamps rather than
  connections everywhere else; a socket here would be the single place that watched a
  connection, and a dropped socket is indistinguishable at the client from an idle agent,
  which is exactly the confusion the em-dash rule exists to prevent. A poll of two small
  JSON documents every 30 seconds, only while someone is looking, is the cheaper and the
  more honest option.
- **Rejected: reading the devcloud heartbeat as presence.** It is a GCS object for a
  _workspace_, not a session, it lives in a bucket Vitals has no client for, and treating it
  as an agent would put a runner and an agent in the same row. A runner is deferred below,
  and stays a workspace.
- **Rejected: having the cloud write into the coordination directory.** A background sync
  that dropped cloud sessions into `coord/agents` would make the directory a second source
  of truth for the very record agentq gates its decisions on — the same objection that
  rejected writing presence back in
  [0049](0049-agent-collaboration-reads-the-coordination-directory.md).
- **Deferred: the devcloud runners.** A runner is a workspace or a job, not a session; its
  live state (heartbeat, idle/busy, queue depth) needs an agentq surface Vitals can call — a
  read-only `agentq-cloud.ps1 -Status -Json` — which does not exist yet. The cloud section
  shows sessions today; workspaces are a later record that names that surface.
- **Deferred: cloud-side resources.** A cloud row shows presence and state, not CPU or
  memory. The local section's per-agent attribution reads process tables this machine owns,
  and the cloud response carries no such figures; inventing them would be the partial sum
  the page already refuses ([0039](0039-ai-agents-page-and-agentq.md), amendment of
  2026-11-24).
- **Deferred: cloud-side messages.** agentq's directed-message channel (`coord/msgs`) is
  local-only and stays so; the cloud section shows who is present, not what they are saying
  to each other. Threads remain a local read until the platform carries them, which this
  record does not decide.
