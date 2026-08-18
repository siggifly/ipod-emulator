# The Apple logo, and the operation that draws it

**The logo was never missing. It was in the framebuffer the whole time, 4 852 halfwords of it,
lying at the top-left corner in rows 62 pixels wide.** What was missing was the *operation* that
puts it where it belongs — one rectangle blit, whose geometry Apple's bootloader states in eight
words it already writes and which this model was storing and ignoring.

Measured 2026-08-14 on `retail-boot.sh --clock=5`. Every number below is from a run of the command
in §7.

---

## 1. What the fragment was

The boot film has always had a frame it could not explain: 2 922 non-black pixels, held from 10 M
to 50 M instructions, byte-identical to the ROM→RetailOS handoff dump.
[research/04](04-bypass-ledger.md) calls it *"the boot ROM's logo"* on a hunch; nothing had opened
it.

Looked at, it is four white diagonal lines across the top fifteen scanlines and black everywhere
else. Looked at **in address order rather than in screen order**, it is periodic: the non-black mask
agrees with itself at lag 62 for 96.4 % of its length, and at no other lag under 400 does better.
Its runs, in address order, are

```
1, 2, 4, 6, 8, 9, 10, 10, 11, 12, 11, 12, 11, 11, 10, 9, 9, 7 …
```

— a shape that grows and shrinks, once per 62 halfwords. Refolded at 62 halfwords per row it is the
**Apple logo**: 62 wide, the whole tile 78 tall, a chrome-shaded silhouette with a proper
anti-aliased edge (`(239,235,239)`, `(247,243,247)`, `(214,211,214)` … all of them true greys in
RGB565, `r5 == b5` and `g6 == 2*r5`).

So the picture existed, at 16 bits per pixel, in the co-processor's memory, 8 M instructions into
every boot this project has ever run.

## 2. Why it was in the wrong place: `0xE0000` is not the panel

`BCMA_CMDPARAM` is Rockbox's name for internal `0xE0000`, and Rockbox's own gloss on it is
*"Parameters/data for commands"*. This project has read it as the panel since research/03, because
for Rockbox the two are the same picture: `lcd_update_rect` writes a bare 320x240 frame there — no
header, rows placed by the host at `BCMA_CMDPARAM + 640*y + 2*x` — and issues `LCD_UPDATE`
(command 0). Rockbox never needs to know there is a frame store on the other side of the command.

Apple's bootloader does know. A new instrument — the co-processor's traffic in the order it arrived,
rather than the state it left — says so in four lines:

```
0x000e0000  76816 halfwords
command 0x5
0x000e0000   4852 halfwords
command 0x5
```

**One run of 4 852.** Not 78 runs of 62 at a 320-halfword stride, which is what a host placing rows
itself would look like, and which is what a broken address latch would have turned into scattered
garbage rather than into a clean 62-halfword pitch. The bootloader wrote the tile linearly and
handed the co-processor a *command*. The placement was the co-processor's job and this model was not
doing it.

`76816` is the same story: `16 + 76800`. Sixteen halfwords of something, then a frame.

## 3. The header

The sixteen halfwords are eight words, and they are readable straight out of the co-processor's
memory at the end of a run — `--bcm-peek=0xe0000:16`, after the second command, so this is the
logo's:

| off | value | meaning |
|---|---|---|
| `+0x00` | `0x00000034` | **unidentified.** Constant across both commands of a retail boot |
| `+0x04` | `0x00000081` | `x0` = 129 |
| `+0x08` | `0x00000051` | `y0` = 81 |
| `+0x0c` | `0x000000be` | `x1` = 190 — inclusive, so 62 wide |
| `+0x10` | `0x0000009e` | `y1` = 158 — inclusive, so 78 tall |
| `+0x14` | `0` | unidentified, zero in both commands |
| `+0x18` | `0` | unidentified, zero in both commands |
| `+0x1c` | `0x000025c8` | 9 672 bytes = **62 × 78 × 2** |

**The length word is what makes this derived rather than fitted.** A rect read out of the wrong four
words would have to coincidentally satisfy a byte count the same firmware wrote in a fifth. Two
further agreements fall out of the same eight words and were not used to find them:

- `(129+190)/2 = 159.5` and `(81+158)/2 = 119.5`, against a panel centre of `(159.5, 119.5)`. The
  logo is dead centre.
- the *other* command in the same boot decodes to `(0,0)-(319,239)` with a length of `0x25800` =
  320 × 240 × 2 — and the run that preceded it was 76 816 halfwords, which is that frame plus this
  header, exactly.

So: **command 5 is `LCD_UPDATERECT`, and its parameter block is an 8-word header at `BCMA_CMDPARAM`
followed by the rectangle it describes.** Rockbox's header file guesses at this in a comment it
never followed up — *"The following might do more depending on word at 0xE00000"*, attached to
`BCMCMD_LCD_UPDATERECT` — and this is what it does.

The four commands a retail boot issues, in order, are now printed by the run report:
`0x13`, `0xa`, `0x5`, `0x5`. The two `5`s are the black fill and the logo. `0x13` and `0xa` are
unidentified and move no pixels.

## 4. The implementation

`Bcm` grows a **frame store** — 320×240 of RGB565 that is not host-addressable, because on the real
part it is not. `kick()` dispatches:

- **command 0, `LCD_UPDATE`** — the frame store takes the bare 320×240 image at `BCMA_CMDPARAM`.
  Rockbox's authority, **not exercised by anything in this project**: Apple's bootloader never sends
  command 0. It is implemented anyway because the alternative is a model that answers "the panel
  never changed" to a Rockbox-shaped host.
