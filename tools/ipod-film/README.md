# `ipod-film` — a recording of the panel, not a screenshot of a window

`--bcm-dump=ADDR:W:H:FILE` writes the co-processor's surface **once**, when the run stops. That
answers "what was on the screen at the end" and nothing else — every screen the boot passed through
on the way is gone by the time the dump is taken, and if the run stopped somewhere uninteresting the
dump is of nothing.

This samples the same surface, with the same converter, on an instruction cadence, and keeps every
frame that differs from the one before it.

```
tools/ipod-film/film.sh --out=_out/film/boot
tools/ipod-film/film.sh --out=_out/film/menu --every=1M -- --clickwheel \
    --wheel='@1500M:touch,+2M:press=select,+2M:release'
```

Everything after `--` goes through to `tools/ipod-boot/retail-boot.sh`, so a wheel script, an
`--enterlog` watch or a second `--bcm-dump` all still work in the same run.

## What comes out

| | |
|---|---|
| `frame-NNNNN.png` | one file per distinct picture — **exactly 320x240**, no scaling, no interpolation, no cursor, no window chrome |
| `frames.tsv` | the manifest: when each frame appeared, when it was last seen, how many samples it held, its non-black count and its digest |
| `frames.concat` | an ffmpeg concat list with each frame's real duration, so the video's timing is the machine's timing |
| `film.mp4` | assembled, if `ffmpeg` is present. `LOSSLESS=1` writes FFV1 in `film.mkv` instead |
| `run.txt` | the whole boot log, including the film's own frame table |

**Without `ffmpeg` the PNG sequence is the deliverable** and the script says so, prints the exact
command to run on a machine that has one, and exits 0. The concat list is written either way.

## The frames are the panel, not a picture of the panel

A frame is 76 800 halfwords read out of `Bcm::mem` — the co-processor's own memory, at internal
`0x000e0000` — expanded RGB565 → RGB888 with the identical bit-replicating expression `--bcm-dump`
uses. Nothing is resampled, composited over, or drawn around.

**One qualification, added 2026-08-14.** `0x000e0000` is `BCMA_CMDPARAM`, and it is a *transfer
buffer*, not the panel: the panel is the co-processor's own frame store and the host cannot address
it. The model publishes the frame store back over the transfer buffer after every command, so the
address a film reads is the panel at every instant except the window between a host staging an image
and issuing the command that consumes it. A film catches that window when it lands on it — it is the
5-pixel frame at 5 M in every boot, which is a rectangle header sitting in the buffer. See
[research/14](../../research/14-the-apple-logo.md) §4. The measured form of the claim above:

```
$ ffmpeg -i _out/film/armB/frame-00004.png -pix_fmt rgb24 -y roundtrip.ppm
$ cmp roundtrip.ppm _out/film/armA.ppm     # silent: 230 415 bytes, identical
```

— a film's last frame, decoded by somebody else's PNG decoder, and a `--bcm-dump` PPM of the same
instant written by a *different process in a different run*, are byte-identical. That is three
claims in one `cmp`: the PNG encoder is correct, the film's RGB565 expansion matches `--bcm-dump`'s,
and the filmed and unfilmed arms reached the same screen.

## Deduplication, and why the manifest is the point

A 1.8 G-instruction boot at a 2 M cadence is ~900 samples, and RetailOS spends 52 % of it on a blank
white screen. Writing 900 near-identical PNGs would bury the four frames that matter. Consecutive
identical samples collapse into one **frame** that carries the instruction count it first appeared
at, the one it was last seen at, and how many samples it held:

```
film 0x000e0000 320x240 every 2000000 -> _out/film/armB
  908 samples, 5 frames, 5 distinct pictures
     0  @0             held       4000000 (    2 samples)        0 non-black  frame-00000.png
     1  @3999970       held       4000000 (    2 samples)        5 non-black  frame-00001.png
     2  @7999970       held      44000000 (   22 samples)     2916 non-black  frame-00002.png
     3  @51999952      held     950000000 (  475 samples)    76800 non-black  frame-00003.png
     4  @1001999952    held     814000000 (  407 samples)    75267 non-black  frame-00004.png
```

That is a whole retail boot: dark panel, five pixels of a rectangle header staged but not yet
commanded, **the Apple logo**, **950 M instructions of blank white**, and then the Language list.
*(Frame 2 read `2922 non-black` and was described as "the handoff frame" until 2026-08-14, when it
turned out to be the logo lying unplaced in the transfer buffer — research/14.)* `held` is the number the
video's timing comes from, and the number a write-up wants: *the white screen is up for 950 M
instructions*, not *frame 3 appears 475 times*.

A frame whose digest matches one seen **earlier but not immediately before** gets its own manifest
row and reuses the earlier file, marked `= frame N again`. "The screen went back to what it was" is
a different fact from "nothing happened", and both are readable off the manifest.

**Do not read the non-black count as "how much was drawn."** research/10 Addendum 30 §8 has three
different white UI screens scoring 76 607 / 75 267 / 75 791 of 76 800. The count separates a
composited frame from the bootloader's 2 922 and nothing finer; **the digest separates screens, and
only the image identifies them.** This is rule 2 of the project's working rules and it has cost a
published conclusion before.

## The control: does filming change the machine?

A recording instrument that perturbs what it records is this project's oldest failure mode, so the
claim is measured rather than argued. Two arms, identical but for the flag:

```
BUDGET=4000000000 retail-boot.sh --clock=5 --stop-when-idle=400000000 --bcm-registry \
    [--bcm-film=0xE0000:140:F0:2M:DIR]  --bcm-dump=0xE0000:140:F0:ARM.ppm
```

