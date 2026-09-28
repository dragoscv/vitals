---
title: Command line
description: Use Vitals from a terminal — a live top, one-shot process lists, JSON output and a headless server.
---

The `vitals` command ships with the app. It reads the same numbers the window shows.

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
seconds and summarises what happened.

## Headless LAN server

```powershell
vitals serve
```

Starts the same [remote access](/guides/remote-access/) server without the desktop app, on port
**7331**. It generates a token and prints it **once**; copy it then, because it is not shown
again. The same rules apply as in the app: local network only, plain HTTP, read scope by default.
Press `Ctrl+C` to stop it.

## Where the numbers come from

When the desktop app is running, the CLI attaches to it over a private named pipe and shows exactly
what the app sees, so one machine runs one sampler. When the app is not running, the CLI samples
the machine itself and says so on standard error.

- `--source app` insists on the running app and fails if it is not there.
- `--source local` always samples directly.
- The default, `auto`, tries the app first.
