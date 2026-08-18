#!/bin/sh
# Warm boot: enter RetailOS directly at 0x10000000, with the bootloader's handoff faked.
#
# The counterpart to cold-boot.sh. Nothing runs Apple's first-stage bootloader here, so the state
# it leaves behind has to be installed by hand — that is bypass #4 (`--sysinfo`) and, inside it,
# the Gestalt ID at `sysinfo+0x84` that was bypass #5.
#
# It existed only as a command line pasted into research/02 until bypass #5 needed re-validating,
# and a bypass whose recipe lives in prose cannot be re-measured. Same defaults, same overrides and
# the same instrument flags as cold-boot.sh, so the two are comparable.
#
#   ./warm-boot.sh --clock=5
#   BUDGET=600000000 ./warm-boot.sh --clock=5 --profile
set -eu

HERE=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$HERE/../.." && pwd)
RES="$ROOT/resources"

: "${TRACE:=$HOME/dev/.cargo-target/release/trace}"
: "${FLASH:=$RES/roms/retail_5g_MA146_HwVr000B0005_internal_rom_000000-0FFFFF.bin}"
: "${DISK:=$RES/drives/ipod8g.img}"
: "${OSOS:=$RES/derived/fw/OSOS_correct.bin}"
: "${BUDGET:=600000000}"

# `--osos-at=0x04000000` is the base that makes RetailOS's scatter-load source pointers land inside
# the image; research/02 found it by trying bases rather than by guessing one.
exec "$TRACE" "$BUDGET" \
  --osos="$OSOS" --boot-osos --osos-at=0x04000000 --sysinfo \
  --flash="$FLASH" --disk="$DISK" \
  --bcm --pmu "$@"
