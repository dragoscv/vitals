# ADR-0038: An AI page that watches agentq and every AI client, local and cloud

- Status: accepted
- Date: 2026-10-17
- Tracker: S18-01..S18-09

## Context

This machine runs 20–40 AI coding agents at once (VS Code Copilot Chat,
Copilot CLI, Codai desktop, Codex, occasionally Claude) plus jobs in the
devcloud. They are coordinated by **agentq**, a PowerShell queue whose state
lives in plain JSON under `%USERPROFILE%\.codai\coord`:

- `queues/<resource>/<seq>-<id>.json` (+ `.hb`, mtime = heartbeat),
- `leases/<repo>/<slot>.json`, `releases/<repo>__<target>.json`,
- `jobs/<id>.json`, `marks/<resource>.json`,
- `journal.jsonl` (append-only, ~45 event kinds, ~50k lines).

Writers use `.tmp` → rename under `Global\agentq-*` mutexes, so a reader never
sees a torn file. The codai hub already mirrors this state (`/ops/agentq`), but
by 15-second polling, over the network, behind a `super_admin` key — useless
for the person sitting at the machine wondering why it is slow.

The AI clients each keep their own session state:

- Copilot CLI and VS Code Copilot Chat share `~/.copilot/session-state/<id>/`
  (`workspace.yaml` + `events.jsonl`: turns, tools, model, cwd, branch).
- Codex writes `~/.codex/session_index.jsonl` and zstd rollouts.
- Claude Code writes `~/.claude/projects/*/*.jsonl`.
- Codai desktop answers `{"op":"list"}` on
  `\\.\pipe\codai-desktop-<profile>-agents` (schema `codai.agents/1`).
- Cloud: the codai gateway (`/v1/hosts`, `/v1/environments`,
  `/v1/sessions?v=2`, `/v1/usage`) and devcloud job objects in GCS.

Measured on 2026-10-17: 468 `pwsh` processes, 409 of them VS Code terminals
that agents opened and nobody closed, holding 27 GB private memory. No tool
on the machine showed it.

## Decision

1. **A new crate `vitals-agents`**, OS-agnostic, owns the model and every
   parser (agentq, Copilot sessions, Codex, Claude). Platform reads (named
   pipe, process command line) stay in `vitals-win`. Parsers read
   incrementally: journal and `events.jsonl` from a remembered byte offset,
   never whole files per tick.
2. **On-demand watching.** The desktop runs one `agents` thread that starts
   when the AI page or a LAN client subscribes and stops when the last one
   leaves. It re-reads on directory change notifications, debounced, with a
   slow safety rescan. It emits `vitals://agents` (full snapshot; the payload
   is small) and serves `get_agents` for first paint.
3. **Liveness is agentq's own rule, recomputed natively**: a pid is alive only
   if its start time matches within 3 s; a ticket is stale when `.hb` is older
   than 10 min; a lease is stale when its owner is dead and idle > 120 min or
   idle > 1440 min. An unknown is `None`, never `0` (the repo principle).
4. **Writes go through agentq, never around it.** Mark, clear, break, note,
   unlease and job kill shell out to `agentq.ps1`/`jobs.ps1` with a mandatory
   reason. Vitals never touches a coord file. Over LAN these need the
   `Control` scope.
5. **Cloud is read-only and opt-in.** Sign-in uses the codai OIDC device flow;
   the resulting key is stored in Windows Credential Manager and never shown
   again. Without it the cloud tab says so; nothing is sent anywhere.
6. **Agent ↔ process attribution** uses command lines read on demand and
   cached per `ProcessKey`, so CPU and memory can be summed per agent tree.
7. **Privacy.** Prompts are never read past the event type. Paths and titles
   go through the frame redaction when LAN redaction is on.

## Consequences

- One page answers "which agents are working, who started them, for how long,
  how much of that was active, what are they waiting on, what does it cost".
- Nothing runs while nobody looks (AGENTS.md, Performance).
- agentq's file format becomes a contract with a second reader; agentq's own
  tests gain a fixture copy so a format change fails there first.
- The agentq TypeScript daemon (ACP-Lock) is not used: it is not live.

## Rejected

- **Read the codai hub mirror.** Network-dependent, 15 s stale, needs a
  `super_admin` key, and blind when the sync loop is down.
- **Run `agentq status -Json` on a timer.** Correct but ~1 s per call and a
  process per tick; kept as a cross-check in tests only.
- **Write coord files directly for actions.** Duplicates agentq's mutex and
  journal logic; one bug corrupts the queue every agent depends on.
