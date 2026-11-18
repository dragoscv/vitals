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

# Signing mode. Automatic (Xcode resolves the profile itself) unless the upload
# driver asked for Manual and shipped the App Store profiles — see the note in
# project.yml. Defaults keep the generated project valid when it is not set.
export VITALS_CODE_SIGN_STYLE="${VITALS_CODE_SIGN_STYLE:-Automatic}"
export VITALS_PROFILE_APP="${VITALS_PROFILE_APP:-}"
export VITALS_PROFILE_WATCH="${VITALS_PROFILE_WATCH:-}"
export VITALS_PROFILE_WIDGET="${VITALS_PROFILE_WIDGET:-}"
export VITALS_PROFILE_WATCHWIDGET="${VITALS_PROFILE_WATCHWIDGET:-}"

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
    # The driver ships the ids and the key path in this 0600 file rather than
    # as `X=y` words on the ssh command line, where PowerShell rewrites them.
    [ -f ./.iosenv ] && . ./.iosenv
    : "${DEVELOPMENT_TEAM:?}" "${ASC_KEY_ID:?}" "${ASC_ISSUER_ID:?}" "${ASC_KEY_PATH:?}" "${BUILD_NUMBER:?}"
    generate
    build_keychain
    # An App Store archive is signed with a DISTRIBUTION identity. The guest
    # held only "Apple Development: Created via API", and xcodebuild's attempt
    # to mint the missing distribution certificate is what surfaced as
    # "Authentication failed: ... bearer token" on every target — while the
    # same key authenticated fine from notarytool on the same machine
    # (2026-11-18). The driver ships the exported identity when it has it.
    if [ -n "${ASC_DIST_P12:-}" ] && [ -f "$ASC_DIST_P12" ]; then
      KC="$HOME/Library/Keychains/codai-build.keychain-db"
      security import "$ASC_DIST_P12" -k "$KC" -P "$(cat "${ASC_DIST_PW:?}")" \
        -T /usr/bin/codesign -T /usr/bin/security -A >/dev/null
      security set-key-partition-list -S apple-tool:,apple:,codesign: -s \
        -k "$(cat "$HOME/.config/codai/build-keychain.pw")" "$KC" >/dev/null
      log "distribution identity imported"
    fi
    # What the keychain actually offers, so a signing failure is explainable
    # from the log alone.
    security find-identity -v -p codesigning \
      "$HOME/Library/Keychains/codai-build.keychain-db" || true
    if [ "$VITALS_CODE_SIGN_STYLE" = "Manual" ]; then
      # Manual signing never calls the provisioning service; the profiles are
      # installed here under the UUID Xcode looks them up by. This is the path
      # taken after -allowProvisioningUpdates failed on Xcode 26.6 (2026-11-18).
      : "${VITALS_PP_DIR:?manual signing needs VITALS_PP_DIR with the .mobileprovision files}"
      PPD="$HOME/Library/MobileDevice/Provisioning Profiles"
      mkdir -p "$PPD"
      for f in "$VITALS_PP_DIR"/*.mobileprovision; do
        uuid="$(security cms -D -i "$f" | plutil -extract UUID raw -)"
        cp "$f" "$PPD/$uuid.mobileprovision"
        log "installed profile $(basename "$f") as $uuid"
      done
      AUTH=()
      # Xcode otherwise signs with the "Apple Development" identity it finds
      # and rejects every distribution profile ("doesn't include signing
      # certificate ..."), which is what the first manual attempt hit.
      SIGN_EXTRA=(CODE_SIGN_IDENTITY="Apple Distribution")
      EXPORT_AUTH=(-authenticationKeyPath "$ASC_KEY_PATH"
        -authenticationKeyID "$ASC_KEY_ID"
        -authenticationKeyIssuerID "$ASC_ISSUER_ID")
    else
      AUTH=(-allowProvisioningUpdates
        -authenticationKeyPath "$ASC_KEY_PATH"
        -authenticationKeyID "$ASC_KEY_ID"
        -authenticationKeyIssuerID "$ASC_ISSUER_ID")
      SIGN_EXTRA=()
      EXPORT_AUTH=(-allowProvisioningUpdates
        -authenticationKeyPath "$ASC_KEY_PATH"
        -authenticationKeyID "$ASC_KEY_ID"
        -authenticationKeyIssuerID "$ASC_ISSUER_ID")
    fi
    rm -rf "$OUT/Vitals.xcarchive" "$OUT/export"
    log "archive (build $BUILD_NUMBER)"
    xcodebuild -project Vitals.xcodeproj -scheme Vitals -configuration Release \
      -destination 'generic/platform=iOS' -derivedDataPath "$OUT/dd" \
      -archivePath "$OUT/Vitals.xcarchive" "${AUTH[@]+"${AUTH[@]}"}" \
      "${SIGN_EXTRA[@]+"${SIGN_EXTRA[@]}"}" \
      DEVELOPMENT_TEAM="$DEVELOPMENT_TEAM" CURRENT_PROJECT_VERSION="$BUILD_NUMBER" archive -quiet
    # Store-eligible upload: a testFlightInternalTestingOnly build can never
    # be attached to an App Store version (ASC 409, codai 2026-10-28).
    # With manual signing the export needs the profile per bundle id spelled
    # out; with automatic it finds them through the service.
    if [ "$VITALS_CODE_SIGN_STYLE" = "Manual" ]; then
      STYLE_BLOCK="<key>signingStyle</key><string>manual</string>
  <key>provisioningProfiles</key><dict>
    <key>app.vitals.ios</key><string>$VITALS_PROFILE_APP</string>
    <key>app.vitals.ios.watchkitapp</key><string>$VITALS_PROFILE_WATCH</string>
    <key>app.vitals.ios.widget</key><string>$VITALS_PROFILE_WIDGET</string>
    <key>app.vitals.ios.watchkitapp.widget</key><string>$VITALS_PROFILE_WATCHWIDGET</string>
  </dict>"
    else
      STYLE_BLOCK="<key>signingStyle</key><string>automatic</string>"
    fi
    cat > "$OUT/ExportOptions.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>method</key><string>app-store-connect</string>
  <key>destination</key><string>upload</string>
  <key>teamID</key><string>$DEVELOPMENT_TEAM</string>
  $STYLE_BLOCK
  <key>manageAppVersionAndBuildNumber</key><false/>
  <key>testFlightInternalTestingOnly</key><false/>
</dict></plist>
PLIST
    log "export + upload to App Store Connect"
    # destination=upload talks to App Store Connect, so it needs the key even
    # when the archive itself was signed offline — without it xcodebuild looks
    # for a signed-in Xcode account and reports "Failed to Use Accounts".
    xcodebuild -exportArchive -archivePath "$OUT/Vitals.xcarchive" \
      -exportOptionsPlist "$OUT/ExportOptions.plist" -exportPath "$OUT/export" "${EXPORT_AUTH[@]}"
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
