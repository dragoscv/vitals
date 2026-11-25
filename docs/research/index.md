# Research notes

Durable copies of the read-only research produced on **2026-11-25**. Each file
opens with a status line marking it a snapshot of what was true when it was read,
not a decision: decisions live in `docs/TRACKER.md` and `docs/adr/`.

| File                                                           | Question it answers                                                                                                                                                                                                                                                                         | Produced   | Kind                                                        |
| -------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------- | ----------------------------------------------------------- |
| [codai-project-health.md](codai-project-health.md)             | What already exists in `E:\gh\codai` for per-project health/status, a status single source of truth, notification channels (email via Brivio, Telegram, in-app), the tenant/"Hub" model, public/private data, and agent read paths (MCP, REST, SDK, CLI) — and where new work would attach. | 2026-11-25 | Snapshot — read-only survey of codai `main` at `5080b28dfd` |
| [agentq-mcp.md](agentq-mcp.md)                                 | The current state of the agentq MCP server (`E:\gh\agentq`): where it lives, how it is built, launched and registered, the tools it exposes today, and what "complete" still requires.                                                                                                      | 2026-11-25 | Snapshot                                                    |
| [vitals-page-patterns.md](vitals-page-patterns.md)             | The exact recipe for adding one page to the Vitals desktop app — the four things that declare a route, the feature-folder convention, every gate a page must pass, and how new wire types are added.                                                                                        | 2026-11-25 | Snapshot of current conventions; `AGENTS.md` overrides      |
| [vitals-codwen-account.md](vitals-codwen-account.md)           | What in `E:\gh\vitals` lets the app talk to a Codwen account today, and the smallest honest path to a signed-in "my projects: status, health, alerts" page.                                                                                                                                 | 2026-11-25 | Snapshot                                                    |
| [codwen-auth-and-read-paths.md](codwen-auth-and-read-paths.md) | Codwen's device-flow, scope registry and read paths for a desktop client's "my projects" view, and how the local agentq tool can surface project information to agents when the owner is signed in.                                                                                         | 2026-11-25 | Snapshot                                                    |
| [tizen-cloud-lane.md](tizen-cloud-lane.md)                     | Whether the Samsung Tizen app can be built **and packaged** on a devcloud workspace instead of this workstation: which half needs only Node, which half needs Tizen Studio and the signing profile, and what would have to be provisioned.                                                  | 2026-11-25 | Snapshot — feasibility; nothing built or submitted          |
| [ios-cloud-lane.md](ios-cloud-lane.md)                         | Whether the Apple half needs a cloud lane at all: what already builds, tests and uploads the app, the state of the `vmui-mac` instance, and the recommendation.                                                                                                                             | 2026-11-25 | Snapshot — feasibility; the VM was not started              |

## Caveats — claims that have moved since these were written

A stale research note is worse than none, so the points below were re-checked on
2026-11-25 and no longer match the reports:

- **`agentq-mcp.md`** states the agentq MCP server is "unregistered" on this
  machine. It is now registered: `~/.copilot/mcp-config.json` contains
  `"agentq": { "type": "stdio", "command": "node", "args": ["E:\\gh\\agentq\\dist\\cli.mjs", "mcp"] }`,
  not the README's documented `npx -y @codai/agentq@0.2.0 mcp` shape.
- **`codai-project-health.md`** and **`codwen-auth-and-read-paths.md`** are
  surveys of codai `main` at `5080b28dfd`; codai is now at `309d5003d7`, so line
  references and any "absent"/"designed only" verdicts may have changed. In
  particular `codwen-auth-and-read-paths.md` flags `agents:read` as absent from
  the scope registry — that was true of that checkout only.
- **`vitals-page-patterns.md`**, **`vitals-codwen-account.md`** and
  **`codwen-auth-and-read-paths.md`** reference `docs/architecture.md`,
  `docs/adr/0039-ai-agents-page-and-agentq.md` and
  `docs/integrations/codwen-project-health.md`. All three exist in this repo
  (checked 2026-11-25). All paths in the reports are relative to the repository
  root, not to this directory.
