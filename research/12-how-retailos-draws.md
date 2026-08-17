# How RetailOS draws

**The ARM side of the display, described as it works.** The counterpart to
[research/11](11-the-videocore-runtime.md), which describes the other side of the bus.

This file is deliberately **not** a narrative of how any of it was found. That record lives in
[research/10](10-the-resource-image.md) Addenda 20–29, it is ten successive corrections, and it is
among the most valuable things in this repo precisely because it shows what was believed and why it
was wrong. It is also, for a reader who wants to know how the pipeline works, ten documents to read
and three retractions to spot. So: the chronology stays where it is, this is the description, and
§9 lists the framings the addenda contain that are dead — with pointers, so neither file can be read
without the other being findable.

**Every count below was measured on 2026-08-14** by running the command in §10 on this branch with a
privately built binary. Where a fact comes from a decompilation rather than a run, it says so and
names the function. Where something in the model is **chosen rather than derived**, §8 says so —
that section is not a caveat, it is part of the description, because a pipeline description that
reads as complete while parts of it are assumed would be worse than the addenda it replaces.

---

## 0. The pipeline, end to end

Six stages. The first three are pure ARM and run whether or not there is a co-processor at all; the
last three are the bus, and until 2026-08-14 the fourth of them had never run.

| | stage | what it is | arrivals, control arm | with `--bcm-registry` |
|---|---|---|---|---|
| 1 | **Paint** | widgets rasterise into a CPU-side graphics context | `0x0021acac` **566** from 5 sites | **566** from 5 sites |
| 2 | **Show** | a walk over a parent's child list, calling each child's visibility slot | `0x00219284` **2 278** from 7 · `0x0021ada8` **68** from 11 | **2 278** · **68** |
| 3 | **Damage → flush** | a registered callback marks a region dirty; another asks the display server to flush | `0x001650f8` **42** | **41** |
| 4 | **Present** | upload the dirty scanlines to a co-processor surface, tell it to show them, flip buffers | `0x00164f44` **0** | **41** |
| 5 | **Transport** | 16-byte framed RPC over a ring pair, on a channel found in a directory at internal `0x1f0` | `0x0028861c` **0** | **165** |
| 6 | **Composite** | the co-processor's DispmanX-shaped display / resource / element model | — | `bcm gencmd: 165 answered, 0 dropped` |

**Stages 1 and 2 are identical in both arms, to the arrival.** That is the single most useful fact in
this file: painting and showing never depended on the co-processor, and any explanation of a blank
screen that reaches for widget state is explaining the wrong stage. What the co-processor gates is
stage 4 onwards, and it gates it completely — 42 flushes producing 0 presents.

**This is RetailOS's path and it is not the only one.** Apple's *bootloader* reaches the same panel
without any of stages 1–5, through a **command interface** the co-processor also carries: stage an
8-word header plus a rectangle at `BCMA_CMDPARAM`, write the command word, and the co-processor
places the rectangle. That is what draws the Apple logo, it is a fixed-function protocol rather than
an RPC, and it is described in [research/14](14-the-apple-logo.md). Two interfaces, one device — and
until 2026-08-14 the model implemented neither the placement nor the command, so the bootloader's
logo sat in the transfer buffer at 62-halfword pitch and was read for weeks as a mangled frame.

The 41-against-42 in stage 3 is not a regression: the control arm's 42nd flush is the one that would
have presented had a layer ever been bound, and with the registry on the run ends 542 400
instructions earlier having done the work.

---

## 1. Paint — `FUN_0021acac`

Widgets rasterise into a graphics context built by `FUN_00211bd4`. `FUN_0021acac` decompiles to a
real painter: **gradient fills interpolated per scanline, four edge lines, corner handling, and
text**. Nothing about it is deferred or symbolic — by the time it returns, pixels exist in ARM
memory.

