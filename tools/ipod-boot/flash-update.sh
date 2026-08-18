#!/bin/sh
# Run Apple's flash updater: the `aupd` path, with the real firmware directory.
#
# This is the reproduction for ledger #12. It differs from cold-boot.sh in three ways, all of them
# load-bearing:
#
#   * the RETAIL bootrom, not the prototype dump. The prototype ROM reads its firmware partition at
#     4x the MBR's LBA (2 KiB blocks against the image's 512-byte ones) and, handed this `aupd`,
#     reads the whole image and then runs an orderly power-off without printing a line. The retail
#     ROM reads LBA 63 as written and runs the updater.
#   * a firmware partition written from the pristine `Firmware-20.6.3` at the MBR's own partition
#     start, so the `!ATA` directory is Apple's — `osos`, `rsrc` AND `aupd`, all three, with nothing
#     removed. That directory is what makes the ROM choose the updater over the OS.
#   * `--disk-writable`. The updater's LAST act is a WRITE SECTORS to the directory it was launched
#     from, setting `aupd`'s +0x08 to 1 so the next boot skips it. Read-only, that write aborts and
#     the machine updates forever.
#
# It boots twice on purpose. The first boot is the update; the second is the proof that the update
# took. Expect `Running 'aupd'` then `iPod CFI Flash Firmware update` on the first, and
# `Retail mode` / `Running 'osos'` on the second.
#
#   ./flash-update.sh                 # build the image under $TMPDIR and boot it twice
#   WORK=/path/to/dir ./flash-update.sh
set -eu

HERE=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$HERE/../.." && pwd)
RES="$ROOT/resources"

: "${TRACE:=$HOME/dev/.cargo-target/release/trace}"
: "${FLASH:=$RES/roms/retail_5g_MA146_HwVr000B0005_internal_rom_000000-0FFFFF.bin}"
: "${SRCDISK:=$RES/drives/ipod8g.img}"
: "${FW:=$RES/derived/fw/Firmware-20.6.3}"
: "${BUDGET:=600000000}"
: "${WORK:=${TMPDIR:-/tmp}/ipod-flash-update}"

mkdir -p "$WORK"
DISK="$WORK/disk.img"

# The updater writes to this image, so it gets its own copy. `cp -c` is an APFS clone: instant, and
# it costs only the blocks that are then written. `--reflink=auto` is the btrfs / XFS equivalent —
# not a flag Apple's `cp` has, which is why it is the second rung rather than the first; see
# retail-boot.sh's `clone` for the full note. macOS never reaches past rung one.
if [ ! -f "$DISK" ]; then
  cp -c "$SRCDISK" "$DISK" 2>/dev/null \
    || { rm -f "$DISK"; cp --reflink=auto "$SRCDISK" "$DISK" 2>/dev/null; } \
    || { rm -f "$DISK"; cp "$SRCDISK" "$DISK"; }
  # MBR partition 0 starts at LBA 63 and is 27 140 sectors — 13 895 680 bytes, which is exactly the
  # size of the pristine firmware. It fits with nothing left over, which is how we know the offset.
  dd if="$FW" of="$DISK" bs=512 seek=63 conv=notrunc status=none
  echo "built $DISK — firmware partition written from $FW"
fi

# Three retired bypasses used to be passed here, and they mattered more than tidiness: this recipe
# is the one that PROVED #12's retirement, so that proof had been obtained with `--rdval` on
# 0x70000030 and 0x7000003c (ledger #1 and #2, both retired once `Xmb` modelled the bus) and with
# `--i2c-fill=0xff` (ledger #3, retired by `--pmu`). Removed 2026-08-14 and re-proved without them.
#
# A/B'd on both the pristine flash and a deliberately perturbed one, four arms in all. The
# retirement proof reproduces exactly: 248 sector erases, 507 904 words programmed, the same cycle
# tallies, and a repaired flash **bit-identical** between the two flag sets and byte-identical to
# the pristine dump. The only differences anywhere are one ATA DMA transfer and 4 BCM halfwords out
# of 370 606, at the instruction where a budget-exhausted run happens to stop — a real PMU spends
# different simulated time in the I²C poll than a constant 0xff does. See research/04
# §"Retiring the flags in flsh.sh and flash-update.sh".
#
# `--pmu` replaces `--i2c-fill` rather than merely dropping it: #3's retirement condition was a real
# PCF50605, not the absence of a fill.
for boot in 1 2; do
  echo "===== boot $boot ====="
  "$TRACE" "$BUDGET" \
    --boot-osos --cold-boot \
    --flash="$FLASH" --disk="$DISK" --disk-writable \
    --bcm --pmu --nor "$@"
done
