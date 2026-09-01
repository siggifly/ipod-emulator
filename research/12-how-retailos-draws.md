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

---

## Addendum — a booted menu that will not redraw (2026-08-30)

There is now a **reproducible arm that reaches a real menu**, which this file has not had before:

```
FLASH=<synthesised A444>  DISK=<drive built from iPod_20.1.3>  BUDGET=900000000 \
  ipod-boot loader --bcm-registry --clickwheel --bcm-png=OUT.png
```

It boots to the **Language menu** — English highlighted, battery indicator, the full list — at
75 267 lit pixels of 76 800, which is Addendum 10 §8's fingerprint. `ipod-boot loader` is the
recipe because it is the high-level boot shape (`--osos-from-disk --boot-osos --sysinfo --bcm
--pmu`), and a synthesised ROM carries no code to cold-boot.

**Input reaches the firmware and the screen does not move.** With
`--wheel='@6s:touch,@7s:rotate=+5,@9s:release'` against a no-script control, everything else pinned:

| | control | with input |
|---|---|---|
| script steps | 0 of 0 | **7 of 7 fired** |
| frames posted | 0 | **7** (2 dropped unread) |
| word reads of `DATA` **by RetailOS** | 0 | **4** |
| framebuffer md5 | `842b710e…` | **`842b710e…` — identical** |

So the wheel is not the open question here: the firmware armed the receiver
(`CTRL 0x600a1f00`, 4 `0x052a` set commands) and *read the frames back*. What does not happen is
the redraw.

**Where it stops, in this file's own terms:**

```
bcm: 0 commands kicked, 0 frame updates
bcm: 552 752 halfwords written, 668 read, 254 862 internal words held
bcm gencmd: 17 requests answered, 0 dropped
```

Stage 6 is answering, but only **17** requests against the 165 in §0's table, and it produces **zero
frame updates** while holding a quarter of a million words internally. The bootloader's
command interface is untouched too — `0 commands kicked` — so neither of the two interfaces in §0 is
putting anything on the panel after the first paint.

**The next measurement is already specified by §0** and needs no new instrument: count arrivals at
`0x00164f44` (Present) and `0x0028861c` (Transport) in the two arms above. If they differ, the
redraw is being issued and lost below stage 5; if they match, RetailOS is not issuing it, and the
question moves back up into stages 3–4 where the damage callback lives.

**Do not read "0 frame updates" as "nothing was drawn"** — 75 267 pixels are lit. Whatever painted
the menu did so by a route these counters do not count, and identifying that route is part of the
same question.

### The stage count, run — the redraw is never issued (2026-08-30)

The measurement the addendum above asked for, with `--enterlog` on all six of §0's stages, control
against input, everything else pinned. **True per-PC totals in both arms:**

| | stage | PC | control | with input |
|---|---|---|---|---|
| 1 | Paint | `0x0021acac` | 85 | **85** |
| 2 | Show | `0x00219284` | 2 276 | **2 276** |
| 2 | Show | `0x0021ada8` | 56 | **56** |
| 3 | Damage → flush | `0x001650f8` | 4 | **4** |
| 4 | Present | `0x00164f44` | 4 | **4** |
| 5 | Transport | `0x0028861c` | 17 | **17** |

Identical at every stage, while the input arm executed **371 M more instructions** (666 368 987
against 295 291 720) and the firmware read the wheel back. By the criterion stated above: the redraw
is **not being issued**. Nothing is lost below stage 5, so the question is above stage 3 — the input
is read by the driver and never becomes a UI event. Stages 1 and 2 being identical to the arrival
says the same thing §0 says: no explanation that reaches for widget state is explaining this.

**Read the per-PC `x{n}` totals, not the arrival listing.** `--enterlog`'s listing is `.take(400)`
(`trace.rs`), so on a 2 442-arrival run it prints a boot-time *sample* — and both arms' samples are
identical because the input lands at 6 s, long after the 400th arrival. Counting those lines gives
428 "arrivals" and answers a question nobody asked. `census()` is the total; the `x{n}` lines above
it are the per-address truth.

**The frames are not the problem, and here they are:**

