#!/usr/bin/env bash
#
# The films this project ships, and the exact recipes that produce them.
#
#   ./post-assets.sh boot        cold boot -> Apple logo -> Language -> menu -> Extras -> Games -> Brick
#   ./post-assets.sh gameplay    Brick, actually played: served, returned, bricks broken, score moving
#   ./post-assets.sh all
#
# Each target films the panel, then writes a `.gif` and an `.mp4` into `_out/post/`, both upscaled
# **2x nearest-neighbour**. The PNG sequence under `_out/film/` is left in place: those are the exact
# 320x240 frames, and the stills come out of them.
#
# `_out/` is gitignored and should stay that way — the frames contain Apple's UI. This script is the
# deliverable; the frames are reproducible from it.
#
# ---------------------------------------------------------------------------------------------
# Everything numeric in here is calibration and all of it was measured. research/13 §2.2 has the
# menu half, §10 has Brick's.
#
#   MENUS.  A one-row step is `touch, rotate=+8, release` with quiet either side — a whole gesture.
#           NOT eight clicks inside a longer burst: a continuous burst accelerates and the same
#           count moves three rows.
#
#   BRICK.  The centre button serves. The ball rests at (4,130), moves (+-8,+-10) px per tick, and
#           every bounce flips exactly one sign — where it lands on the paddle does not steer it.
#           The paddle is 57 px wide, travels [4,262], and moves in 24 px QUANTA: one held
#           `rotate=+2` step is one quantum and the game acts on it about 750 k instructions later.
#           A serve-to-loss is 9 M instructions and a paddle->bricks->paddle leg is 6.1 M, so the
#           paddle has about 195 px of reach per leg and the ball asks for 190 of it. The rally
#           below is scripted against that clock with nothing to spare.
# ---------------------------------------------------------------------------------------------
set -eu

HERE=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$HERE/../.." && pwd)
POST="$ROOT/_out/post"
FILM="$ROOT/_out/film"

# The descent, as named pieces. Identical to research/13 §9 — kept in one place so a change to the
# calibration cannot land in the write-up and miss the script.
HEAD='@1500M:touch,+2M:press=select,+5M:release'          # Select on the Language list
ROW=',+60M:touch,+2M:rotate=+8,+5M:release'               # exactly one row down
SEL=',+60M:touch,+2M:press=select,+5M:release'
TO_GAMES="$HEAD$ROW$ROW$ROW$SEL$ROW"                      # main menu, Extras, Games highlighted
TO_BRICK="$TO_GAMES$SEL,+150M:touch,+2M:rotate=+8,+5M:release$ROW$ROW$ROW$ROW\
,+100M:touch,+2M:press=select,+5M:release"                # five rows to Brick, then launch
# That launch lands at @2437320000 and the playfield is up by 2448 M. Every anchor in the rally
# below is absolute against it, because a rally has to be aimed at the ball rather than at a delay.

