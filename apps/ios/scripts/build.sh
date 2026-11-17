#!/usr/bin/env bash
# Reproducible build of the Vitals iPhone app on a macOS host (vmui-mac,
# Xcode 26.x). Same shape as Kodwen's apps/ios/scripts/build.sh (the shared
# Apple runbook): XcodeGen-pinned project, cloud signing with the team's App
# Store Connect API key, a dedicated build keychain for headless SSH.
#
#   scripts/build.sh test     swift test VitalsCore (no Simulator needed)
#   scripts/build.sh build    generate + compile for iphoneos, unsigned (the
#                             Apple Watch app is embedded and built with it)
#   scripts/build.sh uitest   generate + run the XCUITest smoke tests on a
#                             fresh Simulator (VitalsUITests)
#   scripts/build.sh upload   archive, sign (automatic, API key) and upload to App Store Connect
#   scripts/build.sh screenshots   App Store screenshots from the Simulator (Sample PC, EN + RO, iPhone + Watch)
#
# Env for `upload` (scripts/ios-build.ps1 sets these from Secret Manager and
# deletes the key afterwards): DEVELOPMENT_TEAM, ASC_KEY_ID, ASC_ISSUER_ID,
# ASC_KEY_PATH, BUILD_NUMBER.
set -euo pipefail

MODE="${1:-build}"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$HERE"

XCODEGEN_VERSION="2.46.0"
XCODEGEN_SHA256="4d9e34b62172d645eed6457cac13fc222569974098ef4ee9c3368bedf0196806"
TOOLS="$HOME/.local/codai-ios-tools"
XCODEGEN="$TOOLS/xcodegen-$XCODEGEN_VERSION/bin/xcodegen"
OUT="$HERE/build"

if [ -z "${DEVELOPER_DIR:-}" ] && [ -d "$HOME/Applications/Xcode.app" ]; then
  export DEVELOPER_DIR="$HOME/Applications/Xcode.app/Contents/Developer"
fi

log() { printf '== %s\n' "$*"; }

ensure_xcodegen() {
  if [ -x "$XCODEGEN" ]; then return; fi
  log "installing XcodeGen $XCODEGEN_VERSION"
  mkdir -p "$TOOLS"
  local zip="$TOOLS/xcodegen-$XCODEGEN_VERSION.zip"
  curl -fsSL -o "$zip" "https://github.com/yonaskolb/XcodeGen/releases/download/$XCODEGEN_VERSION/xcodegen.zip"
  echo "$XCODEGEN_SHA256  $zip" | shasum -a 256 -c -
  rm -rf "$TOOLS/xcodegen-$XCODEGEN_VERSION"
  unzip -q "$zip" -d "$TOOLS/tmp-$XCODEGEN_VERSION"
  mv "$TOOLS/tmp-$XCODEGEN_VERSION/xcodegen" "$TOOLS/xcodegen-$XCODEGEN_VERSION"
  rm -rf "$TOOLS/tmp-$XCODEGEN_VERSION" "$zip"
}

generate() {
  ensure_xcodegen
  log "xcodebuild $(xcodebuild -version | tr '\n' ' ')"
  DEVELOPMENT_TEAM="${DEVELOPMENT_TEAM:-}" "$XCODEGEN" generate --spec project.yml --quiet
}

# Over SSH the login keychain is locked; Xcode then fails storing the key of an
# auto-created signing certificate ("User interaction is not allowed"). Shared
# with codai on vmui-mac: same keychain, random password kept 0600 on the Mac.
build_keychain() {
  local kc="$HOME/Library/Keychains/codai-build.keychain-db" pwf="$HOME/.config/codai/build-keychain.pw"
  mkdir -p "$(dirname "$pwf")"
  if [ ! -s "$pwf" ]; then (umask 077; openssl rand -hex 24 > "$pwf"); fi
  if [ ! -f "$kc" ]; then security create-keychain -p "$(cat "$pwf")" "$kc"; fi
  security unlock-keychain -p "$(cat "$pwf")" "$kc"
  security set-keychain-settings -lut 21600 "$kc"
  security list-keychains -d user -s "$kc" "$HOME/Library/Keychains/login.keychain-db"
  security default-keychain -d user -s "$kc"
  log "build keychain unlocked"
}

