#!/usr/bin/env bash
#
# Record the iPod's panel over a whole run, as a deduplicated PNG sequence — and assemble it into a
# video if `ffmpeg` is on this machine.
#
# This is `retail-boot.sh` with `--bcm-film` wired up and the assembly step attached. Everything
# after `--` is passed through to the boot recipe, so a wheel script, an `--enterlog` watch or a
# second `--bcm-dump` all still work:
#
#   ./film.sh --out=_out/film/boot
#   ./film.sh --out=_out/film/menu --every=1M -- --clickwheel \
#       --wheel='@1500M:touch,+2M:press=select,+2M:release'
#
# The frames are the co-processor's surface read straight out of its memory: exactly 320x240, no
# scaling, no interpolation, no cursor, no window chrome. Consecutive identical frames collapse into
# one file; `frames.tsv` records when each appeared and how long it held.
set -eu

HERE=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$HERE/../.." && pwd)

OUT="$ROOT/_out/film/run"
EVERY=2M
# Do not sample before this instruction count. A cadence fine enough to read a moving ball is far
# too fine to spend on the two billion instructions of boot and menu navigation in front of it: the
# scan is the cost, and skipping it is the difference between a four-minute run and a twenty-minute
# one. The machine is unchanged either way — the run is still issued in EVERY-sized chunks from
# instruction 0.
FROM=0
BASE=0xE0000
W=140          # hex, as --bcm-dump reads it: 0x140 = 320
H=F0           # hex: 0xF0 = 240
# Instructions per second of video. 72 000 000 is a PP5021C's rate, so the default plays back at the
# pace the real silicon would have executed these instructions. It is NOT the pace of the emulator's
# simulated clock: at `--clock=5` that clock runs 15x faster per instruction than real silicon, so
# firmware timeouts fire far earlier in the film than they would on hardware. Both numbers are real;
# this one is the one a viewer's intuition matches to "how long did that take".
RATE=72000000
SCALE=1        # integer nearest-neighbour upscale for the VIDEO only; the PNGs are always exact
FPS=30

# A `while` over `$1` rather than a `for` over `"$@"`: the `for` form snapshots the list, so the
# `shift` that a `--` passthrough needs would drop the wrong argument and do it silently.
PASS=()
while [ $# -gt 0 ]; do
  case "$1" in
    --out=*)   OUT=${1#--out=} ;;
    --every=*) EVERY=${1#--every=} ;;
    --from=*)  FROM=${1#--from=} ;;
    --base=*)  BASE=${1#--base=} ;;
    --rate=*)  RATE=${1#--rate=} ;;
    --scale=*) SCALE=${1#--scale=} ;;
    --fps=*)   FPS=${1#--fps=} ;;
    --)        shift; PASS+=("$@"); break ;;
    *)         PASS+=("$1") ;;
  esac
  shift
done

mkdir -p "$OUT"
rm -f "$OUT"/frame-*.png "$OUT"/frames.tsv "$OUT"/frames.concat "$OUT"/film.mp4 "$OUT"/film.mkv

: "${BUDGET:=4000000000}"
export BUDGET
# How long the machine may run without executing new code before the run is called Idle.
#
# 400 M is the project's standard and the right default for a boot. It is the WRONG default for a
# long wheel script: scrolling a list runs no code the list widget has not already run, so a script
# whose gestures are 60 M apart hits the idle stop after seven of them and the run ends with the
# rest of the script unfired. That is not hypothetical — it truncated the first attempt at launching
# Brick, one gesture and one Select short, and the film's last frame looked like a settled screen
# rather than a cut-off run. Raise it for scripts with long quiet stretches.
: "${IDLE:=400000000}"

