#!/usr/bin/env bash
# Build and stage one release archive: dist/ipod5g-<NAME>.tar.gz (or .zip on Windows).
#
#   tools/release/package.sh <rust-target> <name>
#   tools/release/package.sh x86_64-unknown-linux-gnu  linux-x86_64
#   tools/release/package.sh aarch64-apple-darwin      macos-arm64
#   tools/release/package.sh x86_64-apple-darwin       macos-x86_64
#   tools/release/package.sh x86_64-pc-windows-msvc    windows-x86_64   (Git Bash, 7z on PATH)
#
# This is the staging half of the release workflow that was deleted with the GitHub workflows
# (d3c62ac), kept as a script so every platform runs the same steps by hand. Run it from the
# repository root, on the commit the tag points at.
set -euo pipefail

TARGET=${1:?usage: package.sh <rust-target> <name>}
NAME=${2:?usage: package.sh <rust-target> <name>}

# Without the remap every binary carries the absolute path of the machine that built it.
export RUSTFLAGS="${RUSTFLAGS:-} --remap-path-prefix=$PWD=. --remap-path-prefix=$HOME=~"
export CARGO_INCREMENTAL=0

# Linux: a binary needs at least the glibc of the machine it was built on (the script prints the
# floor at the end). Build on the OLDEST distribution you want to support — an Ubuntu 22.04
# runner or container is the reference. Built on NixOS, the loader and library paths point into
# /nix/store; with `patchelf` on PATH the script repoints them at the standard loader, and the
# result runs on any distribution whose glibc is at least the printed floor.
# (cargo-zigbuild does not work here: Skia's prebuilt library needs libstdc++, zig links libc++.)
cargo build --release --locked --target "$TARGET" --workspace --bins

EXE=""
case "$TARGET" in *windows*) EXE=".exe" ;; esac
BIN="${CARGO_TARGET_DIR:-target}/$TARGET/release"
DIR="ipod5g-$NAME"
rm -rf "dist/$DIR"
mkdir -p "dist/$DIR/tools" "dist/$DIR/docs"

cp "$BIN/ipod-emulator$EXE" "dist/$DIR/"
for b in trace ipod-boot dis tcb eapp-inspect disasm; do
  cp "$BIN/$b$EXE" "dist/$DIR/tools/"
done

case "$TARGET" in
  *apple-darwin)
    if ! "$BIN/ipod-emulator" --help >/dev/null 2>&1; then
      echo "the $TARGET binary does not run on this machine, so the .app cannot be built from it." >&2
      echo "On Apple Silicon building x86_64: softwareupdate --install-rosetta --agree-to-license" >&2
      exit 1
    fi
    "$BIN/ipod-emulator" --make-app "dist/$DIR" docs/media/icon-1024.png
    ;;
esac

cp README.md "dist/$DIR/README.md"
cp LICENSE "dist/$DIR/LICENSE"
cp CHANGELOG.md "dist/$DIR/CHANGELOG.md"
cp tools/ipod-gui/README.md "dist/$DIR/README-window.md"
cp tools/ipod-boot/README.md "dist/$DIR/tools/README-recipes.md"
cp -r docs/media "dist/$DIR/docs/media"

cat > "dist/$DIR/FIRST-RUN.txt" <<'EOF'
Run `ipod-emulator` (on macOS, double-click `ipod-emulator.app`).

You do not need an iPod. Pick a model, press the button, and the emulator builds that model's
boot ROM and downloads Apple's own firmware from Apple's servers, checking its SHA-256. If you
do have a dump of your own iPod's boot ROM, or an .ipsw or a drive image, you can drop them on
the window instead.

The command-line tools are in `tools/`. They are for taking firmware apart, not for playing.

NOTHING HERE IS SIGNED WITH A CERTIFICATE OR NOTARIZED, deliberately. macOS refuses the first
launch of anything downloaded and unsigned. On macOS 15 and later: open it, let it be blocked,
then go to System Settings -> Privacy & Security, where a button offers to open it anyway. Once.
`xattr -dr com.apple.quarantine ipod-emulator.app` does the same thing from a terminal.
On Windows, SmartScreen shows "More info" -> "Run anyway". Built from source, neither happens.
EOF

case "$TARGET" in
  *linux*)
    for f in "dist/$DIR/ipod-emulator" "dist/$DIR"/tools/*; do
      [ -f "$f" ] && [ -x "$f" ] || continue
      if readelf -l "$f" | grep -q '/nix/store'; then
        if command -v patchelf >/dev/null; then
          case "$TARGET" in aarch64*) LD=/lib/ld-linux-aarch64.so.1 ;; *) LD=/lib64/ld-linux-x86-64.so.2 ;; esac
          patchelf --set-interpreter "$LD" --remove-rpath "$f"
        else
          echo "WARNING: $f loads from /nix/store and patchelf is not on PATH; it runs on NixOS only" >&2
        fi
      fi
    done
    ;;
esac

cd dist
case "$TARGET" in
  *windows*) 7z a "$DIR.zip" "$DIR" >/dev/null ;;
  *)         tar czf "$DIR.tar.gz" "$DIR" ;;
esac

# A build can report success and leave old binaries in place (0.4.0's first Linux build did).
# These must all be dated today.
ls -la --time-style=long-iso "$DIR/ipod-emulator$EXE" "$DIR"/tools/* 2>/dev/null \
  || ls -laT "$DIR/ipod-emulator$EXE" "$DIR"/tools/*
case "$TARGET" in
  *linux*)
    readelf -V "$DIR/ipod-emulator" "$DIR/tools/trace" 2>/dev/null | grep -oE 'GLIBC_[0-9.]+' \
      | sort -uV | tail -1 | sed 's/^/glibc floor (runs on distributions with at least): /' || true
    ;;
esac
if strings "$DIR/ipod-emulator$EXE" | grep -qE '/Users/|/home/'; then
  echo "WARNING: a home directory path is embedded in ipod-emulator$EXE" >&2
fi
ls -la "$DIR".*
