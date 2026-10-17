# Microsoft Store listing — English (en-us)

Paste each block into Partner Center > Store listings > English (United States).
Limits are Partner Center's, checked October 2026 (sources and character counts
in `docs/store-research-2026-10.md`). Every claim below exists in the Store
build: the self-updater, the Task Manager replacement, the sensors service and
the CLI are not in the MSIX, so they are not mentioned here.

## Product name

Vitals: System Monitor

(Store ID 9P95RK5VMGPJ — https://apps.microsoft.com/detail/9P95RK5VMGPJ. "Vitals" alone is taken in the Store.)

## Short description (≤ 1000 characters; only the first 270 show in some views)

See what your computer is actually doing, and why it is slow. Processes with real names and icons, temperatures, drive health, storage clean-up and a lag watchdog. Free, open source and private.

## Description (≤ 10 000 characters)

Is your PC slow? Vitals tells you why, in plain words.

Press one button and Vitals looks at what is using your processor, memory, disk and network right now, and names the program responsible — "Chrome is using most of your memory", not a wall of numbers.

PROCESSES, MADE CLEARER
• Real app names and each program's own icon, grouped and sorted by what matters
• Totals above every column, so you see how busy the whole machine is at a glance
• Power usage and power trend, estimated from activity — Windows does not report energy per program, and Vitals says so
• Right-click to end a program, change its priority, bring its window to the front or open its folder
• Ending or suspending a program states what will happen first, and anything Windows depends on asks before it is touched

PERFORMANCE AND HARDWARE
• Live graphs for the processor, memory, graphics card, every drive and every network adapter, each kind in its own colour
• CPU speed, and temperatures for every drive, NVIDIA graphics cards and the processor where Windows reports it
• SSD health: how much of the drive's rated life is left, with hours on and data written, where the drive reports them
• Optional CPU load on the taskbar button while the window is open
• Hardware inventory, a Device Manager view, battery and power state

STORAGE
• Scan a drive and explore it as a map; find the largest folders and files
• Put what you no longer need in review, then send it to the Recycle Bin in one step, from where it can be restored
• Free Windows' own space with Windows' own tools (Disk Cleanup, DISM), and see how much was actually freed
• Developer clean-up: build output, unused git worktrees, package caches and Docker leftovers, removed only after you confirm
• When a file will not delete, Vitals names the program holding it

AND MORE
• Startup apps and services you can switch off, start or stop, with Microsoft's own entries hidden until you ask for them
• A lag watchdog that notices when Windows stops responding, names the program behind it and offers to end it, lower its priority or restart a frozen Explorer. It never ends anything on its own
• Alerts, a configurable dashboard, light and dark themes and ten accent colours
• An optional view of your PC from your phone or TV on your home network, plus Prometheus and Home Assistant endpoints — all off by default

Honest numbers: when something cannot be measured, Vitals shows a dash, never a made-up zero.

Private by design: no accounts, no ads, no telemetry, no analytics. Nothing leaves your computer unless you turn on remote access yourself.

Free and open source (MIT). Available in English and Romanian.

## What's new in this version (≤ 1500 characters)

What's new in 1.0.3

• A health strip across the Dashboard answers "why is my PC slow?" in one line
• A first-run card explains Vitals; the freeze watchdog is now off until you turn it on
• Windows Contrast themes, better text contrast, keyboard and screen reader support in the process list
• Processes fully translated into Romanian; numbers follow your language
• Uses less power when hidden: the sampler sleeps when nothing is watching
• A native crash leaves a local crash dump and Vitals restarts to the tray. Nothing is sent anywhere
• Fixed: uninstalling removes the sensors service; a stalled network drive no longer freezes the readings

New in 1.0, the first stable release

• Program icons in the process list, including services and other accounts' processes
• Totals above the CPU, memory, disk, network and GPU columns
• Power usage and power trend columns, plus Type, Working set, Disk read, Disk write and Parent PID
• Bring to front: right-click an app to restore and raise its window
• Temperatures for the CPU, NVIDIA graphics cards and every drive, and SSD health with hours on and data written
• CPU speed, and optional CPU load on the taskbar button

## Features (up to 20, ≤ 200 characters each)

- A plain-language answer to "why is my PC slow?" from live readings
- Processes with real app names, program icons and right-click actions
- Totals for the whole machine above every process column
- Power usage and power trend per program, estimated from activity
- Bring any app's window to the front from the process list
- Live graphs for CPU, memory, GPU, every drive and network adapter
- Temperatures for the CPU, NVIDIA graphics cards and every drive
- SSD health: rated life left, hours on and data written
- CPU speed and an optional CPU gauge on the taskbar button
- Storage map with the largest folders and files
- Clean-up you review first, sent to the Recycle Bin so it can be restored
- Developer clean-up for build output, caches, worktrees and Docker leftovers
- Startup apps and services you can switch off, start or stop
- Lag watchdog that names a frozen program and can restart Explorer
- Hardware inventory and a Device Manager view
- Alerts and a configurable dashboard with light and dark themes
- Optional phone and TV view of your PC on your home network
- Prometheus and Home Assistant endpoints, off by default
- No accounts, no ads, no telemetry
- Free and open source (MIT), in English and Romanian

## Keywords (up to 7, ≤ 40 characters each, ≤ 21 words in total)

task manager, system monitor, slow pc, cpu temperature, disk cleanup, ssd health, startup apps

## Screenshots (1920 × 1080 PNG, caption ≤ 200 characters each)

Upload in this order from `screenshots/`. Dark first; the light set follows so
people on a light Windows theme see themselves too. Real captures of the
release build on a clean machine (`scripts/screenshots/run.ps1`).

| File                        | Caption                                                                                                                         |
| --------------------------- | ------------------------------------------------------------------------------------------------------------------------------- |
| `01-dashboard-dark.png`     | The dashboard: one line tells you whether anything needs your attention, above live processor, memory, disk and network charts. |
| `02-diagnosis-dark.png`     | "Why is my PC slow?" looks at everything at once and answers in plain words.                                                    |
| `03-processes-dark.png`     | Every program with its own icon, and totals for the whole machine above each column.                                            |
| `04-performance-dark.png`   | Live graphs for the processor, memory, graphics card, every drive and network adapter.                                          |
| `05-storage-dark.png`       | See what fills a drive as a map, then clean up safely through the Recycle Bin.                                                  |
| `06-remote-access-dark.png` | Optional: pair your phone with a QR code and watch this PC on your home network.                                                |
| `01-dashboard-light.png`    | The same dashboard in the light theme.                                                                                          |
| `02-diagnosis-light.png`    | A plain-language verdict, in the light theme.                                                                                   |
| `03-processes-light.png`    | Processes with icons and totals, in the light theme.                                                                            |
| `04-performance-light.png`  | Performance graphs, in the light theme.                                                                                         |

Partner Center allows ten desktop screenshots; the light storage and remote
access captures are in the folder for the website but not uploaded.

## Category

Utilities & tools > System tools (fallback: Utilities & tools)

## Privacy policy URL

https://vitals.dragoscatalin.ro/privacy/

## Website

https://vitals.dragoscatalin.ro

## Support contact

dragoscv12@gmail.com

## Copyright and trademark info

© 2026 Dragos Catalin Vladulescu and Vitals contributors. MIT licence.

## Additional licence terms

https://github.com/dragoscv/vitals/blob/main/LICENSE

## Restricted capability justification (runFullTrust)

Vitals is a Win32 system monitor and task manager built with Tauri. It needs full trust to read process, performance, drive and hardware information through Win32 APIs and performance counters; to end, suspend or re-prioritise a process and bring its window to the front when the user asks; to scan drives and move files the user selected to the Recycle Bin; to start Windows' own maintenance tools (Disk Cleanup, DISM, powercfg) through a UAC prompt the user approves; and to run its lag watchdog, a second executable in the package, through the windows.startupTask extension.

## Notes for certification

Vitals runs as a normal desktop app. No account or sign-in is needed. Remote access (a LAN server for the phone and TV views) is off by default and only starts when the user turns it on in Settings > Remote access; while it is off, only a loopback-only API on 127.0.0.1 is open. The self-updater, the Task Manager replacement and the optional sensors service (driver) are disabled in the Store build. Nothing changes a Windows setting without the user's action: actions that need administrator rights ask through UAC, once, for that action only, and the irreversible ones (emptying the Recycle Bin, removing the previous Windows installation, turning hibernation off) need an extra tick first. The lag watchdog only offers actions in a notification; it never ends a process on its own and can be turned off in Settings > Watchdog. CPU load on the taskbar button can be turned off in Settings.
