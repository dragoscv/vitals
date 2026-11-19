# Vitals as tools for an AI agent

Vitals can hand a machine's readings to an AI agent over the
[Model Context Protocol](https://modelcontextprotocol.io) — the same numbers
the window shows, as six tools the agent can call. It is the shortest way to
let something answer "why is my PC slow?" with the actual process names.

Two transports, one server:

| Where        | How it is reached                         | Needs                         |
| ------------ | ----------------------------------------- | ----------------------------- |
| Same machine | `vitals mcp` on standard input and output | the CLI, nothing else         |
| On the LAN   | `POST /mcp` on the remote-access port     | Remote access on, and a token |

## What you get

| Tool        | Reads                                                                                                     | Good for                                |
| ----------- | --------------------------------------------------------------------------------------------------------- | --------------------------------------- |
| `summary`   | CPU, memory, disk, network, GPU, temperature, the busiest processes and the total process count           | "What is happening right now?"          |
| `processes` | The busiest processes, most CPU first, with the total so a short list is not mistaken for a small machine | "What is using the CPU?"                |
| `sensors`   | Temperatures, fan speeds, voltages, power draw, battery state                                             | "Is it thermal-throttling?"             |
| `alerts`    | The conditions Vitals considers a problem, most serious first                                             | "Is anything actually wrong?"           |
| `history`   | Machine-wide samples over the last N seconds (an hour by default, a week at most)                         | "Has memory been climbing all morning?" |
| `diagnosis` | One verdict on what is slowing the machine, with the processes that account for it                        | "Why is it slow?"                       |

Plus one **resource**, `vitals://<machine>`, which lists what the connected
agent can read — useful when someone wants to know what they just granted.

**There are no control tools, and that is permanent in v1.** Nothing here can
end, suspend, resume or re-prioritise a process, and asking for such a tool
fails the same way a typo does. An AI agent is a program that reads a sentence
and decides what to do with it; handing it a kill switch because a user pasted
a prompt is not a trade this project makes.

## A reading that cannot be measured is absent, never zero

The rule the whole product follows applies here too: a GPU that exposes no
counters is not a GPU at 0 %, and a sensor list that is empty means nothing is
measurable on that machine rather than everything reading zero. A tool that
could not ask at all — no sensor service, no history store, a sampler that has
not taken its first reading — answers with `isError` and a sentence saying so,
not with an empty list. The agent sees the difference, which is the point.

## Option 1 — on the same machine: `vitals mcp`

The CLI serves MCP on standard input and output. Most agents launch it
themselves; you only write the configuration.

```jsonc
// Claude Desktop: %APPDATA%\Claude\claude_desktop_config.json
{
  "mcpServers": {
    "vitals": { "command": "vitals", "args": ["mcp"] },
  },
}
```

```jsonc
// VS Code (Copilot): .vscode/mcp.json in the workspace, or the same shape in
// your user settings under "mcp".
{
  "servers": {
    "vitals": { "type": "stdio", "command": "vitals", "args": ["mcp"] },
  },
}
```

```jsonc
// Cursor: .cursor/mcp.json in the project, or ~/.cursor/mcp.json
{
  "mcpServers": {
    "vitals": { "command": "vitals", "args": ["mcp"] },
  },
}
```

If `mcp` is not found, give the full path — an agent's environment is not your
terminal's, and its `PATH` is often shorter:

```jsonc
{ "command": "C:\\Program Files\\Vitals\\vitals.exe", "args": ["mcp"] }
```

### Where the numbers come from

Same rule as every other subcommand: `vitals mcp` attaches to the running
desktop app if there is one — over its named pipe, or its loopback API — and
otherwise samples the machine itself. It says which on standard error, which
an agent's log will show.

One consequence is worth knowing. The attach **pipe** carries frames only, so
`sensors` and `history` need the desktop's loopback API. When the app is
running, `vitals mcp` uses it automatically for those two tools; if the app is
not advertising itself, or if you are sampling directly because the app is not
running, those two tools answer with a sentence saying so. To point the server
at a specific address:

```powershell
vitals mcp --attach http://127.0.0.1:7330
```

`sensors` and `history` are the two tools that need it; the other four work on
every source.

## Option 2 — over the LAN: `POST /mcp`

With Remote access on, the desktop serves the same tools over MCP's
Streamable HTTP transport at `POST http://<address>:<port>/mcp`. It is the
useful option for an agent that will not run on the monitored PC, or for a
headless box running `vitals serve`.

**It needs a token, even from `127.0.0.1`** — unlike the other read routes,
which a local caller may use without one. The reason is that an agent is a
program that acts on what it reads: making every agent a pairing you can list
and revoke is worth more than the convenience. Read scope is enough; there is
nothing here that could use more.

Pair a device in **Settings → Remote access** and use the token as a bearer:

```powershell
curl.exe -s -X POST "http://192.168.1.20:7331/mcp" `
  -H "Authorization: Bearer <43-character-token>" `
  -H "Content-Type: application/json" `
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"summary","arguments":{"top":5}}}'
```

An agent that takes a URL and headers is configured like this:

```jsonc
{
  "mcpServers": {
    "vitals-lan": {
      "type": "http",
      "url": "http://192.168.1.20:7331/mcp",
      "headers": { "Authorization": "Bearer <43-character-token>" },
    },
  },
}
```

A message without a token is refused with `403` and
`{"error":"the MCP endpoint needs a paired token, also on loopback"}`; a wrong
token over the network is `401`, the same answer every other route gives. A
notification — `notifications/initialized`, which every client sends — is
answered `202` with no body.

## The Windows on-device registry

Windows 11 ships an **on-device agent registry** (ODR): a per-machine list of
MCP servers that agents running _on that machine_ can discover, managed with a
command-line tool, `odr.exe`.

Vitals does not register itself there, and one command tells you whether it
could on your machine:

```powershell
Get-Command odr.exe
```

On the machine this was written on the answer is nothing at all: no `odr.exe`
on `PATH`, none in `C:\Windows\System32` or `C:\Windows`, none under
`Program Files` or `WindowsApps` at any depth, no app-execution alias for it in
`%LOCALAPPDATA%\Microsoft\WindowsApps`, and no installed package that would
carry one — while `HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Mcp` holds
the `Command = odr.exe`, `Args = mcp` registration that Windows itself ships.
Windows knows where the tool would live; the tool is not installed.

So there is nothing to register _with_ here, and Vitals writes nothing to the
registry of a machine it merely monitors. Registering an MCP server changes the
machine for every agent and every user on it, which makes it an installer's
job — not something a monitor does to the computer it is watching, and not
something this project would ship without being able to test it.

None of that costs anything today. The configuration snippets above are how
every MCP client finds a stdio server, on every version of Windows, whatever
the registry contains. When `Get-Command odr.exe` _does_ answer on a machine
you want to serve, adding the entry is a packaging change: the server it points
at is already here and already speaks the protocol.

## Troubleshooting

- **The agent says the server closed.** An agent that quits closes standard
  input, which ends `vitals mcp` normally. If it exits immediately, run
  `vitals mcp` yourself and read standard error: on a platform with no
  sampler and no running app, the CLI says so there.
- **`sensors` returns an error.** No sensor service is running (the optional
  sensors service in Settings), or the source is direct sampling. The message
  says which.
- **`history` is empty.** History recording is off, or this host has no store
  — the headless `vitals serve` keeps none. The `seconds` field in the reply
  tells you the span that was read, so an empty answer is never mistaken for
  an unasked one.
- **`processes` returns fewer than `count`.** By design: `count` is every
  process on the machine, and the list is the busiest `top` of them.
