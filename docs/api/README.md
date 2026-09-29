# Vitals local API — integration guide

The full contract is in [`openapi.yaml`](./openapi.yaml). This page is the
short version: how to get a token, the exact commands, and what to expect.

Everything here is **off by default**. Nothing listens until you turn on
Remote access in the desktop app, and it listens on your LAN only, over plain
HTTP. Read the `info.description` in the OpenAPI document for the threat model
before exposing it any further than that.

The one exception is the **local API** below, which scripts on the same
machine can use and which is reachable only from the machine itself.

## Attach pipe (CLI only, local only)

The `vitals` CLI does not normally use HTTP at all. While the desktop app runs
it listens on a per-user **named pipe** — `\\.\pipe\vitals-<username>` on
Windows, a Unix socket of the same name elsewhere — and `vitals top`, `ps`,
`info` and `report` read the app's frames from it, so one machine runs one
sampler. It speaks newline-delimited JSON:

```
→ {"op":"hello"}      ← {"status":"ok","version":"0.1.0","modelVersion":1}
→ {"op":"snapshot"}   ← one Frame (a complete keyframe), then the app hangs up
→ {"op":"subscribe"}  ← a complete keyframe, then every frame as it is sampled
```

It is **not a network surface**: a named pipe has no address, is created with
the calling user's default security descriptor, and cannot be reached from
another machine or another user's session. That is why it needs no token and
never appears in Settings. It is read-only — the three requests above are the
whole protocol; process actions go through the local API or the platform.
The frames are the same `Frame` objects the LAN stream carries.

`vitals --source app` fails rather than falling back when the app is not
running; `--source local` never attaches; the default `auto` tries the pipe,
then the local API below, then samples directly and says so on stderr.

## Local API (loopback)

While the desktop app runs it also serves the same routes on
`http://127.0.0.1:7330`, for scripts and the `vitals` CLI on the same
machine. Two things differ from the LAN server:

- **No token is needed from `127.0.0.1`, and it is read-only.** Requiring a
  token would make `vitals ps` unusable until you had paired with your own
  computer, so loopback callers are granted **read** scope without one.
  `POST /api/v1/control` from loopback without a token is refused with 403:
  loopback is shared by every account on the machine, so a tokenless control
  grant would let another user end your processes. The `vitals` CLI acts on
  local processes through Windows directly instead. The LAN server never
  grants anything tokenless: the decision is made on the connecting address,
  so a request over the network still needs a token.
- **It is not on the network.** The listener is bound to `127.0.0.1` only;
  there is no firewall prompt, no mDNS announcement and nothing in Settings.

If 7330 is already taken the app falls back to a port the OS chooses. Read the
**discovery file** rather than assuming the port:

```
%LOCALAPPDATA%\Vitals\local-api.json
```

```json
{ "port": 7330, "pid": 12345, "version": "0.1.0" }
```

The file is written when the app starts and removed when it exits. After a
crash it may survive; check that `pid` is still alive before trusting `port`.

```powershell
$d = Get-Content "$env:LOCALAPPDATA\Vitals\local-api.json" | ConvertFrom-Json
curl.exe -s "http://127.0.0.1:$($d.port)/api/v1/snapshot"
```

## Getting a token

1. In Vitals, open **Settings → Remote access** and turn it on. Windows will
   ask about the firewall the first time; allow it on private networks or the
   server starts and nothing can reach it.
2. Click **Pair a device**. Choose the scope: **read** (the default; sees
   everything, changes nothing) or **control** (can also end, suspend, resume
   and re-prioritise processes — never give this to a device you would not
   hand your keyboard to).
3. A QR code appears. It encodes a URL of this shape:

   ```
   http://192.168.1.20:7331/mobile.html#t=<43-character-token>
   ```

   The token is the part after `#t=`. It is in the URL **fragment**, so a
   browser never sends it to the server; the mobile page reads it from
   `location.hash`. For a script, scan the QR with anything that shows the raw
   text, or use the **copy** button next to it.

4. The token is shown **once**. Afterwards Settings shows only its first eight
   characters. Lose it and you revoke that pairing and make a new one.

### Without a camera: the six-digit code

A TV cannot scan the QR code. Under **Pair a TV** the desktop shows six digits
for five minutes (same read/control switch as the QR). The device finds the PC
by browsing `_vitals._tcp.local.` over mDNS and exchanges the digits for an
ordinary token at the one other route that needs no token:

