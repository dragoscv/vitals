# Vitals for Android and Wear OS

Native Kotlin apps (ADR-0033, ADR-0035). One Gradle build:

| Module       | What                                                                     |
| ------------ | ------------------------------------------------------------------------ |
| `:core`      | Wire models (kotlinx.serialization), LAN client, pairing, Wake-on-LAN    |
| `:device`    | This device, read through official APIs only (`DeviceMonitor`)           |
| `:shared-ui` | Formatting, palette, thresholds, contrast — shared by phone and watch    |
| `:app`       | Phone: _This phone_ first, paired PCs second, widgets, tile, Live Update |
| `:wear`      | Watch: _This watch_, PC list and detail, tile, complications             |

## Build

```powershell
cd apps/android
$env:JAVA_HOME = 'C:\Program Files\Microsoft\jdk-21.0.12.101-hotspot'
.\gradlew.bat test lintPlayDebug :app:assembleGithubDebug :wear:assembleDebug
```

Release builds are signed only when `ANDROID_KEYSTORE_PATH` and its three
companions are set (CI secrets); otherwise they are unsigned.

### Two flavours: `play` and `github`

`:app` and `:tv` build twice. **`play`** is the bundle Google Play reviews and
does **not** declare `MANAGE_EXTERNAL_STORAGE`: Play rejected all-files access
for a system monitor as "not a core feature" (2026-10-14). There the storage
screen shows categories (usage access), app caches, and a button to the
system's own storage manager. **`github`** is the APK on GitHub releases and
keeps the folder scan and file cleanup (`src/github/AndroidManifest.xml`).
The UI decides from the installed manifest (`Access.allFilesAvailable`), and
the release workflow fails if a Play bundle ever declares the permission.

## What an app may read on Android

Measured as the app's uid, not taken from the docs. Unreadable readings are
shown as an em dash, never 0.

- **Readable:** `/proc/meminfo`, cpufreq `time_in_state` and current clocks,
  thermal zones (S25 Ultra: ~60; A51: none), GPU busy and clock, battery via
  `BatteryManager`, network totals via `TrafficStats`.
- **Refused on Android 10+:** `/proc/stat` (so CPU load is an estimate from
  core speed, and labelled so), other apps' `/proc/<pid>`, disk I/O, battery
  sysfs.
- **With usage access:** per-app screen time, launches, data and storage;
  storage categories.
- **With all-files access (`github` flavour only):** the folder scan and file
  cleanup. Other apps' caches can only be cleared from Settings, so the list
  links there.

## Checking it on a device

The live screens never let `uiautomator` reach idle, and a failed dump
silently re-reads the previous `ui.xml`. Turn animations off for the dump
(`settings put global animator_duration_scale 0`, restore afterwards) or
read a screenshot. Frame cost: `adb shell dumpsys gfxinfo app.vitals`
(reset, wait 10 s, read "Total frames rendered"); an idle live tab should
draw about 12 frames a second at most, and 1 with nothing animating.
