#!/usr/bin/env bash
#
# The whole flow, from an IPSW, on a real NOR dump first and then a synthesised one.
#
# The question it answers is the operator's: **does the path a new user actually walks work?** Not
# "does the emulator boot the one hand-repaired drive in `resources/`" — that drive was surgically
# mended in August (research/03 §40, an `osos` body missing its first sector) and every green run
# taken on it says nothing about the drive `Make me one` builds from an IPSW.
#
# Real NOR first, then synthetic, because that ordering is what makes a synthetic pass mean
# something. If a target passes on both, the ROM was not carrying it.
#
# ── Four ways a run of this can lie, each one hit while writing it ─────────────────────────────
#
# 1. THE CLOCK. `--clock=5` is the research accelerant; 75 is the faithful ratio (NEXT.md §"On
#    --clock=5"). At 75 a 1.6 G budget is 21 s of simulated time, and RetailOS's boot ends inside
#    the bootloader printing `Bootloader could not execute target image!` — which reads as a broken
#    disk and is a timeout. Every row below states the clock it used.
# 2. THE BUDGET. RetailOS's language menu first draws at ~1.05 G instructions at `--clock=5`. A
#    shorter run photographs a blank panel and looks like a rendering failure.
# 3. A REUSED IMAGE. Every run gets a fresh scratch directory and its own CACHE, and the header
#    says so. A row that silently restored somebody else's boot is the thing this exists to rule
#    out.
# 4. AN ABSENCE NOBODY CONTROLLED FOR. Each target reports the number that would be non-zero if it
#    worked, not a bare PASS — so a zero can be read as a zero rather than as a verdict.
#
# **It prints no identifiers.** The real dump carries a live serial and FireWire GUID (AGENTS.md
# §2); this reports the NOR by role, never by content.
#
# Usage:
#   tools/clean-run-matrix.sh            the matrix
#   tools/clean-run-matrix.sh --keep     leave the scratch tree for inspection
#
set -u

BIN="${IPOD_BOOT:-$(dirname "$0")/../../../.cargo-target/release/ipod-boot}"
[ -x "$BIN" ] || BIN="$(command -v ipod-boot || true)"
if [ ! -x "${BIN:-}" ]; then
  echo "no ipod-boot binary. build it: cargo build --release"
  exit 2
fi
REPO="$(cd "$(dirname "$0")/.." && pwd)"
RES="$REPO/resources"

SCRATCH="$(mktemp -d "${TMPDIR:-/tmp}/ipod-matrix-XXXXXX")"
KEEP=0
[ "${1:-}" = "--keep" ] && KEEP=1
cleanup() { [ "$KEEP" -eq 1 ] && echo "scratch kept: $SCRATCH" || rm -rf "$SCRATCH"; }
trap cleanup EXIT

# One cache for this run and nothing else, so no row can inherit a boot it did not do.
export CACHE="$SCRATCH/cache"
mkdir -p "$CACHE"

REAL_NOR="${FLASH:-$(ls "$RES"/roms/*.bin 2>/dev/null | head -1)}"

# ── which firmware belongs with this ROM ───────────────────────────────────────────────────────
#
# **5G and 5.5G are separate downloads and they are not interchangeable by name.** Apple ships a
# model's software under an updater family — 13 and 20 are the 5G's, 25 is the 5.5G's — and the
# number is the one in the filename. The first version of this script took the last `.ipsw`
# alphabetically, got `iPod_7.1.4.1` (another model entirely), built a drive with no `rsrc` in it,
# and reported `retailos FAIL` for both NOR sources. That verdict was about the script.
#
# So the ROM is asked what it takes, rather than the filename being read for a hint. `ipod-boot
# facts` reports `Takes updater family` off the ROM's own `Mod#` through the sourced model table.
fact() { "$BIN" facts "$1" 2>/dev/null | sed -n "s/^$2  *//p"; }

WANT_FAMILY=""
NOR_MODEL=""
if [ -f "${REAL_NOR:-}" ]; then
  NOR_MODEL="$(fact "$REAL_NOR" "Model")"
  WANT_FAMILY="$(fact "$REAL_NOR" "Takes updater family" | sed 's/ or /|/g')"
fi

pick_ipsw() {
  # Newest first within the families this ROM accepts. `sort -V` so 25.1.3 beats 25.1.2.
  local f
  for f in $(ls "$RES"/software/ipsw/*.ipsw 2>/dev/null | sort -Vr); do
    local n; n="$(basename "$f" | sed -n 's/^iPod_\([0-9]*\)\..*/\1/p')"
    [ -n "$n" ] || continue
    case "|$WANT_FAMILY|" in *"|$n|"*) echo "$f"; return ;; esac
  done
}

if [ -n "${IPSW:-}" ]; then
  :                                  # the operator named one; use it and say which
elif [ -n "$WANT_FAMILY" ]; then
  IPSW="$(pick_ipsw)"
else
  IPSW=""                            # cannot say — better than picking one at random
fi

