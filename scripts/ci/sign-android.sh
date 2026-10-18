#!/usr/bin/env bash
# Signs the unsigned Android builds with the upload key: APKs with apksigner,
# Play bundles with jarsigner. Called by release.yml's `publish` job after the
# owner approved the `release` environment; runnable locally against a
# throwaway keystore, which is how it is tested (docs/releasing.md).
#
# Every check says what failed. The step this replaced died in 1.7 s with no
# message on the v1.0.5 tag: it piped `apksigner verify --print-certs` into
# `grep 'Signer #1 certificate'`, build-tools 37 prints `V3.0 Signer:
# certificate ...` instead, and a grep that matches nothing exits 1 silently
# under `set -e -o pipefail`. So: no bare greps, explicit exit codes, an
# ::error:: for each failure, and an ERR trap for anything not anticipated.
#
# Usage: sign-android.sh <unsigned-dir> <apk-out-dir> <aab-out-dir>
#   <unsigned-dir> holds apk/*.apk and play/*.aab (the android-unsigned
#   artefact). Environment: SIGN_KEYSTORE (path to the .jks),
#   ANDROID_KEYSTORE_PASSWORD, ANDROID_KEY_ALIAS, ANDROID_KEY_PASSWORD,
#   BUILD_TOOLS (optional; defaults to the newest under ANDROID_HOME).
set -euo pipefail
trap 'echo "::error::sign-android: \"$BASH_COMMAND\" failed with exit $? (line $LINENO)"' ERR

# To stderr: inside $(...) stdout is captured, and the runner reads workflow
# commands from both streams.
fail() { echo "::error::$*" >&2; exit 1; }

[ "$#" -eq 3 ] || fail "usage: sign-android.sh <unsigned-dir> <apk-out-dir> <aab-out-dir>"
in=$1; apk_out=$2; aab_out=$3
: "${SIGN_KEYSTORE:?SIGN_KEYSTORE is not set}"
: "${ANDROID_KEYSTORE_PASSWORD:?ANDROID_KEYSTORE_PASSWORD is not set}"
: "${ANDROID_KEY_ALIAS:?ANDROID_KEY_ALIAS is not set}"
: "${ANDROID_KEY_PASSWORD:?ANDROID_KEY_PASSWORD is not set}"
[ -s "$SIGN_KEYSTORE" ] || fail "keystore $SIGN_KEYSTORE is missing or empty (ANDROID_KEYSTORE_B64 did not decode?)"