```
@294169913  0xc000001a  stream  pos 0  buttons 0x00  touched
@294320585  0xc001001a  stream  pos 1  buttons 0x00  touched
@294321448  0xc002001a  stream  pos 2  buttons 0x00  touched
@294321448  0xc003001a  stream  pos 3  buttons 0x00  touched
@294321448  0xc004001a  stream  pos 4  buttons 0x00  touched
@294321982  0xc005001a  stream  pos 5  buttons 0x00  touched
@444242174  0x8005001a  stream  pos 5  buttons 0x00  released
```

Position 0→5 with `touched` held and a release after, tag `0x1a`. A textbook clockwise flick.

**A real defect found on the way, which is not this one.** Those seven frames span 152 000
instructions — **about 2 ms** — and three of them share a single instruction. That is not a gesture a
hand can make, and RetailOS **drops 2 of the 7**, reading only 4. At
`--wheel-click-instr=1500000` (20 ms per detent, so 100 ms for the flick) it reads **all 7, 0
dropped**. So the 20 000-instruction default loses input on a machine RetailOS is driving, and the
window's own `click_gap` deserves the same look — this is the same shape as `MIN_BUTTON_HOLD`, where
`diag`'s 150 ms poll made a 30 ms press invisible.

**It is not the cause here.** With every frame read, the six stage counts are still identical and the
framebuffer is still byte-identical. Timing was worth fixing and it does not explain the silence.

### Three instruments that do not answer this, and why (2026-08-31)

An attempt to find *where* the input stops, above stage 3. It did not find it, and the three
dead ends are worth more than the attempt: each one produced a confident wrong answer first.

**1. `--profile` is a 16-byte-bucketed SAMPLER, and its tail is noise.** Diffing the full census
(`--profile=20000`, since the default prints 15 rows of 11 217) gives "94 addresses only the input
arm executed", including `SerialOptoTask+0x24`, `EventManager+0x58` and an address inside the
`InputEvents` region — which reads exactly like the input path lighting up. **It is not.** Eighty of
those carry a *single sample*, and a one-sample difference between two 4.6 M-sample runs is
sampling noise. `--enterlog` on the same addresses says `NEVER REACHED` in **both** arms, and
`SerialOptoTask+0x24` is reached `x1` in **both**. Every profile address ends in `0` because it is a
bucket base, not a PC, so arming a counter on one misses by construction.

**2. The symbol names are string literals, not function entries.** `extract_symbols` recovers RTOS
task-name strings out of loaded SDRAM, so `EventManager+0x58` means *0x58 past where that string
sits*, not *inside that function*. The labels read like a call graph and are not one.

**3. `--callers=` on either stage address reports `none`, correctly.** `0x001650f8` and `0x00164f44`
are **mid-function observation points** picked for §0's arrival table, not call targets, so a scan
for branches to them finds nothing. The flag is fine; the question was wrong.

**What still stands**, because it is deterministic rather than sampled: the six per-PC `x{n}`
totals are identical with and without input, so the redraw is not issued. That result does not
depend on any of the three above.

**Next, and not with these tools.** The runtime samplers cannot give function boundaries. Getting
the *entry* of whatever owns `0x001650f8` is a static job — `tools/ghidra`, or `dis` over the OSOS
image — and until there is an entry address, there is nothing correct to arm a counter on.

### The firmware stops reading the wheel after boot — and v0.4.0 does the same (2026-08-31)

**Not a regression.** `v0.4.0`'s `ipod-boot`, built from the tag and run with an identical recipe,
produces a **byte-identical** framebuffer, the same `4 of 4` steps fired and the same `3 dropped
unread`. Whatever this is, it predates the Slint rebuild and every commit since. The hypothesis that
the window rewrite broke input is dead.

**What the frame log says, and it is not about drawing at all:**

```
@826047      query   buttons 0x00      <- read
@2092671     query   buttons 0x00      <- read
@8179135     query   buttons 0x00      <- read
@369108897   stream  pos 0  buttons 0x00  touched     <- dropped unread
@369166481   stream  pos 0  buttons 0x01  touched     <- the SELECT press, dropped
@369281708   stream  pos 0  buttons 0x00  touched     <- dropped
@369396935   stream  pos 0  buttons 0x00  released    <- dropped
```

