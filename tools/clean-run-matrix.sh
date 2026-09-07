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
echo "  except    brick, which runs at 75 and states so in its ROUTE column. It is the only row"
echo "            that measures PLAY rather than 'does it draw', and at 5 the game runs 15x fast"
echo "            against its own timer. It carries its own budget, in the firmware's clock."
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

printf '%-9s %-6s %-11s %-19s %-8s %s\n' TARGET GEN NOR ROUTE VERDICT EVIDENCE
printf '%-9s %-6s %-11s %-19s %-8s %s\n' ------ --- --- ----- ------- --------
row() { printf '%-9s %-6s %-11s %-19s %-8s %s\n' "$1" "$2" "$3" "$4" "$5" "$6"; }

# ROUTE is 19 wide rather than 14 so `brick`'s route can carry its clock. Every row's route is one
# of a handful of fixed strings, and a column that silently overflows on one of them misaligns the
# whole table from that line down — which is how a reader loses track of which column a number is in.
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

# `brick` — Apple's own built-in, a 0.5 requirement, and **the only row here that measures PLAY.**
#
# Every other row asks *does it draw*. This one asks *does the wheel move the paddle*, and the two
# questions want different machines, different evidence and different verdicts. research/18 is the
# measurement; this is it as a gate.
#
# ── 1. THE CLOCK, and why this row alone breaks the harness's rule ─────────────────────────────
#
# The banner above says every row runs at `--clock=5`, the research accelerant. That is harmless
# for a does-it-draw row — research/04 records a 6 G A/B at 5 and at 75 landing on the same 933 ATA
# commands and the same 75 267 non-black pixels — and it is **not harmless here**, because Brick's
# animation is driven by the firmware's own timer. The operator played it at clock 5 and reported
# *"the balls just shoot immediately super fast, nearly unplayable"*. A row written like its
# neighbours would measure Brick in the state we already know is wrong. So: **75, the real part's
# rate**, and the row says so in its ROUTE column.
#
# The fear that made the accelerant the rule does not survive contact with the clock: it is the
# *instruction budgets* that do not finish booting at 75, not the clock. Measured 2026-09-07 on the
# 5G dump and a drive built from `iPod_20.1.3`, the Language picker draws at **4.8 s simulated**,
# where at clock 5 it takes 200.8 s (research/12). Fifteen times less iPod-time for the same work.
#
# ── 2. THE BUDGET, stated in the clock the firmware waits on ───────────────────────────────────
#
# `--until` decides what a run covers; `BUDGET` is only the ceiling that stops a wedged one. The
# spacing below is measured rather than guessed, and every figure is from research/18 §2:
#
#   4.8 s     the Language picker draws                                        75 267 non-black
#   66.8 s    the main menu answers the Select at 8 s — **58 s of first-boot work, and the one
#             expensive wait in the whole descent**                            75 791
#   0.6-1 s   a one-row gesture lands
#   6-9 s     a Select that opens a submenu
#   15.7 s    the Select that launches Brick                                   76 763
#   1.4 s     the centre button to the ball moving
#
# 208 s of iPod is **15.6 G instructions** at this clock — about 25 minutes on the machine this was
# written on, which is the row's honest price and roughly 1.4x a `retailos` row. There is no
# shorter route to a played game: the 58 s is RetailOS doing first-boot work on a drive this
# harness deliberately builds fresh for every row.
#
# ── 3. BRICK IS ROW 0 HERE, and that is a property of the drive rather than of RetailOS ────────
#
# research/13 §5 has Brick at row 5 of a 56-entry Games list — that was the operator's own drive,
# with 56 purchased titles interleaved alphabetically. A drive built from an IPSW carries none of
# them, so the list is the four built-ins, `Brick · Music Quiz · Parachute · Solitaire`, and Brick
# sorts first and is selected on entry. The descent needs no scroll inside the list. **A row that
# copied research/13's five gestures would walk off the end of a four-entry list.**
#
# ── 4. THE CONTROL, which is the reason this row is worth having ───────────────────────────────
#
# A picture count cannot tell *arrival* from *play* — the `doom` row learned that the hard way and
# its comment says so. Reaching a title screen is not the claim. The claim is that the wheel moves
# the paddle, and the evidence is **a frame that comes back**: four gestures, `+8 -8 +8 -8`, on a
# playfield whose ball has NOT been served, so the paddle is the only thing on screen that can
# move. An inverse gesture then lands on a **byte-identical earlier frame** — the film's own
# `repeat_of` column, which names the earlier index whose 76 800-halfword digest it matched.
#
# `BRICK_NULL=1` is the arm that makes this row fail on demand, and it is not a debug switch — it
# is the control that gives the verdict its meaning. It replaces each `touch, rotate=±8, release`
# with a bare `touch, release` of the same duration at the same microsecond, so the finger lands on
# the wheel and comes off without turning it. Everything else is identical by construction, because
# both arms are built from these same lines. Measured 2026-09-07, research/18 §4.4 — this row, run
# five times:
#
#   5G   real       PASS  script: 100 of 100, playfield 76763, 6 returns, 38 after the serve
#   5G   realB      PASS  the same run again — the manifests are IDENTICAL, every frame, every
#                         instruction count, every digest
#   5G   synthetic  PASS  script: 100 of 100, playfield 76763, 4 returns, 37 after the serve
#   5.5G synthetic  PASS  script: 100 of 100, playfield 76763, 6 returns, 40 after the serve
#   5G   BRICK_NULL FAIL  script:  68 of  68, playfield 76763, 0 returns, 38 after the serve
#
# **The null arm reaches the same playfield and serves the same ball**; it differs in one number and
# that number is the claim. And the three passing machines are not the same machine: Apple's
# bootloader out of a real ROM, and two synthesised ROMs entered through the drive, one of them a
# different generation on a different updater family. All three land on the same three digests —
# `0x14854c1e…` -> `0x70ec99eb…` -> `0x3d5be725…` — over 76 800 halfwords.
#
# **The null arm reaches the same playfield and serves the same ball.** It fails on the paddle and
# on nothing else, which is what makes the verdict mean what it says rather than meaning "something
# happened". Its playfield frame is held across **301 consecutive samples, 3.01 G instructions**,
# through all four gestures, without one new picture.
#
# What that rules out, and nothing weaker does: that the panel changed because the game animates on
# its own (nothing else is moving pre-serve, which is exactly why the digest CAN repeat); that the
# touch did it (the null arm touches at the same microseconds); that a match was luck (76 800
# halfwords agreeing, three times).
boot_brick() {                      # $1 nor  $2 gen  $3 label  $4 drive
  local nor="$1" gen="$2" label="$3" drive="$4"
  local out="$SCRATCH/brick-$gen-$label" work="$SCRATCH/brick-$gen-$label.img" route
  [ -n "$drive" ] || { row brick "$gen" "$label" - BLOCKED "no drive for this generation"; return; }
  mkdir -p "$out"; clone_disk "$drive" "$work"

  # **Every anchor absolute, and in the firmware's clock.** The two instants the verdict keys on —
  # when Brick was launched and when the ball was served — are the same numbers the script used,
  # so the two cannot drift apart. A `+N` chain computes them somewhere else and then they can.
  brick_row() { printf ',@%sms:touch,+500ms:rotate=+8,+2s:release' "$1"; }
  brick_sel() { printf ',@%sms:touch,+500ms:press=select,+2s:release' "$1"; }
  # The paddle gesture and its ablation, at the same instant and for the same duration. 3528 ms is
  # not a round number by accident: it is 500 ms to the first click, seven more 4 ms apart, then
  # 3 s — so the two arms release on the same microsecond.
  brick_pad() {
    if [ -n "${BRICK_NULL:-}" ]; then printf ',@%sms:touch,+3528ms:release' "$1"
    else printf ',@%sms:touch,+500ms:rotate=%s,+3s:release' "$1" "$2"; fi
  }
  local w="@8s:touch,+500ms:press=select,+2s:release"   # English on the Language picker
  w="$w$(brick_row 76504)$(brick_row 83032)$(brick_row 89560)"         # main menu -> Photos, Videos, Extras
  w="$w$(brick_sel 104088)"                                  # open Extras
  w="$w$(brick_row 116592)"                                  # Clock -> Games
  w="$w$(brick_sel 127120)"                                  # open the Games list
  w="$w$(brick_sel 137624)"                                  # launch row 0 = Brick
  w="$w$(brick_pad 160128 +8)$(brick_pad 167656 -8)$(brick_pad 175184 +8)$(brick_pad 182712 -8)"
  w="$w$(brick_sel 191240)"                                  # the centre button serves
  local launch_us=138124000 paddle_us=160128000 serve_us=191740000

  # **`--rtc` and `--battery` are pinned, and this row is the one that could not do without it.**
  # They are the only two things in this machine that come from outside it, and `--rtc`'s own
  # comment in `trace.rs` records the cost of leaving them free: the same recipe an hour apart gave
  # 44 511 132 instructions and 44 509 887. Measured here on 2026-09-07 before this line existed —
  # two runs of THIS row, one at 11:14 and one at 12:36, diverged by **24 298 instructions**, which
  # slid one mid-update frame off a 10 M film sample and turned **6 returns into 5**. The verdict
  # survived; the evidence did not, and evidence a person cannot reproduce is the thing this whole
  # harness exists to stop shipping. The date is arbitrary and fixed; nothing here depends on it
  # being any particular day, only on it being the same day every time.
  #
  # The rows above do not pin it. That is a smaller risk rather than no risk — their evidence is
  # picture counts and ATA censuses, and `trace.rs` records those as not having moved across the
  # same comparison — but a row whose evidence is a count of byte-identical returns has no such
  # margin.
  #
  # `--wheel-click-instr` is stated rather than left to the default even though 300 000 IS the
  # default at this clock since 2026-09-07. A row that names its own calibration cannot be
  # silently re-calibrated by a change to a default somewhere else — which is the exact thing that
  # happened to this flag between 2026-08-17 and 2026-09-07, when the clock default moved to 75 and
  # this one did not, leaving every unqualified script scrolling at 266 µs per click.
  # An ARRAY, not a string. Two flags in a shell variable expanded unquoted are two flags in bash
  # and **one argument in zsh**, which does not word-split — so they arrive as a single unknown
  # option, are ignored, and the run reports nothing wrong. That has cost this project time before,
  # and the failure is invisible: the machine boots, the wheel script still fires in full, and the
  # only symptom is that the numbers move.
  local -a pin=(--rtc=2026-01-01T12:00:00 --battery=100)
  if [ -n "$(fact "$nor" "Build")" ]; then
    route="Apple ROM, clock 75"
    FLASH="$nor" DISK="$work" BUDGET=17000000000 "$BIN" retail --clock=75 --until=208s "${pin[@]}" \
      --clickwheel --wheel-click-instr=300000 --wheel="$w" \
      --bcm-film=0xE0000:140:F0:10000000:"$out" > "$out.log" 2>&1
  else
    route="from the drive, 75"
    "$TRACE" 17000000000 --osos-from-disk --boot-osos --flash="$nor" --disk="$work" \
      --disk-writable --sysinfo --bcm --pmu --nor --clock=75 --until=208s "${pin[@]}" \
      --clickwheel --wheel-click-instr=300000 --wheel="$w" \
      --bcm-film=0xE0000:140:F0:10000000:"$out" > "$out.log" 2>&1
  fi

  local fired nfired mfired pf pf_digest pre_digest returns played
  # **Read, not assumed.** A machine that spends its budget halted fires 0 of N and reads exactly
  # like a firmware that has stopped listening — AGENTS.md §6's first named shape, and the reason
  # every anchor above is in simulated time rather than instructions.
  fired=$(grep -oE "script: [0-9]+ of [0-9]+" "$out.log" | head -1)
  nfired=$(printf '%s' "$fired" | awk '{print $2}')
  mfired=$(printf '%s' "$fired" | awk '{print $4}')
  # The playfield: the last picture standing when the first paddle gesture arrives. Reported as a
  # number rather than compared against 76 763, because that count is this drive's and this
  # build's, and a row that asserts it would go red for a reason that is not a regression.
  pf=$(awk -F'\t' -v t="$paddle_us" '/^[0-9]/ && $5 <= t {n=$9} END{print n+0}' "$out/frames.tsv" 2>/dev/null)
  # **And the launch has to have CHANGED it**, which a pixel count cannot say. `pf > 1000` is true
  # of the Games list too, so a run that pressed Select on a list and got nothing would fail on the
  # returns test instead and report "arrived but did not play" — the right verdict reached for the
  # wrong reason, which is the defect the `$9, not $8` note above this function is about. Comparing
  # the digest standing at the launch against the one standing when the paddle arrives asks the
  # question directly, and asks it without naming 76 763 — a number belonging to this drive and this
  # build, which a gate must not assert.
  pf_digest=$(awk -F'\t' -v t="$paddle_us" '/^[0-9]/ && $5 <= t {d=$NF} END{print d}' "$out/frames.tsv" 2>/dev/null)
  pre_digest=$(awk -F'\t' -v t="$launch_us" '/^[0-9]/ && $5 <= t {d=$NF} END{print d}' "$out/frames.tsv" 2>/dev/null)
  # **A return is a row whose digest matched an earlier one, where BOTH are in the game.** The
  # digest map is global over the whole run, so without the second half of that test a frame
  # matching some boot screen would score as a paddle return.
  returns=$(awk -F'\t' -v t="$launch_us" '/^[0-9]/ {u[$1]=$5; if ($3 != "-" && $5 > t && u[$3] > t) n++} END{print n+0}' \
    "$out/frames.tsv" 2>/dev/null)
  # New pictures after the serve — the ball. Distinct from the returns on purpose: the paddle
  # proves the wheel is read, the ball proves the game is running, and a row that conflated them
  # could pass on either alone.
  played=$(awk -F'\t' -v t="$serve_us" '/^[0-9]/ && $5 > t && $3 == "-" {n++} END{print n+0}' \
    "$out/frames.tsv" 2>/dev/null)
  local ev="$fired, playfield $pf non-black, $returns returns, $played pictures after the serve"

  if [ -z "$fired" ] || [ "$nfired" != "$mfired" ]; then
    row brick "$gen" "$label" "$route" FAIL "not every gesture was delivered: ${fired:-no script line at all}, $ev"
  elif [ "${pf:-0}" -le 1000 ] || [ "${pf:-0}" -eq 76800 ]; then
    row brick "$gen" "$label" "$route" FAIL "never reached a playfield: $ev"
  elif [ -n "$pre_digest" ] && [ "$pf_digest" = "$pre_digest" ]; then
    row brick "$gen" "$label" "$route" FAIL \
      "the Select did not launch anything — the panel at the launch and at the first paddle gesture are the same picture: $ev"
  elif [ "${returns:-0}" -lt 2 ]; then
    # The verdict the BRICK_NULL arm produces, and the one a regression in the wheel produces.
    row brick "$gen" "$label" "$route" FAIL \
      "arrived but did not play — the panel never came back to an earlier frame under the inverse gesture: $ev"
  elif [ "${played:-0}" -lt 8 ]; then
    row brick "$gen" "$label" "$route" PARTIAL "the paddle moves, the ball does not — the serve did not take: $ev"
  else
    row brick "$gen" "$label" "$route" PASS "$ev -> $out"
  fi
}

# ── 0.6 goals: rows that are expected not to pass yet ──────────────────────────────────────────
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
