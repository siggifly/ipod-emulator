#!/usr/bin/env bash
#
# The whole flow, from an IPSW, per generation, on a real NOR dump and on a synthesised one.
#
# The question it answers is the operator's: **does the path a new user actually walks work?** Not
# "does the emulator boot the one hand-repaired drive in `resources/`" — that drive was surgically
# mended (research/03 §40, an `osos` body missing its first sector), and a green run on it says
# nothing about the drive somebody builds from an IPSW tonight.
#
# ── The two things this got wrong before, both of which produced a confident FAIL ──────────────
#
# **5G and 5.5G are separate downloads.** Apple ships a model's software under an updater family —
# 13 and 20 are the 5G's, 25 is the 5.5G's — and the number is the one in the filename. Taking the
# last `.ipsw` alphabetically got another model's firmware, built a drive with no `rsrc` in it, and
# reported `retailos FAIL` against both NOR sources. So the ROM is asked what it takes, through
# `ipod-boot facts`, which reads it off the ROM's own `Mod#`.
#
# **A synthesised ROM has no bootloader, and cannot be booted like one.** It carries an identity
# and a logo; Apple's boot ROM is code we do not have and would not ship. Running `ipod-boot
# retail` against one asks a bootloader that is not there to load an OS, and the honest result is
# nothing at all — which read as "synthetic NOR cannot boot RetailOS". It can: `--osos-from-disk`
# lifts the OS out of the drive's own firmware partition and enters it, which is what the window
# does with a synthesised device. Both routes reach the same language picker; the route is chosen
# here by whether the ROM carries Apple's own build string.
#
# ── Four ways a run of this lies, each hit while writing it ────────────────────────────────────
#
# 1. THE CLOCK. `--clock=5` is the research accelerant; 75 is the faithful ratio. At 75 a 1.6 G
#    budget is 21 s of simulated time and the run ends inside the bootloader printing `Bootloader
#    could not execute target image!` — which reads as a broken disk and is a timeout.
# 2. THE BUDGET. RetailOS's language menu first draws at ~1.05 G instructions at `--clock=5`.
# 3. A REUSED IMAGE. Every run gets a fresh scratch tree and its own CACHE. A drive that has been
#    booted before is not a fresh drive: `my-5.5g.img` fails where the same build passes.
# 4. AN ABSENCE NOBODY CONTROLLED FOR. Each row reports the number that would be non-zero if it
#    worked, so a zero reads as a zero rather than as a verdict.
#
# **It prints no identifiers.** A real dump carries a live serial and FireWire GUID (AGENTS.md §2);
# this reports a ROM by model and generation, never by content.
#
# Usage:
#   tools/clean-run-matrix.sh            the matrix
#   tools/clean-run-matrix.sh --keep     leave the scratch tree, and say where it is
#
set -u

REPO="$(cd "$(dirname "$0")/.." && pwd)"
BIN="${IPOD_BOOT:-$REPO/../../.cargo-target/release/ipod-boot}"
[ -x "$BIN" ] || BIN="$(command -v ipod-boot || true)"
TRACE="$(dirname "$BIN")/trace"
if [ ! -x "${BIN:-}" ] || [ ! -x "$TRACE" ]; then
  echo "no ipod-boot / trace binary. build them: cargo build --release"
  exit 2
fi
RES="$REPO/resources"

SCRATCH="$(mktemp -d "${TMPDIR:-/tmp}/ipod-matrix-XXXXXX")"
KEEP=0; [ "${1:-}" = "--keep" ] && KEEP=1
cleanup() { [ "$KEEP" -eq 1 ] && echo && echo "scratch kept: $SCRATCH" || rm -rf "$SCRATCH"; }
trap cleanup EXIT
export CACHE="$SCRATCH/cache"; mkdir -p "$CACHE"

fact() { "$BIN" facts "$1" 2>/dev/null | sed -n "s/^$2  *//p"; }

echo "clean-run matrix"
echo "  scratch   $SCRATCH   (fresh; CACHE inside it, so no row can inherit a boot)"
echo "  clock     5 instructions per simulated microsecond — the research accelerant, not the"
echo "            faithful 75. At 75 none of these budgets finishes booting."
echo