RetailOS reads `CLICKWHEEL_DATA` **three times, all during boot**, at 0.8 M / 2 M / 8 M
instructions — and never again across the remaining 390 M. `irq 40 asserted 4 times`, `CTRL
0x600a1f00 (receiver armed)`, `0 frames refused for reporting-off, 0 for an unarmed receiver`. The
frames reach an armed receiver, the interrupt is raised, and nothing consumes it.

So the six-stage draw pipeline being idle is a **consequence, not the fault**: no redraw is issued
because no input is ever taken. The question moves off §0's table entirely and onto *why the firmware
stops servicing IRQ 40 once it has finished starting*.

**§0's own `--bcm-registry` arms are unaffected** — those measured a boot, not an input.

### The recipe in NEXT.md cannot fire as written

`NEXT.md`'s Wall-A block records the click wheel reaching the MAIN MENU with:

```
--wheel=@1500M:touch,+2M:press=select,+2M:release   BUDGET=3000000000
```

**Instruction anchors on a machine that halts.** With `--clock=5` this run reports
`usec 599999990` — the full 600 simulated seconds — after retiring only **395 118 540**
instructions, because `cpu sleep: 514697 halts, 520976 ms of simulated time spent halted`. The
`@1500M` anchor is never reached and the report reads `script: 0 of 4 steps fired`. That is
AGENTS.md §6's headline trap by name, and the recipe predates the rule. **Anchor in simulated time**
(`@150s:`) and all four steps fire.

**`press=<btn>` emits a zero-length press.** Its down and up land on the *same instruction*
(`@369306040` twice, above), which `emu.rs` already explains is invisible to firmware that polls —
Apple's `diag` samples once per 150 ms, which is why the window's `MIN_BUTTON_HOLD` is 300 ms.
Spelling it `down=select` … `up=select` two seconds apart makes it a real press. **It changes
nothing here** — the frames are still dropped unread — so it is a defect in the script grammar
rather than the cause of this, and it is recorded so the next script is honest.

### IRQ 40 is pending AND enabled AND unserviced — with two causes ruled out

Register state at the end of a run whose wheel script fired all four steps, read with `--dump`:

| register | | value | |
|---|---|---|---|
| `0x60004100` | `CPU_HI_INT_STAT` | `0x00000100` | **bit 8 set — IRQ 40 asserted** |
| `0x60004110` | `HI_INT_STAT` | `0x00000100` | bit 8 set |
| `0x60004120` | `CPU_HI_INT_EN_STAT` | `0x80800195` | **bit 8 set — IRQ 40 ENABLED by RetailOS** |

IRQ 40 is `OPTO_IRQ_HI = 8` (40 − 32), and `int_pending_hi` is the register the wheel asserts into,
so the model and the controller agree. Meanwhile the same run reports `irqs: 41308406 asserted,
62124 taken` — the core takes interrupts perfectly well — and `3 word reads of DATA`, all three of
them boot-time queries. **The wheel interrupt is pending, enabled, and never serviced.**

**Ruled out, each with the control that proves the arm was real:**

- **The second core.** `int_pending` is shared between cores and the per-core *enable* registers
  decide who takes each line, so an ISR living on the COP while bypass #7 holds it asleep would look
  exactly like this. Re-run with `--cop-awake`: the report confirms the ablation landed —
  `bypasses live: 3`, `ledger #7: COP_STATUS override NOT installed` — and the framebuffer is
  **byte-identical**, with the same `3 dropped unread`. Not it.
- **A press too short to sample.** `down=select` held two simulated seconds instead of
  `press=select`'s zero-length pair: four distinct frame timestamps instead of two on one
  instruction, and the same three dropped frames. Not it.

**What is left is the handler itself** — whether RetailOS installs a vector for HI bit 8 at all, and
what its ISR reads. That is a static question about Apple's code, and `tools/ghidra` is the tool for
it. The runtime instruments have now said everything they can: the line is raised, the controller
would deliver it, and the firmware does not come.

### The exact instruction it stops at, and it is one gate short of Wall B's fix