bt=${BUILD_TOOLS:-}
if [ -z "$bt" ]; then
  [ -n "${ANDROID_HOME:-}" ] || fail "neither BUILD_TOOLS nor ANDROID_HOME is set"
  bt=$(printf '%s\n' "$ANDROID_HOME"/build-tools/* | sort -V | tail -1)
fi
# Windows SDKs ship apksigner.bat and zipalign.exe; Linux has plain names.
tool() { for c in "$bt/$1" "$bt/$1.bat" "$bt/$1.exe"; do [ -f "$c" ] && { echo "$c"; return; }; done; fail "$1 not found in $bt"; }
apksigner=$(tool apksigner); zipalign=$(tool zipalign)
echo "build-tools: $bt"

# Exactly the artefact layout `android` uploads: three APKs, three bundles.
shopt -s nullglob
apks=("$in"/apk/*.apk); aabs=("$in"/play/*.aab)
shopt -u nullglob
if [ "${#apks[@]}" -ne 3 ] || [ "${#aabs[@]}" -ne 3 ]; then
  echo "contents of $in:"; find "$in" -type f -print || true
  fail "expected 3 APKs in $in/apk and 3 bundles in $in/play, found ${#apks[@]} and ${#aabs[@]}"
fi
mkdir -p "$apk_out" "$aab_out"

# The certificate both signatures must carry, read from the keystore itself:
# the SHA-256 of its DER encoding, which is what apksigner prints.
der=$(mktemp); trap 'rm -f "$der"' EXIT
if ! keytool -exportcert -keystore "$SIGN_KEYSTORE" -storepass:env ANDROID_KEYSTORE_PASSWORD -alias "$ANDROID_KEY_ALIAS" -file "$der" >/dev/null; then
  fail "keytool could not read alias from the keystore (wrong ANDROID_KEYSTORE_PASSWORD or ANDROID_KEY_ALIAS?)"
fi
want=$(sha256sum "$der" | cut -d' ' -f1)
echo "upload certificate SHA-256: $want"

for apk in "${apks[@]}"; do
  name=$(basename "$apk"); out="$apk_out/$name"; src="$apk"
  echo "::group::$name"
  # AGP normally zip-aligns already (16 KB pages for native libraries,
  # which Android 15+ devices need); align only when the check says not.
  if ! "$zipalign" -c -P 16 4 "$apk" >/dev/null 2>&1; then
    echo "$name is not 16 KB aligned; aligning"
    aligned=$(mktemp --suffix=.apk)
    "$zipalign" -P 16 -f 4 "$apk" "$aligned" || fail "zipalign failed for $name"
    src=$aligned
  fi
  if ! "$apksigner" sign --ks "$SIGN_KEYSTORE" --ks-key-alias "$ANDROID_KEY_ALIAS" \
      --ks-pass env:ANDROID_KEYSTORE_PASSWORD --key-pass env:ANDROID_KEY_PASSWORD \
      --out "$out" "$src"; then
    fail "apksigner sign failed for $name"
  fi
  rm -f "$out.idsig"
  if ! report=$("$apksigner" verify --verbose --print-certs "$out" 2>&1); then
    echo "$report"; fail "apksigner verify rejected $out"
  fi
  # Shown whole: the output format changes between build-tools releases
  # (`Signer #1 certificate` up to 36, `V3.0 Signer: certificate` in 37).
  grep -v '^WARNING' <<<"$report" || true
  if ! grep -Eq 'Verified using v(2|3) scheme.*: true' <<<"$report"; then
    fail "$out has neither a v2 nor a v3 signature; Android would refuse it"
  fi
  if ! grep -Eq '^Number of signers: 1$' <<<"$report"; then
    fail "$out does not have exactly one signer"
  fi
  if ! grep -Eiq "certificate SHA-256 digest: $want\$" <<<"$report"; then
    fail "$out is not signed with the upload certificate $want"
  fi
  "$zipalign" -c -P 16 4 "$out" >/dev/null 2>&1 || fail "$out lost its 16 KB alignment while signing"
  echo "signed $out"
  echo "::endgroup::"
done

for aab in "${aabs[@]}"; do
  name=$(basename "$aab"); out="$aab_out/$name"
  cp "$aab" "$out"
  if ! jarsigner -keystore "$SIGN_KEYSTORE" -storepass:env ANDROID_KEYSTORE_PASSWORD -keypass:env ANDROID_KEY_PASSWORD \
      -digestalg SHA-256 "$out" "$ANDROID_KEY_ALIAS"; then
    fail "jarsigner failed to sign $name"
  fi
  # Captured whole before matching: `grep -q` closing the pipe early would
  # kill jarsigner with SIGPIPE under pipefail and read as "not verified".
  # An unsigned jar verifies with exit 0 too ("jar is unsigned."), so the
  # text is what decides.
  if ! report=$(jarsigner -verify "$out" 2>&1); then
    echo "$report"; fail "jarsigner -verify rejected $out"
  fi
  if ! grep -q '^jar verified\.' <<<"$report"; then
    echo "$report"; fail "$out did not verify after signing"
  fi
  echo "signed $out"
done

shopt -s nullglob
signed_apks=("$apk_out"/*.apk); signed_aabs=("$aab_out"/*.aab)
[ "${#signed_apks[@]}" -eq 3 ] || fail "expected 3 signed APKs in $apk_out, found ${#signed_apks[@]}"
[ "${#signed_aabs[@]}" -eq 3 ] || fail "expected 3 signed bundles in $aab_out, found ${#signed_aabs[@]}"
echo "all 3 APKs and 3 bundles signed with $want"
