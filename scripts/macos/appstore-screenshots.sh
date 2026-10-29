#!/usr/bin/env bash
# Mac App Store screenshots: real captures of the store-flavour app (the
# sandbox-reduced build reviewers will run) on this Mac, EN + RO, dark.
# Each shot: write settings.json with the locale and section, launch, let
# the charts gather history, capture only the app window at 1440x900 (an
# accepted Mac size, 16:10).
#
#   scripts/macos/appstore-screenshots.sh <out-dir>
#
# Needs a logged-in GUI session (GitHub's macos runners have one; vmui-mac
# has no GPU and no console user, so it cannot capture).
set -euo pipefail
OUT="${1:?usage: $0 <out-dir>}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
mkdir -p "$OUT"
log() { printf '== %s\n' "$*"; }

# The main window at exactly the screenshot size. A config merge replaces
# arrays whole, so the window is restated in full.
CONF="$(mktemp -d)/shots.conf.json"
cat > "$CONF" <<'JSON'
{"app":{"windows":[{"label":"main","title":"Vitals","width":1440,"height":900,"resizable":false,"center":true,"decorations":false,"transparent":false,"dragDropEnabled":false,"visible":false}]}}
JSON

cd "$ROOT"
pnpm install --frozen-lockfile >/dev/null
(
  cd apps/desktop
  pnpm tauri build --no-bundle --config src-tauri/tauri.appstore.conf.json --config "$CONF" \
    --features app-store -- --no-default-features
)
BIN="$ROOT/target/release/vitals-desktop"
[ -x "$BIN" ] || { echo "no binary at $BIN" >&2; exit 1; }

# The window id of our process, from CoreGraphics (screencapture -l wants it).
WINID="$(mktemp -d)/winid.swift"
cat > "$WINID" <<'SWIFT'
import CoreGraphics
let pid = Int32(CommandLine.arguments[1])!
let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as! [[String: Any]]
for w in list where (w[kCGWindowOwnerPID as String] as? Int32) == pid {
  let b = w[kCGWindowBounds as String] as! [String: CGFloat]
  if b["Width"]! >= 1000 { print(w[kCGWindowNumber as String] as! Int); break }
}
SWIFT

DATA="$HOME/Library/Application Support/app.vitals.desktop"
mkdir -p "$DATA"
n=0
for locale in en ro; do
  for route in dashboard performance storage network; do
    n=$((n + 1))
    printf '{"app":{"locale":"%s","lastRoute":"%s","firstRunDismissed":true,"theme":{"mode":"dark"}}}\n' \
      "$locale" "$route" > "$DATA/settings.json"
    VITALS_NO_AUTO_UPDATE=1 "$BIN" &
    pid=$!
    # Long enough for the dashboard charts to stop being flat lines.
    sleep 40
    id=""
    for _ in 1 2 3 4 5 6 7 8 9 10; do
      id="$(swift "$WINID" "$pid" 2>/dev/null || true)"
      [ -n "$id" ] && break
      sleep 2
    done
    [ -n "$id" ] || { kill "$pid" || true; echo "no window for $locale/$route" >&2; exit 1; }
    file="$OUT/$locale/$(printf '%02d' "$n")-$route.png"
    mkdir -p "$OUT/$locale"
    screencapture -x -o -l "$id" "$file"
    kill "$pid" 2>/dev/null || true
    wait "$pid" 2>/dev/null || true
    # Retina runners capture at 2x; both 1440x900 and 2880x1800 are accepted.
    log "$file $(sips -g pixelWidth -g pixelHeight "$file" | awk '/pixel/ {printf "%s ", $2}')"
  done
done