Apple's installed ISR is reached and its hi-bank arm is reached. Armed on the three instructions
that matter, real retail NOR, 20.1.3 drive, wheel script firing all four steps:

| address | | arrivals |
|---|---|---|
| `0x00277128` | the CPU dispatcher body | **62 124** — every interrupt taken |
| `0x002771dc` | `tst r5, #0x100` — *is IRQ 40 pending?* | **539** |
| `0x002771e4` | `bl 0x281350` — the wheel decoder | **NEVER REACHED** |
| `0x00281350` | the decoder itself | **NEVER REACHED** |

So Wall B's fix works as far as it goes: bit 30 of the low bank is raised, the `tst r4, #0x40000000`
gate passes, and the hi-bank block is entered 539 times. **What never happens is `r5` bit 8 being
set on any pass the ISR makes.**

**And it is set at the end of the run.** Dumped after the same run:

```
0x60004000  CPU_INT_STAT        0x40000001   bit 30 — hi aggregate raised
0x60004020  CPU_INT_EN_STAT     0xcc802017
0x60004100  CPU_HI_INT_STAT     0x00000100   bit 8 — IRQ 40 pending
0x60004120  CPU_HI_INT_EN_STAT  0x80800195   bit 8 — enabled
```

A frame is waiting, the line is asserted, the mask allows it — and the 539 ISR passes all read zero
there. The 539 are the drive: `ide irq raised 946 times` is `IDE_DMA_IRQ_HI = 23`, in the same bank.

**The next question is therefore about time, not wiring.** The wheel's frames post at ~369 M
instructions, the run ends at ~395 M, and in between `cpu sleep: 514697 halts, 520976 ms of
simulated time spent halted`. Either the ISR does not run again in that window, or a halted core is
not being woken by an assertion on the hi bank. `Machine::service_interrupts_inner` wakes the **COP**
explicitly (`if self.mem.cop_asleep { … self.cop.irq() }`) and there is no matching wake for the CPU
— worth checking before anything else, and it is a question about our model rather than Apple's.

### After the wake fix: the decoder runs, and the second core is not involved

With `a pending, enabled interrupt wakes a halted core` in place, on the real retail NOR and a
20.1.3 drive:

| | before | after |
|---|---|---|
| `0x00281350` — Apple's wheel decoder | **NEVER REACHED** | **x2** |
| `0x002771e4` — the `bl` to it | **NEVER REACHED** | **x2** |
| word reads of `CLICKWHEEL_DATA` | 3 | **5** |
| frames dropped unread | 3 | **1** |
| instructions retired on a 3 G budget | 395 M | **2.69 G** |

The last row is the fix in one number: the core used to sleep through 85 % of its own budget.

**The second core is not in this path, and costs nothing today.** `--no-second-core` against the
two-core arm gives an identical decoder count, identical reads, and an identical framebuffer —
2 689 807 074 instructions against 2 689 784 301, a difference of 23 k in 2.7 G. That is expected
rather than surprising: **bypass #7 forces the COP asleep in both**, so the choice is between a
parked model and no model. The accuracy is free until something wakes it (`--cop-awake`), and that
arm is separately known not to change this result.

**Also ruled out, with a control**: the capture happening 445 simulated seconds after the input.
Moving the script to `@560s`, four seconds before the 600 s dump, gives the same `x2`, the same five
reads and the same framebuffer. Neither an idle timeout nor a revert is hiding a change.

**Where the chain now stands**, each link measured rather than assumed:

```
wheel posts frame -> IRQ 40 asserted -> hi-bank aggregate bit 30 raised
  -> ISR 0x00277128 entered -> tst r4,#0x40000000 passes
  -> tst r5,#0x100 passes -> bl 0x00281350 TAKEN -> decoder runs
  -> ??? -> widget -> damage -> present -> panel
```

The next link is what the decoder does with the frame. `NEXT.md`'s Wall-A note says it returns
semaphore `0x7f` and that `SerialOptoTask` has been pended on that semaphore since tick 66, so the
question is whether that task is now scheduled. Note the decoder runs **2** times for **5** reads,
so the relationship between a read and a decode is not one-to-one either.

### The decoder scales with input, and what it gates on