| | unfilmed (arm A) | filmed (arm B) |
|---|---|---|
| Idle after | 1 812 316 856 | **1 812 316 856** |
| code buckets | 38 476 | **38 476** |
| ata commands | 706 | **706** |
| bcm | 4 kicked, 2 frame updates | **4 kicked, 2 frame updates** |
| final `--bcm-dump` | 75 267 non-black | **75 267 — `cmp` says the two `.ppm`s are identical** |
| wall clock | ~131 s | ~134 s, for 908 samples |

Identical in every number the run reports. The Idle count also reproduces research/10 Addendum 30
§7's "post-fix, registry on, `GPIOL=8`" row to the instruction, so the machine is the one that table
was written on. (The two arms ran concurrently on the same laptop, so the 3-second difference is a
contended figure, not a benchmark — it is quoted to show the order of the cost, which is small.)

That is not luck, it is construction. `Machine::run(n)` runs `n` iterations of its loop and returns
`BudgetExhausted` having consumed exactly `n`, so issuing a budget in chunks issues the same
iterations in the same order — **the film adds no instruction and skips none**. The only per-call
work is `Memory::invalidate_fast`, which drops a resolution cache: it can change how fast an access
resolves, never what it resolves to. Sampling happens strictly between chunks, reads memory the CPU
is not executing out of, and writes files.

## Cadence

`--every=` is the sampling interval in instructions, and it is a real trade:

| | |
|---|---|
| `2M` (default) | ~800 samples over a boot. At `--clock=5` that is 400 ms of simulated time per sample — fine for menus, coarse for animation |
| `500k` | four times the samples and four times the scan cost, 100 ms apiece; the cadence to use when the question is *how long did a transition take* |
| `20M` | a whole boot in ~80 samples. Enough to see which screens happened, not when |

A screen that appears and disappears entirely **between** two samples is not in the film and nothing
in the output will say so. That is the instrument's one silent failure and there is no way around it
short of sampling every instruction; halve `--every` and see whether a new frame appears.

## The other way a film lies: `--stop-when-idle` cutting the script short

`Idle` means *no code has run that had not run before*, and **scrolling a list is not new code**.
A menu widget executes its novelty once and then repeats itself, so a wheel script whose gestures
are 60 M instructions apart accumulates idle time with every step and trips the 400 M window part
way through. That is not a hypothetical: the first attempt at launching Brick went

```
-> Idle after 2307665339 instructions
   last new code @1907665339; 400000000 instructions since, 1987653 CPU sleeps in them
```

— stopping one gesture and one Select before the thing the run existed to do, and the film's last
frame was a perfectly settled menu screen that gave no hint the script had been cut off. **Check the
`-> Idle after` line against your script's last anchor**, and raise `IDLE=` past it. `BUDGET` is not
the limit that bites here.

## Flags

| | |
|---|---|
| `--out=DIR` | where the sequence lands (default `_out/film/run`); wiped at the start of a run |
| `--every=N` | instructions between samples, `k`/`M` suffixes accepted |
| `--from=N` | **do not sample before instruction N.** The cadence that makes Brick's ball legible is 200 k, and 200 k over the 2.4 G of boot and menu navigation in front of the game is 12 000 surface scans of screens nobody is looking at. The machine is unchanged — the run is still issued in `--every`-sized chunks from instruction 0, so the no-perturbation property below is untouched; what is skipped is the scan |
| `--base=ADDR` | the surface to record, **hex**. Default `0xE0000`, the front buffer. The back buffer is `0x106000` |
| `--rate=N` | instructions per second of video (default 72 000 000 — a PP5021C's rate, so playback matches the pace real silicon would have executed these instructions). It is **not** the simulated clock: at `--clock=5` that runs 15x faster per instruction, so firmware timeouts fire earlier in the film than they would on hardware |
| `--scale=N` | integer nearest-neighbour upscale **for the video only**. The PNGs are always exact |
| `--fps=N` | output frame rate (default 30) |
| `BUDGET=` | instructions, as everywhere else (default 4 000 000 000) |
| `IDLE=` | `--stop-when-idle`'s window (default 400 000 000). **Raise it for any long wheel script** — §"The other way a film lies" |
| `LOSSLESS=1` | FFV1 in `.mkv` rather than H.264 in `.mp4`. H.264's 4:2:0 chroma subsampling softens a one-pixel UI rule, which matters if the video is the evidence rather than the illustration |

## Where the code is

The recorder is `--bcm-film=ADDR:W:H:EVERY:DIR` in the `trace` binary, implemented in
`tools/eapp-loader/src/film.rs` with a dependency-free PNG writer in `tools/eapp-loader/src/png.rs`.
It lives there rather than in this directory for the same reason `ipod-gui` grew a `map_hardware`
delegate instead of a copy: a second front end that stood its own machine up would be a second
machine the first time either copy was corrected. This directory is the recipe over the one machine.

`ADDR`, `W` and `H` are **hex** in the spec string, matching `--bcm-dump` exactly — `0xE0000:140:F0`
is the 320x240 panel. Having the two flags disagree about the width of a frame would be a trap, so
they do not. `EVERY` is decimal with `k`/`M` suffixes, matching `--wheel`'s times.

`--bcm-film` without a co-processor **exits 2** rather than recording nothing. A film of zero frames
and a screen that never changed look identical in the output, and this project has lost conclusions
to exactly that shape of silence.

### One known duplicate

`tools/ipod-gui/src/png.rs` carries an equivalent PNG encoder for its screenshot button. Folding the
two together is a follow-up, deliberately not done here: that crate is under concurrent edit. The
two produce the same bytes for the same pixels; whoever folds them should keep the `eapp-loader`
copy, since both front ends already depend on that crate.