echo "clean-run matrix"
echo "  binary      $BIN"
echo "  scratch     $SCRATCH   (fresh; CACHE inside it, so nothing is reused)"
echo "  real NOR    $([ -f "${REAL_NOR:-}" ] && echo "${NOR_MODEL:-unreadable}, takes updater family ${WANT_FAMILY//|/ or }" || echo "ABSENT")"
echo "  IPSW        $([ -f "${IPSW:-}" ] && basename "$IPSW" || echo "ABSENT — no .ipsw in a family this ROM takes")"
echo

# ── the drive under test: built from an IPSW, the way a new user gets one ──────────────────────
DRIVE="$SCRATCH/from-ipsw.img"
echo "building a drive from the IPSW (this is the path under test, not a fixture) …"
if [ -f "${IPSW:-}" ] && "$BIN" make-disk "$IPSW" "$DRIVE" > "$SCRATCH/make-disk.log" 2>&1; then
  echo "  built: $("$BIN" facts "$DRIVE" 2>/dev/null | awk '/Firmware images/{ $1=""; $2=""; print }' | sed 's/^ *//')"
else
  echo "  FAILED — see $SCRATCH/make-disk.log"
  DRIVE=""
fi

# ── a synthesised ROM, for the second half of every row ────────────────────────────────────────
SYN_NOR="$SCRATCH/synthetic.bin"
"$BIN" make-nor --seed 1 "$SYN_NOR" > "$SCRATCH/make-nor.log" 2>&1 \
  && echo "  synthetic NOR minted (seed 1)" \
  || { echo "  synthetic NOR FAILED — see $SCRATCH/make-nor.log"; SYN_NOR=""; }
echo

printf '%-12s %-10s %-8s %-9s %s\n' TARGET NOR CLOCK VERDICT EVIDENCE
printf '%-12s %-10s %-8s %-9s %s\n' ------ --- ----- ------- --------

row() { printf '%-12s %-10s %-8s %-9s %s\n' "$1" "$2" "$3" "$4" "$5"; }

# `retailos` — did the CPU arrive at 0x10000000, and did the panel draw something that is not the
# boot's white screen? Both, because either alone has been misread here: an entry with a blank
# panel is a hung OS, and a drawn panel without an entry is the bootloader's own splash.
run_retailos() {
  local nor="$1" label="$2"
  local out="$SCRATCH/retailos-$label"
  [ -n "$nor" ] && [ -f "$nor" ] || { row retailos "$label" 5 SKIPPED "no NOR"; return; }
  [ -n "$DRIVE" ] || { row retailos "$label" 5 BLOCKED "the IPSW drive did not build"; return; }
  mkdir -p "$out"
  FLASH="$nor" DISK="$DRIVE" BUDGET=1400000000 "$BIN" retail --clock=5 --clickwheel \
    --enterlog=0x10000000 --bcm-film=0xE0000:140:F0:25000000:"$out" > "$out.log" 2>&1
  local hit pics
  hit=$(grep -oE '0x10000000 +unnamed +(x[0-9]+|NEVER REACHED)' "$out.log" | head -1 | awk '{print $3}')
  pics=$(grep -c '^[0-9]' "$out/frames.tsv" 2>/dev/null || echo 0)
  if [ "$hit" = "NEVER" ] || [ -z "$hit" ]; then
    local why; why=$(grep -c 'could not execute target image' "$out.log")
    row retailos "$label" 5 FAIL \
      "0x10000000 never reached; $([ "$why" -gt 0 ] && echo 'bootloader refused the image' || echo 'no refusal printed either'), $pics pictures"
  elif [ "${pics:-0}" -ge 3 ]; then
    row retailos "$label" 5 PASS "entered $hit, $pics distinct pictures -> $out"
  else
    row retailos "$label" 5 PARTIAL "entered $hit but only $pics pictures -> $out"
  fi
}

# `diag` — one of the NOR's own images. A synthesised ROM carries `logo` only, so this is expected
# to be unavailable there, and saying so is the point: it is the row that proves the two NOR
# sources are not interchangeable.
run_diag() {
  local nor="$1" label="$2"
  local out="$SCRATCH/diag-$label"
  [ -n "$nor" ] && [ -f "$nor" ] || { row diag "$label" 5 SKIPPED "no NOR"; return; }
  local imgs; imgs=$("$BIN" facts "$nor" 2>/dev/null | awk '/^Images/{ $1=""; print }' | sed 's/^ *//')
  case "$imgs" in
    *diag*) ;;
    *) row diag "$label" 5 N/A "this ROM carries [$imgs] — diag is not in it"; return ;;
  esac
  mkdir -p "$out"
  FLASH="$nor" IMG=diag BUDGET=600000000 "$BIN" flsh --clock=5 \
    --bcm-film=0xE0000:140:F0:25000000:"$out" > "$out.log" 2>&1
  local pics; pics=$(grep -c '^[0-9]' "$out/frames.tsv" 2>/dev/null || echo 0)
  if [ "${pics:-0}" -ge 2 ]; then
    row diag "$label" 5 PASS "$pics distinct pictures -> $out"
  else
    row diag "$label" 5 FAIL "$pics distinct pictures — nothing drew"
  fi
}

for pair in "real:$REAL_NOR" "synthetic:$SYN_NOR"; do
  run_retailos "${pair#*:}" "${pair%%:*}"
done
for pair in "real:$REAL_NOR" "synthetic:$SYN_NOR"; do
  run_diag "${pair#*:}" "${pair%%:*}"
done

echo
echo "not yet in this matrix, and each needs a step this script does not take:"
echo "  rockbox   an install onto the IPSW drive, which currently refuses: the drive still"
echo "            carries Apple's updater and the room a bootloader needs is the room it"
echo "            occupies. Start it once first (ipod-boot flash-update)."
echo "  title     a plaintext .ipg played on the iPod's own screen. resources/games/plaintext"
echo "            holds them; the window plays them, and this harness does not drive the window."
