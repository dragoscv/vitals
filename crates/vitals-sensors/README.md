# vitals-sensors

Optional Windows service (`LocalSystem`) that reads CPU package temperature,
the hottest core and package power through the signed
[PawnIO](https://pawnio.eu) driver and serves them **read-only** on
`\\.\pipe\vitals-sensors`. The app only reads the pipe; it never opens the
driver. Decision: [ADR-0031](../../docs/adr/0031-cpu-sensors-service.md).
Ported from codai's `codai-sensors`, with RAPL package power added.

## Commands

| Command                                   | What it does                                                                                                                                                                                                                                                                                         |
| ----------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `service`                                 | Run under the SCM (service `vitals-sensors`).                                                                                                                                                                                                                                                        |
| `read`                                    | Elevated console debug: open PawnIO directly, print one line (two reads 1 s apart, so power is present). Exit 0 ok, 5 access denied, 1 other.                                                                                                                                                        |
| `install [--pawnio-setup <PawnIO_setup>]` | Elevated. Installs PawnIO 2.2.0 if missing (SHA-256-checked copy, `-install -silent`), copies itself to `%ProgramFiles%\Vitals Sensors\`, creates and starts the auto-start service, waits for the first reading. Log: `%ProgramData%\Vitals Sensors\install.log`. Exit 5 not elevated, 3 no PawnIO. |
| `uninstall`                               | Elevated. Stops and deletes the service, removes the install directory. Never removes PawnIO.                                                                                                                                                                                                        |
| `version`                                 | `vitals-sensors <version>`                                                                                                                                                                                                                                                                           |

The app does all of this from **Devices & sensors → CPU temperature and
power**, with one administrator prompt.

## Pipe contract (v1)

Connect, read one line; the server then disconnects. It never reads client
input. A background thread refreshes the reading once a second.

```json
{"v":1,"ok":true,"vendor":"intel","packageC":57.0,"hottestCoreC":61.0,"tjMaxC":100,"packageW":41.5,"source":"pawnio"}
{"v":1,"ok":true,"vendor":"amd","packageC":48.5,"hottestCoreC":null,"tjMaxC":null,"packageW":null,"source":"pawnio"}
{"v":1,"ok":false,"error":"..."}
```

A value that was not measured is `null`, never `0`. Rust clients use
`vitals_sensors::read_pipe(Duration)`. The pipe disconnects right after the
line, so a reader must treat a disconnect after data as the end of the
message (`read_line`), not as an error.

## Modules

`modules/IntelMSR.bin` and `modules/AMDFamily17.bin` are the signed
PawnIO.Modules 0.2.11 release (LGPL-2.1, `modules/COPYING`). They are
embedded in the exe and SHA-256-pinned in `src/lib.rs`.

- Intel: `TjMax` 0x1A2, package 0x1B1, hottest core 0x19C on every logical
  CPU, RAPL 0x606/0x611.
- AMD 17h–1Ah: `Tctl` from SMN 0x59800, RAPL 0xC0010299/0xC001029B.

Prover: `cargo run -p vitals-win --example prove_sensors_service`.