echo "film -> $OUT   (every $EVERY instructions from $FROM, budget $BUDGET, idle window $IDLE)"
"$ROOT/tools/ipod-boot/retail-boot.sh" --clock=5 --stop-when-idle="$IDLE" --bcm-registry \
  --bcm-film="$BASE:$W:$H:$EVERY:$OUT" --bcm-film-from="$FROM" \
  "${PASS[@]+"${PASS[@]}"}" | tee "$OUT/run.txt"

[ -s "$OUT/frames.tsv" ] || { echo "no manifest was written — nothing to assemble" >&2; exit 1; }

# The concat demuxer's list, with each frame held for the instructions it actually held. This is
# where the deduplication pays: 800 samples of the same screen become one entry with one duration,
# instead of 800 identical files.
awk -v rate="$RATE" -v tot="$OUT/frames.total" '
  /^#/ { next }
  { printf "file %c%s%c\nduration %.4f\n", 39, $2, 39, $7 / rate; last = $2; sum += $7 / rate }
  END { if (last != "") printf "file %c%s%c\n", 39, last, 39; printf "%.4f\n", sum > tot }
' "$OUT/frames.tsv" > "$OUT/frames.concat"
# Two things about that last line, both learned by measuring the output rather than trusting it.
#
# The demuxer will not honour the final entry's duration unless something follows it, so the last
# file is listed twice — otherwise the closing screen flashes past in one output frame no matter how
# long it was actually up. But the repeat then plays for the last stated duration AGAIN: nav1's
# 2 088 M instructions should be 29.0 s at 72 M/s and came out 35.8 s, over by exactly the 6.8 s of
# its final frame. So the exact total is computed here and handed to ffmpeg as `-t`, which trims the
# duplicate away. A video whose length does not match its manifest is an instrument that lies.
TOTAL=$(cat "$OUT/frames.total")

ENTRIES=$(grep -c '^file ' "$OUT/frames.concat" || true)
PNGS=$(find "$OUT" -name 'frame-*.png' | wc -l | tr -d ' ')
echo "assembled a $((ENTRIES - 1))-entry concat list from $PNGS PNGs"

if ! command -v ffmpeg >/dev/null 2>&1; then
  cat <<EOF

ffmpeg is not on this machine, so the PNG sequence IS the deliverable:
  $OUT/frame-*.png      exact $((16#$W))x$((16#$H)) frames, one per distinct screen
  $OUT/frames.tsv       when each appeared, how long it held, its digest
  $OUT/frames.concat    an ffmpeg concat list, ready for a machine that has one:
      ffmpeg -f concat -safe 0 -i $OUT/frames.concat -r $FPS -pix_fmt yuv420p $OUT/film.mp4
EOF
  exit 0
fi

# `if`, not `[ … ] && …`: under `set -e` a false test as the last statement of the script's flow
# exits it, which would silently skip the assembly whenever SCALE was left at 1.
VF="fps=$FPS"
if [ "$SCALE" != "1" ]; then
  VF="scale=iw*$SCALE:ih*$SCALE:flags=neighbor,$VF"
fi

if [ "${LOSSLESS:-0}" = "1" ]; then
  # FFV1 in Matroska: every pixel survives, which matters if the video is ever the evidence rather
  # than the illustration. H.264's 4:2:0 chroma subsampling would soften a 1-pixel-wide UI rule.
  ffmpeg -hide_banner -loglevel error -y -f concat -safe 0 -i "$OUT/frames.concat" -t "$TOTAL" \
    -vf "$VF" -c:v ffv1 -level 3 "$OUT/film.mkv"
  echo "video -> $OUT/film.mkv (FFV1, lossless, ${TOTAL}s)"
else
  ffmpeg -hide_banner -loglevel error -y -f concat -safe 0 -i "$OUT/frames.concat" -t "$TOTAL" \
    -vf "$VF" -c:v libx264 -preset veryslow -crf 18 -pix_fmt yuv420p "$OUT/film.mp4"
  echo "video -> $OUT/film.mp4 (H.264, ${TOTAL}s; LOSSLESS=1 gives FFV1 in .mkv instead)"
fi
