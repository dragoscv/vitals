---
title: Command line
description: Use Vitals from a terminal — a live top, one-shot process lists, JSON output and a headless server.
---

The `vitals` command reads the same numbers the window shows.

## Install

The CLI is **not** bundled with the installer, the Microsoft Store build or the package managers
yet. Until a release carries it, build it from source with [Rust](https://rustup.rs) installed:

```powershell
cargo install --git https://github.com/dragoscv/vitals vitals-cli
```

That puts `vitals.exe` in `%USERPROFILE%\.cargo\bin`.

## Live view

```powershell
vitals top
```

A live process list, like the app's, in the terminal. Press `q` to quit.

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

## Efficiency mode

```powershell
vitals eco 1234        # turn Windows 11 efficiency mode on for PID 1234
vitals eco 1234 --off  # restore normal scheduling
```

The PID is checked against a fresh sample first and refused if it is not there, so a recycled PID
is never acted on. The state read back afterwards is printed.

## Headless LAN server

```powershell
vitals serve
```

Starts the same [remote access](/guides/remote-access/) server without the desktop app, on port
**7331**. It generates a token and prints it **once**; copy it then, because it is not shown
again. The same rules apply as in the app: local network only, plain HTTP, read scope by default.
Press `Ctrl+C` to stop it.

## Where the numbers come from

When the desktop app is running, the CLI attaches to it — over a private named pipe first, then
the app's loopback API — and shows exactly what the app sees, so one machine runs one sampler. When
the app is not running, the CLI samples the machine itself. Which one happened is printed on
standard error, so standard output stays clean for `--json`.

- `--source app` insists on the running app and fails if it is not there.
- `--source local` always samples directly.
- The default, `auto`, tries the app first.

## Reference

Global options work with every command.

| Option                        | Meaning                                                              |
| ----------------------------- | -------------------------------------------------------------------- |
| `--json`                      | Machine-readable JSON instead of a table.                            |
| `--source <auto\|app\|local>` | Where to read from. Default `auto`.                                  |
| `--attach <URL>`              | Attach to a local API at this address, e.g. `http://127.0.0.1:7330`. |
| `--no-attach`                 | Never attach; always sample directly. Same as `--source local`.      |
| `-h`, `--help`                | Help for the command.                                                |
| `-V`, `--version`             | Print the version.                                                   |

`--source`, `--attach` and `--no-attach` cannot be combined.

| Command            | Options                                                                                                                                 |
| ------------------ | --------------------------------------------------------------------------------------------------------------------------------------- |
| `vitals ps`        | `-f`, `--filter <TEXT>`: only processes whose name contains it. `-n`, `--top <N>`: the N highest consumers (0, the default, means all). |
| `vitals top`       | `-i`, `--interval <MS>`: refresh interval in milliseconds, default 1000.                                                                |
| `vitals info`      | None.                                                                                                                                   |
| `vitals eco <PID>` | `--off`: restore normal scheduling instead of turning efficiency mode on.                                                               |
| `vitals report`    | `-d`, `--duration <SECONDS>`: how long to record, default 60. `-o`, `--output <PATH>`: where to write the JSON.                         |
| `vitals serve`     | `-p`, `--port <PORT>`: default 7331. `--token <TOKEN>`: use this token instead of generating one. `--control`: give it control scope.   |

`serve` never attaches to the desktop app, so a headless server does not go dark when the window
closes.