# ── which real dumps we have, and what each one is ─────────────────────────────────────────────
declare -a REAL_5G REAL_55G
for n in "$RES"/roms/*.bin; do
  [ -f "$n" ] || continue
  case "$(fact "$n" "Model")" in
    *"a 5G")   REAL_5G+=("$n") ;;
    *"a 5.5G") REAL_55G+=("$n") ;;
  esac
done

# ── pick the newest IPSW in a family this generation takes ─────────────────────────────────────
pick_ipsw() {                       # $1 = "13|20"
  local f n
  for f in $(ls "$RES"/software/ipsw/*.ipsw 2>/dev/null | sort -Vr); do
    n="$(basename "$f" | sed -n 's/^iPod_\([0-9]*\)\..*/\1/p')"
    [ -n "$n" ] || continue
    case "|$1|" in *"|$n|"*) echo "$f"; return ;; esac
  done
}

printf '%-9s %-6s %-11s %-14s %-8s %s\n' TARGET GEN NOR ROUTE VERDICT EVIDENCE
printf '%-9s %-6s %-11s %-14s %-8s %s\n' ------ --- --- ----- ------- --------
row() { printf '%-9s %-6s %-11s %-14s %-8s %s\n' "$1" "$2" "$3" "$4" "$5" "$6"; }

pictures() { grep -c '^[0-9]' "$1/frames.tsv" 2>/dev/null || echo 0; }
last_digest() { awk '/^[0-9]/{d=$NF} END{print d}' "$1/frames.tsv" 2>/dev/null; }
# **$9, not $8.** The manifest gained a `first_usec` column when the assets were re-anchored in
# simulated time, and every index after `first_instr` shifted by one. This read `held_instr` and
# reported it as a pixel count — `last nonblack 2236000000` — which is obviously wrong to a person
# and passed the `-gt 1000` test silently, so the verdict it fed was arrived at for the wrong reason.
last_nonblack() { awk -F'\t' '/^[0-9]/{n=$9} END{print n+0}' "$1/frames.tsv" 2>/dev/null; }

# **A working drive is a clone, not a byte copy.** The same three-rung ladder `clone_disk` climbs in
# `ipod-gui/src/emu.rs`, whose doc comment says it exists in three places because each front end has
# to make a writable disk before it has anything to share code with. This script is a fourth, and it
# never got the ladder: it paid a full 8 GB `cp` per row. Measured on this machine — **ten minutes a
# row against eleven milliseconds**, and four rows in a run.
#
# 1. `cp -c` — Apple's `clonefile(2)`. Not a GNU flag; on Linux it is an invalid option.
# 2. `cp --reflink=auto` — the btrfs / XFS / bcachefs equivalent. Never fails for want of reflink
#    support; it silently does a full copy instead.
# 3. plain `cp` — anything else.
#
# `rm -f` between rungs because a failed `cp` can leave a truncated destination, and a partial 8 GB
# image that later opens as a valid file is the exact silent failure this project keeps paying for.
#
# The `chmod` is not optional. A clone inherits the source's mode, the pristine images are
# `r--r--r--`, and a working drive that cannot be written is not a working drive — the byte copy
# this replaces produced `rw-------` and so never needed it.
clone_disk() {                      # $1 src  $2 dst
  local flag
  for flag in -c --reflink=auto ''; do
    rm -f "$2"
    if [ -n "$flag" ]; then cp "$flag" "$1" "$2" 2>/dev/null; else cp "$1" "$2"; fi \
      && { chmod u+w "$2"; return 0; }
  done
  rm -f "$2"; return 1
}

