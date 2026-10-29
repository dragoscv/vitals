#!/usr/bin/env bash
# App Store screenshots of the Vitals iPhone app: real Simulator captures of
# the labelled Sample PC, in English and Romanian, at the 6.9" size App Store
# Connect requires (iPhone 17 Pro Max, 1320x2868). Needs a Mac with a GPU:
# on vmui-mac (no Metal) iOS 26's glass tab bar renders only the selected
# item, so the images are made by .github/workflows/ios-screenshots.yml on an
# Apple-silicon runner. By hand:
#
#   bash scripts/build.sh screenshots   -> build/screenshots/<lang>/NN-name.png
#                                         build/screenshots-watch/<lang>/NN-name.png
# (build.sh generates the Xcode project first.)
# The watch images sit in their own folder so scripts/ios-screenshots-upload.mjs,
# which reads build/screenshots/<lang>, keeps uploading only iPhone sizes.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$HERE"
if [ -z "${DEVELOPER_DIR:-}" ] && [ -d "$HOME/Applications/Xcode.app" ]; then
  export DEVELOPER_DIR="$HOME/Applications/Xcode.app/Contents/Developer"
fi
OUT="$HERE/build/screenshots"
DERIVED="$HERE/build/sim"
BUNDLE="app.vitals.ios"
log() { printf '== %s\n' "$*"; }

[ -d Vitals.xcodeproj ] || { echo "Vitals.xcodeproj missing: run scripts/build.sh build first" >&2; exit 1; }

log "build for the Simulator"
xcodebuild -project Vitals.xcodeproj -scheme Vitals -configuration Release \
  -destination 'generic/platform=iOS Simulator' -derivedDataPath "$DERIVED" \
  CODE_SIGNING_ALLOWED=NO build -quiet
APP="$(find "$DERIVED/Build/Products" -maxdepth 2 -name 'VitalsMonitor.app' -path '*iphonesimulator*' | head -1)"
[ -n "$APP" ] || { echo "no simulator .app" >&2; exit 1; }

RUNTIME="$(xcrun simctl list runtimes -j | /usr/bin/python3 -c 'import json,sys; r=[x for x in json.load(sys.stdin)["runtimes"] if x["platform"]=="iOS" and x["isAvailable"]]; print(r[-1]["identifier"])')"
# The newest "Pro Max" the runtime offers: 6.9" is the size App Store Connect
# requires, and the iPhone 17 Pro Max is what the app was tested on.
DEVICE_TYPE="$(xcrun simctl list devicetypes -j | /usr/bin/python3 -c 'import json,sys; d=[x["identifier"] for x in json.load(sys.stdin)["devicetypes"] if x["name"].startswith("iPhone") and x["name"].endswith("Pro Max")]; print(d[-1])')"
log "device $DEVICE_TYPE on $RUNTIME"
UDID="$(xcrun simctl create vitals-shots "$DEVICE_TYPE" "$RUNTIME")"
trap 'xcrun simctl shutdown "$UDID" >/dev/null 2>&1 || true; xcrun simctl delete "$UDID" >/dev/null 2>&1 || true' EXIT
xcrun simctl boot "$UDID"
xcrun simctl bootstatus "$UDID" -b >/dev/null
# A clean, honest status bar: full signal, charged, 9:41.
xcrun simctl status_bar "$UDID" override --time 9:41 --batteryState charged --batteryLevel 100 \
  --cellularBars 4 --wifiBars 3 --dataNetwork wifi
xcrun simctl install "$UDID" "$APP"

shot() { # locale lang name args...
  local locale="$1" lang="$2" name="$3"; shift 3
  xcrun simctl terminate "$UDID" "$BUNDLE" >/dev/null 2>&1 || true
  xcrun simctl launch "$UDID" "$BUNDLE" -AppleLanguages "($lang)" -AppleLocale "$locale" "$@" >/dev/null
  sleep 6 # the Sample PC ticks once a second; let the charts fill a little
  mkdir -p "$OUT/$lang"
  xcrun simctl io "$UDID" screenshot --type=png "$OUT/$lang/$name.png" >/dev/null
  log "$lang/$name.png"
}