`rotate=+10` at 20 ms a detent instead of a four-step press:

| | 4 steps | 12 steps |
|---|---|---|
| frames posted | 7 | 15 |
| word reads of `DATA` | 5 | 12 |
| decoder `0x00281350` | **x2** | **x9** |

The reads reconcile exactly — **3 boot queries plus one per decode**, in both arms — so decoding is
proportional to input rather than a fixed artefact. The wheel path is alive from the peripheral to
Apple's own decoder and scales.

**What the decoder gates on**, read out of its first six instructions:

```
0281350  ldr r1, [pc,#0xa8]     ; the wheel base, 0x7000c000
0281358  ldr r0, [r1, #0x104]   ; STATUS
028135c  tst r0, #0x04000000    ; RX_READY — bit 26
0281360  beq 0x2813e0           ; nothing waiting: acknowledge and leave
0281364  ldr r0, [r1, #0x140]   ; CLICKWHEEL_DATA
0281374  cmp ip, #0x8000001a    ; the frame tag
```

Bit 26 is `ClickWheel::RX_READY`, write-1-to-clear, and `lib.rs` already names the acknowledging
instruction at `0x002813e4`. The tag it compares against is the one the model posts. Nothing here
is unmodelled, which is why the frames get through.

**So the open question is now strictly downstream of the decode**: the frame is read, tag-matched
and acknowledged, and the panel does not change. That is the decoder-to-widget hop —
`SerialOptoTask` and the `0x7f` semaphore in `NEXT.md`'s Wall-A note — and it is the only link in
the chain that has never been measured.

### The whole input chain now runs, the draw pipeline woke up, and the panel is still identical

**§0's table, re-measured after the interrupt-wake fix.** The earlier reading of it was taken on a
machine that slept through 85 % of its budget, so it measured a stalled machine rather than a
stalled pipeline:

| | stage | before (both arms) | control | with input |
|---|---|---|---|---|
| 1 | Paint | 85 | 221 | 101 |
| 2 | Show | 2 276 · 56 | 2 276 · 56 | 2 276 · 56 |
| 3 | Damage → flush | 4 | **21** | **6** |
| 4 | Present | 4 | **21** | **6** |
| 5 | Transport | 17 | **85** | **25** |

**Presents went 4 -> 21.** The pipeline was never severed; the core was asleep.

**And the chain above it runs end to end.** `--watch-range=0x1081d998:0x30` over the wheel's state
block — the address is the third literal in the decoder's own pool at `0x00281408` — shows it
written by the decoder *and* by everything downstream:

```
0x1081d9b8  <- 0x0028138c  x44   the decoder's  str ip, [r3,#0x20]
0x1081d9bc  <- 0x002813ac  x44   the decoder's  str r0, [r3,#0x24]
0x1081d9a0  <- 0x0028563c  x44   SerialOptoTask's region, matching count
0x1081d9ac  <- 0x00118224  x4    the InputEvents region
```

Eleven words written by the decoder, eleven consumed by the task. So: interrupt taken, decoder run,
state written, task scheduled, input layer reached, and the display pipeline live at 21 presents.

**The panel is still byte-identical.** Post-fix control against post-fix input, `cmp -l` reports
**0 differing bytes** — and that comparison is against a fresh baseline, not the pre-fix capture
every earlier md5 in this file was taken against.

**One new signal, unexplained**: input *reduces* the draw counts — 21 presents without it, 6 with.
Input is not being ignored; it is making RetailOS do something that draws less. That is the next
thread, and it is a different question from the one this file has been asking.


### The input chain is complete, all of it — and the panel has nowhere to land (2026-09-01)

The thread above ends on *"input reduces the draw counts — 21 presents without it, 6 with"*. This
carries the chain past the decoder to its end, and the answer is that **the question was never about
input**.

**Every remaining hop, measured.** Real retail NOR, a drive built from `iPod_20.1.3` in the same
run, `--clock=5 --wheel-click-instr=300000`, input at `@250s` — which lands at ~@531.9 M because the
machine idles, and that is why the anchor is in seconds:

