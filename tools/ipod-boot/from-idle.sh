#!/usr/bin/env bash
#
# Run an experiment from the idle point WITHOUT replaying the boot.
#
# RetailOS reaches its idle at ~1.61 G instructions, and almost every question worth asking now is
# about what happens there. Replaying the boot to reach it costs ~80 s per run; restoring a snapshot
# costs ~0.2 s. Measured on an M-series Mac, same experiment both ways:
#
#   cold boot to 1.66 G, wheel events, two --enterlog watches     110 s
#   restore + 60 M instructions, same events, same watches          3.1 s
#
# The snapshot is built once and cached. Usage is otherwise identical to retail-boot.sh:
#
#   ./from-idle.sh --clickwheel --wheel="@1610M:touch,+10M:rotate=+12" --enterlog=0x00281350
#   BUDGET=200000000 ./from-idle.sh --enterlog=0x001acca8
#
# Instruction anchors are ABSOLUTE and the restored machine already has 1.6 G behind it, so a script
# reading `@10M:touch` fires nothing — its time is in the past. Anchor past SNAP_AT. BUDGET is the
# number of instructions to run *in addition* to the snapshot's.
set -eu

HERE=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$HERE/../.." && pwd)

: "${TRACE:=$HOME/dev/.cargo-target/release/trace}"
: "${SNAP_AT:=1600000000}"
: "${BUDGET:=60000000}"
: "${CACHE:=${TMPDIR:-/tmp}/ipod-from-idle}"

mkdir -p "$CACHE"

# The cache is keyed on the emulator binary, and this is the load-bearing line in the file.
#
# A snapshot records the machine as one build of the model produced it. Restore it under a different
# build and you are measuring a hybrid: the first 1.6 G of behaviour from the old model, everything
# after from the new one, with nothing in the output saying so. This project has lost six separate
# published conclusions to instruments that failed silently, and a stale snapshot is the most
# convincing silent failure available to it — the numbers stay plausible and stop meaning anything.
#
# So: hash the binary, and any change to lib.rs or trace.rs mints a new snapshot automatically.
#
# `shasum` first, `sha256sum` second: the first is the one macOS has (it is perl's, and every
# measurement in research/ was keyed with it), the second is what a Linux box without perl has.
# Same digest either way, so the two agree on the filename and share one cache — as does
# `ipod-boot from-idle`, which computes SHA-256 itself for exactly this reason.
KEY=$( { shasum -a 256 "$TRACE" 2>/dev/null || sha256sum "$TRACE"; } | cut -c1-16 )
SNAP="$CACHE/idle-$KEY-$SNAP_AT.snap"
DISK="$CACHE/idle-$KEY-$SNAP_AT.img"

if [ ! -f "$SNAP" ]; then
  echo "building snapshot at $SNAP_AT for trace $KEY (one-off, ~80 s) …" >&2
  rm -f "$DISK"
  WORKDISK="$DISK" BUDGET=$((SNAP_AT + 1000000)) TRACE="$TRACE" \
    "$HERE/retail-boot.sh" --clock=5 --snapshot="$SNAP_AT:$SNAP" >&2
  # A partial snapshot is worse than none — it would restore and quietly under-run.
  [ -s "$SNAP" ] || { echo "snapshot was not written; refusing to continue" >&2; exit 1; }
fi

WORKDISK="$DISK" BUDGET="$BUDGET" TRACE="$TRACE" \
  exec "$HERE/retail-boot.sh" --clock=5 --restore="$SNAP" "$@"
