# 0031 — An optional SYSTEM service reads CPU temperature and package power through PawnIO

- Status: Accepted
- Date: 2026-09-28
- Tracker: S12-31
- Supersedes, in part: [0012](0012-helper-stays-stubbed.md) (for sensors only;
  `apps/helper` itself stays a stub)

## Context

CPU package temperature, per-core temperature and package power live in
model-specific registers (`IA32_PACKAGE_THERM_STATUS` 0x1B1,
`IA32_THERM_STATUS` 0x19C, `MSR_PKG_ENERGY_STATUS` 0x611, AMD SMN
`THM_TCON_CUR_TMP`). `RDMSR` is ring 0, so these were permanent "gaps" on
Devices & sensors. Every tool that shows them ships a kernel driver.
`WinRing0`, the common one, is on Microsoft's vulnerable-driver blocklist
because it hands any caller arbitrary MSR and port access.

`docs/distribution.md` rules out paid driver-signing infrastructure. codai
desktop solved the same problem on 2026-09-28 with the signed third-party
**PawnIO** driver: it runs signed, sandboxed modules that expose only the
registers they declare, not raw ring-0 access. codai reads it from a small
`LocalSystem` service that serves read-only JSON on a named pipe. The owner
asked for Vitals to do the same ("vezi cum a fost implementat în codai și
aplică la fel").

## Decision

- New crate **`crates/vitals-sensors`**, a port of codai's `codai-sensors`:
  - The **library** is the pipe contract (`Reading`, v1), register decoding,
    pins, and a std-only client. The app links only this part.
  - The **binary** `vitals-sensors.exe` has the commands `service`, `read`,
    `install`, `uninstall` and `version`. It owns the driver.
- **Package power added** beyond codai: RAPL energy counters, 32-bit wrap-safe.
  A one-second refresher thread (codai reads per client) gives every reading
  the same interval.
- The PawnIO.Modules 0.2.11 blobs are **embedded** (`include_bytes!`) and
  checked against SHA-256 pins before loading. There is no module file on
  disk to swap.
- When PawnIO is missing, `PawnIO_setup.exe` **2.2.0** is downloaded from
  its GitHub release and checked against a pinned SHA-256 twice: in the app,
  before the UAC prompt, and by the elevated helper, on its copy in
  `%ProgramFiles%` (the file that runs is the file that was hashed).
- Install and removal are user-initiated from Devices & sensors, behind **one
  UAC prompt**, and there is **no LAN equivalent**: a paired phone cannot
  install a SYSTEM service. Uninstall never removes PawnIO, which other tools
  share.
- The pipe is outbound-only and rejects remote clients. Its DACL gives
  SYSTEM/Administrators full access and interactive/authenticated users read
  access. `FILE_FLAG_FIRST_PIPE_INSTANCE` prevents name squatting.
- The sampler reads the pipe every tick. That costs about 0.3 ms, and when
  the service is absent it backs off for 10 s. `cpu.temperature` and
  `cpu.power` in the frame, Prometheus `vitals_cpu_power_watts`, and the
  Home Assistant package all come from it. Values out of physical range are
  dropped, not clamped.

## Consequences

- Vitals now ships code that runs as SYSTEM, but only after the user installs
  it, and it can be removed from the same screen. It can only read, and its
  whole surface is five commands and one output line. SECURITY.md describes it.
- The installer carries a 400 KB helper, staged by `scripts/bundle-sensors.ps1`
  in `beforeBuildCommand` from the same commit as the app.
- On ARM64 and unsupported CPUs (AMD before family 17h) the service installs
  and reports its reason; the UI shows it and offers removal.
- "CPU core temperature" and "CPU package power" gaps close only while
  measured. Board, VRM, fan RPM and rail voltages remain (Super-I/O, out of
  PawnIO's module set).
- A pipe-reader bug found by the prover — std reports a disconnect after the
  line as OS error 233 — is fixed in `vitals_sensors::read_line`. codai's
  `read_pipe` has the same shape and should be checked for the same bug.

## Addendum 2026-09-28 (S12-32): board fan speeds

The same service now reads motherboard fan tachometers from the Super-I/O
chip, through the signed PawnIO.Modules 0.2.11 **`LpcIO`** module (embedded,
SHA-256-pinned like the CPU modules; release zip digest checked against the
GitHub release before vendoring).

- `LpcIO` only allows the chip's config port pair and the base addresses it
  discovers itself; the PCI config ports are blocklisted by the module. The
  service writes only protocol bytes — config-mode entry/exit, logical-device
  and bank select, register _indexes_ — and, on Nuvoton, the one lock bit
  the Linux driver also clears. No fan, PWM or voltage register is written.
- Every Super-I/O transaction holds the global `Access_ISABUS.HTP.Method`
  mutex, which LibreHardwareMonitor, FanControl and HWiNFO use, so concurrent
  tools do not interleave each other's multi-write sequences.
- Supported: ITE chips with 16-bit fan counters (IT8613E…IT8696E, IT8792E,
  IT87952E) and Nuvoton NCT6791D–NCT6799D. Refused, reported as "no
  supported chip": 8-bit-divisor ITE parts and MSI's NCT668x EC-space parts.
  Register maps from Linux `it87.c`/`nct6775.c` and LibreHardwareMonitor.
- A header with no tachometer signal is omitted; a fan the chip reports as
  stopped is 0 RPM. Fans reach the frame as `SystemMetrics.fans`
  (`#[serde(default)]`, so older frames load), Prometheus `vitals_fan_rpm`,
  the Thermals panel, and the Devices readings table.
- Header names are the chip's numbering (`Fan 1`…), not the board's
  silkscreen (`CPU_FAN`, `SYS_FAN2`): the chip cannot know the mapping.
