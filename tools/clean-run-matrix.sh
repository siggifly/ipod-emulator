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
  local pics fired last end
  pics=$(pictures "$out")
  fired=$(grep -oE "script: [0-9]+ of [0-9]+" "$out.log" | head -1)
  # **A picture count cannot tell "it started" from "it is playing", and it said PASS for a game
  # that had hung.** Doom's startup scrolls a console — R_InitPlanes, I_InitSound, "Starting
  # Graphics engine" — and that alone is 29 distinct pictures, which sailed past `pics >= 8`.
  # Measured on 5G-real while the interrupt controller was throwing away Doom's timer enable: the
  # last panel change was at **153 s** and the run continued to 2 400 s, so the game initialised and
  # then drew nothing for another 2 247 seconds. The row said PASS. That defect is fixed
  # (KNOWN-BUGS, 2026-09-06) and this check is what would catch the next one of its shape.
  #
  # So the question is not how many pictures but **whether it was still drawing at the end**. A
  # game being played changes the panel constantly; a hung one stops. `last` is the last frame's
  # `first_usec` (column 5) and `end` is where the run got to, both from the film's own manifest
  # and the run's `usec` line, so this cannot drift from what the recording says.
  last=$(awk -F'\t' '/^[0-9]/{n=$5} END{print n+0}' "$out/frames.tsv" 2>/dev/null)
  end=$(grep -oE "usec [0-9]+" "$out.log" | tail -1 | awk '{print $2}')
  if [ "${pics:-0}" -lt 8 ]; then
    row doom "$gen" "$label" "shortcut" FAIL "$pics pictures, $fired"
  elif [ -n "$end" ] && [ "${last:-0}" -lt $((${end:-0} / 2)) ]; then
    row doom "$gen" "$label" "shortcut" FAIL \
      "started but not playing: $pics pictures, last drew at $((${last:-0} / 1000000)) s of $((${end:-0} / 1000000)) s — the panel stopped"
  else
    row doom "$gen" "$label" "shortcut" PASS \
      "$pics pictures, still drawing at $((${last:-0} / 1000000)) s, $fired -> $out"
  fi
}

# ── 0.6 goals: rows that are expected not to pass yet ──────────────────────────────────────────

# Brick — RetailOS's own built-in, and a 0.5 requirement. **Not implemented, deliberately.**
#
# It was demonstrated playable, and the recipe that did it was never written down: no file in
# research/, no recipe in tools/, and grep finds neither the digest nor a descent. Guessing a
# descent here would produce a confident FAIL about a thing that works, which is the exact shape
# AGENTS.md §6 is about — so this row states the gap instead of inventing a number.
#
# **And it cannot be written like the rows above even once the descent is known.** Every other row
# runs at `--clock=5`, the accelerant, because at the faithful 75 these budgets do not finish
# booting — harmless for *does it draw*, since research/04 records a 6 G A/B at 5 and 75 landing on
# the same 933 ATA commands and 75 267 non-black pixels. A PLAYABILITY row is the exception: at
# clock 5 the operator reports *"the balls just shoot immediately super fast, nearly unplayable"*.
# Brick needs --clock=75 and a budget sized for real time.
boot_brick() {                      # $1 nor  $2 gen  $3 label  $4 drive
  row brick "$2" "$3" - BLOCKED "the descent was never recorded — issue #24. Needs --clock=75, not 5"
}
#
# **They report GOAL, never FAIL, and the distinction is the whole point.** A matrix with three
# permanently-red rows is a matrix people stop reading, and the failure it is meant to catch —
# a 0.5 target regressing — arrives in the same colour as the three things that were never
# expected to work. So a goal row says *how far it got*, a number that is meaningful to compare
# against itself run to run, and shouts only when it does better than expected.
#
# Each records the measurement that was true when it was written, so a change is visible without
# reading anything else.
goal() {                            # $1 target  $2 gen  $3 label  $4 route  $5 got  $6 expected
  if [ "${5%% *}" = "MET" ]; then
    row "$1" "$2" "$3" "$4" "GOAL-MET" "${5#MET }  (expected: $6)"
  else
    row "$1" "$2" "$3" "$4" GOAL "$5  (expected: $6)"
  fi
}

