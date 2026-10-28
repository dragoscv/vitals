#!/usr/bin/env bash
# Developer ID signing + notarisation of the macOS app, then a signed,
# notarised DMG and the updater tarball. Used by release.yml's tag-only
# `macos-sign` job (after the owner approves) and by hand on vmui-mac.
#
#   scripts/macos/sign-notarise.sh <path/to/Vitals.app> <version> <out-dir>
#
# Env:
#   APPLE_CERTIFICATE           base64 Developer ID Application .p12
#   APPLE_CERTIFICATE_PASSWORD  its password
#   APPLE_API_KEY               App Store Connect key id
#   APPLE_API_ISSUER            issuer id
#   APPLE_API_KEY_PATH          path to the AuthKey .p8 (0600)
#
# Writes <out-dir>/Vitals_<v>_universal.dmg and Vitals_<v>_universal.app.tar.gz.
set -euo pipefail

APP="$1"; VERSION="$2"; OUT="$3"
: "${APPLE_CERTIFICATE:?}" "${APPLE_CERTIFICATE_PASSWORD:?}" "${APPLE_API_KEY:?}" "${APPLE_API_ISSUER:?}" "${APPLE_API_KEY_PATH:?}"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENT="$HERE/../../apps/desktop/src-tauri/macos/Entitlements.direct.plist"
mkdir -p "$OUT"
log() { printf '== %s\n' "$*"; }

# A throwaway keychain: nothing persists on the runner, and an SSH session on
# vmui-mac cannot unlock the login keychain anyway.
KC="$(mktemp -d)/sign.keychain-db"
KCPW="$(openssl rand -hex 16)"
P12="$(mktemp)"
cleanup() { security delete-keychain "$KC" 2>/dev/null || true; rm -f "$P12"; }
trap cleanup EXIT
printf '%s' "$APPLE_CERTIFICATE" | tr -d '\r\n ' | base64 --decode > "$P12"
[ "$(head -c 2 "$P12" | xxd -p)" = "3082" ] || { echo "APPLE_CERTIFICATE is not a base64 PKCS#12" >&2; exit 1; }
security create-keychain -p "$KCPW" "$KC"
security set-keychain-settings -lut 3600 "$KC"
security unlock-keychain -p "$KCPW" "$KC"
security import "$P12" -k "$KC" -f pkcs12 -P "$APPLE_CERTIFICATE_PASSWORD" -T /usr/bin/codesign >/dev/null
security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$KCPW" "$KC" >/dev/null
security list-keychains -d user -s "$KC" $(security list-keychains -d user | tr -d '"')
IDENTITY="$(security find-identity -v -p codesigning "$KC" | sed -n 's/.*"\(Developer ID Application:[^"]*\)".*/\1/p' | head -1)"
[ -n "$IDENTITY" ] || { echo "no Developer ID Application identity in the .p12" >&2; exit 1; }
log "signing as: $IDENTITY"

notarise() {
  xcrun notarytool submit "$1" --key "$APPLE_API_KEY_PATH" --key-id "$APPLE_API_KEY" \
    --issuer "$APPLE_API_ISSUER" --wait --timeout 30m --output-format json > "$OUT/notary.json"
  local status
  status="$(sed -n 's/.*"status" *: *"\([^"]*\)".*/\1/p' "$OUT/notary.json" | head -1)"
  log "notarisation of $(basename "$1"): $status"
  if [ "$status" != "Accepted" ]; then
    local id; id="$(sed -n 's/.*"id" *: *"\([^"]*\)".*/\1/p' "$OUT/notary.json" | head -1)"
    xcrun notarytool log "$id" --key "$APPLE_API_KEY_PATH" --key-id "$APPLE_API_KEY" --issuer "$APPLE_API_ISSUER" || true
    exit 1
  fi
}

# 1. Sign the app (hardened runtime, timestamp), innermost code first. The
#    main executable is signed as part of the bundle, never on its own
#    (codesign then reports "unsealed contents present in the bundle root").
MAIN="$APP/Contents/MacOS/$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$APP/Contents/Info.plist")"
find "$APP/Contents" -type f \( -perm -u+x -o -name '*.dylib' \) ! -path '*/_CodeSignature/*' ! -path "$MAIN" -print0 |
  while IFS= read -r -d '' f; do
    if file "$f" | grep -q 'Mach-O'; then
      codesign --force --timestamp --options runtime --keychain "$KC" --sign "$IDENTITY" "$f"
    fi
  done
codesign --force --timestamp --options runtime --entitlements "$ENT" --keychain "$KC" --sign "$IDENTITY" "$APP"
codesign --verify --strict --deep --verbose=2 "$APP"

# 2. Notarise the app (as a zip) and staple the ticket into it.
ZIP="$(mktemp -d)/Vitals.zip"
ditto -c -k --keepParent "$APP" "$ZIP"
notarise "$ZIP"
xcrun stapler staple "$APP"

# 3. The updater tarball from the stapled app. COPYFILE_DISABLE keeps
#    AppleDouble ._ files out, which would otherwise break the signature.
TAR="$OUT/Vitals_${VERSION}_universal.app.tar.gz"
COPYFILE_DISABLE=1 tar -czf "$TAR" -C "$(dirname "$APP")" "$(basename "$APP")"

# 4. DMG with an Applications link, signed, notarised, stapled.
STAGE="$(mktemp -d)"
ditto "$APP" "$STAGE/Vitals.app"
ln -s /Applications "$STAGE/Applications"
DMG="$OUT/Vitals_${VERSION}_universal.dmg"
rm -f "$DMG"
hdiutil create -volname "Vitals" -srcfolder "$STAGE" -ov -format UDZO "$DMG" >/dev/null
codesign --force --timestamp --keychain "$KC" --sign "$IDENTITY" "$DMG"
notarise "$DMG"
xcrun stapler staple "$DMG"

# 5. What Gatekeeper will say on a user's Mac.
spctl --assess --type execute --verbose=2 "$APP"
spctl --assess --type open --context context:primary-signature --verbose=2 "$DMG"
log "OK: $DMG"
log "OK: $TAR"