```powershell
curl.exe -s -X POST -H "Content-Type: application/json" `
  -d '{"code":"482913","label":"Living room TV"}' "$V/api/v1/pair"
# {"token":"<43-character-token>","scope":"read"}
```

The code works once, and five wrong codes in total — from anyone — burn it.
Every refusal is the same `403 {"error":"invalid pairing code"}`, whether the
code is wrong, expired, burnt or was never shown. A body that is not
`{"code":"<six digits>"}` is `400` and costs no attempt. `vitals serve` prints
a code at start-up beside its token.

Below, `$T` is your token and `$V` is the base URL. In PowerShell:

```powershell
$V = 'http://192.168.1.20:7331'
$T = 'paste-the-token-here'
```

Use `curl.exe`, not `curl` — in Windows PowerShell `curl` is an alias for
`Invoke-WebRequest` and the flags below will not work.

## Quick start

**Is anything there?** The only call that needs no token. Use it to tell
"wrong address" from "wrong token", because every other route returns the
same `401` whether the token is missing or merely unknown — that is deliberate.

```powershell
curl.exe -s "$V/api/v1/health"
# {"ok":true,"version":"0.1.0","modelVersion":1}
```

**The current picture** — the latest keyframe: every system reading and every
process. `204 No Content` (empty body) means the sampler has not ticked yet;
wait a second and try again.

```powershell
curl.exe -s -H "Authorization: Bearer $T" "$V/api/v1/snapshot"
```

**Static facts** about the machine — hostname, OS, CPU model, core topology:

```powershell
curl.exe -s -H "Authorization: Bearer $T" "$V/api/v1/host"
```

**A small summary** — every system reading plus the five busiest processes,
about 3 KB instead of a keyframe's 250 KB. What the watch and widgets use.
`?top=N` asks for up to 25.

```powershell
curl.exe -s -H "Authorization: Bearer $T" "$V/api/v1/summary?top=5"
```

**History** from the desktop's store, oldest first. `?seconds=` defaults to
an hour and is clamped to a week. `[]` means history is off.

```powershell
curl.exe -s -H "Authorization: Bearer $T" "$V/api/v1/history?seconds=86400"
```

**Sensors** — every temperature, fan, power and voltage reading the Devices
screen shows, cached for five seconds on the PC.

```powershell
curl.exe -s -H "Authorization: Bearer $T" "$V/api/v1/sensors"
```

Polling `/summary` or `/sensors`, or holding a stream open, keeps the desktop
sampling at least once a second for 45 seconds, even with its window hidden.

**Live frames as server-sent events.** `-N` turns off curl's output buffering
so you see each frame as it arrives. The first event is the current keyframe;
the rest are usually deltas (`"kind":"delta"`) carrying only the processes
that changed plus the PIDs that exited. Apply `exited` before `changed`.

```powershell
curl.exe -s -N -H "Authorization: Bearer $T" "$V/api/v1/stream"
```

An `EventSource` in a browser cannot set headers, so there the token goes in
the query string instead: `$V/api/v1/stream?token=$T`. Prefer the header
anywhere you can.

**End a process.** Needs a **control** token. Copy the whole `key` object from
a process in a frame — both `pid` and `startTime`. The start time is what stops
you killing whatever recycled the PID between you reading it and you acting on
it; the server will not accept a bare PID.

```powershell
curl.exe -s -o NUL -w "%{http_code}`n" -X POST `
  -H "Authorization: Bearer $T" -H "Content-Type: application/json" `
  -d '{"action":"terminate","key":{"pid":4242,"startTime":133724800000000000}}' `
  "$V/api/v1/control"
```

| Status | Meaning                                                                                                             |
| ------ | ------------------------------------------------------------------------------------------------------------------- |
| `204`  | Done.                                                                                                               |
| `403`  | `{"kind":"forbidden"}` — your token is read-only. `{"kind":"access-denied"}` — Windows refused (protected process). |
| `404`  | `{"kind":"not-found"}` — the process is gone, or the PID was recycled and the start time no longer matches.         |
| `501`  | `{"kind":"unsupported", …}` — this host cannot do that (no controller, unknown priority, no efficiency mode).       |
| `500`  | `{"kind":"internal", …}` — something else went wrong; the message says what.                                        |