# ── the two rows that need a drive nobody has, and the one command that would build it ─────────
#
# `ipodloader2` and iPodLinux are not entered from the NOR and are not raw images handed to the
# machine. They live on a drive that has to be BUILT: the bootloader written into the firmware
# partition where Apple's own bootloader looks for an OS, and ZeroSlackr's five directories written
# onto the FAT32 volume beside it. `ipod-boot install-linux` is that command, and both rows below
# stand or fall on it, so it runs once per generation and both rows read the same answer.
#
# **Neither vendor artefact is missing, and the paths this used to gate on never existed.** The
# rows read `resources/vendor/ipodloader2/bin/loader.bin is not here` and
# `resources/vendor/ipodlinux/boot/vmlinux is not here`; there is no `bin/` under `ipodloader2/`
# and no `ipodlinux/` under `vendor/` at all. Both files are on this machine, verified 2026-09-06:
#
#   resources/vendor/ipodloader2/loader.bin          57 676 B  iPL 2.9.0d, built from upstream
#                                                              master at a41ec49 — rebuilt into a
#                                                              scratch tree and compared, byte for
#                                                              byte identical, so it is that source
#   resources/vendor/zeroslackr/tree/boot/vmlinux  1 531 200 B  sha256 9c7b66e2…, the kernel
#                                                              research/16 recorded, hash checked
#
# **`install-linux` reaches for neither of them, deliberately.** `ipodlinux::resolve_loader` does
# not consult `resources/vendor/` even as a fallback — it is gitignored, so preferring it made the
# command work only inside this checkout — and fetches the v2.8.1 RELEASE instead (56 912 B, SHA-256
# on record, already in the cache here and verified). It takes the ZeroSlackr tree from
# `resources/vendor/zeroslackr/tree` when that is unpacked, and fetches the 101 MB archive when it
# is not; unpacking shells out to `7z`/`7za`/`7zz` and says so plainly when none is on PATH. This
# script's whole question is the path a NEW user walks, so these rows set no IPOD_LOADER and get the
# release like anybody else. That also means they do not measure the 2.9.0d numbers in research/17,
# which nobody has yet re-measured against 2.8.1.
#
# **There is no patch, and the row must not imply one.** This comment used to cite
# `tools/patches/ipodloader2-vfs.patch` for two upstream bugs. That file is deleted and the claim
# was retired in research/16 §"The patch was compensating for our test disk": the FAT32-LBA `0x0C`
# case was our own fixture rather than upstream's defect — `make-disk` writes `0x0B`, which upstream
# handles — and the inverted `mlc_strncmp` firmware-magic test is real but COSMETIC, because the
# loader boots from the FAT32 volume and never needed partition 0. Nothing in this project patches
# an operating system it runs.
#
# **`--rdval=0x70000000=0x3232432D` is gone too**, and the runs below must not pass it. research/16
# §"RESOLVED: the part is a PP5022" measured the cause on 2026-08-20: what was missing was a USB
# clock, not a register value. `0x70000000` now reports `PP5022C-` truthfully to every guest with
# nothing supplied and no per-operating-system flag, and retail is unchanged at 599 ATA commands.
#
# Sets LINUX_DRIVE to a drive that boots iPodLinux, or leaves it empty and puts the reason in
# LINUX_BLOCKED. Memoised per generation — including the failure, so a refusal is reported once per
# row rather than re-attempted per row.
LINUX_DRIVE=""; LINUX_BLOCKED=""
build_linux_drive() {               # $1 gen  $2 source drive
  local gen="$1" src="$2"
  local img="$SCRATCH/linux-$gen.img" err="$SCRATCH/linux-$gen.err" part="$SCRATCH/linux-$gen.part"
  LINUX_DRIVE=""; LINUX_BLOCKED=""
  [ -n "$src" ] || { LINUX_BLOCKED="no drive built for this generation"; return; }
  # **A refused `install-linux` still leaves its 8 GB output file behind**, because it copies the
  # source drive into place and only then discovers it cannot fit the bootloader. So the existence
  # of the output is NOT evidence that the install worked, and the first version of this function
  # tested `-f $img` before the error marker: `loader2` reported BLOCKED correctly, and then
  # `ipodlinux` and `triple` picked up the leftover copy — a plain RetailOS drive with no
  # bootloader on it — booted it through the `loader` recipe and reported a verdict for it.
  # Observed, on the run that was supposed to check this change. That is AGENTS.md §6 exactly: an
  # instrument reporting a number it could not have measured.
  #
  # Building to `.part` and renaming only on success removes the ambiguity instead of ordering
  # around it, which is the same "nothing renamed into place until it verifies" rule the firmware
  # fetcher follows. The error marker is still consulted first, so a generation that cannot build
  # is reported once per row rather than re-attempted per row.
  [ -f "$err" ] && { LINUX_BLOCKED="$(cat "$err")"; return; }
  [ -f "$img" ] && { LINUX_DRIVE="$img"; return; }
  rm -f "$part"
  if "$BIN" install-linux "$src" "$part" > "$SCRATCH/il-$gen.log" 2>&1 && [ -f "$part" ]; then
    mv "$part" "$img"; LINUX_DRIVE="$img"; return
  fi
  rm -f "$part"
  # Its own refusal, verbatim. The line above it in the log names which loader was resolved, which
  # is not the failure and must not be reported as one. A row that invents its own wording for
  # somebody else's error is a row that goes stale the moment the error changes.
  LINUX_BLOCKED="$(sed -n 's/^ipod-boot install-linux: //p' "$SCRATCH/il-$gen.log" | head -1)"
  [ -n "$LINUX_BLOCKED" ] || LINUX_BLOCKED="ipod-boot install-linux failed; see $SCRATCH/il-$gen.log"
  printf '%s\n' "$LINUX_BLOCKED" > "$err"
}