| hop | address | count |
|---|---|---|
| frames posted by the model | — | 17 (4 dropped unread) |
| IRQ 40 asserted | — | 13 |
| word reads of `DATA` | — | 12 |
| **acknowledged** (`STATUS` bit 26 written) | `0x002813e4` | **12 — every read** |
| decoder | `0x00281350` | 9 |
| poster | `0x000cd6a0` | 4 |
| **queue send** | `0x00151a40` | 4 |
| **queue receive** | `0x00130960` | 4 |
| dispatcher | `0x00151890` | 4 |
| handler-table body | `0x001518a8` | 4 |
| **handler, claims the event** | `0x00170de0` | 4 |
| dispatcher's post-claim call | `0x001517d4` | 4 |
| *fell off the table, unhandled* | `0x001518fc` | **0** |

Send and receive pair up one-for-one, ~2 000 instructions apart every time:

```
send @531920310 -> receive @531922429      send @536299982 -> receive @536301796
send @532008899 -> receive @532010713      send @536373120 -> receive @536375239
```

**Two things NEXT.md says about this do not reproduce and are retired.** `0x00151a40` is described
there as "the queue consumer"; its five instructions load the queue handle from `[0x1081e0e0+0x18]`,
set `r2 = 0x1c` and tail-branch into a *send* primitive — it is a producer. And "the widget at
`0x001ae214` receives none of them" — it receives x6, though at @281 M and @369 M, which is the menu
being drawn rather than anything to do with input.

**The handler is five instructions and it claims every event:**

```asm
00170de0  ldr r1, [pc, #0xc]   ; 0x1081e134
00170de4  ldr r1, [r1, #0x8]   ; the field the dispatcher gates on
00170de8  str r1, [r0, #0x0]   ; *out
00170dec  mov r0, #1           ; claimed
00170df0  bx  lr
```

`0x001518fc` — the dispatcher's "nobody wanted it" exit — is **never reached**, and `0x001518e4`,
which runs only when that field is non-zero, is reached **x4**. So the event is delivered, claimed,
and acted on. There is no unbound delegate and no empty handler table.

### What is actually missing: we accept the display server and never run it

```
                       control (no input)   with input
bcm commands kicked            4                4
bcm frame updates              2                2
bcm gencmd answered           57               41
```

**Two frame updates, in both arms.** The white screen and the language picker. After that the panel
is never updated again, and no amount of input changes that — which is exactly what §7 predicts:
`0xE0000` *"is a transfer buffer, not the panel — the panel is the co-processor's own frame store,
and the model publishes the store back over the buffer"*.

`hw/video.rs` says what the model is, in its own first paragraph: *"This models the protocol and its
internal address space, **not the video hardware**: enough for Apple's bootloader to upload the
`vmcs` firmware and get the acknowledgement it waits for."* RetailOS uploads `vmcs.bin` — 101 728
bytes of VideoCore code, [research/11](11-the-videocore-runtime.md)'s 183-symbol runtime — through
that window, we store it, we acknowledge it, and **we never execute it**. DispmanX lives in that
image. Every redraw after the first is an IPC transaction with a service that does not exist here.

**This is why Rockbox and the bootloaders are not affected and RetailOS is.** Rockbox drives the
panel directly through the same bus window (`lcd-video.c`), and Apple's bootloader has its own small
command interface; neither needs the runtime. RetailOS is the only thing in this machine that boots
the co-processor and then talks to it. The counters say so: `4 commands kicked` is the bootloader's
interface, and 41–57 `gencmd` answers against the 165 requests §0 tabulates is a stub replying to
what it recognises.

**So #40 is misnamed.** "RetailOS reads wheel input but never issues a redraw" describes the
symptom and points at the wrong half. It reads the input, decodes it, queues it, dispatches it and
hands it to a handler that claims it. What it cannot do is draw the result, because the drawing is
done by a processor we do not run. The next work is on the co-processor, not the wheel — and §0's
opcode table plus research/11 §3's DispmanX model are the specification for it.

### One instrument that lied, and it was the film (2026-09-01)

