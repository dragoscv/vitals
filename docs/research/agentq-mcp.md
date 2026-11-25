> Status: research snapshot, 2026-11-25. Not normative — see docs/TRACKER.md and the ADRs for decisions.

# agentq MCP server — current state and what "complete" requires

Repository: `E:\gh\agentq` (read-only pass, 2026-11-25). Three other agents had uncommitted work in
`ps/bin/agentq-cloud.ps1`, `ps/install.ps1`, `ps/bin/agentq-perf.ps1`, `ps/tests/test-agentq-slotshare.ps1`
(`git status`: ` M ps/bin/agentq-cloud.ps1`, ` M ps/install.ps1`, ` M ps/tests/README.md`,
` M ps/tests/run-all.ps1`, ` M ps/tests/test-install-wiring.ps1`, `?? ps/bin/agentq-perf.ps1`,
`?? ps/tests/test-agentq-slotshare.ps1`, `?? ps/tests/test-cloud-evidence.ps1`, `?? tests-land-suites.ps1`).
Nothing was edited, staged, built or installed.

## 1. Where the MCP server lives, how it is built, launched, registered

- **Source**: `src/mcp.ts` — one file, `runMcp(ep, session)`. VERIFIED.
- **Launcher**: `src/cli.ts:12` imports `runMcp`; the `mcp` subcommand dispatches at
  `src/cli.ts:313-315` (`case "mcp": await runMcp(ep, session()); return await new Promise(...)`).
  `ep` is the daemon endpoint from `connect()` (`src/client.ts:56+`), autostarting the daemon if none
  answers. `session()` is `src/cli.ts:33`. VERIFIED.
- **Build**: tsdown. `package.json:26` `"build": "tsdown"`; `tsdown.config.ts` entry is
  `["src/index.ts","src/cli.ts"]`, `format: esm`, `platform: node`, `target: node24`, `dts: true`.
  `mcp.ts` is **bundled into** `dist/cli.mjs` (it is not its own entry); built output present:
  `dist/cli.mjs` (12,577 B), `dist/index.mjs` (426 B). VERIFIED.
- **Bin**: `package.json:10-12` `"bin": { "agentq": "./dist/cli.mjs" }`. So the server runs as
  `agentq mcp` (a stdio MCP server). VERIFIED.
- **Registration — documented, not installed**: `README.md:43-44` gives the shape

  ```jsonc
  // .vscode/mcp.json
  { "servers": { "agentq": { "command": "npx", "args": ["-y", "@codai/agentq@0.2.0", "mcp"] } } }
  ```

  VERIFIED. There is **no** `mcp-config.json`, `.vscode/mcp.json` or `.vscode` directory in the repo
  (`Test-Path .vscode` → False) and `ps/install.ps1` never mentions MCP (grep for `mcp` → no hits): the
  installer does **not** register the server. VERIFIED.

- **On this machine** the server is currently **unregistered**: grep for `agentq` in
  `~/.copilot/mcp-config.json` and in VS Code `User\mcp.json` found nothing. The global
  `~/.copilot/mcp-config.json` uses the shape
  `{ "mcpServers": { "<name>": { "type": "local", "tools": [...], "command": "...", "args": [...], "env": {...} } } }`
  (seen for tavily/sentry/context7) — that is the shape a registration would take. VERIFIED.
- No test references MCP (`rg -l mcp test` → none). VERIFIED.

## 2. Tools exposed today

Built in one file by seven `server.registerTool(...)` calls on an `McpServer` (no tools map, no
decorators); each handler forwards to the daemon over JSON-RPC via `call(ep, "acp.*", …)`
(`src/client.ts:36`). Tickets are owned by the server pid and heartbeated (`src/mcp.ts:14-18`).

| Tool name     | What it does                                               | file:line        |
| ------------- | ---------------------------------------------------------- | ---------------- |
| `acp_acquire` | Queue/acquire a resource, wait ≤25 s; heartbeats the lease | `src/mcp.ts:24`  |
| `acp_wait`    | Wait up to N s more for a queued ticket                    | `src/mcp.ts:55`  |
| `acp_release` | Release a held/queued ticket (optional exit)               | `src/mcp.ts:63`  |
| `acp_status`  | Holders/waiters/freeze marks (read-only)                   | `src/mcp.ts:76`  |
| `acp_mark`    | Freeze/block/clear a resource                              | `src/mcp.ts:85`  |
| `acp_note`    | Handoff note into the journal                              | `src/mcp.ts:93`  |
| `acp_journal` | Recent journal entries (read-only)                         | `src/mcp.ts:102` |