# One RetailOS boot. The route is decided by the ROM, not by the caller: a dump with Apple's own
# build string runs Apple's bootloader; a synthesised one is entered through the firmware
# partition, because there is no bootloader in it to run.
boot_retailos() {                   # $1 nor  $2 gen  $3 label  $4 drive
  local nor="$1" gen="$2" label="$3" drive="$4"
  local out="$SCRATCH/os-$gen-$label" work="$SCRATCH/os-$gen-$label.img" route
  [ -n "$drive" ] || { row retailos "$gen" "$label" - BLOCKED "no drive for this generation"; return; }
  mkdir -p "$out"; clone_disk "$drive" "$work"
  # **The descent, not just the boot.** "RetailOS works" has to mean the wheel moves it, and a
  # static picker proves only that something drew once.
  #
  # **Stated in the firmware's clock, and that is what makes this row mean anything.** `BUDGET`
  # counts OUR instructions; `7e30c1f` made a halt cost a cycle, so a fixed budget buys about a
  # fifth of the iPod-time it used to and every recipe written before it silently began measuring a
  # shorter run. This row said `PARTIAL — input changed nothing` for exactly that reason: 2.6 G buys
  # 520 s, and **RetailOS does not answer its first press until 1 423 s** on a cold boot. It was
  # reporting starvation as behaviour. `--until` is the iPod's own clock and `BUDGET` is now only
  # the ceiling that stops a wedged run. See research/12 §"RESOLVED".
  #
  # The spacing is measured, not guessed: after the settle the machine answered at 1491 -> 1649 ->
  # 1790 -> 1854 s, so steps are 150 s apart. `press=select` and a `down=`/`up=` pair were measured
  # side by side at the same anchor and produced byte-identical runs, so the older comment here
  # claiming `press=` is too short for a polling firmware is wrong and the shorter form is used.
  local w="@80s:touch,+2s:press=select,+5s:release"
  w="$w,+1320s:touch,+2s:rotate=+8,+5s:release"
  w="$w,+150s:touch,+2s:rotate=+8,+5s:release"
  w="$w,+150s:touch,+2s:press=select,+5s:release"
  w="$w,+150s:touch,+2s:rotate=+8,+5s:release"
  if [ -n "$(fact "$nor" "Build")" ]; then
    route="Apple ROM"
    FLASH="$nor" DISK="$work" BUDGET=24000000000 "$BIN" retail --clock=5 --until=2200s --clickwheel \
      --wheel="$w" --enterlog=0x10000000 --bcm-film=0xE0000:140:F0:2000000:"$out" > "$out.log" 2>&1
  else
    route="from the drive"
    "$TRACE" 24000000000 --osos-from-disk --boot-osos --flash="$nor" --disk="$work" \
      --disk-writable --sysinfo --bcm --pmu --nor --clock=5 --until=2200s --clickwheel \
      --wheel="$w" --enterlog=0x10000000 --bcm-film=0xE0000:140:F0:2000000:"$out" > "$out.log" 2>&1
  fi
  local hit pics nb vec
  hit=$(grep -oE '0x10000000 +unnamed +(x[0-9]+|NEVER REACHED)' "$out.log" | head -1 | awk '{print $3}')
  pics=$(pictures "$out"); nb=$(last_nonblack "$out")
  # **The vector-page guard turns a symptom into a cause.** A store at 0x18 is RetailOS writing
  # through a null voice pointer onto its own IRQ vector; the next interrupt branches into the boot
  # ROM's signature and the machine wedges. Without this the row reads PARTIAL / "input changed
  # nothing", which is true and tells nobody what to fix. See KNOWN-BUGS.md.
  vec=$(grep -cE '^  0x0018  IRQ' "$out.log" 2>/dev/null || true)
  if [ "${vec:-0}" -gt 0 ]; then
    row retailos "$gen" "$label" "$route" FAIL \
      "wrote through NULL onto the IRQ vector at 0x18 — the voice pool is empty (KNOWN-BUGS)"
  elif [ "${hit:-NEVER}" = "NEVER" ]; then
    row retailos "$gen" "$label" "$route" FAIL "0x10000000 never reached, $pics pictures"
  elif [ "${pics:-0}" -ge 7 ] && [ "${nb:-0}" -gt 1000 ] && [ "${nb:-0}" -ne 76800 ]; then
    # More pictures than a boot alone produces (five), so the wheel moved it somewhere.
    row retailos "$gen" "$label" "$route" PASS "entered $hit, $pics pictures, last $(last_digest "$out")"
  else
    row retailos "$gen" "$label" "$route" PARTIAL \
      "entered $hit, $pics pictures (a boot alone gives 5, so input changed nothing), last nonblack $nb"
  fi
}