case "$MODE" in
  test)
    log "swift test VitalsCore"
    (cd VitalsCore && swift test)
    ;;
  build)
    generate
    log "compile (iphoneos, unsigned)"
    xcodebuild -project Vitals.xcodeproj -scheme Vitals -configuration Release \
      -destination 'generic/platform=iOS' -derivedDataPath "$OUT/dd" \
      CODE_SIGNING_ALLOWED=NO CODE_SIGNING_REQUIRED=NO build -quiet
    APP="$OUT/dd/Build/Products/Release-iphoneos/VitalsMonitor.app"
    # The watch app rides inside the phone app ("Embed Watch Content"); an
    # iPhone build without it would ship a phone app whose watch half is gone.
    [ -d "$APP/Watch/VitalsMonitorWatch.app" ] || { echo "watch app not embedded in $APP/Watch" >&2; exit 1; }
    log "OK: $APP (+ Watch/VitalsMonitorWatch.app)"
    ;;
  upload)
    : "${DEVELOPMENT_TEAM:?}" "${ASC_KEY_ID:?}" "${ASC_ISSUER_ID:?}" "${ASC_KEY_PATH:?}" "${BUILD_NUMBER:?}"
    generate
    build_keychain
    AUTH=(-allowProvisioningUpdates
      -authenticationKeyPath "$ASC_KEY_PATH"
      -authenticationKeyID "$ASC_KEY_ID"
      -authenticationKeyIssuerID "$ASC_ISSUER_ID")
    rm -rf "$OUT/Vitals.xcarchive" "$OUT/export"
    log "archive (build $BUILD_NUMBER)"
    xcodebuild -project Vitals.xcodeproj -scheme Vitals -configuration Release \
      -destination 'generic/platform=iOS' -derivedDataPath "$OUT/dd" \
      -archivePath "$OUT/Vitals.xcarchive" "${AUTH[@]}" \
      DEVELOPMENT_TEAM="$DEVELOPMENT_TEAM" CURRENT_PROJECT_VERSION="$BUILD_NUMBER" archive -quiet
    # Store-eligible upload: a testFlightInternalTestingOnly build can never
    # be attached to an App Store version (ASC 409, codai 2026-10-28).
    cat > "$OUT/ExportOptions.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>method</key><string>app-store-connect</string>
  <key>destination</key><string>upload</string>
  <key>teamID</key><string>$DEVELOPMENT_TEAM</string>
  <key>signingStyle</key><string>automatic</string>
  <key>manageAppVersionAndBuildNumber</key><false/>
  <key>testFlightInternalTestingOnly</key><false/>
</dict></plist>
PLIST
    log "export + upload to App Store Connect"
    xcodebuild -exportArchive -archivePath "$OUT/Vitals.xcarchive" \
      -exportOptionsPlist "$OUT/ExportOptions.plist" -exportPath "$OUT/export" "${AUTH[@]}"
    log "UPLOADED build $BUILD_NUMBER"
    ;;
  screenshots)
    generate
    bash "$HERE/scripts/screenshots.sh"
    ;;
  uitest)
    generate
    # A simulator of its own, the newest runtime the host has, discovered the
    # same way screenshots.sh does it — device names and runtimes change with
    # every Xcode, so nothing here is hard-coded. Signing is off: a Simulator
    # build needs none, and over SSH on vmui-mac the login keychain is locked.
    RUNTIME="$(xcrun simctl list runtimes -j | /usr/bin/python3 -c 'import json,sys; r=[x for x in json.load(sys.stdin)["runtimes"] if x["platform"]=="iOS" and x["isAvailable"]]; print(r[-1]["identifier"])')"
    DEVICE_TYPE="$(xcrun simctl list devicetypes -j | /usr/bin/python3 -c 'import json,sys; d=[x["identifier"] for x in json.load(sys.stdin)["devicetypes"] if x["name"].startswith("iPhone")]; print(d[-1])')"
    UDID="$(xcrun simctl create vitals-uitest "$DEVICE_TYPE" "$RUNTIME")"
    trap 'xcrun simctl shutdown "$UDID" >/dev/null 2>&1 || true; xcrun simctl delete "$UDID" >/dev/null 2>&1 || true' EXIT
    xcrun simctl boot "$UDID"
    xcrun simctl bootstatus "$UDID" -b >/dev/null
    log "XCUITest smoke on $DEVICE_TYPE ($RUNTIME)"
    xcodebuild -project Vitals.xcodeproj -scheme Vitals -configuration Debug \
      -destination "id=$UDID" -derivedDataPath "$OUT/uitest-dd" \
      CODE_SIGNING_ALLOWED=NO CODE_SIGNING_REQUIRED=NO test
    log "OK: XCUITest smoke passed"
    ;;
  *)
    echo "usage: $0 test|build|uitest|upload|screenshots" >&2
    exit 2
    ;;
esac