# ipodloader2 — the third bootloader. research/16: Apple's bootloader finds it in the firmware
# partition and enters it exactly as it enters RetailOS and the Rockbox bootloader, and it then
# prints its own console, walks the FAT32 volume and loads `/boot/vmlinux`.
#
# **The recipe is `ipod-boot loader`, not `retail`, and that is not a preference.** research/17
# §"Reproducing the rows is two steps" is explicit: `loader` is `--osos-from-disk`, which enters the
# bootloader sitting in the drive's own firmware partition. `retail` runs Apple's bootloader from
# the ROM, which on a drive built by `install-linux` would boot RetailOS sitting in front of the
# loader — and this row would then report RetailOS's pixels as ipodloader2's, which is the exact
# shape of AGENTS.md §6. The recipe also supplies `--sysinfo`, without which the loader dereferences
# the wrong sysinfo pointer, leaves hw_rev 0 and addresses a 1G iPod's registers forever.
boot_loader2() {                    # $1 nor  $2 gen  $3 label  $4 drive
  local nor="$1" gen="$2" label="$3" drive="$4"
  local out="$SCRATCH/ldr-$gen-$label" work="$SCRATCH/ldr-$gen-$label.img"
  build_linux_drive "$gen" "$drive"
  [ -n "$LINUX_DRIVE" ] || { row loader2 "$gen" "$label" - BLOCKED "$LINUX_BLOCKED"; return; }
  mkdir -p "$out"; clone_disk "$LINUX_DRIVE" "$work"
  FLASH="$nor" DISK="$work" BUDGET=2000000000 "$BIN" loader --clock=5 --clickwheel \
    --bcm-film=0xE0000:140:F0:25000000:"$out" > "$out.log" 2>&1
  local pics nb ata; pics=$(pictures "$out"); nb=$(last_nonblack "$out")
  # **ATA commands are the evidence, and the pixels are the corroboration.** research/17 records
  # 3 196 ATA commands and no unmapped accesses, identical on all three ROMs — the loader reads the
  # drive, walks the volume and jumps, and does not care which ROM it came through. `trace` prints
  # `ata commands: N` once, at the end; the old row grepped for `ata ` and counted log lines.
  # **The count only, because the line carries prose after it.** `trace` prints
  # `ata commands: 3870  (log below shows the first 256 — SAMPLE, NOT A CENSUS)`, and taking
  # everything after the colon put that parenthesis in the middle of the evidence column. The
  # caution is about the per-command LISTING below it, not about the total, which is a census.
  ata=$(sed -n 's/^ata commands: \([0-9][0-9]*\).*/\1/p' "$out.log" | tail -1)
  if [ "${nb:-0}" -gt 1000 ]; then
    goal loader2 "$gen" "$label" "chain" "MET ${ata:-0} ATA, $pics pictures, $nb non-black — it drew" \
      "3 196 ATA commands, no unmapped (research/17, 2026-08-20, on 2.9.0d)"
  else
    goal loader2 "$gen" "$label" "chain" "${ata:-0} ATA, $pics pictures, $nb non-black" \
      "3 196 ATA commands, no unmapped (research/17, 2026-08-20, on 2.9.0d)"
  fi
}