# gif and mp4 out of a film directory, 2x nearest-neighbour, timing straight off the manifest.
#
# `-t TOTAL` is not optional: the concat demuxer will not honour the last entry's duration unless
# something follows it, so film.sh lists the last file twice — and the repeat then plays for that
# duration a second time. The manifest's own total trims it back.
# The fourth argument is the gif's palette mode, and it is spelled out at each call site rather
# than defaulted, because the two films want opposite answers and the reason is measured.
#
#   held    one palette PER FRAME, frames held at their real durations.
#   resampled  one palette for the WHOLE film, frames resampled to a constant rate.
#
# `held` is correct and `resampled` is not, so `resampled` needs a reason every time it is used.
# The reason is never "it looks fine" — it is the frame-merge described under MINIMUM DELAY below.
publish() {
  local dir=$1 name=$2 fps=$3 mode=$4
  [ -s "$dir/frames.concat" ] || { echo "no film in $dir" >&2; return 1; }
  local total
  total=$(cat "$dir/frames.total")
  mkdir -p "$POST"
  # `dither=none`: this is a 16-bit UI of flat fills and one-pixel rules, and dithering it adds
  # noise that was never on the panel. That half was right and is unchanged.
  #
  # THE PALETTE. `stats_mode=single` is one palette per frame. This comment used to read "one
  # palette generated from the whole film is enough", and that claim was false. Measured on the
  # frames this script writes: the boot film is 24 distinct screens whose colours UNION to 548, and
  # one 256-entry table cannot hold 548. So it quantised, and the loss is per-frame and large — the
  # main menu reached the gif with 147 of its 211 colours and Brick's playfield with 167 of its 238.
  # That is why the battery's green and Brick's bricks read as wrong in the gif while the stills
  # beside them, written straight from the same PNGs, read as right. The stills were never broken.
  #
  #   boot film, per frame, source -> gif        colours        RMSE vs the source PNG
  #     one palette for the whole film           211 -> 147     0.00111
  #     one palette per frame                    211 -> 211     0.000067
  #     Brick, one palette for the film          238 -> 167     0.00212
  #     Brick, one palette per frame             238 -> 238     0.0000142
  #
  # Frames that carry more than 256 colours of their own still lose the excess — five of the boot
  # film's 24 do, topping out at 270 — but that is the GIF format's limit rather than this recipe's.
  #
  # `reserve_transparent=0` spends the 256th slot on a colour rather than on a transparency index
  # that opaque panel frames never use. `new=1` is what makes the rest of it real: without it
  # paletteuse takes the first palette and reuses it for the whole film, so the per-frame palettes
  # are generated and then thrown away and the output is the broken one again.
  #
  # THE COST, and why `fps=` goes away with the palette. A new palette per frame forces every frame
  # to be a full keyframe, so resampling 24 distinct screens up to 1084 constant-rate frames writes
  # 1084 keyframes — 21.5 MB, against 370 KB for the broken global-palette version of the same film.
  # Held at their real durations the same film is 24 frames and 617 KB, which is 1.7x the broken
  # one for exact colour. Constant-rate resampling is also lossy in its own right: it dropped
  # frame-00010 outright, a screen the machine really displayed, because it was held 0.0278 s and
  # the 30 fps grid had no slot for it.
  #
  # MINIMUM DELAY, and why the gameplay film does not get any of this. The gif muxer will not write
  # a frame delay shorter than 4 centiseconds. The boot film does not care: its 24 screens are held
  # 0.0278 s to 12.4 s and all 24 survive. The gameplay film is 253 screens because the ball moves
  # in every one of them, and 93 of those are held exactly 0.02 s — under the floor. Encoding it
  # held merges them away: 253 screens in, 215 out, 38 of the machine's own frames gone from a film
  # whose entire subject is motion. It is also 12.5 MB against 193 KB. So it stays `resampled`, and
  # what that costs is a 282-colour union quantised to 256 — measured as 238 -> 229 with RMSE
  # 0.0002, roughly a tenth of the boot film's damage and the reason nobody reported it. That is a
  # real defect being kept on purpose, not a clean bill of health. Making it exact costs 30 MB
  # (`held` + constant rate keeps all 253 frames); if that is ever the right trade, change the word
  # at the call site.
  #
  # `-final_delay` is the held half of the `-t TOTAL` problem described above. In constant-rate mode
  # the trailing duplicate frame is what gives the last screen its length; held, the trim removes it
  # and the final screen collapses to a single tick — the boot film measured 34.08 s against a
  # manifest saying 36.1389. Handing the muxer the last frame's own duration puts it back (36.15 s,
  # which is centisecond rounding). A film whose length does not match its manifest is an
  # instrument that lies, and that rule does not stop applying because the palette got better.
  local final_delay
  final_delay=$(awk '/^duration/ {d=$2} END {printf "%d", d*100 + 0.5}' "$dir/frames.concat")
  case $mode in
    held)
      ffmpeg -hide_banner -loglevel error -y -f concat -safe 0 -i "$dir/frames.concat" -t "$total" \
        -vf "scale=iw*2:ih*2:flags=neighbor,split[a][b];[a]palettegen=stats_mode=single:reserve_transparent=0[p];[b][p]paletteuse=dither=none:new=1" \
        -fps_mode vfr -final_delay "$final_delay" -loop 0 "$POST/$name.gif" ;;
    resampled)
      ffmpeg -hide_banner -loglevel error -y -f concat -safe 0 -i "$dir/frames.concat" -t "$total" \
        -vf "scale=iw*2:ih*2:flags=neighbor,fps=$fps,split[a][b];[a]palettegen=stats_mode=full[p];[b][p]paletteuse=dither=none" \
        -loop 0 "$POST/$name.gif" ;;
    *) echo "publish: unknown palette mode '$mode' (want held|resampled)" >&2; return 2 ;;
  esac
  ffmpeg -hide_banner -loglevel error -y -f concat -safe 0 -i "$dir/frames.concat" -t "$total" \
    -vf "scale=iw*2:ih*2:flags=neighbor,fps=$fps" \
    -c:v libx264 -preset veryslow -crf 18 -pix_fmt yuv420p "$POST/$name.mp4"
  echo "  -> $POST/$name.gif and .mp4   (640x480, ${total}s, ${fps} fps)"
}

