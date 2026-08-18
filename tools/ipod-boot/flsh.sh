#!/bin/sh
# Boot one of the NOR flash's own images: disk mode, diagnostics, or the disk scanner.
#
# The flash directory at 0xffe00 lists five 40-byte entries — disk, diag, scan, logo, vmcs — each
# with the same contract as `osos`: a raw ARM image loaded at 0x10000000 and entered there. So
# these boot exactly like RetailOS does, and they are far smaller: diag is 200 KB against 7.5 MB.
#
# Diagnostics is Apple's OWN hardware test suite, which makes it an oracle of a different kind from
# Rockbox — same codebase family as RetailOS, and it exists to report what the hardware is doing.
#
#   ./flsh.sh            # diagnostics
#   IMG=disk ./flsh.sh   # disk mode
set -eu
HERE=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$HERE/../.." && pwd)
RES="$ROOT/resources"

: "${TRACE:=$HOME/dev/.cargo-target/release/trace}"
: "${IMG:=diag}"
: "${FLASH:=$RES/internal_rom_000000-0FFFFF/internal_rom_000000-0FFFFF.bin}"
: "${DISK:=$RES/drives/ipod8g.img}"
: "${BUDGET:=200000000}"

# The two `--rdval` guesses that used to live here are gone, for the same reason they left
# cold-boot.sh: 0x70000030 bit 27 and 0x7000003c bit 31 are the external memory bus controller and
# it is modelled (`Xmb` in lib.rs, installed unconditionally). A/B'd before removing them, and the
# result is stronger than "byte-identical": `--readlog` says **neither register is read even once**
# by either image, against a positive control in the same run that logged 2 000 000 reads of
# 0xb0020000. All four variants — as-is, as-is plus the COP/PLL pair that `--rdval` suppresses,
# flags removed, flags removed plus `--nor` — produce identical output apart from the banner lines.
# See research/04 §"Retiring the flags in flsh.sh and flash-update.sh".
#
# `--nor` makes the flash a JEDEC part rather than a byte array, matching cold-boot.sh. It is
# currently unexercised here — both images report `nor: 0 sector erases, 0 words programmed`,
# because neither gets far enough to issue a flash command — and is passed so that when one does,
# it meets a device instead of a mismatch.
#
# NOTE, measured 2026-08-14: this recipe does not currently boot either image. `diag` spins at
# 0x1000c6a0 polling an unmapped halfword at 0xb0020000 (51 185 488 reads in a 200 M budget), and
# `disk` goes Lost after 127 952 instructions. That is why the A/B above is inert — the flags could
# not have mattered — and it is an open defect, not a property of the flags.
exec "$TRACE" "$BUDGET" \
  --osos="$RES/derived/fw/flsh/$IMG.bin" --boot-osos \
  --flash="$FLASH" --disk="$DISK" --sysinfo \
  --bcm --pmu --nor "$@"
