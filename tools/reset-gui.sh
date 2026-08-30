#!/usr/bin/env bash
#
# Put the window back to a known state between manual tests.
#
# Manual GUI testing is only worth anything if each run starts from the same place, and this
# program deliberately remembers a great deal between launches — a parked machine resumes at
# 1.6 G instructions rather than cold-booting, which silently turns "test the boot" into "test
# the restore".
#
# NOTHING IS EVER DELETED. AGENTS.md §3: resets are `mv` aside, and the script says where it
# put things. Disk images are sometimes the only copy of an iPod somebody owns, and a reset
# script that removes one is a reset script that eventually removes the wrong one. Everything
# lands in a timestamped directory beside the data directory, and putting it back is one `mv`.
#
# Usage:
#   tools/reset-gui.sh            what a reset would move, and nothing else
#   tools/reset-gui.sh snapshot   drop the parked machine — next start is a COLD BOOT
#   tools/reset-gui.sh settings   the above, plus devices and preferences → first-run wizard
#   tools/reset-gui.sh all        everything, including drives and downloaded firmware
#
set -u

DATA="${IPOD_EMULATOR_DATA:-$HOME/Library/Application Support/ipod-emulator}"
STASH_ROOT="$(dirname "$DATA")/ipod-emulator-reset"

if [ ! -d "$DATA" ]; then
  echo "no data directory at:"
  echo "  $DATA"
  echo "nothing to reset — the next launch is already a first run."
  exit 0
fi

LEVEL="${1:-show}"

# ── what each level touches ──────────────────────────────────────────────────────────────
#
# snapshot  the parked machine and its frame. Costs nothing to lose: the next start is the
#           cold boot you probably wanted to test anyway.
# settings  + settings.txt. Devices and preferences go; the wizard runs again. Drives and
#           downloaded firmware stay, so nothing has to be rebuilt or re-fetched.
# all       + drives and firmware. `my-5.5g.img` is rebuildable from an IPSW, and the IPSWs
#           re-download — but both are slow, which is why they are only in this level.
case "$LEVEL" in
  show)     TARGETS=() ;;
  snapshot) TARGETS=(snapshots) ;;
  settings) TARGETS=(snapshots settings.txt) ;;
  all)      TARGETS=(snapshots settings.txt drives firmware) ;;
  *)        echo "unknown level: $LEVEL"; echo "use: show | snapshot | settings | all"; exit 2 ;;
esac

echo "data directory:"
echo "  $DATA"
echo
echo "current state:"
for p in snapshots settings.txt drives firmware; do
  if [ -e "$DATA/$p" ]; then
    printf '  %-14s %8s\n' "$p" "$(du -sh "$DATA/$p" 2>/dev/null | cut -f1)"
  else
    printf '  %-14s %8s\n' "$p" "—"
  fi
done

if [ "$LEVEL" = "show" ]; then
  echo
  echo "nothing moved. levels:"
  echo "  snapshot   next start is a cold boot; devices and drives kept"
  echo "  settings   also forgets devices and preferences; drives kept"
  echo "  all        also moves drives and downloaded firmware"
  exit 0
fi

STASH="$STASH_ROOT/$(date +%Y%m%d-%H%M%S)-$LEVEL"
mkdir -p "$STASH"

echo
echo "moving aside → $STASH"
moved=0
for p in "${TARGETS[@]}"; do
  if [ -e "$DATA/$p" ]; then
    mv "$DATA/$p" "$STASH/$p"
    printf '  moved %-14s %s\n' "$p" "$(du -sh "$STASH/$p" 2>/dev/null | cut -f1)"
    moved=$((moved + 1))
  fi
done

if [ "$moved" -eq 0 ]; then
  rmdir "$STASH" 2>/dev/null
  echo "  nothing was there to move — already reset."
  exit 0
fi

# The check that makes this trustworthy rather than merely reassuring: read the directory back
# and confirm the things are actually gone. A reset script that reports success without
# looking is the exact failure mode this project keeps being bitten by.
echo
echo "verifying:"
fail=0
for p in "${TARGETS[@]}"; do
  if [ -e "$DATA/$p" ]; then
    echo "  STILL PRESENT: $p — the reset did not take"
    fail=1
  fi
done
[ "$fail" -eq 0 ] && echo "  all $moved item(s) gone from the data directory"

echo
echo "to undo:"
echo "  mv \"$STASH\"/* \"$DATA\"/"