Input schemas (Zod, exact): `acp_acquire {resource:string, purpose:string, waitSeconds:int 0..25=25, capacity?:int≥1}`;
`acp_wait {ticket:string, waitSeconds:int 1..25=25}`; `acp_release {ticket:string, exit?:int}`;
`acp_status {resource?:string}`; `acp_mark {resource:string, state:"frozen"|"blocked"|"clear", reason:string}`;
`acp_note {message:string, resource?:string}`; `acp_journal {limit:int 1..200=30, resource?:string}`. VERIFIED.

Daemon methods behind them: `src/daemon.ts:106-145` (`acp.hello/acquire/wait/heartbeat/release/break/mark/note/status/journal/verifyJournal`).

## 3. Gap table — capabilities not reachable through MCP

The MCP face is the **daemon's** RPC surface only. Everything implemented in PowerShell (queue verbs,
jobs, cloud) is invisible to it. The TS CLI itself (`src/cli.ts:200`) knows only
`daemon|run|status|mark|break|note|log|verify-journal|mcp` — no commit/lease/land/publish.

| Capability                         | Implemented by (script : function)                                                                                     | Reachable today                | Wrapper a new tool would need                                  |
| ---------------------------------- | ---------------------------------------------------------------------------------------------------------------------- | ------------------------------ | -------------------------------------------------------------- |
| `since`                            | `ps/bin/agentq.ps1:2011` `Show-Since`                                                                                  | No                             | spawn `agentq since`                                           |
| `status` (rich, PS)                | `ps/bin/agentq.ps1:1862` `Show-Status`                                                                                 | Partial (`acp_status`)         | —                                                              |
| `run` (queue+hold+run+release)     | `ps/bin/agentq.ps1:1523` `Invoke-Run`                                                                                  | No                             | spawn `agentq run -r .. -p .. -- ..`                           |
| `commit` (pathspec commit + push)  | `ps/bin/agentq.ps1:1696` `Invoke-Commit`                                                                               | No                             | spawn `agentq commit -Paths .. -Message ..`                    |
| `lease` / `unlease` / `renew`      | `ps/bin/agentq-v2.ps1:1048 / 1123 / 1156`                                                                              | No                             | spawn `agentq lease\|unlease\|renew`                           |
| `claim`                            | `ps/bin/agentq-v2.ps1:1036`                                                                                            | No                             | spawn `agentq claim`                                           |
| `land`                             | `ps/bin/agentq-v2.ps1:2606` `Invoke-Land`                                                                              | No                             | spawn `agentq land -Branch .. -Onto ..`                        |
| `publish`                          | `ps/bin/agentq-v2.ps1:2856` `Invoke-Publish`                                                                           | No                             | spawn `agentq publish -Paths .. -Message ..`                   |
| `sweep`                            | `ps/bin/agentq-v2.ps1:1309`                                                                                            | No                             | spawn `agentq sweep`                                           |
| `recover`                          | `ps/bin/agentq-v2.ps1:1251`                                                                                            | No                             | spawn `agentq recover`                                         |
| `note`                             | `ps/bin/agentq.ps1:524` `Write-Journal` (verb dispatch `:2037`)                                                        | Yes (`acp_note`)               | —                                                              |
| `mark` / `break`                   | `ps/bin/agentq.ps1:2027 / 2046`                                                                                        | `acp_mark` yes; `break` **No** | spawn `agentq break`                                           |
| `propose` / `bump` / `burst`       | `ps/bin/agentq.ps1:1269 / 1232 / 1250`                                                                                 | No                             | spawn `agentq propose\|bump\|burst`                            |
| `workspace`                        | `ps/bin/agentq.ps1:2071` `Invoke-Workspace`                                                                            | No                             | spawn `agentq workspace`                                       |
| jobs: start / wait / list          | `ps/bin/job-start.ps1` (`param` L21), `ps/bin/wait-job.ps1`, `ps/bin/jobs.ps1`                                         | No                             | spawn those scripts                                            |
| **cloud submit** (devcloud job)    | `ps/bin/agentq-cloud.ps1` top-level submit path (`:318-440`); routed from `ps/bin/agentq.ps1:1506` `Invoke-CloudRun`   | No                             | spawn `agentq-cloud.ps1 -Ws .. -Purpose .. -- <cmd>`           |
| **cloud log/result**               | `ps/bin/agentq-cloud.ps1:407-412` (`Read-Gcs …/jobs/<id>/log`, `…/result.json`)                                        | No                             | wrap read of the job bucket                                    |
| **cloud -Status**                  | `ps/bin/agentq-cloud.ps1:273-288` (`gcloud … describe`, `Get-VmStatus :268`)                                           | No                             | spawn `agentq-cloud.ps1 -Ws .. -Status`                        |
| **cloud suspend / resume / reset** | `ps/bin/agentq-cloud.ps1:295` (`-Suspend`), `:320-321` (`resume`/`start`), `:397-399` (`reset`) via `Invoke-VmOp :269` | No                             | spawn the same script with `-Suspend`; resume/reset are inline |
| **cloud-heal**                     | `ps/bin/agentq-cloud.ps1:389-402` (heartbeat-age reset)                                                                | No                             | inline in the submit path; not a separate verb                 |
| **list workspaces**                | registry built in `ps/bin/agentq-cloud.ps1:60-70` (built-ins + config `cloudRegistry`)                                 | No                             | spawn with an unknown ws (`exit 2` lists known)                |