for pair in "en_GB en-GB" "ro_RO ro"; do
  set -- $pair
  locale="$1"; lang="$2"
  shot "$locale" "$lang" 01-now -vitalsRoute sample -vitalsTab 0
  shot "$locale" "$lang" 02-sensors -vitalsRoute sample -vitalsTab 1
  shot "$locale" "$lang" 03-history -vitalsRoute sample -vitalsTab 2
  shot "$locale" "$lang" 04-processes -vitalsRoute processes
  shot "$locale" "$lang" 05-machines
done
ls -la "$OUT"/*

# Apple Watch: the watch app opens on the labelled Sample PC by itself with
# -vitalsRoute sample (no paired iPhone in a Simulator); one image of the list.
WOUT="$HERE/build/screenshots-watch"
WBUNDLE="app.vitals.ios.watchkitapp"
log "build the watch app for the Simulator"
xcodebuild -project Vitals.xcodeproj -scheme VitalsWatch -configuration Release \
  -destination 'generic/platform=watchOS Simulator' -derivedDataPath "$DERIVED" \
  CODE_SIGNING_ALLOWED=NO build -quiet
WAPP="$(find "$DERIVED/Build/Products" -maxdepth 2 -name 'VitalsMonitorWatch.app' -path '*watchsimulator*' | head -1)"
[ -n "$WAPP" ] || { echo "no watch simulator .app" >&2; exit 1; }
WRUNTIME="$(xcrun simctl list runtimes -j | /usr/bin/python3 -c 'import json,sys; r=[x for x in json.load(sys.stdin)["runtimes"] if x["platform"]=="watchOS" and x["isAvailable"]]; print(r[-1]["identifier"] if r else "")')"
# The newest Ultra, else the newest Series: either size is accepted by App Store Connect.
WTYPE="$(xcrun simctl list devicetypes -j | /usr/bin/python3 -c 'import json,sys; d=[x for x in json.load(sys.stdin)["devicetypes"] if x["name"].startswith("Apple Watch")]; u=[x["identifier"] for x in d if "Ultra" in x["name"]]; s=[x["identifier"] for x in d if "Series" in x["name"]]; print((u or s or [""])[-1])')"
if [ -z "$WRUNTIME" ] || [ -z "$WTYPE" ]; then
  # The iPhone images above are still good; say so loudly and stop here.
  echo "WARNING: no watchOS Simulator runtime or device type, no watch screenshots" >&2
  exit 0
fi
log "watch $WTYPE on $WRUNTIME"
WUDID="$(xcrun simctl create vitals-watch-shots "$WTYPE" "$WRUNTIME")"
trap 'for u in "$UDID" "$WUDID"; do xcrun simctl shutdown "$u" >/dev/null 2>&1 || true; xcrun simctl delete "$u" >/dev/null 2>&1 || true; done' EXIT
xcrun simctl boot "$WUDID"
xcrun simctl bootstatus "$WUDID" -b >/dev/null
xcrun simctl status_bar "$WUDID" override --time 9:41 >/dev/null 2>&1 || true
xcrun simctl install "$WUDID" "$WAPP"

wshot() { # locale lang name
  local locale="$1" lang="$2" name="$3"
  sleep 3
  mkdir -p "$WOUT/$lang"
  xcrun simctl io "$WUDID" screenshot --type=png "$WOUT/$lang/$name.png" >/dev/null
  log "watch $lang/$name.png"
}

for pair in "en_GB en-GB" "ro_RO ro"; do
  set -- $pair
  locale="$1"; lang="$2"
  xcrun simctl terminate "$WUDID" "$WBUNDLE" >/dev/null 2>&1 || true
  xcrun simctl launch "$WUDID" "$WBUNDLE" -AppleLanguages "($lang)" -AppleLocale "$locale" \
    -vitalsRoute sample >/dev/null
  wshot "$locale" "$lang" 01-pcs
done
ls -la "$WOUT"/*