# One frame of a film, upscaled 2x, as a still.
still() {
  local dir=$1 frame=$2 name=$3
  [ -f "$dir/$frame" ] || { echo "  (no $frame in $dir — skipping $name)"; return 0; }
  mkdir -p "$POST"
  ffmpeg -hide_banner -loglevel error -y -i "$dir/$frame" \
    -vf "scale=iw*2:ih*2:flags=neighbor" "$POST/$name.png"
  echo "  -> $POST/$name.png"
}

do_boot() {
  echo "== boot to Brick =="
  # 2 M is the right cadence: nothing in a menu moves faster, and the run is 2.6 G instructions.
  # 72 M/s is a PP5021C's rate, so playback is the pace real silicon would have executed these.
  # The Apple logo is up from 8 M to 52 M, which at that rate is 0.6 s — short, and it is short
  # because RetailOS's first frame arrives 44 M instructions after the bootloader's blit. The film's
  # timing is the machine's timing and this is what the machine does.
  BUDGET=2600000000 IDLE=2000000000 "$HERE/film.sh" --out="$FILM/boot-to-brick" \
    --every=2M --rate=72000000 --fps=30 -- --clickwheel --wheel="$TO_BRICK"
  # `held`: 24 distinct screens, none of them under the muxer's 4 cs delay floor, and a 548-colour
  # union that one palette cannot hold. This is the film the wrong-colour report was about.
  publish "$FILM/boot-to-brick" ipod-01-boot-to-brick 30 held
  # Frame indices, not instruction counts, because the film's dedup is what assigns them — and they
  # are stable as long as the descent is. `frames.tsv` is the check: the non-black counts below are
  # 75267 / 75791 / 75565 / 74160 / 76763 / 2916, and rule 2 says look at the picture as well.
  #
  # Three of these indices were stale and the check above is what caught it. They read 10 / 13 / 21
  # for Extras / Games / Brick, and today's film puts those screens at 11 / 15 / 23 — the descent
  # now resolves two more distinct pictures than it did when the numbers were written, so every
  # index after the Extras menu had slid. Running the script as it stood would have published the
  # half-drawn Extras frame as `ipod-04-extras`, the Extras menu as `ipod-05-games-list` and the
  # Games list as `ipod-06-brick`: three stills of the wrong screen, from a script that looked like
  # it worked. The non-black counts never moved — 75565 / 74160 / 76763 are exactly what frames
  # 11 / 15 / 23 measure today — which is why they are the check and not decoration. Verified the
  # other way as well: each shipped still compares against its frame here at RMSE 0.
  local d="$FILM/boot-to-brick"
  still "$d" frame-00004.png ipod-02-language
  still "$d" frame-00006.png ipod-03-main-menu
  still "$d" frame-00011.png ipod-04-extras
  still "$d" frame-00015.png ipod-05-games-list
  still "$d" frame-00023.png ipod-06-brick
  still "$d" frame-00002.png ipod-07-apple-logo
}

# `sweep AT N DIR` — N held `rotate` steps, **200 k apart**, starting at AT.
#
# 200 k is not a round number picked for tidiness, it is the calibration: the same steps issued
# 400 k apart move the paddle 29 px per million instructions and 200 k apart move it 150, because
# the wheel accelerator is rate-sensitive. This function had 400 k in it for one take and the film
# it produced is a different game — same ball, different paddle, different rally. research/13 §10.3.
sweep() {
  local t=$1 n=$2 d=$3 i
  W="$W,@$t:rotate=$d"
  for i in $(seq 2 "$n"); do W="$W,+200k:rotate=$d"; done
}

