#!/bin/sh
# Run Rockbox on the emulator as a source-available oracle.
#
# RetailOS is stripped C++ with no symbols; Rockbox drives the same PP5022 hardware and ships an
# ELF with full symbols, so a divergence names a function and a line instead of a hex address.
#
#   ./rockbox.sh                 # the main binary, warm-entered at 0x10000000
#   IMG=rb-bootloader.raw ./rockbox.sh
#
# WITH THE DEFAULT DISK, THIS RUNS ROCKBOX ON A VOLUME ROCKBOX IS NOT INSTALLED ON.
#
# `ipod8g.img` is a stock Apple volume: no `.rockbox`, no theme, no fonts. Rockbox mounts it, finds
# none of its own files, and falls back silently to the 8 px sysfont compiled into the binary — the
# menu still draws, so nothing looks wrong, and the project shipped a screenshot of it for a day.
# There is no error to grep for: a themeless install is an ordinary condition for Rockbox.
#
# To run it against a real install, and get the 15 px font `settings_list.c` actually asks for:
#
#   cp -c ../../resources/drives/ipod8g.img /tmp/rb.img
#   unzip -q ../../resources/vendor/rockbox/bin/rockbox-ipodvideo-4.0.zip -d /tmp/rbzip
#   ipod-boot put-files /tmp/rb.img /tmp/rbzip
#   DISK=/tmp/rb.img ./rockbox.sh --disk-writable
#
# `--disk-writable` is not optional there: Rockbox writes to a volume that has `.rockbox` on it, and
# without it the boot panics at `dc_writeback_callback()` about 20 M instructions in, long before a
# font is loaded. On the stock volume it changes nothing — there is nothing to write to — which is
# why its absence went unnoticed. Both halves are measured in research/06.
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
