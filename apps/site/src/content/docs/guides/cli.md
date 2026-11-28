---
title: Command line
description: Use Vitals from a terminal — a live top, one-shot process lists, JSON output and a headless server.
---

The `vitals` command reads the same numbers the window shows.

## Install

The CLI is **not** inside the installer, the Microsoft Store build or the package managers, but
every [GitHub release](https://github.com/dragoscv/vitals/releases/latest) ships it as a separate
zip beside the installer — `vitals-cli_<version>_x64.zip` and `_arm64.zip`, each holding
`vitals.exe`, the licence and the third-party notices. Download that, or build it from source with
[Rust](https://rustup.rs) installed:

```powershell
cargo install --git https://github.com/dragoscv/vitals vitals-cli
```

That puts `vitals.exe` in `%USERPROFILE%\.cargo\bin`.

## Live view

```powershell
vitals top
```

A live process list, like the app's, in the terminal, redrawn in place; press `q`, `Esc` or `Ctrl-C`
to quit. When its output is not a terminal — a pipe or a redirected file — it prints one snapshot and
exits 0 instead of drawing a screen, so `vitals top | head` and CI logs work. `--json` prints one
JSON object per tick until `Ctrl-C`.

## One-shot process list

```powershell
vitals ps --top 10
```

The ten busiest processes, printed once. Add `--json` to get machine-readable output for another
program:

```powershell
vitals ps --json
vitals ps --top 5 --json | ConvertFrom-Json
```

## About the machine

```powershell
vitals info
vitals report --duration 30
```

`info` describes the hardware and Windows version. `report` samples for the given number of
seconds and writes what happened to a JSON file (`vitals-report-<timestamp>.json` in the current
folder unless you pass `--output`).

## Sensors, history and alerts

```powershell
vitals sensors
vitals history --seconds 600
vitals alerts
```

`sensors` prints every temperature, fan, voltage and battery reading the machine can make, from the
same source the desktop's `/api/v1/sensors` route serves; a sensor that could not be read is absent
from the list rather than shown as a zero. `history` reads machine-wide samples from the desktop
app's history store, with `-s`, `--seconds <N>` setting how far back to look (default 300) — history
belongs to the desktop, so a direct sampler has none to give and exits 5. `alerts` lists the
conditions the alert engine has raised, each with its severity, the condition, the subject and the
alert's values.

## Widgets

```powershell
vitals widget --kind cpu
vitals widget --list
vitals widget --close w3
```

A widget is a window, so only the desktop app can draw one: every `widget` command attaches to the
running app, and there is no direct-sampler equivalent. `--kind` opens one showing `cpu`, `memory`,
`disk`, `network`, `thermal` or `overview`; `--list` prints the widgets that are open, each with the
id `--close` takes. An open also accepts `--size small|medium|large` (default `small`) and
`--layout stack|split|ring` (default `stack`; `ring` is drawn only for `cpu` and `memory`), and an
optional `--x`/`--y` position in pixels — without one the app cascades the widget to a free spot.
Exactly one of `--kind`, `--close` and `--list` is required. `--json` prints the action, the
resulting widget list and, for an open or close, the affected id.

When no app answers the exit code is 3; when an app answers and will not do it — a build older than
this command, or one with no widget handler — it is 6.

## Efficiency mode

```powershell
vitals eco 1234        # turn Windows 11 efficiency mode on for PID 1234
vitals eco 1234 --off  # restore normal scheduling
```

The PID is checked against a fresh sample first and refused if it is not there, so a recycled PID
is never acted on. The state read back afterwards is printed.

## Ending, pausing and reprioritising a process

```powershell
vitals kill 1234                    # end PID 1234
vitals suspend 1234                 # freeze it
vitals resume 1234                  # let it run again
vitals priority 1234 below-normal   # idle, below-normal, normal, above-normal, high, realtime
```

The same PID check applies. Processes Windows cannot do without are refused rather than ended or
frozen: the desktop app asks before touching them, and a script cannot be asked. A process that
belongs to another account needs an elevated prompt. Each command exits non-zero with the reason on
standard error when it is refused, so a script only has to check the exit code.

## Serving an AI agent

```powershell
vitals mcp
```

Speaks the Model Context Protocol on standard input and output — one JSON message per line — so an AI
agent can read this machine. It is read-only: nothing reachable here can end, freeze or
re-prioritise a process. See
[`docs/integrations/mcp.md`](https://github.com/dragoscv/vitals/blob/main/docs/integrations/mcp.md).

## Headless LAN server

```powershell
vitals serve
vitals serve --json
```

Starts the same [remote access](/guides/remote-access/) server without the desktop app, on port
**7331**. It generates a token and prints it **once**; copy it then, because it is not shown
again. The same rules apply as in the app: local network only, plain HTTP, read scope by default.
Press `Ctrl+C` to stop it.

With `--json` the server prints one object describing itself as the first line on standard output —
the port, the scope, the token, whether it generated one, the pairing URL and the pairing code — and
then keeps serving, so a script that let the server generate the token can still learn it:

```json
{
  "port": 7331,
  "scope": "read",
  "token": "…",
  "generated": true,
  "url": "http://192.168.1.20:7331/mobile.html#token=…",
  "pairingCode": "ABCDEF"
}
```

`url` is `null` when no LAN interface was found. `--mqtt <URL>` also publishes to an MQTT broker with
Home Assistant discovery, and `--mqtt-interval <N>` sets the seconds between publishes (clamped to
1..=3600). The token scope is always read-only: `--control` is refused with exit code 4, because a
headless server holds no pairing to grant it.

## Shell completions

```powershell
vitals completions powershell > vitals-completions.ps1
```

Prints a completion script for `bash`, `elvish`, `fish`, `powershell` or `zsh`; redirect it to a file
and have your shell source it.

## Where the numbers come from

When the desktop app is running, the CLI attaches to it — over a private named pipe first, then
the app's loopback API — and shows exactly what the app sees, so one machine runs one sampler. When
the app is not running, the CLI samples the machine itself. Which one happened is printed on
standard error, so standard output stays clean for `--json`.

- `--source app` insists on the running app and fails if it is not there.
- `--source local` always samples directly.
- The default, `auto`, tries the app first.

## Reference

Global options work with every command. `--json` is accepted on every command too: it prints the same
data the table shows, and a value that was never measured is `null` or omitted, never `0`.

| Option                        | Meaning                                                              |
| ----------------------------- | -------------------------------------------------------------------- |
| `--json`                      | Machine-readable JSON instead of a table.                            |
| `--source <auto\|app\|local>` | Where to read from. Default `auto`.                                  |
| `--attach <URL>`              | Attach to a local API at this address, e.g. `http://127.0.0.1:7330`. |
| `--no-attach`                 | Never attach; always sample directly. Same as `--source local`.      |
| `-h`, `--help`                | Help for the command.                                                |
| `-V`, `--version`             | Print the version.                                                   |

`--source`, `--attach` and `--no-attach` cannot be combined.

| Command                         | Options                                                                                                                                                                                                                                                  |
| ------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `vitals ps`                     | `-f`, `--filter <TEXT>`: only processes whose name contains it. `-n`, `--top <N>`: the N highest consumers (0, the default, means all).                                                                                                                  |
| `vitals top`                    | `-i`, `--interval <MS>`: refresh interval in milliseconds, default 1000.                                                                                                                                                                                 |
| `vitals info`                   | None.                                                                                                                                                                                                                                                    |
| `vitals eco <PID>`              | `--off`: restore normal scheduling instead of turning efficiency mode on.                                                                                                                                                                                |
| `vitals kill <PID>`             | None. Ends the process.                                                                                                                                                                                                                                  |
| `vitals suspend <PID>`          | None. Freezes the process until `vitals resume`.                                                                                                                                                                                                         |
| `vitals resume <PID>`           | None.                                                                                                                                                                                                                                                    |
| `vitals priority <PID> <LEVEL>` | `idle`, `below-normal`, `normal`, `above-normal`, `high` or `realtime`.                                                                                                                                                                                  |
| `vitals report`                 | `-d`, `--duration <SECONDS>`: how long to record, default 60. `-o`, `--output <PATH>`: where to write the JSON.                                                                                                                                          |
| `vitals sensors`                | None. Every sensor reading the machine can make.                                                                                                                                                                                                         |
| `vitals history`                | `-s`, `--seconds <N>`: how far back to look, default 300.                                                                                                                                                                                                |
| `vitals alerts`                 | None. What the alert engine has raised.                                                                                                                                                                                                                  |
| `vitals widget`                 | `--kind <cpu\|memory\|disk\|network\|thermal\|overview>`: open a widget. `--size <small\|medium\|large>`, `--layout <stack\|split\|ring>`, `--x <N>`, `--y <N>`: its shape and position. `--close <ID>`: close one. `--list`: the widgets that are open. |
| `vitals mcp`                    | None. Serve the machine over MCP on standard input and output (read-only).                                                                                                                                                                               |
| `vitals completions <SHELL>`    | `bash`, `elvish`, `fish`, `powershell` or `zsh`.                                                                                                                                                                                                         |
| `vitals serve`                  | `-p`, `--port <PORT>`: default 7331. `--token <TOKEN>`: use this token instead of generating one. `--json`: print the server object and keep serving. `--mqtt <URL>`, `--mqtt-interval <N>`: publish to MQTT. `--control` is refused (exit code 4).      |

`serve` never attaches to the desktop app, so a headless server does not go dark when the window
closes.

## Exit codes

| Code | Meaning                                                                                                                                                                         |
| ---- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 0    | Success.                                                                                                                                                                        |
| 1    | An error that is not one of the codes below.                                                                                                                                    |
| 2    | A usage error (bad arguments), reported by the argument parser.                                                                                                                 |
| 3    | Could not reach the app (no attach pipe, nothing at `--attach`), or this machine has no sampler.                                                                                |
| 4    | A control action was refused — read-only scope, a protected process, or a headless `serve --control`.                                                                           |
| 5    | The named thing was not found: a PID absent from the sample, or data the source does not keep.                                                                                  |
| 6    | The app is running and heard the request, but cannot carry it out — an older build that does not know the message, or one whose UI does not serve it over the transport in use. |

A script can branch on these without reading English off standard error, for example
`if ($LASTEXITCODE -eq 3) { 'is Vitals running?' }`.

## Colour

The CLI writes no ANSI colour when standard output is not a terminal, and none at all when the
`NO_COLOR` environment variable is set (to anything), following
[no-color.org](https://no-color.org), so a redirected run never collects escape bytes into a log
file.
