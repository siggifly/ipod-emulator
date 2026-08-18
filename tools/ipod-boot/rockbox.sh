#!/bin/sh
# Run Rockbox on the emulator as a source-available oracle.
#
# RetailOS is stripped C++ with no symbols; Rockbox drives the same PP5022 hardware and ships an
# ELF with full symbols, so a divergence names a function and a line instead of a hex address.
#
#   ./rockbox.sh                 # the main binary, warm-entered at 0x10000000
#   IMG=rb-bootloader.raw ./rockbox.sh
set -eu
HERE=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$HERE/../.." && pwd)
RES="$ROOT/resources"
RB="$RES/vendor/rockbox/bin"

: "${TRACE:=$HOME/dev/.cargo-target/release/trace}"
: "${IMG:=rb-main.raw}"
: "${FLASH:=$RES/roms/retail_5g_MA146_HwVr000B0005_internal_rom_000000-0FFFFF.bin}"
: "${DISK:=$RES/drives/ipod8g.img}"
: "${BUDGET:=200000000}"

# Rockbox's own contract, per tools/scramble.c + bootloader/ipod.c: the image is linked for
# address 0, loaded to DRAM_START (0x10000000) and entered there; it remaps SDRAM to 0 itself.
exec "$TRACE" "$BUDGET" \
  --osos="$RB/$IMG" --boot-osos \
  --flash="$FLASH" --disk="$DISK" --sysinfo \
  --bcm --pmu "$@"