Every conclusion above nearly died on a sampling rate. `--bcm-film`'s `EVERY` was 25 000 000
instructions while the whole input window — first frame posted to last acknowledged — is **4.5 M**.
The window fell cleanly between two samples, so the film could not have shown a change if one had
happened, and it was being read as evidence that none did. Re-run at 1 000 000 the picture is the
same, and only *then* is "the panel does not change" a measurement rather than a coincidence.

**Match the cadence to the event, not to the run.** A film sampled coarsely enough to be cheap over
a 2.6 G budget is sampled far too coarsely to see anything a person does.


### Opcode 3's payload, measured — and the flip implemented (2026-09-01)

§5's table marks its DispmanX column *"proposed, not derived — only opcode 8's argument and reply
shape force it"*. The payloads are now read rather than proposed. `--bcm` keeps a bounded sample of
each opcode's bytes and `trace` prints them; four consecutive opcode-3 payloads from a retail boot:

```
opcode 0x03, 32 bytes, four consecutive calls
  +0x00  00000000   display id — DISPMANX_ID_MAIN_LCD is 0
  +0x04  ffffffce   -50, a layer (int32)
  +0x08  00000000
  +0x0c  00000001 / 00000002 / 00000001 / 00000002   <- ALTERNATES
  +0x10  00000000
  +0x14  00000000
  +0x18  00f00140   320 | 240<<16, the rect as two u16
  +0x1c  13e6acc4   an ARM-side pointer
```

**`+0x0c` alternates 1, 2, 1, 2 while every other word holds still.** Those are the handles the two
`resource_create`s were answered with, so each call names which surface is now the front one — which
is §4's `FUN_00286b6c(back + 0x20, …)` followed by `ctx->back <-> ctx->front`, seen from the bus.

Opcode 8's descriptor is confirmed by the same method, against the layout that branch had already
documented from the ARM side: `+0x08 = 0x140` (320), `+0x0c = 0xf0` (240), `+0x10 = 0x280`
(640 = 320 × 2, RGB565).

**The model now composites on opcode 3**: look `+0x0c` up in the surface table, copy that surface's
RGB565 halfwords out of the internal address space at its own pitch into the frame store, publish.
A/B, one variable, everything else pinned:

| | frame updates | panel |
|---|---|---|
| composite off | 2 | the language picker |
| composite on | **12** | **byte-identical** |

**Identical is the right answer and it is the check that matters.** A wrong address, a wrong handle
offset or a wrong pitch would have blacked or sheared the panel; ten extra publishes that leave it
unchanged means the pixels being read are the pixels that were there. What it does *not* do is make
anything new appear — RetailOS renders the same screen ten times.

**So this is a path made correct, not a redraw made to happen.** Before it, every RetailOS present
uploaded scanlines into a surface nothing ever read back, and the only thing on the panel was what
the *bootloader's* command interface had placed — which is why "0 frame updates" could sit next to
75 267 lit pixels without contradiction. RetailOS's own compositing now reaches the glass. Why it
draws the same picture after input it drew before is the next question, and it is above stage 4.


### The leading Select is what stops RetailOS reading the wheel (2026-09-01)

Four variables tested against the language picker, one at a time, everything else pinned — real
retail ROM, a drive built from `iPod_20.1.3` in the same run, `--clock=5`, 2.6 G budget. Three of
them change nothing and the fourth changes everything.

| | frames posted | dropped unread | **word reads of `DATA`** |
|---|---|---|---|
| `touch, rotate, rotate, release` | 17 | 4 | **12** (and 12 acknowledged) |
| `touch, SELECT, rotate, rotate, SELECT` | 31 | 25 | **5** |

**Pressing Select on the language picker is what stops the firmware servicing the wheel.** Before it,
RetailOS reads and acknowledges every frame it is given. After it, it reads five — three of which are
the boot-time queries — and drops twenty-five.

**What is NOT the variable**, each with its own run:

- **Click spacing.** `--wheel-click-instr` at 20 000, 100 000 and 300 000 — 4 ms, 20 ms and 60 ms per
  detent at this clock — give *identical* counts: 31 posted, 25 dropped, 5 read, the same five
  pictures, the same digest. An earlier note here reasoned that the default floods the firmware; on
  the select-first script it makes no difference at all.