- **command 5, `LCD_UPDATERECT`** — read the eight words, check `x0 <= x1 < 320`, `y0 <= y1 < 240`
  and `len == w*h*2`, and blit `w × h` halfwords from `BCMA_CMDPARAM + 0x20` into the frame store at
  `(x0, y0)`. A header that fails the check is **recorded and skipped**, and the run report prints
  it, rather than being smeared across the panel. Both commands of a retail boot pass; zero are
  rejected.

One step in this is the model's own and a reader must not mistake it for hardware: after a command
the frame store is **written back over `BCMA_CMDPARAM`**, so that `--bcm-dump`, `--bcm-ppm`,
`--bcm-film` and the GUI keep reading the panel at the address they always have. The alternative was
to give the frame store an address of its own — which would have been a *second* invented address,
and would have moved every recipe in `research/` onto it. The two disagree only in the window
between a stage and its command, and the film catches exactly one sample of that: the 5-pixel frame
at 5 M, which is this header sitting in the buffer before the command that consumes it. Six of its
eight words are non-zero; five of the six render non-black.

**That accounts for the last six pixels, too.** The fragment scored 2 922; the placed logo scores
**2 916**. The difference is those six header words, which used to be pixels of the panel and are now
not.

## 5. What is on the screen, and a premise this corrects

The job this came from was written as *"a real iPod 5G boots to a white screen with a dark Apple
logo, and ours gets the white and not the logo."* The white is real and the logo is real, and they
are not the same screen and not in that order:

| @ | what | who |
|---|---|---|
| 0 | dark panel | — |
| 5 M | 5 pixels | the first rect header, staged, not yet commanded |
| 8 M | **black screen, chrome Apple logo, centred** | the bootloader: a 320×240 black fill, then a 62×78 blit |
| 52 M | solid white | **RetailOS**, DMA-ing 76 800 halfwords into its own first surface |
| 1 002 M | the Language list | RetailOS's compositor |

Apple's bootloader stages a full-screen **black** frame and then a **light** Apple onto it. That is
what its own bytes are; nothing here is inferred. The white screen that follows is RetailOS's, it
arrives 44 M instructions after the logo, and it is what covers the logo up — on this machine, in
about 0.6 s of PP5021C time, because our disk is faster than a 30 GB 1.8-inch drive and the logo's
real dwell is however long the drive takes.

## 6. Predictions that measured out to nothing

- **"The logo is an image operation like `vc_image_blt`, and we need the 21 `vc_image_*` entry
  points in `vmcs.bin`."** The most reasonable hypothesis available and it was aiming one level too
  deep. It is one rectangle blit, and its geometry is in eight words the firmware already writes
  into memory we already model. Nothing had to execute.
- **"If the answer is that the logo needs the VideoCore to actually execute code, say so and
  stop."** It does not, and the reason is worth keeping: the bootloader is talking to the
  co-processor through the *command* interface, which is a fixed-function protocol, not through the
  RPC ring RetailOS uses later. Two interfaces, one device.
- **"The logo lives in NOR and is drawn by a path we do not reach."** It is staged into the
  co-processor 8 M instructions into every boot this project has run, by a path that was already
  being executed and already being counted.
- **"The address latch is desynchronising and scattering the rows."** The most promising lead for
  about ten minutes — the latch is a stateful low/high alternation and a host writing only the low
  half once would poison every latch after it. Measured: 81 718 latches, all well-formed pairs in
  the sample, and the logo arrived in **one contiguous run**, so no latch was involved in placing it
  at all. A desync would have scattered it, not pitched it evenly.
- **"The fragment might be a mangled logo."** This one measured out to *everything*, and it was in
  the job as the third candidate. Worth recording as a hit, since the other four are misses.

## 7. Reproducing it

```sh
# the timeline, the commands, and the two blits with their rects
BUDGET=60000000 tools/ipod-boot/retail-boot.sh --clock=5 \
    --bcm-ppm=_out/logo.ppm --bcm-peek=0xe0000:16

# the logo on the panel, held from 10 M to 50 M, in a film
BUDGET=250000000 ipod-film run --out=_out/film/logoboot --every=5M
```

The baseline is **unmoved by all of it** — `--clock=5 --stop-when-idle=400000000` at
`BUDGET=4000000000` still reports `Idle after 1562789429`, 38 220 code buckets, 770 ATA commands,
4 unmapped reads, `230572 halfwords written` and `177508 internal words held`, pre-fix and post-fix,
to the instruction and to the word — and again after this work was rebased onto the cross-platform /
IPSW-disk merge, which moved `retail-boot.sh`, `film.rs`, `trace.rs` and `lib.rs` underneath it. It could hardly do otherwise: nothing here changes a value the
CPU can read, because nothing in this boot ever reads `BCMA_CMDPARAM` back — the run report's
`internal reads` block lists ten distinct offsets and none of them is in the buffer.

## 8. What this settles elsewhere

**[research/12](12-how-retailos-draws.md) §8 item 4 — "surfaces are allocated from `0xE0000` upward,
on Rockbox's authority" — is now constrained rather than free.** `0xE0000` is the co-processor's
command-parameter buffer, so a real co-processor would not hand it out as a resource, and the
model's `--bcm-registry` surface allocator handing it out is now a *known* wrong choice rather than
an unexamined one. It is still what the model does, and the reason it has not been changed is that
moving it requires modelling what makes a surface visible — `element_add` / `update_submit`, whose
DispmanX reading research/12 §5 marks *proposed, not derived*. Recorded, not fixed.

**The fifth wall of the week that was our own model, and the shape is the one R12 names.** A device
that stores pixels and executes no operations answers exactly like a device that is not there: the
picture is present, the picture is wrong, and nothing downstream can say which. The discriminator
here was to stop looking at the panel in screen order and look at it in address order — one
autocorrelation over the non-black mask, which said 62 and meant *somebody else was supposed to
place these rows*.