Other actions: `suspend`, `resume`, `set-priority` with an extra
`"priority"` of `idle`, `below-normal`, `normal`, `above-normal`, `high` or
`realtime`, and `set-efficiency-mode` with a required boolean `"enabled"`
(Windows 11 efficiency mode — the leaf icon in Task Manager; `false` restores
normal scheduling). The CLI wraps the last one as `vitals eco <pid> [--off]`.

## Reading the JSON

**A field that is `null` was not measured. It is never zero-when-unknown.**
A GPU whose driver hides VRAM has `"memoryUsed": null`, a process whose
network traffic is not being traced has `"netRx": null`, a desktop has
`"battery": null`. Show a dash, skip the point on a chart, leave it out of an
average — do not coerce it to `0`. The desktop renders every one of these as
an em dash, and a client that renders `0` is telling the user something the
machine never said.

Rates (`rx`, `diskRead`, …) are already per second, divided by the frame's
real `elapsedMs`, not the nominal interval. Check `health.modelVersion` before
parsing frames if you cache a parser; a server with a shape you do not know
should be refused, not half-rendered.

## Prometheus

`GET /metrics` serves the text exposition format. A metric that cannot be
measured is **omitted**, never `0`, so use `absent()` in alerts rather than
`== 0`. Add to `prometheus.yml`:

```yaml
scrape_configs:
  - job_name: vitals
    scrape_interval: 5s
    static_configs:
      - targets: ['192.168.1.20:7331']
    # A read-scope token is enough. Keep it out of the config file:
    bearer_token_file: /etc/prometheus/vitals.token
```

Series you will see (all gauges):

| Metric                            | Labels                 | Notes                                                    |
| --------------------------------- | ---------------------- | -------------------------------------------------------- |
| `vitals_cpu_percent`              |                        | Whole machine.                                           |
| `vitals_cpu_kernel_percent`       |                        | Kernel-mode share.                                       |
| `vitals_cpu_core_percent`         | `core`                 | Per logical processor.                                   |
| `vitals_cpu_temperature_celsius`  |                        | Only with the optional sensors service (ADR-0034).       |
| `vitals_cpu_power_watts`          |                        | CPU package power (RAPL); same service.                  |
| `vitals_fan_rpm`                  | `fan`                  | Motherboard fan headers (Super-I/O); same service.       |
| `vitals_memory_bytes`             | `state`                | `total`, `used`, `available`, `cached`.                  |
| `vitals_disk_bytes_per_second`    | `disk`, `direction`    | `read` / `write`.                                        |
| `vitals_disk_active_percent`      | `disk`                 | Share of time with IO outstanding.                       |
| `vitals_disk_capacity_bytes`      | `disk`, `state`        | `total` / `free`.                                        |
| `vitals_network_bytes_per_second` | `adapter`, `direction` | `rx` / `tx`.                                             |
| `vitals_gpu_percent`              | `gpu`                  | Busiest engine. Absent for GPUs with no engine counters. |
| `vitals_gpu_memory_bytes`         | `gpu`                  | Absent where the driver does not report VRAM.            |
| `vitals_power_draw_watts`         |                        | Whole system; laptops and some desktops.                 |
| `vitals_battery_percent`          |                        | Absent on machines without a battery.                    |
| `vitals_process_count`            |                        |                                                          |
| `vitals_thread_count`             |                        |                                                          |
| `vitals_uptime_seconds`           |                        |                                                          |
| `vitals_process_cpu_percent`      | `name`, `pid`          | Top 20 processes by CPU, then private memory.            |
| `vitals_process_memory_bytes`     | `name`, `pid`          | Private bytes, same top 20.                              |

Per-process series are capped at twenty because one series per process on a
600-process machine is 1200 series a second, and the tail is noise.

## WebSocket

`GET /api/v1/ws` upgrades to a socket that sends every `Frame` as a JSON text
message and accepts `ControlRequest` JSON in the other direction, replying
`{"ok":true}` or `{"ok":false,"error":{"kind":…}}` once per request. Browsers
pass the token as `?token=`. The typed client in `packages/client` wraps this
with reconnection; read `packages/client/src/client.ts` for a working
implementation.

## Home Assistant

See [`../integrations/home-assistant.md`](../integrations/home-assistant.md)
for REST and command-line sensor examples against this API.

## Keeping this in step with the code

`crates/vitals-server/tests/openapi.rs` fails if a route exists in
`router.rs` and not in `openapi.yaml`, or the other way round, or if a metric
name in `prometheus.rs` is missing from the table above. Change the server,
change this.
