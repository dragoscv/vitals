# Vitals in Home Assistant

Show your PC's CPU, memory, GPU, disk and network load on a Home Assistant
dashboard, and know when the machine is on — with nothing installed on the
Home Assistant side. Everything here uses integrations that ship with Home
Assistant: `rest`, `template`, and optionally `prometheus`.

## What you get

| Entity                                               | Reads                        | Notes                                              |
| ---------------------------------------------------- | ---------------------------- | -------------------------------------------------- |
| `sensor.vitals_cpu`                                  | total CPU load, %            |                                                    |
| `sensor.vitals_memory_used`                          | RAM in use, %                | used ÷ total                                       |
| `sensor.vitals_memory_used_gib`                      | RAM in use, GiB              |                                                    |
| `sensor.vitals_gpu`                                  | busiest GPU engine, %        | `unknown` when the GPU does not report it          |
| `sensor.vitals_cpu_temperature`                      | °C                           | `unknown` on most desktops without a sensor driver |
| `sensor.vitals_processes`                            | number of running processes  |                                                    |
| `sensor.vitals_uptime`                               | seconds since boot           | rendered as "2d 3h" by Home Assistant              |
| `sensor.vitals_busiest_disk_read` / `_write`         | MiB/s on the busiest volume  |                                                    |
| `sensor.vitals_busiest_network_download` / `_upload` | MiB/s on the busiest adapter | bytes, not bits                                    |
| `binary_sensor.vitals_pc_reachable`                  | the Vitals API answered      | `unavailable` while the PC is off                  |
| `binary_sensor.vitals_pc_online`                     | plain on/off for automations | folds `unavailable` into `off`                     |

A value Vitals cannot measure shows as **unknown**, never 0. That is
deliberate: a GPU that exposes no counters is not a GPU at 0 %, and a 0 on
your dashboard would be a lie. The package's templates are written to keep
that distinction — see the comments in the YAML before changing them.

## 1. Turn on remote access and get a token

On the PC, in Vitals:

1. **Settings → Remote access → turn it on.** Nothing listens on the network
   until you do this. Windows will ask once whether to allow Vitals through
   the firewall — say yes for private networks.
2. **Pair a device.** Vitals shows a QR code. Under it is the URL the code
   encodes, something like
   `http://192.168.1.20:7331/mobile.html#t=Kq9…`. The part after `#t=` is
   the token. Copy it now: it is shown **once**; afterwards the pairing list
   only shows its first eight characters.
3. The new pairing is **read-only** by default. That is the right scope for
   Home Assistant — it only ever reads.

Treat the token like a password for your PC's activity. It grants no control
over the machine, but it does reveal what is running on it.

## 2. Put the token and address in `secrets.yaml`

Add these to `<config>/secrets.yaml`:

```yaml
# The whole header value, including "Bearer ". Home Assistant cannot join a
# literal prefix to a secret, so the prefix lives here.
vitals_authorization: 'Bearer Kq9…paste-the-token-here'
vitals_url_snapshot: 'http://192.168.1.20:7331/api/v1/snapshot'
vitals_url_health: 'http://192.168.1.20:7331/api/v1/health'
```

`7331` is the default port; if you changed it in Settings, change it here.

### If your PC's IP address changes

Most home routers hand out addresses by DHCP, so the PC may not be
`192.168.1.20` forever. Two fixes, pick one:

- **Reserve the address in your router** (usually called "DHCP reservation"
  or "static lease"). Simplest and most reliable.
- **Use the `.local` name.** While remote access is on, Vitals advertises
  itself over mDNS as `_vitals._tcp`, under the PC's hostname — so
  `http://DESKTOP-ABC.local:7331/…` works from any client that resolves
  mDNS. Home Assistant OS does; Home Assistant in a plain Docker container
  usually does not unless it runs with `network_mode: host`. If the
  `.local` name fails from Home Assistant, fall back to the reservation.

## 3. Install the package

Copy [`home-assistant/vitals.yaml`](home-assistant/vitals.yaml) to
`<config>/packages/vitals.yaml`. If you have never used packages, add this to
`configuration.yaml`:

```yaml
homeassistant:
  packages: !include_dir_named packages
```

Then **Developer tools → YAML → Check configuration**, and restart Home
Assistant. Within ten seconds the entities above appear under **Settings →
Devices & services → Entities**, searchable by "vitals".

### About `scan_interval`

The package polls the snapshot every **5 seconds**. Vitals itself samples
once a second, but a dashboard cannot show that and the recorder database
would grow five times faster for nothing. Anything between 5 and 10 s is a
good choice; go lower only if you are driving an automation that needs it.
The health check runs every 30 s — its job is to notice the PC going to
sleep, not to chart anything.

