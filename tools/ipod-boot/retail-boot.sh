#!/usr/bin/env bash
#
# Cold boot on the RETAIL hardware configuration — Apple's shipping 5G bootloader and a firmware
# partition it accepts. This is the configuration the project is actually emulating.
#
# `cold-boot.sh` runs a *prototype's* NOR instead (archive.org, "SA JULY 12 2007 ipod video
# prototype firmware dump": serial U1234567890, blank HwId, Mod# M8976, the unpublished
# HwVr 0x000b0011). That was never a decision — it was the first dump we had, and the retail one
# sat in the repo mislabelled `A1238` (the iPod *classic* 6G's model number, though the bytes are
# plainly PP502x) until 2026-08-13.
#
# The difference is not cosmetic. Measured at 600 M instructions, --clock=5, same day:
#
#                              prototype        retail
#   arrivals at address 0      314 (157 resets) 2 — the cold reset and the handoff
#   unmapped accesses          640 reads        none
#   ATA commands               77               96
#   DMA                        60 xfers/7.6 MB  72 xfers/8.1 MB
#
# and only the retail path ever reads `rsrc`: LBA 14864 (the FAT boot sector), 14870 (the FAT),
# then RenderServer.bin, vmcs.bin and the codec libraries, before `0xe0` STANDBY IMMEDIATE spins
# the drive down. The prototype's 157 self-resets are `BX` to address zero through a null `this`
# — see research/20 Addendum 5.
#
# Kept as a separate recipe rather than flipped into cold-boot.sh so that every number already
# recorded in research/ stays attributable to the configuration it was measured on.
set -eu

HERE=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$HERE/../.." && pwd)
RES="$ROOT/resources"

# Mislabelled upstream; the bytes are a retail 5G. SrNm <SERIAL-ROM>, Mod# MA146, HwVr 0x000b0005.
: "${FLASH:=$RES/reference/ipod-bootrom-archive/A1238/internal_rom_000000-0FFFFF.bin}"
# The image whose firmware partition this bootloader accepts — the pristine 13 895 680-byte
# Firmware-20.6.3 written over a partition it fits exactly. The retail ROM validates what the
# prototype waved through, and rejects the other image with Apple's own restore screen.
: "${DISK:=$RES/derived/disk/ipod8g-retail.img}"
# Exported, not merely assigned: cold-boot.sh reads them as environment defaults, and the first
# version of this file set them as plain shell variables — so `exec` passed neither and it silently
# ran the prototype configuration this recipe exists to avoid. It looked like it worked.
export FLASH

# The drive must accept writes, because RetailOS writes to it during boot. Without this the machine
# stops mid-startup: RetailOS bootstraps its own volume — FSInfo, both FATs, Contacts/Calendars/
# Notes, iPod_Control/Device/Accessories — and blocks on RTXC semaphore 0xd1 waiting for a 1-sector
# WRITE DMA to LBA 32894 (the first sector of FAT #1) that a read-only drive aborts. It is not a
# deadlock; it retries on a 3.9 s timeout, forever. See research/20 Addendum 15.
#
# Cloned per run rather than written in place, for two different reasons that both matter:
# `$RES/derived/disk/` is reference material and a recipe must never mutate it, and a disk that
# accumulates state across runs makes every measurement depend on how many times it was run before.
# APFS `cp -c` is a copy-on-write clone — ~3 ms for 8 GB, so a fresh disk per run is free.
#
# Set WORKDISK to keep a disk across runs when you *want* the accumulated state — a second boot
# finding the volume RetailOS built on the first is a real scenario, just not a measurable one.
#
# `clone` tries three copies, in this order, and the order is the whole content of the function:
#
#   cp -c              Apple's clonefile(2). Not a GNU flag — on Linux it is an invalid option.
#   cp --reflink=auto  the btrfs / XFS / bcachefs equivalent, and the rung that was missing until
#                      2026-08-14: without it a Linux run paid a full 8 GB byte copy per boot.
#                      GNU cp never fails for want of reflink support; it silently does a full
#                      copy. So reaching the third rung means neither cp understood either flag.
#   cp                 everything else.
#
# macOS behaviour is unchanged: `cp -c` succeeds on the first rung and the other two are never
# reached, so every number in research/ is measured through the same copy it always was.
clone() {
  cp -c "$1" "$2" 2>/dev/null && return 0
  rm -f "$2"
  cp --reflink=auto "$1" "$2" 2>/dev/null && return 0
  rm -f "$2"
  cp "$1" "$2"
}

if [ -n "${WORKDISK:-}" ]; then
  WORK="$WORKDISK"
  [ -f "$WORK" ] || clone "$DISK" "$WORK"
else
  WORK="${TMPDIR:-/tmp}/ipod-retail-boot-$$.img"
  clone "$DISK" "$WORK"
  trap 'rm -f "$WORK"' EXIT INT TERM
fi
DISK="$WORK"
export DISK

"$HERE/cold-boot.sh" --disk-writable "$@"