Measured over a full boot, `0x0016b044` (`MP3ExampleTask`'s body) live as a reached control:

```
0x00139428 x66    0x0017e80c x452   0x001ab84c x2   0x0023b094 x1   0x002448f8 x45
                                                              566 arrivals, 5 sites
```

Identical with `--bcm-registry` on. **"RetailOS never draws" was always wrong; it draws constantly.**

**Visibility gates the painter.** `FUN_00255b50` is `(flags & 0x1800) == 0x800` and it is the first
line of both the painter and the show walk in §2 — so a subtree in the wrong visibility state is
neither painted nor descended into. That gate is real and it does block one subtree (§9, ①), but it
is not why the screen was blank: 566 paints happen regardless.

---

## 2. Show — a walk over `[container + 0x78]`

**Showing is not a decision made about a widget.** It is a walk over a **parent's child list**.
`FUN_0017db04`, decompiled:

```c
void FUN_0017db04(int container, int visible, undefined4 p3, int *p4)
{
  FUN_001e2828(&it, container + 0x78);                 // iterator over the child collection
  while (FUN_001e27e0(&it, &child) != 0) {
    (**(code **)(*child + 0xa0))(child, visible ? 1 : 0);   // the visibility slot on each child
  }
  FUN_001e2838(&it);
}
```

Show and hide are the same walk with a different argument. So a widget can fail to be shown in
exactly two ways: it is in no container's `+0x78` collection, or its container is never itself
shown — and the second recurses.

**The vtable slots**, read out of the table at `0x0066daf4` rather than inferred:

| slot | meaning | implementation |
|---|---|---|
| `+0xa0` | **`setVisible(bool)`** | `0x00219284` for the list-widget class · `0x0021ada8` for others |
| `+0xa4` | the **visibility-changed hook** `setVisible` calls | `0x001ae070` for that class |

`FUN_00219284` — the `+0xa0` implementation — is a state machine, and the important property is that
**both arms return silently when the state is neither**:

```c
void FUN_00219284(int *this, int visible)          // vtable +0xa0
{
  if (visible == 0) {
    if ((this[8] & 0x1800U) != 0x800)  return;     // must currently be VISIBLE to hide
    FUN_0021a0fc(this, 0x1000);
  } else {
    if ((this[8] & 0x1800U) != 0x1000) return;     // must currently be HIDDEN to show
    FUN_0021a0fc(this, 0x800);
  }
  (*(code **)(*this + 0xa4))(this, visible);       // only now does the hook fire
}
```

`this[8]` is `[obj + 0x20]`. `FUN_0021a0fc(obj, bits)` is
`[obj+0x20] = ([obj+0x20] & 0xffffe7ff) | bits` — it **clears both bits before setting one**, so it
can never itself produce `0x1800`. An object holding `0x1800` therefore got there from some other
writer, and `setVisible` on it is inert in both directions.

Measured, both arms identical:

```
0x00219284  x2278 from 7 sites   (0x0017dbd8 x671 · 0x002199d8 x1280 · five more)
0x0021ada8    x68 from 11 sites  (0x0017db20 x15 · 0x0017dbd8 x8 · nine more)
```

Sixty of `0x0021ada8`'s sixty-eight come from two functions in `0x0017dxxx` / `0x0017exxx`, which
are the show walk above.

---

## 3. Damage and flush — the display server declines

The display server singleton is `FUN_001647c0`. `FUN_0017eeb0` initialises the display and registers
a callback trio:

```
FUN_000f223c(obj + 0x24, 0, 0x0017ef98, 0x000cab18, 0x000c3004)
                            ^ marks damage        ^ flushes it
```

Both run. `FUN_001650f8` is the flush: **lock → `FUN_00164cb8` → present → signal**. It presents only
if `FUN_00164cb8(server, layer)` holds, and that needs `[server + 0x6c4] == FUN_00164610(server,
layer)` — i.e. a **bound layer**.

The bind is `FUN_00164878`, and `--enterlog` on it alone — so its rows fall inside the detail print
rather than past it — names the reason in the arguments. Both arms, same recipe:

```
control        0x00164878 lr=0x0017efe4  r0=0x10882c3c r1=0xffffffff r2=0 r3=0x0017ef98   x42
--bcm-registry 0x00164878 lr=0x0017efe4  r0=0x10882c3c r1=0x00000000 r2=0 r3=0x0017ef98   x41
```

**The layer index is `r1`, and on the control arm it is `-1`.** `FUN_00164610` returns 0 for any
index ≥ 11 (unsigned compare), so the bind returns 0, forty-two times, silently — and `FUN_00164f44`
is never called. `r0` is the same server object in both arms, so nothing else about the call
changed: one argument did.

It is `-1` because the one attempt to create a layer failed, four calls lower:

```
0x001649ac  create layer     x1     -> -1
0x00164450  create surface   x1     -> 0
0x00286ca8  allocate it on the co-processor   x1   -> -1 without touching the bus
0x0028861c  send the RPC     x0
```

`FUN_00286ca8` opens on `if (-1 < *DAT_00286d70)`, and `DAT_00286d70` is `0x1082359c` — the RPC
channel index, which no code writes after the BSS initialiser. With `--bcm-registry` the same chain
reads:

```
0x001649ac  x1   ·  0x00164450  x2   ·  0x00286ca8  x2   ·  0x0028861c  x165
0x00164878  x41, r1 = 0x00000000     ·  0x001650f8  x41  ·  0x00164f44  x41
```

Two surfaces, two addresses, a bound layer, and every flush presenting.

---

## 4. Present — `FUN_00164f44`

RetailOS's `lcd_update`. It uploads **only the dirty scanlines**, tells the co-processor to show
them, and flips:

```c
FUN_00287be8(back->bcmAddr + stride*y0, cpuFB + stride*y0, stride*(y1-y0), 1);  // upload
FUN_00286b6c(back + 0x20, ...);                                                 // show
tmp = ctx->back; ctx->back = ctx->front; ctx->front = tmp;                       // flip
```

The scanline upload is why the co-processor's halfword-write count moves from 230 572 to 2 749 468
between the two arms: 41 frames of compositing is 5.2 MB the machine never used to move.

**Double buffering is confirmed rather than assumed.** Dumping both buffers at the end of the same
run:

```
--bcm-ppm         (front, 0x000e0000)   76 607 non-black pixels of 76 800
--bcm-dump=0x106000:0x140:0xF0 (back)   76 607 non-zero pixels of 76 800
cmp front back  ->  byte-identical, 230 415 bytes
```

which is what a double-buffered *static* screen should look like, and is not what an accidental
single allocation or a stale bootloader frame would look like. The image is the iPod 5G **"Charged"**
screen — title bar, centred battery, plug glyph, anti-aliased text. Nothing in the model draws; every
pixel came out of RetailOS's own compositor through this function.

### Mailbox `0x16` is not this stage

`t_graphicsManager` sits in `KS_receive` on mailbox `0x16` for the whole boot, and it is tempting to
read that as the output stage stalled. It is not. Its pump at `0x00189060` decodes tag `0xaaaa0001`
and dispatches a virtual `+0xc` on either the display server or `FUN_00163b0c()+8`, chosen by
`[msg+0x10]`. **It is the display server's asynchronous blit-request channel** — used when a view
wants the server to composite rather than drawing CPU-side. Its only sender is `FUN_00189008`, whose
caller `FUN_00180e54` gates on `[obj + 0xa0]`, and `[obj + 0xa0]` is **the image a full-screen image
view displays** (§9, ②). No photo is ever opened on this boot, so no image view has an image, so
nothing is sent. Nothing is broken.

---

## 5. Transport — the channel table at internal `0x1f0`

Everything above `FUN_0028861c` is ARM; everything below it is the bus. The transport is a
**directory of numbered channels**, each a ring pair with a numeric service tag, discovered before
any display call. Full derivation in [research/10](10-the-resource-image.md) Addendum 29 §2–§3; the
shape, because a description of the pipeline needs it:

**The header at `0x1f0`** — read by `FUN_00288058`, 16 bytes:

| off | width | meaning | test |
|---|---|---|---|
| `+0x00`, `+0x04` | u32 | read, never examined | — |
| `+0x08` | u32 | firmware / registry ready | must be **exactly 1** |
| `+0x0c` | u32 | address of the channel directory | non-zero, `& 3 == 0` |

**The directory** is **eight `u16` slots**. Slot value `0` means "no service"; otherwise it is a byte
offset from the directory base to a record. **The matching slot's index is the channel id.**

**The record** is 0x50 bytes: service tag at `+0x04` (1, 2 or 7), TX ring bounds at `+0x06`/`+0x08`,
RX at `+0x0a`/`+0x0c`, and four ring pointers — TX read `+0x10`, TX write `+0x20`, RX read `+0x30`,
RX write `+0x40` — **each alone in its own 16-byte block**, so either side updates its own without
touching the other's. Wrap is explicit (`if (wr == txEnd) wr = txStart`); the writer keeps `0x10`
bytes free so full never reads as empty.

**The wire format**, built by `FUN_0028861c`, is a 16-byte header followed by the payload padded up
to 16:

```
+0x00 u32  0xf1a55a1f          the magic
+0x04 u32  sequence            (prev + 1) & 0x7fffffff, one counter per channel
+0x08 u32  opcode
+0x0c u16  payload length, UNPADDED
+0x0e u16  0
```

The reply is the same shape. `FUN_002872fc` **rejects any reply whose word 0 is not the magic**, and
takes its length from word 3's low `u16`. Six display call sites read exactly `0x20` bytes and take
the word at `+0x10` — header plus one 16-byte payload, payload word 0 = result.

**The whole protocol is 16-byte granular**: header 16, payload padded to 16, ring slack 16, the four
pointer blocks 16 apart.

**Tag 2 is the display; tag 1 is GENCMD** — one service on one channel, a printf-formatted command
string in and a text stream back, *not* the directory itself; **tag 7 is unidentified**.

### The opcodes, counted by their call sites

`--enterlog=0x0028861c` with `--bcm-registry`, grouped by return address, and the totals reconcile
against the model's own counter (`bcm gencmd: 165 requests answered, 0 dropped`):

| lr | sending function | opcode | count | DispmanX shape |
|---|---|---|---|---|
| `0x00286f78` | `FUN_00286f34` | 1 | 41 | `update_start` |
| `0x00286ef8` | `FUN_00286eb4` | 2 | 41 | `update_submit` |
| `0x00286be8` | `FUN_00286b6c` | 3 | 41 | `element_add` — handle, two points, four rects |
| `0x00286c6c` | `FUN_00286c24` | 4 | 40 | `element_remove` — a handle |
| `0x00286d2c` | `FUN_00286ca8` | 8 | 2 | `resource_create` — (type, w, h, pitch) in, **(handle, address)** back |
| | | | **165** | |

The DispmanX column is **proposed, not derived** — it is the correspondence argued in
[research/11](11-the-videocore-runtime.md) §4, and only opcode 8's argument and reply shape force it.

---

## 6. The co-processor side

Described in full in [research/11](11-the-videocore-runtime.md) §3. In one paragraph, because the
ARM side is unreadable without it: the co-processor's display model has four object kinds —
**display** (a physical output, opened by numeric id), **resource** (off-CPU pixel memory the host
cannot address directly and pushes into), **element** (the binding of a resource onto a display, with
layer, source rect in 16.16 fixed point, destination rect, alpha and transform), and **update** (a
transaction bracket: nothing is visible until the update is submitted, and then everything in it
becomes visible together on one vertical blank).

`FUN_00164f44`'s sequence — opcode 1, then 4 if a previous object is live, then upload, then 3 — is
that bracket in that order, which is why the correspondence is worth stating even while it is
unproven.

Rows are padded to an **aligned pitch**, not `width × bpp`, on the documented later part. This
project has not yet had to care, because a 320-wide RGB565 frame at pitch 640 is already aligned —
but a host implementation that starts scaling will meet it, and a wrong pitch produces a sheared
image rather than an error.

---

## 7. Where the frame ends up

```
--bcm-registry off   0x000e0000   2 916 non-black pixels   the Apple boot logo, centred
--bcm-registry on    0x000e0000  76 607 non-black pixels   RetailOS's "Charged" screen
                     0x00106000  76 607                    byte-identical to the front buffer
```

`0xE0000` is `BCMA_CMDPARAM`, and Rockbox's own gloss on that name is *"Parameters/data for
commands"*. Rockbox stages a bare 320×240 frame there; Apple's bootloader stages a header plus a
rectangle. **It is a transfer buffer, not the panel** — the panel is the co-processor's own frame
store, and the model publishes the store back over the buffer so that one address is "the panel" for
every instrument. [research/14](14-the-apple-logo.md) §2–§4.

> *This block read `2 922 non-black … byte-identical to the ROM→RetailOS handoff dump` until
> 2026-08-14, and the sentence under it said `0xE0000` is where Rockbox **puts the panel image**.
> Both were true statements about the model and wrong about the machine. The 2 922 were the Apple
> logo lying unplaced in the transfer buffer plus the six non-zero words of the rect header that
> described where it should have gone; with the header consumed and the tile placed the same pixels
> score **2 916**.*

---

## 8. What in this description is assumed

Four things in the transport model are **chosen rather than derived**, and one thing is simply
absent. None of them would make the machine complain.

1. **Where the ring base lives (`0x40000`) and how big the rings are (8 KiB each).** The reader
   constrains the base only to be non-zero and 4-aligned, and the rings only to fit in a `u16`
   offset from it.
2. **Handle values are a counter.** Nothing in the reader constrains them beyond non-zero.
3. **Non-8 opcodes reply with a handle in payload word 0.** Six call sites read that word; none
   branches on it in any path reached here, so the value is unconstrained by measurement.
4. **Surfaces are allocated from `0xE0000` upward — on Rockbox's authority, not the
   co-processor's.** The reply format says the co-processor returns *an* address, not *which*. If
   this is wrong, the frame lands somewhere else and §7's pixel count is about the wrong buffer.
   ~~This is the one to attack first.~~ **Attacked 2026-08-14, and it came back a known-wrong choice
   rather than an unexamined one.** `0xE0000` is the co-processor's **command-parameter buffer**
   ([research/14](14-the-apple-logo.md) §2): the host stages images there for the command interface,
   so a real co-processor would never hand it out as a free resource. The model still hands it out,
   because moving it requires modelling what makes a surface *visible* — `element_add` /
   `update_submit`, whose DispmanX reading §5 marks proposed rather than derived — and that is a
   larger speculative step than the one it would fix. **Recorded, not repaired.** The pixel counts
   in §7 are not affected: RetailOS's compositor writes there and the panel is read there, and
   implementing the command interface left the whole registry arm byte-identical (§10).
5. **There is no timing model.** The reply is placed **synchronously, inside the doorbell write**.
   RetailOS tolerates that because `FUN_002883d4` refreshes the co-processor's write pointer before
   it blocks — but a real co-processor answers later, and **any bug that only appears when the reply
   is late cannot appear here.** This emulator has already made the answers-too-early mistake twice,
   with the drive's `IDE_COMPLETION_USEC` and the wheel's `OPTO_REPLY_USEC`; both times the firmware
   armed a wait and only *then* acknowledged.

So: **a drawn frame is evidence that RetailOS's own pipeline works end to end. It is not evidence
that we have a co-processor.** Bypass #6 is still 🔴 —
[research/04 §#6 today](04-bypass-ledger.md) carries the retirement condition in these terms.

---

## 9. Framings in the addenda that are dead

Kept as pointers rather than as summaries, because the addenda are the record and this section exists
only so a reader of *this* file is not sent to reconstruct a retraction on their own.

① **Addendum 24 — "Wall A is a stuck visibility state."** The mechanism is real and correctly
described: two widget chains cross an object whose flags are `0x5a00`, `0x5a00 & 0x1800 == 0x1800`,
and `setVisible` on it is inert in both directions, so the show walk never descends past it. What is
dead is the framing that this is *why the screen was blank*. Retracted by Addendum 25: 566 paints
happen regardless, and the output stage is a different stage entirely. It remains the reason **that
subtree** is not shown, and that question is still open.

② **Addendum 25 — "RetailOS renders and never presents."** The headline survives and is confirmed
here. Two things under it do not. `[obj+0xa0]` was called a **draw target**; it is **a photo** — the
image a full-screen image view displays, fetched as record type 3 from a media database, and nothing
assigns it because no photo is ever opened. And mailbox `0x16` was called the output stage; it is
the display server's async blit-request channel. Both retracted by Addendum 26 §2. The addendum's
own preamble also carries "RetailOS never touches the VideoCore", which was a saturated
`--watch-range` log and is retracted in Addendum 26 §1.

③ **Addendum 26 §4 — "the words at `0x1f0` come back zero, so only a running `vmcs.bin` could
populate the block."** The chain is right and the cause is wrong. The co-processor's memory held
`1` at `0x1f8` the whole time; **our own `Bcm::read8` corrupted it on the way to the CPU**, serving
each half of an `ldrh` from a fresh FIFO pop so a 16-byte read drained 32 bytes and spliced bytes
from alternate words. The CPU was handed `0x2f01fc78`, byte-exactly what the co-processor's own
bytes at `0x200`/`0x204` predict. Retracted by Addendum 29 §1. **The general lesson is the one worth
carrying out of this whole file: a model defect looks exactly like missing hardware**, and it drove
the strategy toward emulating a co-processor for two sessions. §3 of that addendum — the arrival
counts through the flush chain — stands unchanged and is what made the derivation possible.

④ **Addendum 28 §2 — "GENCMD is the service directory."** It is one service *on* one channel of the
directory: tag 1, opcode 1, a printf-formatted command string. `gencmd_register` registers a
*command name* with that service and has nothing to do with how a channel is found. Corrected in
Addendum 29 §4.

⑤ **Addendum 28 §3 — "none of the 183 runtime symbol names appear in `vmcs.bin`."** Exactly
backwards. The search hit the **NOR** copy rather than the `rsrc` copy, and used `grep -c` on binary
data, which reports no count at all. All 183 are present in the `rsrc` copy, indexed by a real export
table at `0x2160C`. Corrected in Addendum 29 §5 and §3c.

---

## 10. Reproducing every number in this file

```sh
# stages 1–5, both arms. 0x0016b044 is the reached control.
tools/ipod-boot/retail-boot.sh --clock=5 --stop-when-idle=400000000 \
  --enterlog=0x0021acac,0x0021ada8,0x00219284,0x001650f8,0x00164878,0x00164f44,\
0x00164450,0x001649ac,0x00288058,0x00286aa8,0x00286ca8,0x0028861c,0x0016b044

# the same, plus the frame and its back buffer
… --bcm-registry --bcm-ppm=front.ppm --bcm-dump=0x106000:0x140:0xF0:back.ppm
cmp front.ppm back.ppm

# §3's register file at the bind site. Watch it ALONE, or its 42 arrivals fall past the
# 400-row detail print and only the histogram shows them — which is how the r1 value went
# unmeasured in the combined run above.
… --enterlog=0x00164878,0x0016b044
```

# the command interface, and the A/B that says implementing it disturbed nothing here.
# One variable: the pre-fix binary against the post-fix one, same flags, same disk.
… --bcm-registry --bcm-dump=0xE0000:140:F0:reg.ppm
#   both arms: Idle @1812316856, 38 476 buckets, 706 ata commands, 521 gencmd answered,
#   and `cmp reg-pre.ppm reg-post.ppm` is silent.

with `BUDGET=4000000000`. Read the `callers (uncapped census, N distinct)` block at the bottom of
the arrivals report, not the rows above it: the rows are an ordered sample and say so, the histogram
is counted on arrival and cannot saturate.

**Note the widths.** `--bcm-dump`'s width and height parse as **hex**, so `0x140:0xF0` is exactly the
320×240 panel; passing `140:F0` without the prefix reads out an 800×576 window instead.