# `diag` is one of the NOR's own images, so it exists only where the ROM carries it. A synthesised
# ROM has `logo` and nothing else, and saying so is the point: it is the row that proves the two
# NOR sources are not interchangeable.
boot_diag() {                       # $1 nor  $2 gen  $3 label  $4 drive
  local nor="$1" gen="$2" label="$3" drive="$4"
  local out="$SCRATCH/diag-$gen-$label" work="$SCRATCH/diag-$gen-$label.img"
  case "$(fact "$nor" "Images")" in
    *diag*) ;;
    *) row diag "$gen" "$label" - N/A "this ROM carries [$(fact "$nor" "Images")]"; return ;;
  esac
  mkdir -p "$out"; [ -n "$drive" ] && clone_disk "$drive" "$work" || work="$drive"
  FLASH="$nor" DISK="$work" IMG=diag BUDGET=600000000 "$BIN" flsh --clock=5 --clickwheel \
    --bcm-film=0xE0000:140:F0:25000000:"$out" > "$out.log" 2>&1
  local pics nb; pics=$(pictures "$out"); nb=$(last_nonblack "$out")
  if [ "${pics:-0}" -ge 2 ] && [ "${nb:-0}" -gt 1000 ]; then
    row diag "$gen" "$label" "NOR image" PASS "$pics pictures, last $(last_digest "$out")"
  else
    row diag "$gen" "$label" "NOR image" FAIL "$pics pictures, last nonblack $nb"
  fi
}

# Rockbox is entered as a raw image the way its own build produces it — the oracle, run exactly as
# upstream ships it, which is the only way it can tell us we are wrong (AGENTS.md §4).
#
# **Its digest is not a fingerprint and must not be compared across runs.** Rockbox's main menu
# draws a clock in the status bar, so two identical runs minutes apart differ in a few hundred
# pixels and hash differently. The digest is printed because a *change of screen* is still visible
# in it; RetailOS's language picker has no clock and its digest is stable, which is why the three
# `retailos` rows can be compared to each other and these cannot.
boot_rockbox() {                    # $1 nor  $2 gen  $3 label  $4 drive
  local nor="$1" gen="$2" label="$3" drive="$4"
  local out="$SCRATCH/rb-$gen-$label" work="$SCRATCH/rb-$gen-$label.img"
  [ -f "$RES/vendor/rockbox/bin/rb-main.raw" ] || {
    row rockbox "$gen" "$label" - BLOCKED "resources/vendor/rockbox/bin/rb-main.raw is not here"; return; }
  mkdir -p "$out"; [ -n "$drive" ] && clone_disk "$drive" "$work" || work="$drive"
  FLASH="$nor" DISK="$work" BUDGET=800000000 "$BIN" rockbox --clock=5 --clickwheel \
    --bcm-film=0xE0000:140:F0:25000000:"$out" > "$out.log" 2>&1
  local pics nb; pics=$(pictures "$out"); nb=$(last_nonblack "$out")
  if [ "${pics:-0}" -ge 2 ] && [ "${nb:-0}" -gt 100 ]; then
    row rockbox "$gen" "$label" "raw image" PASS "$pics pictures, last $(last_digest "$out")"
  else
    row rockbox "$gen" "$label" "raw image" FAIL "$pics pictures, last nonblack $nb"
  fi
}