do_gameplay() {
  echo "== Brick, played =="
  W=",@2502340000:touch,+2M:rotate=+8,+5M:release"    # paddle 4-60 -> 28-84
  W="$W,@2539480000:touch,+2M:rotate=+8,+5M:release"  # -> 52-108, under where the serve lands
  W="$W,@2576620000:touch,+2M:press=select"           # SERVE, and the finger stays down

  # Two sweeps, and only two, because the ball tells you where to be and it is not where a naive
  # alternation would put you. research/13 §10.4 has the trajectory each one is aimed at.
  sweep 2581600000 10 +8   # after the first return: the ball comes back down on the RIGHT, at x~282
  sweep 2601500000 5  -8   # after the fourth: it breaks out on the left and drops at x~173

  W="$W,+2M:release"

  # 2630 M stops the run a little after the second ball, which is where the film wants to end. The
  # game would go on: there is a third ball behind it.
  BUDGET=2630000000 IDLE=2000000000 "$HERE/film.sh" --out="$FILM/brick-gameplay" \
    --every=100k --from=2574M --rate=5000000 --fps=50 -- --clickwheel --wheel="$TO_BRICK$W"
  # `resampled`, deliberately, and it is the worse of the two options on colour: 93 of this film's
  # 253 screens are held 0.02 s, under the muxer's 4 cs floor, so encoding it `held` would merge 38
  # of them away — motion is this film's entire subject. It keeps a real defect (238 -> 229, a tenth
  # of the boot film's) to keep every frame. See MINIMUM DELAY in publish().
  publish "$FILM/brick-gameplay" ipod-08-brick-gameplay 50 resampled
  still "$FILM/brick-gameplay" frame-00060.png ipod-09-brick-rally
}

write_readme() {
  mkdir -p "$POST"
  cat > "$POST/README.md" <<'EOF'
# Post assets

Regenerate everything here with `tools/ipod-film/post-assets.sh`. Nothing in this directory is
tracked — `_out/` is gitignored, the frames are Apple's UI, and the script is the artifact.

All frames are read straight out of the co-processor's memory at `0x000e0000`, 320x240 RGB565, and
upscaled **2x nearest-neighbour**. No interpolation, no window chrome, no cursor. Every pixel is
what the panel showed.

| file | what it is |
|---|---|
| `ipod-01-boot-to-brick.gif` / `.mp4` | the whole run: cold boot, **the Apple logo**, white, Language, main menu, Extras, Games, Brick |
| `ipod-08-brick-gameplay.gif` / `.mp4` | Brick played: served, returned, bricks broken, score moving |
| `ipod-02-apple-logo.png` | the boot logo, centred, drawn by an implemented `LCD_UPDATERECT` |
| `ipod-05-games-list.png` | the Games list — real titles off the disk |
| `ipod-06-brick.png` | Brick's playfield |
| `ipod-07-brick-rally.png` | mid-rally: the ball in flight, the paddle under it |

## Timing

The two films play at different rates and both are honest, for the same reason.

`ipod-01` plays at **72 000 000 instructions per second of video** — a PP5021C's rate, so the
playback is the pace real silicon would have executed these instructions. The Apple logo is on the
panel for 44 M instructions of that, which is 0.6 s: short, because RetailOS's first frame lands
44 M after the bootloader's blit, and the film's timing is the machine's timing.

`ipod-08` plays at **5 000 000**, which is one second of video per second of the machine's own
*simulated* time. Brick's animation is driven by the firmware's clock, and at `--clock=5` that clock
runs 14.4x faster per instruction than a PP5021C's — so at the boot film's rate the whole rally is
over in under a second. 5 M/s is the rate the game's own timer thinks it is running at. See
research/13 §10.5.

## Not filmed, deliberately

Settings > About renders the disk image's serial number and FireWire GUID. Music, Photos and Videos
show the original owner's library. None of those appear in any frame here, and the runs that
produced them never navigated to them.
EOF
  echo "  -> $POST/README.md"
}

case "${1:-all}" in
  boot) do_boot; write_readme ;;
  gameplay) do_gameplay; write_readme ;;
  all) do_boot; do_gameplay; write_readme ;;
  *) echo "usage: $0 [boot|gameplay|all]" >&2; exit 2 ;;
esac
