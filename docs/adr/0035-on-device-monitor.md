# ADR-0035: The phone and the watch monitor themselves

- Status: accepted
- Date: 2026-09-29
- Extends: ADR-0033

## Context

ADR-0033 built the Android and Wear OS apps as remotes for a PC. The owner
then asked for the opposite as the main feature: the phone app is first a
resource monitor for the phone it runs on, with the PC a bonus, and the watch
the same for the watch. "Everything the desktop does, adapted to the OS, not
just remote data."

Android is not Windows. Measured as the app's own uid on the S25 Ultra
(Android 16), 2026-09-28:

| Readable                                    | Refused                                    |
| ------------------------------------------- | ------------------------------------------ |
| `/proc/meminfo`                             | `/proc/stat`, `/proc/loadavg`              |
| cpufreq `scaling_cur_freq`, `time_in_state` | `/proc/net/*`, `/proc/diskstats`, `vmstat` |
| ~60 thermal zones (S25), none on the A51    | `/proc/<pid>/stat` of any other app        |
| kgsl `gpubusy`, `/sys/kernel/gpu/*`         | battery sysfs                              |

So true CPU utilisation, per-process CPU and disk I/O are out of reach for any
app on current Android, and a monitor that pretends otherwise lies.

## Decision

1. **Official APIs only.** No root, no Shizuku, no ADB-granted permissions.
   Two special accesses the user grants in Settings, each unlocking a named
   set of readings, and the app says which: usage access (per-app screen time,
   data and storage; storage categories) and all-files access (the folder scan
   and file cleanup). Without them those readings are the em dash, never 0.
2. **A shared `:device` module** holds the samplers behind a `DeviceMonitor`
   interface, used by the phone and the watch. Parsers are pure functions
   tested against text captured from the three test devices.
3. **CPU load is an estimate, labelled as one.** Frequency-weighted residency
   from `time_in_state`: time at each step times (step − min) / (max − min),
   over the interval. The governor parks idle clusters at the bottom step, so
   it follows real load; the UI says it is estimated.
4. **Units follow the contract, not a guess.** `BATTERY_PROPERTY_CURRENT_NOW`
   is microamps. A magnitude heuristic for "vendors that report mA" was wrong
   both ways on one phone (S25: 5 468 µA trickle, 523 437 µA charging). GPU
   clocks are the exception, because the node itself differs: MHz on Adreno's
   `/sys/kernel/gpu`, kHz on Mali, Hz on kgsl — decided by magnitude, and
   tested with each device's value.
5. **Cost.** Sampling runs only while a screen collects it (lifecycle-aware,
   stops 2 s after the last collector). History is one sample every 15
   minutes through WorkManager plus one a minute while the screen is open,
   kept a week in a JSON-lines file (~2 MB). Live values animate with 200 ms
   tweens read in the draw phase: the low-stiffness springs used first never
   settled inside the one-second update and kept the A51 drawing ~200 frames
   every ten seconds.
6. **Watch.** The same `:device` module, sampled every 2 s on the list and
   every 1 s on its own screen. Wear OS has no Settings screen for the special
   accesses, so the watch manifest removes both permissions.

## Consequences

- The phone opens on "This phone": Now, Battery, Storage, Apps, Sensors,
  History, About. PCs are the second tab.
- No per-process list for the phone: Android does not allow it. The Apps tab
  (usage, data, storage per app over 24 h) is what an app can honestly offer.
- App caches cannot be cleared by another app on Android 11+; the cleanup list
  shows them with a button to the app's Settings page.
- Play review will ask why `MANAGE_EXTERNAL_STORAGE` is needed. The answer is
  the storage analyser, a permitted use; if it is refused, the scan becomes a
  document-tree picker and the permission goes.
