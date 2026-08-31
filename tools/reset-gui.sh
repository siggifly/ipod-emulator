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
#   tools/reset-gui.sh cache      drop everything regenerable — next start is a COLD BOOT
#   tools/reset-gui.sh settings   the above, plus devices and preferences → first-run wizard
#   tools/reset-gui.sh all        everything, including the drives
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
# cache     everything the program can rebuild: parked machines, their frames, downloaded
#           IPSWs. Costs nothing but time to lose — the next start is the cold boot you
#           probably wanted to test anyway, and a download you will do once.
# settings  + settings.txt. Devices and preferences go; the wizard runs again. The drives
#           stay, so nothing has to be rebuilt.
# all       + drives. `my-5.5g.img` is rebuildable from an IPSW, which is exactly why it is
#           only in this level: rebuildable is not the same as cheap, and a drive the user
#           supplied is not rebuildable at all.
case "$LEVEL" in
  show)     TARGETS=() ;;
  cache)    TARGETS=(cache) ;;
  settings) TARGETS=(cache settings.txt) ;;
  all)      TARGETS=(cache settings.txt drives) ;;
  *)        echo "unknown level: $LEVEL"; echo "use: show | cache | settings | all"; exit 2 ;;
esac

echo "data directory:"
echo "  $DATA"
echo
echo "current state:"
for p in cache drives settings.txt; do
  if [ -e "$DATA/$p" ]; then
    printf '  %-14s %8s\n' "$p" "$(du -sh "$DATA/$p" 2>/dev/null | cut -f1)"
  else
    printf '  %-14s %8s\n' "$p" "—"
  fi
done

if [ "$LEVEL" = "show" ]; then
  echo
  echo "nothing moved. levels:"
  echo "  cache      next start is a cold boot; devices and drives kept"
  echo "  settings   also forgets devices and preferences; drives kept"
  echo "  all        also moves the drives"
  # Pre-split leftovers. `snapshots/` and `firmware/` used to sit beside `drives/`, under one
  # policy that had to be as strict as the strictest thing in it — which is how 301 MB of
  # regenerable restore points ended up guarded like an irreplaceable disk image. They are
  # reported rather than moved, because deciding what to do with the operator's data is the
  # operator's call and this script's whole discipline is that it never makes that call.
  for p in snapshots firmware; do
    if [ -e "$DATA/$p" ]; then
      echo
      echo "left over from before the cache split: $p ($(du -sh "$DATA/$p" 2>/dev/null | cut -f1))"
      echo "  nothing reads it now; it lives under cache/ today. to reclaim:"
      echo "    mv \"$DATA/$p\" \"$STASH_ROOT/\""
    fi
  done
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