## 4. A dashboard card

Gauges suit percentages. Add a **Manual** card and paste:

```yaml
type: horizontal-stack
cards:
  - type: gauge
    name: CPU
    entity: sensor.vitals_cpu
    min: 0
    max: 100
    severity:
      green: 0
      yellow: 60
      red: 85
  - type: gauge
    name: Memory
    entity: sensor.vitals_memory_used
    min: 0
    max: 100
    severity:
      green: 0
      yellow: 75
      red: 90
  - type: gauge
    name: GPU
    entity: sensor.vitals_gpu
    min: 0
    max: 100
```

For the rest, an **Entities** card is enough:

```yaml
type: entities
title: PC
entities:
  - binary_sensor.vitals_pc_online
  - sensor.vitals_uptime
  - sensor.vitals_processes
  - sensor.vitals_cpu_temperature
  - sensor.vitals_busiest_disk_read
  - sensor.vitals_busiest_disk_write
  - sensor.vitals_busiest_network_download
  - sensor.vitals_busiest_network_upload
```

Every sensor carries `state_class: measurement`, so clicking one opens a
history graph and the built-in **Statistics graph** card works on all of them.

## Alternative: the Prometheus route

If you already run Prometheus, Vitals exposes the same numbers at
`GET /metrics` (same bearer token) in Prometheus text format. Point a scrape
job at it:

```yaml
scrape_configs:
  - job_name: vitals
    scrape_interval: 5s
    authorization:
      type: Bearer
      credentials_file: /etc/prometheus/vitals.token
    static_configs:
      - targets: ['192.168.1.20:7331']
```

Metric names: `vitals_cpu_percent`, `vitals_cpu_kernel_percent`,
`vitals_cpu_core_percent{core}`, `vitals_cpu_temperature_celsius`,
`vitals_memory_bytes{state}`, `vitals_disk_bytes_per_second{disk,direction}`,
`vitals_disk_active_percent{disk}`, `vitals_disk_capacity_bytes`,
`vitals_network_bytes_per_second`, `vitals_gpu_percent`,
`vitals_gpu_memory_bytes`, `vitals_power_draw_watts`,
`vitals_battery_percent`, `vitals_process_count`, `vitals_thread_count`,
`vitals_uptime_seconds`, plus per-process `vitals_process_cpu_percent` and
`vitals_process_memory_bytes` for the twenty busiest. A reading that cannot
be measured is **omitted**, so `absent()` works and no fabricated zero
poisons an average.

Note that Home Assistant's own `prometheus` integration goes the other way —
it exports Home Assistant's state _to_ Prometheus. To get Vitals numbers into
Home Assistant from Prometheus you would query Prometheus with a `rest`
sensor again, which is more moving parts than reading Vitals directly. Use
this route when Prometheus and Grafana are already where you look at graphs.

## Why the templates are guarded

Two fields in the package can legitimately be `null` on the wire:
`gpus[0].utilization` and `cpu.temperature`. The templates check
`is not none` and return `none` otherwise, which Home Assistant displays as
`unknown`. Do not replace that with `| default(0)` or `| float(0)`: you
would turn "this machine cannot measure that" into "this machine is idle and
cold", and your history graph would say so forever.

The JSON paths the package reads are checked by a test in the Vitals
repository (`crates/vitals-server/tests/home_assistant.rs`) against a real
serialised frame. If a field is renamed on the PC side, that test fails —
the alternative was a dashboard that quietly reads `unknown` from the day of
the rename.

## Troubleshooting

| Symptom                                                      | Likely cause                                                                                                              |
| ------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------- |
| Everything `unavailable`, `vitals_pc_reachable` too          | Wrong address or port, PC asleep, or Windows firewall blocked Vitals. Open the health URL in a browser on another device. |
| `vitals_pc_reachable` is `on`, everything else `unavailable` | Wrong token. Check `secrets.yaml` has the `Bearer ` prefix and no trailing newline.                                       |
| All sensors `unknown` for the first seconds                  | Normal. Vitals answers `204 No Content` until its first sample.                                                           |
| `sensor.vitals_gpu` permanently `unknown`                    | The GPU exposes no engine counters to Vitals (virtual display, some laptops). Not a bug.                                  |
| `sensor.vitals_cpu_temperature` permanently `unknown`        | Most desktop boards need Vitals to run elevated, or a vendor sensor driver. Not a bug.                                    |
| Values lag a few seconds behind the Vitals window            | `scan_interval` is 5 s. Expected.                                                                                         |
