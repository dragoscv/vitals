# ADR-0039: An AI page that watches agentq and every AI client, local and cloud

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

   _Amended 2026-11-24 (S26-01):_ the write surface is a **tab of fifteen
   verbs**, not six fixed buttons: clear, freeze, block, break, bump, burst,
   land, run, commit, note, unlease, propose, sweep, reap terminals and job
   kill. Each carries its own fields, its tone (safe, careful, danger) and
   whether it is confirmed before running, and one planner in `vitals-agents`
   turns a verb and its values into argv under the guards above, so a verb
   cannot be added without them. The panel previews the exact command from the
   same values it sends. An unset optional flag is sent as `null` rather than
   omitted: the shape stays explicit and a missing value is never read as a
   deliberate absence. **Vitals bounds only what would be meaningless** — an
   empty reason, a zero bump, a place that is not `local`, `cloud` or `auto` —
   and leaves a bump above agentq's cap or a burst above its own to agentq,
   whose caps are configurable and therefore belong where the number is known.
   The tab and its sidebar entry are **absent unless agentq is installed**, and
   the probe that decides it fails _open_: a status read that throws shows the
   page, because a monitor whose probe broke must not claim the feature does
   not exist.

5. **Cloud is read-only and opt-in.** Sign-in uses the codai OIDC device flow;
   the resulting key is stored in Windows Credential Manager and never shown
   again. Without it the cloud tab says so; nothing is sent anywhere.

   _Amended 2026-10-18 (S21, owner decision):_ "read-only" is now enforced by
   codai, not only by what Vitals chooses to call. The device flow requests
   `openid agents:read` (no `inference`, `keys:manage`, `email`, `profile`);
   codai mints a key whose gateway allowlist is the agent reads plus
   `POST /v1/keys/self/revoke`. The key is labelled `Vitals desktop`, never
   with the computer name. Sign-out revokes the key with itself, then deletes
   it locally (offline: deleted locally, and the UI says to revoke it in the
   hub). A full uninstall deletes the credential (`hooks.nsh`); an update
   keeps it. The device code stays in Rust behind an opaque handle, and
   `cancel_cloud_sign_in` stops an abandoned poll (ENG2-13).

   _Amended 2026-10-18 (S21-04, ENG2):_ the page subscribes only while the
   window is visible (an autostart launch with `lastRoute = agents` used to
   keep the reader polling from the tray). `/api/v1/agents`, its actions and
   the agent series in `/metrics` need a token even on the local API:
   loopback is shared by every account on the PC. agentq actions are refused
   while Vitals is elevated (a user-writable script must never run as
   administrator), run only `<profile>\.copilot\bin` scripts owned by this
   account or by administrators (profile from the shell, not `USERPROFILE` /
   `AGENTQ_BIN`), and are killed after 120 s.

6. **Agent ↔ process attribution** uses command lines read on demand and
   cached per `ProcessKey`, so CPU and memory can be summed per agent tree.

   _Amended 2026-11-24 (S26-03):_ the page gains a **Resources** section, and
   the attribution rule is now a decision rather than an implementation note. A
   process is credited to a tool by a **recorded identity, never by its
   executable name alone**: agentq by its own `running`/`suspect` jobs in
   `.codai/coord/jobs/*.json` (`pid` + `pidStart`), and each client by the
   identity it recorded in `coord/agents/*.json` (`appPid`/`shellPid` plus
   start time), with name rules (VS Code and its forks, Cursor, Claude,
   Copilot, Codex, Hermes, Openclaw, Kodwen/Codwen/Codai) and a description
   fallback used **only** when the description names the product. Shared
   runtimes are **refused on purpose** — `node.exe`, `pwsh.exe`, `python`,
   `git`, `cargo`, `deno`, `bun` — because `node.exe` hosts most npm CLIs _and_
   every build script, so claiming it would put everything compiling on the
   machine on an agent's bill. A start time is matched within 2 s, because
   Windows recycles PIDs; a process is counted once, under its **nearest
   claimed ancestor**; a tool with no process gets no row; and a series any
   contributor cannot report is **`null`, never a partial sum** — the same rule
   the rest of the page follows. Network and per-process VRAM stay `null`
   everywhere, because Windows exposes no per-process network counter without
   an elevated ETW kernel session and WDDM reports GPU utilisation but not
   per-process VRAM.

   _Amended 2026-11-24 (S26-04):_ agentq's own row is broken down into the
   **work it is executing**, one row per thing that owns a process, because
   "agentq is using 12 GB" answers no question a person actually has: a build,
   a test run, a land and an install cost the machine differently and one of
   them is usually the thing that is slow. The same recorded identity decides
   it — a **held queue ticket**, a **running release run** and a **running or
   suspect job** — and never a process name, which is why this needed no new
   sampling: the owners were already in agentq's files. Three rules make the
   breakdown trustworthy rather than decorative.

   - **The rows are the total, not a second opinion.** Each row is folded from
     the same process table in the same pass as the agentq figure, so the rows
     are exactly that figure partitioned — a panel cannot show a breakdown that
     disagrees with its own header. A test asserts the identity rather than
     trusting it.
   - **The identity is mandatory.** A claim needs a recorded start time and a
     match within the same 3 s tolerance as everywhere else
     (`PROC_START_TOLERANCE_MS`); a stale entry
     whose process is gone, or one whose recorded holder has been recycled,
     claims **nothing** and appears as no row. A ticket is `held: true` only; a
     release is `state: "running"`; a job must carry a pid. `dead` holders are
     absent on purpose — "running and costing you" is the question.
   - **Later claims win, so the row is where the work happens.** The editor's
     own process stays its client's; the shell a client queued a build through
     becomes agentq's, because agentq's record is the more specific statement
     about what that process is doing now. A process nobody recorded stays with
     the client that started it, and that limit is stated in the panel's own
     footnote rather than implied. Leases are deliberately **not** claimed: a
     lease's pid is the session that created it, so claiming it would steal the
     client's row, and the work a lease represents is already visible as the
     release run and the tickets that hold it.
   - **The kind comes from the record, and only twice from text.** The resource
     prefix of a held ticket (`build`, `commit`, `push`, `land`, `install`,
     `deploy`), or the command it stored when the resource names no kind, with
     a test-run marker ranking above the prefix and the command — so a
     `gate-run` inside a build resource reads as a test. A release run is a
     deploy and a job is a job because that is what agentq says they are: their
     purpose and command are prose or an inline script path, and matching those
     names the file rather than the work (measured 2026-11-24: a job waiting for
     a freeze to clear read as a land because its script is called
     `land-when-unfrozen.ps1`).

   The kind shows as a **word on a text token with a coloured dot beside it**,
   not as coloured text: the chart and status fills are not contrast-proven at
   this size, and the accessibility audit forbids a status fill used as a text
   colour outright, so the word carries the meaning and the dot only makes a
   wall of rows scannable.

7. **Privacy.** Prompts are never read past the event type. Paths and titles
   go through the frame redaction when LAN redaction is on.

   _Amended 2026-10-17 (S18-07):_ the LAN route redacts **unconditionally**,
   not behind a setting. Home directories become `~`, credential-looking
   values become `[redacted]`, and only identifiers an action needs (`id`,
   `leaseId`, `runId`, `session`, `sha`, `resource`, `slot`) pass through.
   The walk is over the serialised JSON, so a field added later is redacted
   by default. The phone card is read-only; actions over LAN are for scripts
   holding a `control` token (`vitals-server::agents`).

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