## 4. How MCP should spawn PowerShell on Windows — the pattern to copy

No MCP tool spawns anything today; the server is pure JSON-RPC (`src/mcp.ts` → `src/client.ts:36`).
The existing Node→process spawn pattern is `runCommand` in `src/cli.ts:49-90`:

```ts
function runCommand(argv: string[], env: NodeJS.ProcessEnv): Promise<number> {
  const once = (file: string, args: string[], verbatim: boolean): Promise<number | 'enoent'> =>
    new Promise((resolve) => {
      const child = spawn(file, args, {
        stdio: 'inherit',
        env,
        windowsVerbatimArguments: verbatim,
      });
      child.on('exit', (c, sig) => resolve(c ?? (sig ? 1 : 0)));
      child.on('error', (e: NodeJS.ErrnoException) => {
        if (e.code === 'ENOENT') resolve('enoent');
        else {
          console.error(`agentq: ${e.message}`);
          resolve(127);
        }
      });
    });
  const quote = (a: string): string =>
    /^[\w./:\\=@+-]+$/.test(a)
      ? a
      : `"${a.replace(/(\\*)"/g, '$1$1\\"').replace(/(\\+)$/, '$1$1')}"`;
  return once(argv[0]!, argv.slice(1), false).then((r) => {
    if (r !== 'enoent') return r;
    if (process.platform !== 'win32') {
      console.error(`agentq: command not found: ${argv[0]}`);
      return 127;
    }
    const line = `"${argv.map(quote).join(' ')}"`;
    return once(process.env['ComSpec'] ?? 'cmd.exe', ['/d', '/s', '/c', line], true).then((x) =>
      x === 'enoent' ? 127 : x,
    );
  });
}
```

Key points to reuse: pass the **argv array** (never a shell string) to `spawn`, read the exit code from
the `exit` event (`c ?? (sig ? 1 : 0)`), and on Windows retry `.cmd`/`.ps1` shims through
`%ComSpec% /d /s /c` with `windowsVerbatimArguments: true`. `gitInfo` (`src/cli.ts:117`) is the
`spawnSync` variant. **For an MCP tool the `stdio: "inherit"` must change** to captured pipes
(`["ignore","pipe","pipe"]`) — the MCP transport owns stdio (`src/mcp.ts:112`), so child output must be
read and returned as the tool result, not inherited. PowerShell scripts are invoked as
`spawn("pwsh", ["-NoProfile","-File", script, ...args], …)` (or via `agentq.ps1 <verb> …`). EXPECTED
(the capture variant is a designed adaptation of the verified pattern, not existing code).

## 5. Trust-surface verdict

- The trust-surface guard is `~/.copilot/hooks/guard-rules-surface.ps1`; its patterns are defined in
  `~/.copilot/hooks/lib/RulesSurface.ps1:74-100`. It protects only:
  `~/.copilot/{rules,hooks,instructions,agents,skills}/**`, `~/.claude/{rules,skills}/**`, `CLAUDE.md`,
  `settings*.json`, `~/.agents/skills/**`, and `E:\gh\*\{.github\copilot-instructions.md,
.github\instructions\**, AGENTS.md, CLAUDE.md}`. **It does not cover `src/**` or `ps/**` source.**
  VERIFIED (read the patterns).
- Therefore **editing `src/mcp.ts`, `src/cli.ts`, `tsdown.config.ts` etc. needs no ticket.** The only
  agentq changes that would need one are to a guarded path — notably adding a root `AGENTS.md`
  (`docs/agent-contract.md:2-6` states a root `AGENTS.md` is blocked "until the owner issues a
  ticket"). VERIFIED.
- Two legitimate paths are documented in `docs/agent-contract.md:131-145`:
  1. **Proposal file** — `~/.copilot/rules/proposals/<ulid>.proposal.json`, shape
     `{ rule, reason, evidence[], created_by }` + the exact snippet; the owner reviews it
     (the only always-writable path under the rules dir, `RulesSurface.ps1:74`).
  2. **Ticket** — owner-only, interactive, HMAC/DPAPI-bound:
     `pwsh -NoProfile -File "<home>\.copilot\hooks\rules-ticket.ps1" -Issue -Scope '<glob>' -Hours 2`
     (file `~/.copilot/rules/edit-ticket.json`). Pass `-Scope` as separate array entries, not a comma
     list. VERIFIED.
- **No test forbids shell spawning from `src/`.** The TS tests themselves spawn
  (`test/hook.test.ts:1`, `test/daemon.test.ts:1`); there is no MCP test. `ps/tests/test-script-scope.ps1`
  is a `$null`-reset static guard, unrelated to spawning. VERIFIED.

## Files that must change to complete the server (dependency order)

1. **`src/pws.ts` (new)** — a small spawn helper: `runPwsh(argv, opts) => Promise<{code, stdout, stderr}>`,
   copied from `src/cli.ts:49-90` but with pipes, plus a resolver for `$HOME/.copilot/bin/agentq.ps1`
   and `ps/bin/*.ps1`. (No MCP tool spawns anything today.)
2. **`src/mcp.ts`** — add tools that call the helper: `agentq_since`, `agentq_run`, `agentq_commit`,
   `agentq_lease`, `agentq_unlease`, `agentq_land`, `agentq_publish`, `agentq_sweep`, `agentq_recover`,
   `agentq_break`, `agentq_propose` (+ mark `acp_status`-style read-only hints), and the cloud family
   `cloud_submit`, `cloud_status`, `cloud_log`, `cloud_result`, `cloud_suspend`, `cloud_resume`,
   `cloud_reset`, plus `job_start`, `job_wait`, `job_list`. Keep the name prefix consistent (`acp_*` for
   RPC, `agentq_*`/`cloud_*` for spawn).
3. **`package.json`** — no dep change needed (Node built-ins only); bump `version` and add a
   `test`/`mcp` note if you add an entry point. (Optional: add `src/mcp.ts` to `tsdown.config.ts` entry
   only if you want a standalone `dist/mcp.mjs`; not required — it already ships inside `cli.mjs`.)
4. **`test/mcp.test.ts` (new)** — register-tool assertions + a spawn round-trip against a throwaway
   `AGENTQ_HOME` (mirror `test/hook.test.ts`).
5. **`README.md:41-44`** — update the tool list; **`CHANGELOG.md`** — unreleased entry.
6. **`docs/agent-contract.md`** — only if a guarded path changes (a root `AGENTS.md` would need a ticket).

**Registration file to update**: the machine/global registration is
`~/.copilot/mcp-config.json` under `mcpServers` (`{ "type":"local","command":"npx","args":["-y","@codai/agentq@0.2.0","mcp"] }`),
and/or the repo-local `.vscode/mcp.json`
(`{ "servers": { "agentq": { "command": "npx", "args": ["-y","@codai/agentq@0.2.0","mcp"] } } }`) which
the README documents. Neither is written by the installer today.

**Build / install / test commands (read from the repo)**:

- Build: `pnpm build` (tsdown) — `package.json:26`.
- Typecheck: `pnpm typecheck` (`tsc --noEmit`) — `package.json:27`.
- Tests: `pnpm test` (vitest run) — `package.json:28`; PS suites: `pwsh -NoProfile -File ps\tests\run-all.ps1`.
- Lint/format: `pnpm lint` (`prettier --check .`) — `package.json:29`.
- Install into `~/.copilot`: `pwsh -NoProfile -File ps\install.ps1` (`docs/agent-contract.md:11`, `ps/install.ps1:36`).
  CI runs `lint typecheck test build` + a hook smoke test (`.github/workflows/ci.yml:19-25`).

## Fix plan (ranked)

1. Add `src/pws.ts` spawn helper (pipes, exit-code contract, script-path resolver).
2. Register the `agentq_*` queue/land verbs in `src/mcp.ts` — highest value, smallest surface.
3. Register the cloud family (`cloud_*`) by wrapping `ps/bin/agentq-cloud.ps1` (`:318-440`, `:273`, `:295`).
4. Add `test/mcp.test.ts`; update README tool list + CHANGELOG.
5. Register the server in `~/.copilot/mcp-config.json` (or document `.vscode/mcp.json`) — it is not
   registered on this machine today.
