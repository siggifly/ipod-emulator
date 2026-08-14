#!/bin/sh
# Cold boot: run Apple's own first-stage bootloader out of the iPod's NOR flash.
#
# This is the reproducible recipe for B7's cold-boot path. It enters at 0x0 — where the CPU fetches
# out of reset — so Apple's bootloader performs SDRAM bring-up, uploads the video co-processor's
# firmware, talks to the PMU, and reads the disk, instead of us reconstructing the state it leaves.
#
# Paths default to the gitignored resources/ tree; override by exporting them.
#
#   ./cold-boot.sh                       # default budget
#   BUDGET=6000000000 ./cold-boot.sh     # ~80 s of simulated iPod time
#   ./cold-boot.sh --clock=5 --profile   # 15x-faster simulated time
set -eu

HERE=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$HERE/../.." && pwd)
RES="$ROOT/resources"

: "${TRACE:=$HOME/dev/.cargo-target/release/trace}"
: "${FLASH:=$RES/internal_rom_000000-0FFFFF/internal_rom_000000-0FFFFF.bin}"
: "${DISK:=$RES/derived/disk/ipod8g.img}"
: "${BUDGET:=150000000}"

# The two `--rdval` guesses that used to live here — 0x70000030 bit 27 and 0x7000003c bit 31 —
# are gone: both are the external memory bus controller, and it is modelled now. Bit 30 of
# 0x70000030 is the NOR write gate and bit 27 is the controller's ready flag; bit 24 of
# XMB_RAM_CFG is the SDRAM configuration command and bit 31 its completion. Removing both flags
# leaves a 600 M-instruction boot byte-identical. See research/12 #1 and #2.
#
# There is deliberately no `--osos=` here either. Handing a cold boot the image it exists to load
# was ledger bypass #14; the ROM finds `osos` in the firmware directory, DMAs 7 559 680 bytes into
# SDRAM, and what lands is byte-identical to `OSOS_correct.bin`. Measured, not assumed — the run is
# otherwise identical with the flag and without it, down to the instruction.
#
# `--nor` makes the flash a JEDEC device rather than a read-only region. The bootloader identifies
# the chip before it will touch it, and against bytes that reply is `0x1ffe`/`0xea00` — the reset
# branch read as two IDs — so its 40 command writes landed unmapped and no row of its device table
# matched. With the model it reads SST `0xbf`/`0x273f` and selects row 3 — `SST39WF800A`, the
# spelling iPodLinux and the EE Times 5.5G BOM both carry. See research/12 #12 and its
# §"The flash part". (This comment said row 4 / `0x2781` until 2026-08-14, from before that switch.)
exec "$TRACE" "$BUDGET" \
  --boot-osos --cold-boot \
  --flash="$FLASH" --disk="$DISK" \
  --bcm --pmu --nor "$@"
