# The `vitals` command line

`vitals` is the scriptable half of Vitals. It reads the same numbers the
window shows and prints them as a table or, with `--json`, as JSON a script
can parse. It does not need the desktop app: when the app is running it
attaches to it, otherwise it samples the machine itself.

## Install

The CLI ships as its own download, not inside the installer:

```
vitals-cli_<version>_<arch>.zip
```

from the [releases page](https://github.com/dragoscv/vitals/releases). It is a
plain zip, not an installer, containing:

- `vitals.exe` — the binary;
- `LICENSE`;
- `THIRD_PARTY_NOTICES.md`.

Extract it anywhere and put `vitals.exe` on your `PATH`, or call it by its full
path. There is nothing to install and nothing to uninstall.

## Where the numbers come from

By default the CLI attaches to a running Vitals — over its per-user named pipe
first, then its loopback API — and shows the same numbers the window does. If
no Vitals is running it samples the machine directly. Which one happened is
printed on stderr, so stdout stays clean for `--json | ConvertFrom-Json`.

| Flag             | Meaning                                                    |
| ---------------- | ---------------------------------------------------------- |
| `--source auto`  | Attach if there is an app, else sample directly (default). |
| `--source app`   | Attach to the running app, or fail (exit code 3).          |
| `--source local` | Always sample directly, even if the app is running.        |
| `--attach <url>` | Attach to a Vitals local API at this address.              |
| `--no-attach`    | Same as `--source local`.                                  |
| `--json`         | Emit machine-readable JSON instead of a human table.       |

`--json` is accepted on every subcommand. The data it prints is the same the
table shows; an unmeasured value is `null` (or omitted), never `0`.

## Subcommands

### `vitals ps`

List processes once. `--filter <text>` keeps names containing the text
(case-insensitive); `-n/--top <n>` limits to the `n` busiest by CPU, applied
after the filter.

### `vitals top`

A live summary for a human. When stdout is a terminal it enters a redrawing
alternate screen and quits on `q`, `Esc` or `Ctrl-C`. **When stdout is not a
terminal — a pipe or a redirected file — it prints one snapshot and exits 0**,
so `vitals top | head` and CI logs work. `--json` prints one JSON object per
tick until `Ctrl-C`. `-i/--interval <ms>` sets how often it refreshes, in
milliseconds (default 1000); when the CLI has attached to a running app the
interval is ignored, because the app decides the rate it publishes at.

### `vitals info`

Static host facts (name, OS, CPU, cores, memory, motherboard, BIOS) plus a
current system summary.

### `vitals sensors`

Every sensor reading the machine can make, from the same source the desktop's
`/api/v1/sensors` route serves. A sensor that could not be read is absent from
the list, never present as a zero.

### `vitals history`

Machine-wide samples from the desktop app's history store. `-s/--seconds <n>`
sets how far back to look (default 300). History belongs to the desktop, so a
direct sampler has none to give and exits 5.

### `vitals alerts`

The conditions the alert engine has raised, with severity, the condition, the
subject and the alert's values.

### Process actions

These take a PID as `vitals ps` shows it. The PID is resolved to a full key
(PID plus start time) from a fresh sample, so a recycled PID is never hit;
a PID that is not running exits 5.

| Command                         | Effect                                                                                         |
| ------------------------------- | ---------------------------------------------------------------------------------------------- |
| `vitals eco <pid> [--off]`      | Windows 11 efficiency mode on (default) or off.                                                |
| `vitals kill <pid>`             | End the process.                                                                               |
| `vitals suspend <pid>`          | Freeze the process.                                                                            |
| `vitals resume <pid>`           | Let a suspended process run.                                                                   |
| `vitals priority <pid> <level>` | Set scheduling priority: `idle`, `below-normal`, `normal`, `above-normal`, `high`, `realtime`. |

Critical and protected processes are refused (exit code 4): a script cannot see
the confirmation the desktop shows for them.

### `vitals report`

Capture a diagnostic report for sharing. `-d/--duration <seconds>` records
first (default 60); `-o/--output <path>` sets the file.

### `vitals mcp`

Serve this machine to an AI agent over the Model Context Protocol: one JSON
message per line on stdin, answers on stdout. Read-only — nothing here can end,
freeze or re-prioritise a process. See `docs/integrations/mcp.md`.

### `vitals serve`

Serve metrics to the LAN without the desktop app (headless):

| Flag                  | Meaning                                                                   |
| --------------------- | ------------------------------------------------------------------------- |
| `-p/--port <n>`       | Port to bind on every interface (default 7331).                           |
| `--token <secret>`    | Bearer token clients must present; generated and printed once if omitted. |
| `--mqtt <url>`        | Publish to an MQTT broker with Home Assistant discovery.                  |
| `--mqtt-interval <n>` | Seconds between MQTT publishes (clamped to 1..=3600).                     |
| `--json`              | Print one JSON object describing the server to stdout, then keep serving. |

Token scope is always **read-only**. `--control` is refused (exit code 4): a
headless server cannot act on a process, and pairing a phone with the desktop
app is what grants that.

With `--json`, the first line on stdout is a single JSON object:

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

`url` is `null` when no LAN interface was found. `token` is included so a
script that let the server generate one can still learn it.

### `vitals completions <shell>`

Print a shell completion script: `bash`, `elvish`, `fish`, `powershell` or
`zsh`. For example:

```powershell
vitals completions powershell > vitals-completions.ps1
```

## Exit codes

| Code | Meaning                                                                                                                      |
| ---- | ---------------------------------------------------------------------------------------------------------------------------- |
| 0    | Success.                                                                                                                     |
| 1    | An error that is not one of the codes below.                                                                                 |
| 2    | A usage error (bad arguments), reported by the argument parser.                                                              |
| 3    | Could not reach the app (no attach pipe, nothing at `--attach`), or this machine has no sampler.                             |
| 4    | A control action was refused — read-only scope, a protected process, or a headless `serve --control`.                        |
| 5    | The named thing was not found: a PID absent from the sample, or data the source does not keep (history on a direct sampler). |

A script driving `vitals` can branch on these without reading English off
stderr, for example `if ($LASTEXITCODE -eq 3) { 'is Vitals running?' }`.

## Colour

The CLI writes no ANSI escape bytes when stdout is not a terminal, and none at
all when the `NO_COLOR` environment variable is set (to anything), following
[no-color.org](https://no-color.org). A redirected run never collects escapes
into a log file.