- **When the input arrives.** `@80s` (seconds after the picker draws at 73.2 s) and `@150s` are
  identical to each other in every number. RetailOS is not "not ready yet".
- **The second core.** `--cop-awake` against the default, same script: identical frames posted,
  dropped, read, ATA commands, `bcm` frame updates, distinct pictures **and final digest**
  (`0xc25f6a64ddfcf335`). This is worth stating plainly because it is measured *after* the
  interrupt-wake fix and *after* the co-processor began compositing, which the earlier reading of it
  could not claim: **the COP is irrelevant to RetailOS on this path.** It is not irrelevant to
  Rockbox — see research/06, where waking it takes Doom from 3 982 ATA commands to zero.

**Neither arm changes the panel.** Five distinct pictures in both, same digest. So this is not yet
"the wheel works if you do not press Select" — it is that the *reading* stops there, and the reading
was the half that looked healthy.

**The ISR, measured across the select-first script**, which narrows it further:

| address | | arrivals |
|---|---|---|
| `0x00277128` | the CPU dispatcher body | **137 561** |
| `0x002771dc` | `tst r5, #0x100` — is IRQ 40 pending? | **545** |
| `0x002771e4` | `bl 0x281350` — the branch to the decoder | **2** |
| `0x00281350` | the decoder | **2** |

and from the model's own side of the same run:

```
27 frames posted (21 dropped unread), 5 word reads of DATA
irq 40 asserted 6 times; CTRL 0x600a1f00 (receiver armed), STATUS 0x05000000
5 acknowledged
```

**It is a self-limiting loop, and the numbers say which link is short.** `RX_READY` is
write-1-to-clear, so a post while a frame is unread does not raise a new edge — 27 posts produce
only **6** assertions. The firmware acknowledges **5** times, which is the only thing that lowers
the line, so there are only five chances for a sixth edge. Every frame after that is dropped
against a line nobody lowered.

The 545 against 137 561 is not the anomaly it looks like: the dispatcher runs for every source and
the hi-bank aggregate gate (`tst r4, #0x40000000`) is what admits 545 of them, most being the drive
— research/06 records `IDE_DMA_IRQ_HI = 23` in the same bank. **The anomaly is 2 of 545**: on all
but two of the passes that DO reach the hi-bank test, the wheel's bit 8 reads clear.

So the question is no longer "does the interrupt reach the firmware" — it reaches it 545 times and
the bit is not set. Either the assertion is not surviving to the moment the ISR samples it, or
something clears it between. `STATUS` still reads `0x05000000` at the end of the run — bit 26 set,
a frame waiting, unacknowledged — so the model believes the line is up when the run stops.

**The model is not stealing the interrupt, and that is now measured rather than assumed.**

The obvious suspicion, given six assertions and two sightings, was that the line was being withdrawn
under a waiting frame: the level is `irq_enabled && RX_READY && ARM`, so a receiver disarmed between
a post and the ISR would lower it — and nothing would raise it again, because a later post finds
`RX_READY` already set and produces no edge. `ClickWheel::line_dropped_waiting` counts exactly that
case, and on the select-first run it is **zero**.

With that ruled out the numbers reconcile completely:

```
27 frames posted, 21 dropped unread, 5 word reads, 5 acknowledged, 6 assertions
```

Six assertions is five completed cycles — assert, read, acknowledge, line clears, next post asserts
— plus one still pending when the run ends. The twenty-one dropped are frames posted while an
earlier one was unread, which is what the hardware does too: `RX_READY` is write-1-to-clear and the
receiver holds one packet.

**So the wheel model is behaving correctly and RetailOS stops acknowledging after five.** That is a
different sentence from the one this file has been able to write until now, and it moves the whole
question out of the peripheral and into the firmware's own scheduling.

**Where this points.** Something the Select does takes the firmware off the path that services
IRQ 40. The obvious candidates are a task switch — the picker's own task exiting and its successor
not arming the receiver — or a mode change in the driver. `CTRL` still reads `receiver armed` at the
end of the run, so it is not a disarm this model can see. The next measurement is the ISR itself
across the Select: `--enterlog` on `0x00277128` and `0x00281350` in both arms, counting arrivals
before and after the button rather than over the whole run.