# `doom` — Rockbox's plugin, reached by a SHORTCUT rather than a counted descent.
#
# **Rockbox accelerates the wheel**, and research/06 measured what that costs a script: two runs of
# the same forward descent, 24 clicks and 18 clicks, landed on `Shortcuts` and on `Settings` — half
# the clicks moved less than half as far. A fixed click count cannot target a menu item, and a
# descent several menus deep multiplies the error.
#
# The design around it: `Shortcuts` is the LAST item of the main menu, so one small backward step
# from `Files` wraps onto it — too short to accelerate, and where it lands does not depend on how
# far it travelled. Same trick inside Doom's own menu: `Play Game` is index 4 of 6, so it is two
# backward steps from `Game` rather than four forward ones.
#
# Needs a drive carrying `/.rockbox/doom/rockdoom.wad` (the wiki attachment, 285 048 B, 186 lumps),
# an IWAD as `doom2.wad` (Freedoom 0.13.0 — Rockbox's own manual names it as the free substitute),
# and `/.rockbox/shortcuts.txt`. Reported BLOCKED rather than FAIL when they are absent, because a
# missing asset is not a failing emulator.
boot_doom() {                       # $1 nor  $2 gen  $3 label  $4 drive
  local nor="$1" gen="$2" label="$3" drive="$4"
  local out="$SCRATCH/doom-$gen-$label" work="$SCRATCH/doom-$gen-$label.img"
  local rb; rb="$(ls "$RES"/drives/*rockbox*.img 2>/dev/null | head -1)"
  [ -n "$rb" ] || { row doom "$gen" "$label" - BLOCKED "no Rockbox drive in resources/drives"; return; }
  mkdir -p "$out"; clone_disk "$rb" "$work"
  # **The three files are installed into the clone, not demanded of the source drive.** `doom.rs`
  # has carried their URLs, sizes and SHA-256s since 2026-09-01, and this row read BLOCKED first
  # for want of any caller and then for want of one *here* — a harness that builds its own drives
  # from an IPSW and fetches Rockbox itself was still asking a drive in `resources/` to have been
  # prepared by hand. Three more downloads is not a new dependency, and they land in `$CACHE` with
  # the rest, so a second run of the matrix fetches nothing.
  #
  # Rockbox will not ship them: `rockdoom.wad` is its own, and the game data is Freedoom, which is
  # BSD-licensed and stands in for `doom2.wad`. Installed under the names the plugin looks for.
  "$BIN" doom-assets "$work" > "$out-install.log" 2>&1
  local have; have="$("$BIN" fat "$work" 2>/dev/null | grep -ciE "rockdoom|doom2\.wad|shortcuts\.txt")"
  if [ "${have:-0}" -lt 3 ]; then
    row doom "$gen" "$label" - BLOCKED "installing the assets left $have of 3 — see $out-install.log"
    return
  fi
  # research/06, and every offset is a duration rather than a click count.
  local w="@25s:touch,+600ms:rotate=-6,+2s:release"
  w="$w,+1s:down=select,+300ms:up=select,+4s:down=select,+300ms:up=select"
  w="$w,+50s:touch,+600ms:rotate=-6,+2s:release,+2s:touch,+600ms:rotate=-6,+2s:release"
  w="$w,+2s:down=select,+300ms:up=select"
  FLASH="$nor" DISK="$work" BUDGET=12000000000 "$BIN" rockbox --clock=5 --clickwheel \
    --wheel="$w" --bcm-film=0xE0000:140:F0:2000000:"$out" > "$out.log" 2>&1
  local pics fired; pics=$(pictures "$out")
  fired=$(grep -oE "script: [0-9]+ of [0-9]+" "$out.log" | head -1)
  if [ "${pics:-0}" -ge 8 ]; then
    row doom "$gen" "$label" "shortcut" PASS "$pics pictures, $fired -> $out"
  else
    row doom "$gen" "$label" "shortcut" FAIL "$pics pictures, $fired"
  fi
}

# ── one generation: build its drive, mint its ROM, run every target on both NOR sources ────────
generation() {                      # $1 label  $2 model  $3 families  $4 seed  $5.. real dumps
  local gen="$1" model="$2" fams="$3" seed="$4"; shift 4
  local ipsw drive="" syn="$SCRATCH/syn-$gen.bin"
  ipsw="$(pick_ipsw "$fams")"
  if [ -n "$ipsw" ] && "$BIN" make-disk "$ipsw" "$SCRATCH/$gen.img" > "$SCRATCH/mk-$gen.log" 2>&1; then
    drive="$SCRATCH/$gen.img"
  fi
  echo "# $gen — $model, updater family ${fams//|/ or }"
  echo "#   IPSW  $([ -n "$ipsw" ] && basename "$ipsw" || echo 'none in a family this model takes')"
  echo "#   drive $([ -n "$drive" ] && fact "$drive" "Firmware images" || echo 'NOT BUILT')"
  "$BIN" make-nor --model "$model" --seed "$seed" "$syn" > "$SCRATCH/mknor-$gen.log" 2>&1 || syn=""

  local n
  for n in "$@"; do
    boot_retailos "$n" "$gen" real "$drive"
    boot_diag     "$n" "$gen" real "$drive"
    boot_rockbox  "$n" "$gen" real "$drive"
    boot_doom     "$n" "$gen" real "$drive"
  done
  [ ${#} -eq 0 ] && row retailos "$gen" real - ABSENT "no real dump of this generation on this machine"
  if [ -n "$syn" ]; then
    boot_retailos "$syn" "$gen" synthetic "$drive"
    boot_diag     "$syn" "$gen" synthetic "$drive"
    boot_rockbox  "$syn" "$gen" synthetic "$drive"
    boot_doom     "$syn" "$gen" synthetic "$drive"
  fi
  echo
}

generation 5G   A146 "13|20" 1 ${REAL_5G[@]+"${REAL_5G[@]}"}
generation 5.5G A446 "25"    2 ${REAL_55G[@]+"${REAL_55G[@]}"}

echo "not in this matrix yet:"
echo "  title     a plaintext .ipg on the iPod's own screen. resources/games/plaintext holds them,"
echo "            the window plays them, and this harness does not drive the window."