# iPodLinux — further than "boots": the kernel EXECUTES. research/16 measured it ending inside
# `ldmia sp, {r0-pc}^`, an ARM exception return restoring user-mode registers, having taken
# interrupts — a Linux kernel servicing its own traps — and reaching ZeroLauncher's splash.
#
# **Where it stops is named, and it is no longer an unmapped page.** This comment used to say the
# kernel polls `0x64004000..0x64004103` 8 385 336 times from two PCs inside the interrupt path, and
# that modelling that address was the work. It was, and it was done: research/16 §"The kernel boots,
# and it says so" records that with the mirror in place a 12 G run reports the unmapped set EMPTY.
# The kernel now ends at `00024cb8  b 0x00024cb8`, which is not a hang on hardware — the three
# instructions before it are `mrs r3, cpsr` / `bic r3, r3, #0x80` / `msr cpsr_c, r3` and the string
# loaded just above is `"<0>In idle task - not syncing"`. That is the tail of Linux's `panic()`,
# and `--enterlog` on `printk` turns its 163 calls into a readable console.
#
# So the open question moved from a register to a filesystem: `root=/dev/hda3` is the compiled-in
# default command line at `0x12efa`, and this drive has two partitions. A working iPodLinux drive
# also needs an `hda3` to mount, which `install-linux` does not create.
#
# docs/GUI.md §15 names a stall from the other end — "iPodLinux boots ... and then ZeroLauncher
# stalls at 'Finishing Up…'". Whether that is this panic is NOT established; GUI.md's runs come
# from `ipod-boot install-linux` drives and research/16's from the ipodloader2 chain.
boot_ipodlinux() {                  # $1 nor  $2 gen  $3 label  $4 drive
  local nor="$1" gen="$2" label="$3" drive="$4"
  local out="$SCRATCH/ipl-$gen-$label" work="$SCRATCH/ipl-$gen-$label.img"
  build_linux_drive "$gen" "$drive"
  [ -n "$LINUX_DRIVE" ] || { row ipodlinux "$gen" "$label" - BLOCKED "$LINUX_BLOCKED"; return; }
  mkdir -p "$out"; clone_disk "$LINUX_DRIVE" "$work"
  FLASH="$nor" DISK="$work" BUDGET=6000000000 "$BIN" loader --clock=5 --clickwheel \
    --bcm-film=0xE0000:140:F0:25000000:"$out" > "$out.log" 2>&1
  local pics nb ata; pics=$(pictures "$out"); nb=$(last_nonblack "$out")
  # **The count only, because the line carries prose after it.** `trace` prints
  # `ata commands: 3870  (log below shows the first 256 — SAMPLE, NOT A CENSUS)`, and taking
  # everything after the colon put that parenthesis in the middle of the evidence column. The
  # caution is about the per-command LISTING below it, not about the total, which is a census.
  ata=$(sed -n 's/^ata commands: \([0-9][0-9]*\).*/\1/p' "$out.log" | tail -1)
  goal ipodlinux "$gen" "$label" "loader" "${ata:-0} ATA, $pics pictures, $nb non-black" \
    "kernel executes, then panics in idle (research/16)"
}

# Triple boot — RetailOS, Rockbox and iPodLinux, chosen from one loader menu.
#
# **The register blocker this row used to name is RESOLVED and the wording was false.** It said
# `0x70000000` bits 16..23 admit no value serving both Apple's bootloader and ipodloader2. They do:
# research/16 §"RESOLVED: the part is a PP5022" measured on 2026-08-20 that the missing piece was a
# USB clock — map `USB_BASE` (`0xc5000000`, Rockbox pp5020.h:580), treat bit 1 of `+0x140` as a
# self-clearing reset, and raise the clock-ready bit from that write instead of from `DEV_INIT2`.
# With those three, the register reports `PP5022C-` truthfully to everybody and retail is unchanged
# at 599 ATA commands. Leaving the old text here would have kept a solved problem on the board.
#
# What actually blocks the row now is upstream of it: the same `install-linux` refusal the two rows
# above report, because a triple-boot drive is that drive plus a Rockbox image. No number is
# expected until a loader drive can be built at all.
boot_triple() {                     # $1 nor  $2 gen  $3 label  $4 drive
  build_linux_drive "$2" "$4"
  if [ -z "$LINUX_DRIVE" ]; then
    row triple "$2" "$3" - BLOCKED "$LINUX_BLOCKED"
  else
    goal triple "$2" "$3" "-" "drive builds; the menu is not driven yet" \
      "RetailOS, Rockbox and iPodLinux chosen from one loader menu"
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
    boot_brick    "$n" "$gen" real "$drive"
    boot_loader2  "$n" "$gen" real "$drive"
    boot_ipodlinux "$n" "$gen" real "$drive"
    boot_triple   "$n" "$gen" real "$drive"
  done
  [ ${#} -eq 0 ] && row retailos "$gen" real - ABSENT "no real dump of this generation on this machine"
  if [ -n "$syn" ]; then
    boot_retailos "$syn" "$gen" synthetic "$drive"
    boot_diag     "$syn" "$gen" synthetic "$drive"
    boot_rockbox  "$syn" "$gen" synthetic "$drive"
    boot_doom     "$syn" "$gen" synthetic "$drive"
    boot_brick    "$syn" "$gen" synthetic "$drive"
    boot_loader2  "$syn" "$gen" synthetic "$drive"
    boot_ipodlinux "$syn" "$gen" synthetic "$drive"
  fi
  echo
}

generation 5G   A146 "13|20" 1 ${REAL_5G[@]+"${REAL_5G[@]}"}
generation 5.5G A446 "25"    2 ${REAL_55G[@]+"${REAL_55G[@]}"}

echo "not in this matrix yet:"
echo "  title     a plaintext .ipg on the iPod's own screen. resources/games/plaintext holds them,"
echo "            the window plays them, and this harness does not drive the window."
