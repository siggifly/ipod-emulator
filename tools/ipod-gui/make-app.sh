#!/bin/sh
# Wrap `ipod-emulator` in a macOS .app bundle.
#
#   make-app.sh <path-to-ipod-emulator> <output-dir> [icon.png]
#
# The icon defaults to `docs/media/icon-1024.png` — a drawn iPod with Brick on its screen. It is an
# emblem rather than a catalogue: this emulator is named for the line, not for one model, and the
# name is `ipod-emulator` everywhere for that reason. macOS shows the EXECUTABLE's name in the
# switcher, so the copy inside the bundle carries it too.
#
# WHY. A bare Unix executable double-clicked in Finder opens a Terminal window and runs the program
# inside it; the window appears with no menu bar, no Dock name, and no icon. A .app is the same
# executable in a directory with an `Info.plist`, and it costs nothing: no certificate, no developer
# account, no notarisation. Gatekeeper treats an unsigned .app exactly as it treats an unsigned
# binary — it refuses the first launch and the user allows it once.
#
# NOT SIGNED WITH A CERTIFICATE, deliberately, and the README says so. On Apple Silicon every binary
# must carry at least an ad-hoc signature to run at all; `rustc` applies one automatically, and this
# script re-applies it after writing the bundle because editing a bundle invalidates the signature
# it had. Ad-hoc is not notarisation and does not pretend to be.
set -eu

BIN=${1:?usage: make-app.sh <ipod-emulator binary> <output dir> [icon.png]}
OUT=${2:?usage: make-app.sh <ipod-emulator binary> <output dir> [icon.png]}
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
ICON=${3:-$ROOT/docs/media/icon-1024.png}

# Read the version rather than repeat it. It was written out here as a literal and was still 0.1.0
# when the workspace had moved on — a bundle claiming a version its binary does not have is the
# exact drift `[workspace.package]` exists to stop, reintroduced by a second copy.
VERSION=$(sed -n 's/^version = "\(.*\)"$/\1/p' "$ROOT/Cargo.toml" | head -1)
[ -n "$VERSION" ] || VERSION=0.0.0

APP="$OUT/ipod-emulator.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN" "$APP/Contents/MacOS/ipod-emulator"

# An .icns from a single PNG. `sips` and `iconutil` both ship with macOS, so this needs nothing
# installed. Skipped without complaint when no PNG is given — an app with no icon still runs.
if [ -n "$ICON" ] && [ -f "$ICON" ] && command -v iconutil > /dev/null 2>&1; then
  SET=$(mktemp -d)/icon.iconset
  mkdir -p "$SET"
  for s in 16 32 128 256 512; do
    sips -z $s $s "$ICON" --out "$SET/icon_${s}x${s}.png" > /dev/null 2>&1
    sips -z $((s * 2)) $((s * 2)) "$ICON" --out "$SET/icon_${s}x${s}@2x.png" > /dev/null 2>&1
  done
  iconutil -c icns "$SET" -o "$APP/Contents/Resources/icon.icns" 2> /dev/null || true
  rm -rf "$(dirname "$SET")"
fi

cat > "$APP/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key>                 <string>ipod-emulator</string>
  <key>CFBundleDisplayName</key>          <string>ipod-emulator</string>
  <key>CFBundleExecutable</key>           <string>ipod-emulator</string>
  <key>CFBundleIdentifier</key>           <string>net.siggifly.ipod-emulator</string>
  <key>CFBundleVersion</key>              <string>__VERSION__</string>
  <key>CFBundleShortVersionString</key>   <string>__VERSION__</string>
  <key>CFBundlePackageType</key>          <string>APPL</string>
  <key>CFBundleIconFile</key>             <string>icon</string>
  <key>LSMinimumSystemVersion</key>       <string>11.0</string>
  <!-- The panel is 320x240 upscaled; without this it renders at 1x and looks soft on Retina. -->
  <key>NSHighResolutionCapable</key>      <true/>
</dict>
</plist>
PLIST
sed -i '' "s/__VERSION__/$VERSION/g" "$APP/Contents/Info.plist"

# Editing the bundle invalidates whatever signature the binary arrived with, and an unsigned binary
# does not run at all on Apple Silicon. Ad-hoc, so this needs no identity.
codesign --force --deep --sign - "$APP" 2> /dev/null || true

echo "$APP"
