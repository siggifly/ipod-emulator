# The resource image nobody reads

**RetailOS's boot loop is a missing font.** It asks its font registry for *Podium Sans* at 18 pt,
the registry does not have it, the lookup returns a null delegate, that null is bound into 400
objects, and the first call through one of them lands on `bx 0` — RetailOS's own reset vector.

`PodiumSans18.ttf` is real, it is 126 524 bytes, and it is on the iPod's disk. It lives in **`rsrc`**,
a 5 MB image in the firmware partition, listed in the same directory the ROM used to load `osos`.
**Nothing has ever read it.**

The same image also holds `vmcs.bin` and the VideoCore codec libraries — so the display bypass
([#6](04-bypass-ledger.md)) and the boot loop turn out to be the same missing 5 MB.

---

## The chain, measured link by link

Each step below is a measurement, not an inference. The two instruments that made it possible —
`--storelog`/`--storeaddr` (stores keyed by *instruction* and by *target*) and `--enterlog`
(arguments on arrival) — were built for this and are described at the end.

### 1. The failing object is not special

[research/03](03-rtxc-and-the-video-coprocessor.md) §52 framed the blocker as an object "barely
constructed at all — one field out of the whole thing", and asked which construction path had gone
wrong. `--storelog=0x00211a98`, on the one instruction that writes `+0x1c`, enumerates **every
object that constructor ever built**: 791 of them, and the failing one at `0x13e26f6c` is
constructed at instruction 414 132 235 by the same caller, with the same arguments, as the other 790.

```
0x00211a70 lr=0x00211bf8  r0=0x13e26f6c r1=0x10883634 r2=0 r3=0   @414132235   <- the failing one
0x00211a70 lr=0x00211bf8  r0=0x13dd4490 r1=0x10883634 r2=0 r3=0   @438992725   <- and 790 like it
```

Three dynamic callers, all inside the same function family; `r1` is the same singleton 394 times
over. **Nothing distinguishes the object that kills the boot.** The construction-order framing was
wrong.

### 2. `+0x20` *is* written — §52's "never written" was a `--watch` artefact

`--storeaddr` over the `+0x20` word of all 790 heap objects finds **95 456 stores**, and every
single object is written. The failing one is written three times:

```
@414132196  0x0017f188 = 0x00000000     allocation zeroing
@414132221  0x00210684 = 0x00000000     the initialiser
@414133781  0x00211778 = 0x00000000     the delegate setter
```

§52 reported `+0x20` as never written because it used `--watch`, which records value **changes**.
Three writes of zero to a word already zero change nothing, so the instrument was silent and the
silence was read as absence.

> **This is the third time this exact limitation has produced a wrong conclusion in this project**
> — [research/06](06-rockbox-as-oracle.md) recorded the first, the stale-heap sibling in
> [research/09](09-what-the-hardware-must-supply.md) the second. `--watch-range` was built after the
> first one and would have caught this. The lesson has been written down twice and applied to the
> next question both times, not to the standing ones. **A new instrument's first job is to re-run
> the conclusions the old instrument produced.**

### 3. The setter handles null deliberately

`0x00211774` is a four-line leaf:

```
00211774  ldmia r1, {r2, r3}       ; load a 2-word delegate from *r1
00211778  str   r2, [r0, #0x20]    ; object half
0021177c  str   r3, [r0, #0x24]    ; function half
00211780  bics  r2, r2, #0x1       ; ...and if the object half is null,
00211784  ldrneb r2, [r2, #0x4c]   ;    skip the load
0021178c  moveq r2, #0x0           ;    and record flags 0
00211798  strb  r2, [r0, #0x28]
```

A null delegate is an **expected, handled** state here. The setter is not the bug; it is faithfully
copying a null that arrived pre-formed. So the origin is upstream, and the source is `*r1`.

### 4. The source is a stack temporary, and it was good 900 instructions earlier

`--enterlog=0x00211774` gives the source pointer per call. For the failing object it is
`0x13edb7ac` — an address written 132 425 times by dozens of instructions, opening with the fill
pattern `0x5e285e28`. That is a **stack** slot, not a field of a parent object.

`--storeaddr` on that word, ordered against the bind at 414 133 781:

```
@414132882  0x0021a168 = 0x13e26f6c / 0x13e26fb1    <- a VALID delegate is formed
@414133775  0x00221f78 = 0x00000000 / 0x00000000    <- and then zeroed
@414133781                                          <- bound
```

The delegate the caller built was fine. `0x00221f78` overwrote it — and that instruction is the
tail of a two-line wrapper that returns a delegate by out-pointer from `0x00221e24`.

### 5. `0x00221e24` is a registry lookup, and it missed

```
00221e28  ldr r5, [pc, #0x84]      ; the table at 0x108858e4
00221e40  mov r7, #0               ; result object = null
00221e44  mov r6, #0               ; result function = null
00221e50  bl  0x0025454c           ; entry -> descriptor r4
00221e5c  cmp r0, r10              ; [r4+0x48] == arg r2
00221e64  cmpeq r0, r8             ; [r4+0x4c] == arg r3 &~4
00221e74  bl  0x00284170           ; and the name at r4+6 == arg r1
00221e7c  ldreq r6, [r5, #0x8]     ; MATCH -> the function half
00221e80  moveq r7, r4             ; MATCH -> the object half
00221e88  add r5, r5, #0x10        ; else next entry
00221ea8  stmia r11, {r0, r6}      ; return {object, function} — {0,0} on a miss
```

A three-key lookup — a name, a word and a byte — over a `0x10`-stride table, **returning a null
delegate when nothing matches**. Falling off the end is a designed outcome, not a crash.

### 6. The key is a font

`--enterlog=0x00221e24` reads the arguments. 390 of 400 lookups ask for the same name pointer,
`0x004b1360`, with `r2 = 0x12` and `r3 = 1`. That string is **`"Podium Sans"`**, in a table of font
names — Chicago, Espy Sans, Osaka, Helvetica, Lucida Grande, Myriad Set Medium.

The descriptors registered in the table, read out of the image (`r4 = ptr-4`, which the field values
themselves corroborate — every `+0x48` lands on a plausible point size):

| descriptor | name | `+0x48` (size) | `+0x4c` (style) |
|---|---|---|---|
| `0x00603fc4` | Osaka | 12 | 0 |
| `0x00501370` | AppleGothic | 12 | 0 |
| `0x0058efa8` | Chicago | 12 | 0 |
| `0x00591f78` | Espy Sans | 10 | 0 |
| `0x00594344` | Espy Sans | 9 | 1 |
| `0x005955d8` | Espy Sans | 10 | 1 |
| `0x005f3360` | **Podium Sans** | **14** | 1 |
| `0x005cc554` | **Podium Sans** | **16** | 1 |
| `0x005e1770` | **Podium Sans** | **22** | 1 |
| `0x005e8748` | **Podium Sans** | **28** | 1 |

**Podium Sans is registered at 14, 16, 22 and 28. The lookup asks for 18.** The name matches; the
size does not; the lookup falls off the end of the table and returns null.

That is corroborated by the key distribution across all 400 lookups: size 18 is asked for **348**
times and never found, while 14, 16, 22 and 28 — the sizes that *are* registered — account for the
rest and succeed. 68 objects end up with a bound delegate; the remainder are null.

### 7. Size 18 comes from a file, and the file is in `rsrc`

The image contains **no TrueType data at all** — zero `glyf`, `cmap` or `loca` tables in 7.5 MB. What
it contains is eight *filenames*: `CJK.ttf`, `MonoHope_15_{Plain,Italic}.ttf`,
`MonoHope_20_{Plain,Italic}.ttf`, `Helvetica_{17,23}.ttf`, and **`PodiumSans18.ttf`**.

The firmware partition's directory lists three images, not one:

| tag | devOffset | length |
|---|---|---|
| `osos` | `0x00004400` | `0x00735a00` — 7.5 MB, the OS. **Loaded by the ROM.** |
| **`rsrc`** | `0x0073a000` | `0x00500000` — **5 MB. Never read by anything.** |
| `aupd` | `0x00c3a200` | `0x00106400` — the flash updater ([#12](04-bypass-ledger.md)) |

`rsrc` opens `eb 3c 90 "MTOOL399"` — it is a **FAT volume**, and it holds exactly the eight fonts the
image names:

```
/Resources/Fonts/       CJK.TTF  3 446 748      HELVET~1.TTF  56 816    HELVET~2.TTF  81 440
                        MONOHO~1..4.TTF         PODIUM~1.TTF 126 524
/Resources/VideoCore/Boot/       RENDER~1.BIN 104 540        VMCS.BIN  201 376
/Resources/VideoCore/Library/    AACDEC.VLL  H264DEC.VLL  MPG4DEC.VLL
                                 MPLAYER.VLL  PASSTH~1.VLL  SLIDES~1.VLL
```

The Podium Sans file carries a real TrueType `name` table — `Podium Sans / Regular /
FONTLAB30:TTEXPORT`. Eight `.ttf` names in the image, eight fonts in the volume.

### 8. RetailOS stops one read short

The disk log for a full boot: 59 DMA transfers load `osos` (the ROM), then **one** more —

```
lba 0  -> 0x17edbea0  2048 bytes
```

RetailOS reads the **MBR**, and never issues another transfer. Stopping at the first font lookup
(`--stop-at=0x00221e24:1`, which fires at 411 117 545 instructions) shows all 60 transfers already
done: **the disk stack had gone quiet 360 M instructions before the font system asked.**

So it is not that the font load failed. It is that the sequence that mounts `rsrc` — read the MBR,
find the firmware partition, read its directory, load `rsrc`, mount the FAT, open
`/Resources/Fonts/PodiumSans18.ttf` — **stops after the first step**.

---

## What this settles, and what it opens

**Settled.** The blocker is not a construction-order bug, not an unbound delegate, not a device
model, and not USB. Every "RetailOS never does X" from §41–§53 is downstream of one fact: a 5 MB
resource volume that the firmware expects to have mounted is not mounted. The null delegate is the
font registry's designed answer to "you asked for a font I do not have."

**It also gives [#6](04-bypass-ledger.md) a concrete retirement path.** The synthesised BCM replies
stand in for a VideoCore we cannot execute because we do not have its firmware. `vmcs.bin` is
201 376 bytes in `/Resources/VideoCore/Boot/`, beside `render.bin` and six codec libraries. The
display bypass and the boot loop were never two problems.

**Open — and this is now the whole question:** *why does RetailOS read the MBR and stop?* The
candidates are ordinary and each is testable:

1. It parses the MBR and rejects it — the firmware partition is type `0x00`, which is normal for an
   iPod but is also "empty" to a naive parser. Our image has it at LBA 63, 27 140 sectors.
2. It reads the MBR into `0x17edbea0` and the result never reaches the code that would act on it —
   the alias arithmetic on that address is worth checking against the MMAP decode.
3. It is waiting on something before continuing, and the wait never completes.
4. The disk model refuses the *next* command in a way the log does not show.

Distinguishing them is one `--enterlog` on whatever consumed the MBR buffer, plus a
`--storeaddr` over `0x17edbea0`. That is where this picks up.

> Worth stating plainly: there are **two** `!ATA` directories on this disk — the original at LBA 96
> and a copy at LBA 285 with `aupd` removed, which is bypass #12's mechanism and is what the ROM
> actually reads. Both list `rsrc` at the same offset, so the finding does not depend on which one
> is authoritative. It was still a genuine surprise to find two, and a directory parse that assumed
> one would have been reporting on a file the machine does not use.

---

## The instruments this needed

Three flags were added, and the shape of all three is the same: **key on the code, not on the
address.** Heap addresses move between runs; the instruction that writes them does not.

| flag | question it answers |
|---|---|
| `--storelog=PC[,PC…]` | every store *this instruction* makes — `(pc, addr, value, icount)`. One known writer inside a constructor enumerates every object it ever built, in creation order. |
| `--storeaddr=ADDR…\|FILE` | every store that *lands here*, whatever made it. `--watch-range` answers this for one contiguous span; "does anything write `+0x20` of any of these 791 objects" is 791 disjoint words. |
| `--enterlog=PC[,PC…]` | `r0`–`r3` and `lr` on every **arrival**. Deliberately not hooked on `BL`: a plain `B` is a tail call and virtual dispatch arrives by `BX` from a vtable slot — hooking the call instruction misses both. |

`--storelog-dump=FILE` writes the log as TSV so a set of addresses from one run can be fed straight
into the next run's `--storeaddr`, which is how steps 1→2 and 4→5 above were actually chained.

The whole chain — 791 objects to a named font file — took six runs. The previous framing had
absorbed several sessions, and what it lacked was not insight but the ability to ask "who wrote
this" and "what was it holding" without knowing the address in advance.

---

## Addendum: RetailOS does not "stop after the MBR" — it retries the drive and gives up

§8 above described the disk log as one MBR read and then silence. That was read off the DMA
*transfer* log, which only shows transfers that completed. `--storeaddr` over the IDE taskfile
(`0xc30001e0..0x1ff`) and DMA registers (`+0x400..0x410`) shows the **commands**, and the picture is
different:

| when | who | what |
|---|---|---|
| @8.2 M | ROM | IDENTIFY, SET FEATURES ×3, READ SECTORS ×3 |
| @8.6 M | ROM | IDENTIFY, SET FEATURES ×3, **READ DMA ×59** — `osos` loads |
| @50.6 M | **OS** | IDENTIFY, SET FEATURES |
| @101.8 M · @152.9 M · @204.2 M · @255.4 M | OS | one SET FEATURES each — **51.2 M apart, 10.2 simulated seconds** |
| @306.6 M | OS | READ DMA |
| @357.8 M | OS | SET FEATURES ×5 (transfer mode, write cache, look-ahead, APM) then READ DMA |
| @409.0 M | OS | **SET FEATURES ×5 — and no read follows** |
| @411.1 M | | the first font lookup |

So RetailOS **re-runs its whole drive initialisation on a ~10 s cycle**, and on the third pass it
completes the SET FEATURES sequence and never issues the read. Two million instructions later the
font system asks for Podium Sans 18 and gets the null it was always going to get.

That is a driver **timing out and retrying** — ordinary, correct behaviour for a driver whose disk
is not answering the way it expects. It also means the sequence that would have mounted `rsrc` is
not missing; it is *being attempted and failing*.

Also visible, and unexplained: a write of `0x0003000a` to `IDE_BASE+0x400` and `0` to `+0x410` every
2 991 488 instructions (0.6 s), from `0x00001318` — code in the low vector region — continuing to
the end of the run. `+0x410` is one byte past the modelled DMA window, so it lands in backing memory
and is never read.

### A predicted cause, fixed, with no effect — recorded because the prediction was wrong

The interrupt accounting reads `raised 148, delivered 19, acked by status read 70`. The delivery
code cleared the drive's pending bit **outside** the test for whether the drive's interrupt was the
one being delivered, so any interrupt taken — a timer tick would do — dropped a pending IDE edge.
That is a real defect and an obvious candidate for 129 lost interrupts.

Moving the clear inside the test changes **nothing**: 148 raised, 19 delivered, 88 commands, 60 DMA
transfers, byte-identical. The undelivered interrupts are undelivered because the drive's interrupt
is **masked** — `enabled=0` — and RetailOS polls the status register instead, which is exactly what
the 70 status-read acknowledgements say. The fix is kept, because clearing a bit that was never
delivered is wrong on its own terms, but it is not a fix for anything observed.

Worth keeping for the same reason the false GPIO breakthrough is kept: the hypothesis was good, the
mechanism was real, and the measurement said no.

### Where this picks up

The question is now sharp and small: **what does RetailOS's ATA driver wait for after SET FEATURES
that our drive never provides?** It polls rather than takes interrupts, so the answer is in a status
or configuration register it reads and does not like. `--readlog` over the taskfile — the instrument
that did not exist this morning — answers it directly, and bypasses **#9** (`IDE0_CFG` bit 3 as
"interrupt pending", never confirmed) and **#10** (edge-triggered IRQ, known-wrong) are both sitting
in that path.

---

## Addendum 2: the drive is healthy, and the second read was armed before it was commanded

The addendum above called the ~10 s cycle "a driver timing out and retrying". **That is wrong**, and
the correction matters because it moves the fault out of the ATA model.

`--readlog` now records the value a device *returned* rather than what sat in the backing region,
and with that the driver's own view is:

| register | values seen |
|---|---|
| `+0x1fc` STATUS | `0x50` ×165 (DRDY\|DSC), `0x58` ×25 (…\|DRQ) |
| `+0x028` IDE0_CFG | `0x28` ×509, `0x20` ×1, `0x00` ×4 |

**The drive answers healthily every time.** The last thing the driver does is read `STATUS = 0x50`
at 409 053 999 and `IDE0_CFG = 0x28` at 409 054 264, and then it stops — no error, no retry storm,
no poll loop. All disk traffic after that is a 0.6 s heartbeat from low-vector code reading
`+0x400`/`+0x410`. A driver that had timed out would look nothing like this.

> The instrument lied first, and the lie was pretty. Before the fix, every STATUS read logged as
> `0x00` — 190 of them, and IDE0_CFG `0x00` 514 times. "Every register always reads zero" is a
> beautiful bug story and it was **an artefact of where the hook sat**: `count()` runs ahead of every
> device model, so peeking the target reports the backing region, not the drive. Two instrument bugs
> in one day, both caught by the same reflex — *this result is too tidy, test the instrument.* The
> first (`read32`'s fast path skipping accounting) hid 99.97% of reads.

### The real anomaly: one of the two OS reads never moved

RetailOS issues exactly **two** `READ DMA`s in the whole run, and only one delivers:

| command | DMA length | DMA target | delivered? |
|---|---|---|---|
| @306 613 404 | `0x7ffc` (32 KB) | **`0x93eea730`** | **no** |
| @357 837 591 | `0x07fc` (2 KB) | `0x17edbea0` | yes — this is the MBR |

Nothing is reported dropped (`every staged byte landed`), so the 32 KB transfer was never *staged*,
not staged-and-lost.

The programming order is the same for both, and it is the order bus-master DMA requires — **arm the
engine, then command the drive**:

```
@306613259  +0x408 = 0x00007ffc      length
@306613261  +0x40c = 0x93eea730      target
@306613277  +0x400 = 0x0003000b      GO
@306613404  +0x1fc = 0xc8            READ DMA        <- 127 instructions LATER
```

Our engine commits on the GO bit. At GO time the command has not run and `dma_staged` is empty, so
there is nothing to commit — and the model has no path that revisits an armed engine once the data
arrives. **The second read landing is the thing that needs explaining, not the first one failing**,
and that is the next measurement.

`0x93eea730` is worth its own look: bit 31 set, and `0x13eea730` with bit 31 clear lands squarely in
the region RetailOS uses for stack and heap (`0x13edb7ac` is a stack slot from §4). Either the top
bits of that register are flags we are folding into the address, or the driver is programming a
translated address. Both are testable by masking and re-running.

### Where item 1 stands

**Not** "what does the driver wait for" — it waits for nothing and reports no error. The question is
now: **why does our DMA engine deliver a transfer armed before its command, and not deliver the
other one?** That is a defect in our model, in a path the ROM never exercises because the ROM arms
and commands in the opposite order.

## Addendum 3: the second read never succeeded either — it inherited the first one's buffer

The anomaly named at the end of Addendum 2 — *why does the second read land under the same ordering
that loses the first* — has an answer, and it is not "the second one works."

`Ata::dma_go()` fired only on the GO write, and returned early when `dma_staged` was empty. Trace the
two reads through that:

| | RetailOS's first read | RetailOS's second read |
|---|---|---|
| arm (`+0x408`/`+0x40c`/`+0x400`) | @306613259–277, target `0x93eea730`, len 32 KB | @357837323–341, target `0x17edbea0`, len 2 KB |
| `dma_staged` at GO time | **empty** — command has not run | **holds the first read's 32 KB** |
| result of GO | returns early, nothing committed | commits the *stale* 32 KB, truncated to 2 KB, to `0x17edbea0` |
| ATA command (`+0x1fc` = `0xc8`) | @306613404, stages 32 KB — never committed | @357837591, stages 2 KB — never committed |

The one transfer the ledger recorded, `lba 0 -> 0x17edbea0 2048 bytes`, is **the first read's data
delivered to the second read's address at the second read's length**. Neither read completed; a
stale buffer crossed into a later arm and was recorded as a success.

It read as correct for months for one reason: **both of RetailOS's DMA reads are LBA 0**
(`[76] cmd 0xc8 nsector 0x40 lba 0` and `[82] cmd 0xc8 nsector 0x04 lba 0`). The wrong buffer held
the right bytes by coincidence. Had the two reads targeted different sectors, this would have
surfaced as corruption years earlier and been found in an afternoon.

### The fix, and what it did not fix

The engine now models the two events as independent. `dma_armed` records GO; `dma_try_start()` runs
on both the GO write and the staging command and does nothing until both have landed. The ROM's
order (command, then GO) and RetailOS's (GO, then command) both work, and a stale buffer can no
longer cross into a later transfer.

The 32 KB transfer now issues. **The boot does not advance by one instruction.** The ATA command log
is byte-identical: 88 commands, same two reads, same re-init after each, same give-up.

What the fix did do is make an existing instrument stop lying. The run used to print `dma: every
staged byte landed`; it now prints

```
DMA DROPPED 32768 bytes at 1 destinations, first 0x93eea730
```

because that counter only ever measured bytes that were *handed over*, and none were. `0x93eea730`
is unmapped — bit 31 set, where `0x13eea730` would be squarely inside RetailOS's stack/heap region.

### The measurement that moved the fault again

The obvious next step was to decide whether bit 31 is a flag or an address. `--readlog` over both
candidates answered a different and better question:

```
0x93eea730   0 reads
0x13eea730   0 reads
0x17edbea0   0 reads      <- the destination that DID receive data
```

with `0xc30001fc` carried as a control and logging its expected 190. **RetailOS never reads back any
DMA destination.** Not the one that failed, and not the one that received correct LBA 0 content in
the pre-fix run. So the masking question is moot for now — a buffer nobody reads cannot be the thing
that blocks the boot, at whatever address.

The same run shows what RetailOS is not doing:

| address | reads | by |
|---|---|---|
| `IDE_BASE+0x400` (DMA CONTROL) | 2 per transfer | `0x00233468` @306613275, `0x002330dc` @357808635 — the read half of the read-modify-write that sets GO |
| `IDE_BASE+0x1fc` (ATA STATUS) | ~20 total from RetailOS PCs | `0x00189cf4` ×11, `0x00232e20` ×7 |
| `IDE_BASE+0x404/0x408/0x40c` | **0** | — |

Two reads of the control register is a read-modify-write, not a poll. Twenty status reads across a
600 M-instruction run is not a poll either. **RetailOS arms the transfer, issues the command, and
blocks.** It is interrupt-driven, and the run ends with

```
ide irq: raised 149 times, DELIVERED to a handler 18 times, acked by status read 70 times;
enabled=0 pending=0
```

`enabled=0`. The fault has moved off the DMA engine and onto interrupt delivery: what does RetailOS
expect to signal DMA completion, and why is that line masked when it waits on it?

### Consequence for the `rsrc` question

`rsrc` was never the blocker, and "mount it" is not a step that exists. The volume is already on the
disk the emulator serves, at LBA 14864, and RetailOS has not reached partition discovery — **both of
its reads are LBA 0**. It is still looking for the MBR. Every conclusion in §§1–8 stands; the font
lookup that fails is downstream of a driver that has not yet read a byte it trusts.

## The full inventory of `rsrc`

Extracted from `resources/drives/ipod8g-retail.img`: MBR partition 0 (type `0x00`, LBA 63),
`!ATA` entry `rsrc` at devOffset `0x0073a000`, length `0x00500000`, so the FAT volume begins at
absolute byte `0x742000` (LBA 14864) after the 0x200 image header. **Verified byte-exact**:
`sum(payload) & 0xffffffff == 0x18319bab`, matching the directory's checksum field, and the three
images tile exactly (`0x73a000 + 0x500200 == 0xc3a200`, `aupd`'s devOffset).

### The volume is FAT12, and its own BPB says otherwise

| field | value |
|---|---|
| OEM name / label | `MTOOL399` / `IPODRESOURC`, serial `0x33d24af8` |
| bytes per sector · sectors per cluster | 512 · 4 → 2048 B clusters |
| reserved · FATs · sectors per FAT | 1 · 2 (byte-identical) · 10 |
| root entries · total sectors | 512 · 10 240 (= 5 242 880 B) |
| FAT1 · FAT2 · root · data | sector 1 · 11 · 21 · 53 |
| cluster count | **2546** |

**The BPB's filesystem-type string reads `"FAT16   "` and it is wrong.** 2546 clusters is below the
4085 boundary, `FAT[0]=0xff0`, and only the FAT12 decode yields valid chains — a chain-sanity check
scores FAT12 21/21 against FAT16 0/1. The FAT *area* was sized for FAT16 (10 sectors holds 5096 B;
FAT12 needs 3822) which is presumably where the string came from. **Any reader that trusts the
string reads garbage** — worth knowing before we implement one.

87.4 % used, every file contiguous, no deleted entries, all 309 free clusters verified zero. All
timestamps 2008-03-10, consistent with `Firmware-20.6.3`.

### 16 files, 5 directories

```
/Resources/
├── Fonts/
│   ├── CJK.ttf                   3 446 748    All iPodFont W6-3, strike 18
│   ├── Helvetica_17.ttf             56 816    internally "Sub-LCD",  strike 17
│   ├── Helvetica_23.ttf             81 440    internally "Sub-TV",   strike 23
│   ├── MonoHope_15_Italic.ttf       22 772    MonoHope-LCD, strike 15
│   ├── MonoHope_15_Plain.ttf        21 612    MonoHope-LCD, strike 15
│   ├── MonoHope_20_Italic.ttf       40 348    MonoHope-TV,  strike 25
│   ├── MonoHope_20_Plain.ttf        37 592    MonoHope-TV,  strike 25
│   └── PodiumSans18.ttf            126 524    Podium Sans,  strike 18   <- §6's missing key
└── VideoCore/
    ├── Boot/
    │   ├── RenderServer.bin        104 540
    │   └── vmcs.bin                201 376
    └── Library/
        ├── aacdec.vll               52 664
        ├── h264dec.vll             106 960
        ├── mpg4dec.vll             147 232
        ├── mplayer.vll              51 620
        ├── passthruhandler.vll       6 528
        └── slideshow.vll            47 940
```

All eight fonts are `sfnt 1.0` with the same 16 tables — `EBDT`/`EBLC`/`EBSC` embedded bitmaps
*plus* real `glyf` outlines. They are **pre-rendered bitmap fonts carrying exactly one strike each**,
outlines retained as fallback. `PodiumSans18.ttf` carries its single strike at **18 ppem** — the size
§6 measured RetailOS asking for 348 times and never finding. Family `Podium Sans`, subfamily
`Regular`, psName `PodiumSans`.

Three cautions for anyone reading these:

- **The filenames lie about the internal names.** `Helvetica_17/23.ttf` are internally `Sub-LCD` and
  `Sub-TV` — subtitle fonts, and the `LCD`/`TV` split is screen versus TV-out, not weight.
- `MonoHope_20_*.ttf` carry **25 ppem** strikes, the only files whose name and strike disagree.
- `CJK.ttf` is a licensed **Hiragino Kaku Gothic Pro W6** derivative, © 1993-2002 Dainippon Screen
  Mfg. Third-party licensed material — same handling as the rest of `resources/`.

### The VideoCore side, and what it says about bypass #6

All six `.vll` are ELF32 LSB `ET_DYN` with `e_machine = 0x5f` (**EM_VIDEOCORE**), built by MetaWare
Universal Linker v5.3h. `.vll` = VideoCore Loadable Library. Their exports are exactly what the
names promise (`H264InitDecoder`, `AACDecode`, `alpha_blt_block`, `aes_decipher`, …).

Their **undefined** symbols are the more useful half, because they name the runtime the co-processor
expects: `TCC_Create_Task`, `TCC_Delete_HISR`, `TCC_Relinquish`, `SMC_Obtain_Semaphore`,
`EVC_Retrieve_Events`, `TCS_Change_Preemption` — that is **Nucleus PLUS**, not RTXC. No contradiction
with [research/03](03-rtxc-and-the-video-coprocessor.md): RTXC is the ARM side, Nucleus PLUS is the
VideoCore side. Two processors, two RTOSes, and we had only ever seen one of them.

`vmcs.bin` is the VideoCore host firmware and **carries its own ELF dynamic loader** — its strings
include `%s is not a valid ELF file.`, `%s has the wrong endian.`, `%s: fixup #%d (@0x%x) has
unrecognized type 0x%x`, `%s: symbol %s is not defined.` That is the mechanism that loads the six
`.vll`s. It also carries the **GENCMD** command interface (`display_control`, `power_control`,
`audio_control`, `disk_notify`, `usb_inserted`, `load_application`, `vmcs_display_enable`, …) and the
`FILESERV`/`DISPSERV` services. `RenderServer.bin` is the GL server — `ShaderMachine: Attempt to set
uniform when no shader is bound`, `gldMallocSlow`, `FrontBufferA`, `MPU stripe`.

For bypass **#6** this matters concretely: the synthesised BCM replies stand in for a co-processor
whose firmware, loader, command interface and codec libraries are **all present on the disk we
already serve**. The retirement is no longer "find the firmware" — it is "run it."

## Addendum 4: the storm that justified bypass #10 was an instrument ceiling

Bypass #10 modelled the drive's interrupt as edge-triggered — raised, then dropped as soon as any
interrupt was taken — because level-triggering had once produced a storm of **9 078 058** assertions
against 15 411 for the timers. That number is the whole reason the bypass existed.

`9 078 058` is **97 % of 600 000 000 ÷ 64**. `service_interrupts` runs every 64 instructions, so
9.375 M is the most assertions a 600 M-instruction run can physically emit. The figure does not mean
*this many interrupts*; it means **saturated** — the sampler was asserting on essentially every tick
it had. A ceiling reading was filed as a measurement, and a bypass was built on it.

### There are two acknowledgements, and the storm was modelling only one

Real ATA asserts INTRQ at completion and holds it until the host acknowledges. We knew about one
ack — a read of the primary status register — because that is the one Apple's bootloader uses. It
polls with interrupts masked, so it never needed any other.

RetailOS uses a second one. Measured:

```
0x000fc6c8  [0xc3000028] = 0x20400020     its ISR, acknowledging a completion
0x00232c64  [0xc3000028] = 0x20408028     its ATA driver, arming a wait
```

Both set IDE0_CFG's `0x30` clear bits. Modelling that write as dropping only `Ata::irq_pending` —
the bit the driver reads back — while leaving IRQ 23 asserted is what stormed: the line survived the
very write meant to clear it. With both acks modelled, a correct level model asserts **fewer**
interrupts than the edge model did:

| | edge (bypass #10) | level, both acks | level, ack ablated |
|---|---|---|---|
| irqs asserted | 402 864 | **183 452** | 8 585 404 |
| IDE deliveries | 18 | 7 | 968 014 |

The last column is `--no-cfg-ack`, an ablation switch added for exactly this: it reproduces the
historic storm on demand, which is how we know that is what the 2026 measurement was.

### The real blocker was that our drive is infinitely fast

Retiring #10 was necessary and not sufficient. RetailOS still sat out a **10.24 s timeout** on every
`READ DMA`, and the interleaving says why:

```
SET FEATURES  cmd @50582955  →  +208 instructions  →  ISR acks IDE0_CFG      worked
READ DMA      cmd @50630252  →  +12  instructions  →  driver arms its wait   stalled
```

Our model finishes a transfer *inside the store to the command register*. The completion is
therefore already asserted when the driver, twelve instructions later, writes the clear bits as part
of arming — so the acknowledgement lands on an interrupt whose handler has not run, and the driver
waits for a completion it has already destroyed. `SET FEATURES` survived only by scheduling luck:
208 instructions is longer than the 64-instruction service interval, so a tick slipped in.

A real 1.8" drive takes milliseconds. `IDE_COMPLETION_USEC = 50` is the smallest delay that is
unambiguously *later than the driver's own arming sequence*, which is the only property the model
needs. The PIO path Apple's bootloader polls is untouched.

### `0x93eea730` is an address after all, and bit 31 is not a flag

Addendum 3 left this open. It is settled, and the guess was wrong: RetailOS points the engine at
`0x93eea730` **and reads the transfer back from that same address** — 2048 word reads from
`0x000000fc`. Masking the top bit made the bytes land at `0x13eea730` while the read-back still went
to `0x93eea730` and answered zero. It is a third 64 MB window onto the same SDRAM, the same
"OR a window base over the low 26 bits" shape as the uncached view at `0x14000000`, and it is
registered as an alias rather than special-cased in the DMA engine.

### Where this leaves the boot

RetailOS's ATA driver is **satisfied**. Its whole disk sequence is now

```
[70] IDENTIFY   [71-75] SET FEATURES ×5   [76] READ DMA 64 sectors from LBA 0
```

and it stops there — **no re-init, no back-off to 4 sectors, no retry.** The read completes, every
byte lands, and it reads them back. The re-init-after-every-read that Addendum 2 mistook for a retry
storm is gone, because there is nothing left to retry.

The blocker has moved off the disk entirely. RetailOS now soft-resets itself at **@133 747 887**,
re-entering the image entry at `0x00000260` — 37 resets per run before, 157 now. It fails *sooner*
because it no longer spends 10.24 s per timeout getting there, which is what progress looks like
from here. The next question is what branches to that reset: a watchdog, or a panic path.

---

## Addendum 5: neither. It is `BX` to zero, and the headline of this file was right all along

**Nothing branches to `0x00000260`.** Two instructions branch to *address zero*, and address zero is
`b 0x000001f0` — RetailOS's own ARM reset vector, three hops upstream of the image entry. There is no
watchdog, no panic call, no assert, and no ARM exception. The mechanism is the one this file opens
with: **a C++ member call on a null object**, made through the ARM PMF thunk, with the vtable slot
read out of unmapped space.

### The two instructions, counted

`--enterlog=0x00000000` on the current cold recipe at 600 M / `--clock=5`, 157 arrivals, all of them:

```
0x00000000 from lr=0x117ffffc  x1      the cold reset (flash at 0, @1)
0x00000000 from lr=0x000fb8ec  x1      @133 750 533   <- the first real fault
0x00000000 from lr=0x00183e90  x155    every 2 991 488 instructions thereafter
```

Both `lr`s are `mov lr,pc` values, so the branch is `BX`, not a vector entry:

```
000fb8e4  mov  lr, pc            ; lr = 0x000fb8ec
000fb8e8  bx   r2                ; r2 = 0

00183e88  mov  lr, pc            ; lr = 0x00183e90
00183e8c  bx   r12               ; r12 = 0
```

`--enterlog` on the other six vectors — `0x04` undefined, `0x08` SWI, `0x0c` prefetch abort, `0x10`
data abort, `0x14` reserved, `0x1c` FIQ — returns **zero arrivals in 600 M instructions**. The only
vectors RetailOS ever enters are `0x00` and `0x18`. No SWI at all means no semihosting panic dump
either, and RetailOS's panic dumps arrive by semihosting.

### Where the zero comes from: the vtable slot is unmapped and we answer 0

`0x000fb8a4` is the ARM C++ **pointer-to-member-function invoker**:

```
000fb8a4  stmdb sp!, {r0-r3}
000fb8b4  ldmia r1, {r0, r1}     ; the {this, ...} pair the caller passed
000fb8b8  bic   r8, r0, #0x1
000fb8bc  mov   r10, r6, asr #1  ; r6 = the PMF adj word, = 1 -> virtual
000fb8c4  tst   r6, #0x1
000fb8c8  add   r0, r10, r8      ; adjusted this
000fb8cc  ldrne r1, [r0, #0x0]   ; vptr = *this
000fb8d4  ldrne r2, [r7, r1]     ; fn = vtable[offset]   r7 = 0x1c
000fb8e8  bx    r2
```

`this` is **0**, so `ldr r1,[r0]` reads `*(0x00000000)` — which is RetailOS's own reset-vector word,
`0xea00007a`. The vtable read then lands at `0xea0000xx`, which is mapped nowhere, and the bus
answers 0. `bx 0`. The `--profile` unmapped report names both culprits and nothing else:

```
unmapped: 640 reads, 0 writes across 1 page
  0xea000078..0xea0000d7   640 reads   first pc 0x000a09b4
        pc 0x00183e7c  x620      ldr r12, [r1, #0x5c]
        pc 0x000a09b4  x8
        pc 0x000a0bd0  x8
        pc 0x000fb8d4  x4        ldr r2,  [r7, r1]
```

The second site has the same shape one indirection earlier — `ldr r0,[r4,#0x440]; ldr r0,[r0,#0x1c]`
loads a null delegate out of an object's `+0x1c`, and `ldr r1,[r0,#0]` then reads address 0.

### The reset path, disassembled

```
00000000  b    0x000001f0            the ARM reset vector
000001f0  mov  r11, #0x700           ; r11 = 0x700 is still in the register file at every halt
00000238  mov  pc, r1                ; r1 = [0x000002fc] = 0x0000023c
00000240  mov  r0, #0x60000000       ; PROC_ID
00000244  ldrb r2, [r0]
00000248  cmp  r2, #0x55             ; 0x55 = CPU core, 0xaa = COP
0000025c  ldr  sp, [0x00000304]      ; sp = 0x40003ff8
00000260  bl   0x000015f8            <- the image entry
000002a0  bl   0x00083848            ; scatter-load -> 0x000843a0 BSS zero, 40% of the run
```

`--enterlog` counts on that path are `0x000001f0` ×156, `0x00000238` ×156, `0x00000260` ×157,
`0x00001600` ×157, `0x00083848` ×157 — 156 where the cold boot enters below the stub, 157 where it
does not. So "re-entering the image entry at `0x00000260`" was right; it is just three hops
downstream of the actual branch, and naming the branch is what was missing.

> **Instrument note.** `--enterlog`'s per-arrival detail print is capped at 400 rows and its log at
> 65 536 entries; the `callers:` histogram below it is not. Reading the detail rows as the count
> gave "`0x00000260` is entered once" for most of an hour. Watching `0x000843a0` in the same run —
> 65 172 arrivals — silently truncated everything after it. Both are the same trap this file's §2
> records: **read the histogram, and never watch a hot address alongside a rare one.**

### The condition, measured: it is the missing font, and §5–§7 reproduce exactly

`--enterlog=0x00221e24` on the current tree, 447 lookups, keyed (name, size, style):

| name ptr | size | style | count |
|---|---|---|---|
| `0x004b1360` Podium Sans | **0x12 = 18** | 1 | **348** |
| `0x004b1360` Podium Sans | 0x0e = 14 | 1 | 21 |
| `0x004b1360` Podium Sans | 0x10 = 16 | 1 | 13 |
| `0x004b1360` Podium Sans | 0x1c = 28 | 1 | 5 |
| `0x004b1318` | 0x0a = 10 | 0 | 5 |
| `0x004b12c4` | 0x07 | 1 | 4 |
| `0x004b1360` Podium Sans | 0x16 = 22 | 1 | 3 |
| `0x0023c730` | 0x0e = 14 | 1 | 1 |

§6's distribution is bit-for-bit the same after every disk fix. Size 18 is asked for 348 times and is
not registered; 14/16/22/28 are. `--storelog=0x00221ea8` shows the miss returning `{0,0}` into a
stack temporary 394 times, and `--storeaddr` on the object that dies closes the loop:

```
@55 685 791  0x0017f188 -> [0x13e27a88] = 0    allocation zeroing
@55 685 816  0x00210684 -> [0x13e27a88] = 0    the initialiser
@55 687 376  0x00211778 -> [0x13e27a88] = 0    the delegate setter
```

Those are the same three instructions §2 found writing the failing object's `+0x20`. 78 M
instructions later, text layout for character `0x30` walks a string through `0x000cd880` →
`0x000caa80` → `0x0011e16c` → the PMF thunk, with `r1 = 0x13e27a88` and the PMF itself
(`{offset 0x1c, adj 1}`) read from the image literal at `0x004f3fb4`. The *function* half is a static
constant and is fine. The **object** half is the registry's null.

### The ablation, and what is behind the wall

`--null-dispatch=survive`, which reports `BX 0` as a null return instead of taking it:

| | baseline | `--null-dispatch=survive` |
|---|---|---|
| arrivals at address 0 | 157 | **1** (the cold reset) |
| distinct code buckets | 19 535 | **26 373** |
| last first-time execution | @136 742 970 | **@344 998 464** |
| ATA commands | 77 | 83 — adds `0xe0` STANDBY IMMEDIATE + a re-init |
| hottest code | `0x000843a0` BSS zero, 40 % | `ICAPTPCameraIOTask`, `ImagePresentationEngine`, `ABCDEF` |
| resting state | restart loop | `000af4a4 bl 0x000c1648 / b 0x000af4a4` — **the RTXC idle loop** |
| null dispatches | — | 53 732 at `0x000fb8d4` alone |

So the null branch is the *whole* cause of the reset loop: suppress it and RetailOS reaches the same
idle state research/03 §40 described, having spun the drive down on the way. Nothing else resets it.

### Whose defect

**RetailOS's behaviour is correct and is reacting to something we get wrong.** A registry miss
returning a null delegate is designed (§5); binding it into an object is designed; calling through it
is the bug, and it is Apple's, but it only ever fires on a machine where `PodiumSans18.ttf` was never
registered. On real hardware it is registered, because `rsrc` is mounted.

`rsrc` is still not mounted, and the ablation proves the reset was never what stopped that: with 344 M
instructions of extra progress and a full ATA power-management cycle, RetailOS's disk activity is
*still* exactly one 64-sector `READ DMA` from LBA 0. It never asks for `devOffset 0x0073a000`.

**One thing here is our defect, and it is a diagnostic gap rather than a cause.** `Exception::DataAbort`
and `Exception::PrefetchAbort` exist in `cpu.rs` with correct vectors and target modes and are
**constructed nowhere in the tree** — `grep -rn 'Exception::' tools/` finds only `Undefined`, `SoftwareInterrupt`
and `Irq` being raised. An unmapped read returns 0 and is recorded; it never faults. That is why a
wild pointer presents as a clean reboot instead of stopping at the instruction that made it. The fix
is not to abort by default — the unmapped report is more useful than a fault — but a
`--abort-on-unmapped` diagnostic would have cost minutes instead of hours here.

Symmetrically, and worth stating because it is what rules the watchdog out structurally: **the
emulator has no path that can reset the CPU.** `Exception::Reset` is never raised, no device model
writes `regs[15]`, and no watchdog register is modelled. The only way PC becomes 0 is an instruction
executing, so the branch was always going to be in the firmware.

## Addendum 6: on the retail path the co-processor is never handed anything

Measured 2026-08-13 on `retail-boot.sh`, and it re-reads the framebuffer dump that looked like
evidence of rendering.

> **Half retracted 2026-08-14 by the Addendum 14 audit — and the other half is now the strongest
> thing in this file.** Three sentences below are wrong, all for the same reason: they were measured
> before the second DMA controller was modelled (Addendum 9), so RetailOS could not upload anything.
> On the current baseline it uploads all 201 376 bytes.
> **Wrong:** "RetailOS's entire lifetime contribution to `0x30000000` is 12 reads and 30 writes" — it
> is **24 reads and 100 696 halfword writes**. "It is read off disk, parked, and **never uploaded**"
> — it is uploaded, in 4 chunks. "**Bypass #6 is therefore not merely open, it is unreachable**" —
> it is reachable, and the ledger row has been rewritten twice since.
> **Right, and confirmed by a control this section did not have:** *"It is the ROM's own logo, and
> RetailOS has drawn nothing at all."* Dumping the framebuffer at the exact ROM→RetailOS handoff
> (`--stop-at=0x10000000:1`, @46 397 133) and at the end of a 600 M run produces **byte-identical
> PPM files** — 2 922 non-black pixels of 76 800 in both. **Addendum 12 §1's "RetailOS reaches the
> display" is the claim that does not survive**, not this one. See Addendum 14 §2.

**The BCM report is byte-identical at @46 M — before RetailOS starts — and at @2000 M.** Same four
commands kicked, same two frame updates, same 129 876 halfwords, same six latch writes. All of it is
**the boot ROM's logo**. RetailOS's entire lifetime contribution to `0x30000000` is **12 reads and
30 writes**.

So the 320×240 frame at `0x000e0000` that dumps as diagonal streaks over black, 2 922 non-black
pixels of 76 800, is not a half-drawn UI. It is the ROM's own logo, and RetailOS has drawn nothing
at all.

The firmware does land in RAM: `vmcs.bin`'s first-sector signature `4a 0b 00 00 46 0b 00 00` appears
**exactly once** in the 64 MB of SDRAM, at `0x13eaf188` — a filesystem read buffer beside the 32 KB
DMA bounce buffer at `0x13eea730`. It is read off disk, parked, and **never uploaded**.

**Bypass #6 is therefore not merely open, it is unreachable.** The display path stops before the
handoff. Emulating the VideoCore today would be building an engine for a car that does not start:
there is no measurement it could change, because nothing ever asks the co-processor to run.

That reorders the work. The bypass is last, not next — and its retirement condition should now read
"once RetailOS uploads the firmware it has already loaded," which is downstream of naming the task
that never gets kicked.

---

## Addendum 7: the task is `APPLEBOOT`, and it is blocked mid-upload of `vmcs.bin`

Measured 2026-08-13 on `retail-boot.sh`, `BUDGET=600000000 --clock=5`, 96 ATA commands and zero
unmapped accesses in every run quoted. Reproduced identically before and after the CPU-sleep model
landed — the pre-idle instruction counts are the same to the instruction.

### 0. The premise of the question was wrong: there is no hook table

The idle loop was described as walking a NULL-terminated hook table at `0x002a9c84`. It is not a
table. `--dump` shows exactly one non-zero word:

```
002a9c84  01 00 00 00 00 00 00 00 00 00 00 00 ...
```

and the loop that reads it is a **one-shot**, at the top of the function the idle loop lives in:

```
000af468  stmdb sp!, {r4-r6, lr}
000af474  str  r0, [r1]           ; 0xf -> 0x1493718c
000af480  msr  cpsr_cf, r0        ; interrupts on
000af484  ldr  r5, [pc, #0x24]    ; r5 = 0x002a9c84
000af488  mov  r4, #0
000af490  bl   0x000a6374         ; per non-zero entry
000af498  ldr  r0, [r5, r4, lsl #2]
000af4a0  bne  0x000af490
000af4a4  bl   0x000c1648         ; then the real idle loop, forever
```

`r4 = 1` in the registers at halt: the walk ended after one entry and never runs again.
And `0x000a6374` is not a hook — it is the RTXC service wrapper for **`KS_execute`**:

```
000a6374  stmdb sp!, {r0-r4, lr}
000a6378  str r0, [sp, #0x8]      ; arg: task id
000a637c  mov r0, #0x15           ; syscall 0x15
000a6380  str r0, [sp, #0x0]
000a6384  mov r0, sp              ; r0 -> {number, args...}
000a6388  bl  0x00084644          ; the kernel trampoline
```

So the whole of `0x002a9c84` says "start task 1, then idle". Nothing is enumerated there and
skipped. The 43 other tasks are created by task 1 and its children.

### 1. The RTXC internals, recovered

Everything below is read out of a saved SDRAM region (`--save-region=sdram:FILE`), so it costs no
run at all once the file exists.

| | |
|---|---|
| kernel trampoline | `0x00084644` — saves `{cpsr, r0-r12, lr, lr}`, switches to the kernel stack at `0x009368d8`, calls the dispatcher with `r0` = frame, takes a new SP back |
| dispatcher | `0x0027f5f0`; `r4 = [r0+4]` = request, `r12 = [r4]` = syscall, `addls pc, pc, r12, lsl #2` over a 49-entry table at `0x0027f640` (services `0x00`..`0x30`) |
| service wrappers | 38 of them, contiguous `0x000a613c`..`0x000a69cc` |
| TCB array | `0x0087198c`, stride `0x3c`. 45 in use (ids 0-44), 35 free (state `0x100`, prio 126) |
| current task | `[0x0081db00]` (kernel globals at `0x0081daf4`) |
| named-task registry | `0x00880f8c`, 5-word records `{stack, entry, name, id, prio}`, 22 device tasks |

TCB fields, from the code that writes them (`KS_execute` at `0x00280b44` unlinks via `+0x00`/`+0x04`;
`0x000a63b8` indexes the array as `base + id*60`):

```
+0x00 next   +0x04 prev   +0x0c entry   +0x10 saved SP   +0x14 stack   +0x18 size
+0x1c state  +0x20 id     +0x24 priority   +0x28 tick when the task last ran
```

Syscall numbers, measured from the `mov r0, #imm` in each wrapper (they are **not** the numbers on
freemyipod's S5L-era page — `KS_pend` is `0x01` here, not `0x03`):

| | | | |
|---|---|---|---|
| `0x01` `KS_pend(sem)` `0xa6924` | `0x02` `KS_signal(sem)` `0xa67b0` | `0x05` `KS_receive(mbx)` `0xa66a4` | `0x0e` `KS_lock` *(bl @`0xa65e8`)* |
| `0x14` `KS_delay` `0xa62b4` | `0x15` `KS_execute` `0xa6374` | `0x19` `KS_suspend` `0xa6844` | `0x22` `KS_waitm` `0xa6958` |

Because the saved frame's last word is the resume PC, and every resume PC is `BL-site + 4`, **the
whole system's blocking state falls out of the SDRAM dump with no instrumentation at all**: read
each TCB's saved SP, take word 15, look up which wrapper it returns into, and read the request
struct the wrapper built on the task's own stack.

### 2. The resting state of all 45 tasks

`tick` is TCB `+0x28`, the RTXC tick at which the task last ran; the run ends at tick 32 860.

| | task | pri | blocked in | on |
|---|---|---|---|---|
| 0 | *(main, becomes idle)* | 127 | — | running |
| 5 | `t_csa` | 5 | `KS_receive` | mailbox 2 |
| 6 | `t_device` | 29 | `KS_waitm` | — |
| 7 | `t_ppfs` | 30 | `KS_receive` | mailbox 3 |
| 8 | `t_power` | 1 | `KS_pend` | sem `0x49` *(alive, tick 32 853)* |
| **9** | **`APPLEBOOT`** | **15** | **`KS_pend`** | **sem `0xe0` — tick 473** |
| 10 | `t_graphicsManager` | 31 | `KS_receive` | mailbox `0x16` — tick 216 |
| 18, 22, 23, 25, 38 | Firewire / CNA / USBPowerSense / HoldSwitch / RsistrAccsry | | `KS_delay` | *alive, tick 32 860* |
| 24 | `DiskMgrTask` | 10 | `KS_receive` | mailbox `0x14` |
| 37 | `BotSerAccsryMgr` | 7 | `KS_lock` | resource `0x130` |
| 20, 21, 26-36, 39 | the rest of the device layer | | `KS_pend`/`KS_waitm` | their own semaphores |

Five tasks are still alive on periodic `KS_delay` timers. **Everything else last ran before tick
11 478 of 32 860, and the interesting ones stopped in the first 500 ticks.**

> **Correction to the symboliser.** `extract_symbols`'s pattern A assumes *name then pointer*. The
> six boot tasks are declared in a literal pool that is *pointer then name*, so every one of them is
> reported shifted by one record: the profile calls `0x00284ea0` "APPLEBOOT" when it is
> `t_graphicsManager`, and calls `0x002844e0` "t_power" when it is `APPLEBOOT`. The correct mapping
> is not inferred, it is read off the creation code at `0x000d3b60`, which builds one 0x18-byte
> descriptor per task and memcpy's it into `0x108c77b4 + 0x18*k`:
> `add r2, pc, #imm` gives the name, `ldr r1, [pc, #imm]` the entry, `mov r3, #imm` the priority.
> Every priority and stack size so recovered matches the resulting TCB exactly — `t_device` 29/0x800,
> `t_ppfs` 30/0xc00, `t_power` 1/0x400, `APPLEBOOT` 15/0x1000, `t_graphicsManager` 31/0x800.
> Pattern A cannot simply be reversed: in the *device* registry at `0x0025d63c` the word before each
> name is the previous entry's pointer, so a blind reversal renames `OptoTask` to `SerialOptoTask`.

### 3. `APPLEBOOT` blocks once, at @51 764 626, and ~~is never woken~~

> **"Never woken" superseded 2026-08-13 by Addendum 9, marked here 2026-08-14.** It is woken: the
> engine at `0x60009000` is a real DMA controller, it completes the chunk, and `0xe0` is signalled.
> `APPLEBOOT` then runs on and re-blocks on `0xea` (Addendum 8 §6), which is where it still sits at
> the end of a 600 M baseline — TCB 9, `state = 0x40`, tick 527 of 236 975. The counts below (82
> pends, 60 signals) are a 45-task, 96-ATA-command machine that no longer exists; the baseline is 62
> tasks and 256 commands. **What survives is the identification** — the object, the channel fields,
> the `sync()` vtable slot and the chunk loop, all of which Addendum 9 reproduced against a working
> engine.

`--enterlog=0x000a6924` — every `KS_pend` in a 600 M-instruction run. There are **82**, and exactly
one is `0xe0`:

```
0x000a6924 lr=0x00159c70  r0=0x000000e0  @51764626
```

`--enterlog=0x000a67b0` — every `KS_signal` in the same run. There are **60**. `0xe0` is not among
them. The producer that services this family is alive and well: `0x000ddc84` signals `0xc1`, `0xc4`,
`0xc6` … `0xdf` — twenty-two of them, the disk reads — and stops one short.

`APPLEBOOT`'s stack, walked for return addresses (every candidate is preceded by a `BL`):

```
000a694c   KS_pend wrapper
00159c70   <- 00159c6c  bl 0x000d1e78  ->  ldr r0,[r0]; b 0x000a0ebc
00287c78 · 002877cc · 00094dfc · 001affc4 · 001b04f0 · 001afb94 · 001b0334
00284518   <- APPLEBOOT+0x34:  mov r1,#1;  bl 0x001b02d8
```

`0x000a0ebc` is a counting acquire: decrement, and if it goes negative, `KS_pend(obj->[4])`.

### 4. What it is waiting for: the `vmcs.bin` upload

`--enterlog=0x00159c5c` fires **once** in the whole run, 37 instructions before the pend:

```
0x00159c5c lr=0x0028704c  r0=0x13ee0824 r1=0  @51764589
```

`r0` is the `this` of a transfer channel, and its fields say what the transfer is:

```
13ee0824  00669a2c 60000000 1086cb54 1086ca90   vtable, ...
13ee0834  00000001 00000000 00000002 13eaf188   <- +0x1c = the vmcs.bin buffer (Addendum 6)
13ee0844  30000000 00010000 ...                 <- +0x20 = the BCM bus window, +0x24 = 64 KB
13ee0874  ...                           13ee0814 <- +0x5c = the object holding sem 0xe0
13ee0814  ffffffff 000000e0 ...                  count -1, sem 0xe0
```

`[0x00669a2c + 0x18] = 0x00159c5c`, so the wait is vtable slot 6 of that channel — `sync()`.

`lr = 0x0028704c` puts the call inside a chunking loop:

```
00286ff4  add  r0, r4, #0x10000    ; r4 = the byte count
00287000  sub  r6, r0, #1          ; r6 = ceil(count/64K) - 1
00287004  mov  r9, #0x10000
00287010  ...
00287030  bl   0x001599fc          ; submit one 64 KB chunk
00287040  ldr  r2, [r1, #0x18]     ; -> 0x00159c5c
00287048  bx   r2                  ; sync(): waits on sem 0xe0   <- never returns
0028704c  add  r5, r5, r8, lsl #2  ; next chunk
```

and the literal pools of the two functions above it in the stack name the subsystem outright:
`0x001afc80 "vmcs.bin"`, `0x001b0038 "RenderServer.bin"`, `0x001afc94 "set_vll_dir %s"`,
`0x001b01e4 "pm_set_policy min"`, `0x001b0200 "pm_show_stats 0 10 10 90 16"` — VideoCore host
commands — and `0x002878f3 "0VMCS    BIN"`, the 8.3 directory entry it matched in `rsrc`.

**`APPLEBOOT` is stuck on the first 64 KB chunk of the VideoCore firmware upload.** Addendum 6 asked
why `vmcs.bin` lands at `0x13eaf188` and is never uploaded. This is why: the upload was started and
the first chunk never completed.

### 5. Nothing in the machine could have completed it — *§5's second half is WRONG; see Addendum 8 §6*

`--storeaddr` on the driver's state byte, and a dump of the driver object, give the arming sequence:

```
001da1e0 -> [1086cb60] = 60009000  @49341956    ; the engine's register base, hard-assigned
001da1f0 -> [1086cb84] = 0000000c  @49341960    ; state
00125320 -> [1086cb84] = 0000000d  @51764412
0028e3dc -> [1086cb84] = 0000000f  @51764569    ; armed, 57 instructions before the pend
```

So the channel really is armed, and it names a register block at **`0x60009000`**.

> **RETRACTED 2026-08-13.** The paragraph below is an instrument artefact, and the sentence
> defending it — "silence here is a measurement and not a broken flag" — is the exact claim that was
> false. `--watch-range` could not see word-sized writes into a mapped region at all; the 130 649
> writes it offered as a positive control were *byte* writes, which take a different path. The
> engine **is** programmed, 208 byte-writes' worth, by the command dispatcher at `0x0028e2xx`
> immediately before the pend. See Addendum 8 §6 for the mechanism, the fix, and its control.

`--watch-range=0x60008000:0x8000` over the whole run: **222 byte-writes, every one of them in the
GPIO block at `0x6000d0xx`, all but five from the boot ROM.** Not one write anywhere in
`0x60009000..0x60009fff`. The engine is never programmed, no completion interrupt can be raised, and
`0xe0` can never be signalled. The instrument is the same one that reports 130 649 writes to
`0x60007000` in the same build, so silence here is a measurement and not a broken flag.

### 6. Negative results, recorded because they were predicted and did not measure out

- **The inter-processor mailbox is never touched.** `--readlog` and `--storeaddr` over all twelve
  words of `0x60001000..0x6000102f`: **zero reads, zero writes**. Positive control in the same run
  and the same log: `0x60007004` recorded 5 470 reads and `0x60007000` 130 649 writes. RetailOS does
  not send the COP a message and does not look for one.

  > **CONFIRMED on re-measurement, 2026-08-13.** This bullet's original controls were the same
  > byte-width `0x60007000` traffic that failed §5, so it was re-run with controls matched to the
  > measurement in width, region and code path. Current build, current machine (idle @123.4 M, after
  > the second DMA controller was modelled): `--readlog` carrying `0x60005010` as a word-read control
  > logged **438 345 reads, none of them in `0x600010xx`**; `--storeaddr` carrying `0x60009000` as a
  > word-store control into the same MMIO block logged **42 stores, none of them in `0x600010xx`**.
  > Both instruments were live and provably so in the same run, and both were unaffected by the hoist
  > bug in any case — `read_addrs` was always in the read hoist, and `note_store_pc` sits above it.
  > The mailbox really is silent, on a machine that now boots roughly twice as far.
- **But it does try to wake the COP, 10 198 times, and our model guarantees it cannot.** `0x000cfb20`
  is Rockbox's `wake_cop` verbatim — `cmp r0, #0x55`; read `COP_CTL`; `tst #0x80000000`; clear the
  bit; write back. `trace.rs` installs an unconditional read override returning `0x80000000` at that
  address (bypass #7), so the write-back is discarded and the next read still says COPSLEEPING. The
  retries begin at @60.8 M, *after* `APPLEBOOT` blocks at @51.8 M, so this is not the direct cause —
  but it is the only plausible agent left for a transfer engine the CPU never programs.
- **IRQ 4 is enabled.** `0x001fc9a4` writes `0x10` to `CPU_INT_EN` (`0x60004024`), alongside `0x04`,
  `0x01` and `0xc0000000`. If `0x60001000` is the mailbox and IRQ 4 its line, RetailOS has armed the
  interrupt it would need and is simply never sent one.
- **`CPU_CTRL` is written with wait-counts the new sleep model ignores.** 130 644 of the 130 649
  writes are plain `0x80000000` (modelled), but five are not: `0x4800001f`, `0x4800000f`,
  `0x420000ff`, `0x4800001f`, `0x4800000f` from `0x001b8c70`..`0x001b8d70` — `PROC_WAIT_CNT` with
  `PROC_CNT_CLKS`/`PROC_CNT_USEC` and a counter in bits 0-7. All five are at @49.32 M, before the
  block, so they do not gate this finding.
- ~~**`OptoTask` is declared and never created.** The device registry reserves it a stack at
  `0x10874ebc` and an entry at `0x00284f90`; no TCB has either. The other 21 registry entries all
  have TCBs. Noted, not pursued — it is not on the display path.~~

  > **WRONG — retracted 2026-08-13 by Addendum 13, and the TCB half re-checked 2026-08-14.** It is
  > created and dispatched at **@49 678 867**, with RTXC's task-entry register fill. It also has a
  > TCB, and the reason this looked otherwise is that the two records were compared on the wrong
  > field: the registry's stack `0x10874ebc` is the stack **top**, and TCB 18 records the **base**,
  > `0x10874abc`, with `size = 0x400`. `0x10874abc + 0x400 = 0x10874ebc` — the same stack. TCB 18 is
  > `state = 0x0000` (runnable), priority 4, `tick = 236 974` of 236 975: not merely created, still
  > alive at the end of the run. So RetailOS's click-wheel task has been executing against
  > `0x7000c140` — which we answer as zero — for the whole boot. See Addendum 14 §3.
  >
  > > **The TCB number is WRONG — corrected 2026-08-14 by Addendum 17 §3.** `OptoTask` is created,
  > > but it is **task 19**, not 18: `--enterlog=0x00284f90` fires once at `@49 678 867` carrying
  > > RTXC's task-entry register fill, whose low byte is the id (`0x13`). TCB **18**'s entry is
  > > `0x00284e58`, which the device table at `0x00880f8c` pairs *by pointer* with the name
  > > `"FirewireTask"`; the stacks merely abut, so the `+0x400` arithmetic above matched a
  > > neighbour. `OptoTask` runs once and its slot is recycled — at the idle stop TCB 19 is a
  > > pooled thread on sem `0xbf`. The last sentence dies with it, and Addendum 16 §4 had already
  > > measured **zero** reads of `0x7000c140` by RetailOS.

### 7. What this means for the roadmap

> **The first sentence is WRONG — retracted 2026-08-14. Seven of the ten names below are created,
> and four of the seven only appear past the point the old recipe stopped looking.**
> `--enterlog=0x0011c808` — RTXC's task-creation entry, `r0` = the name pointer — logs **27 named
> creations** on the baseline recipe. Of this list: `USBDeviceTask` @49 705 952 · `VCUpdateTask`
> @50 441 379 · `ATAWorkLoopTask` @50 494 071 · `TrackCacheReadTask` @53 608 893 ·
> `StreamCacheReadTask` @174 340 885 · `SearchHelperThread` @174 462 018 · `PhotoCopyTask`
> @249 931 182. Only `DiskReaderTask`, `ImagePresentationEngine` and `iMAImageCacheThread` are still
> absent. The TCB array holds **62** tasks, not 45.
> **The second sentence survives**: `t_graphicsManager` is TCB 10, `state = 0x20` (`KS_receive`),
> `tick = 216` of 236 975 — it has not run since tick 216, so nothing has been posted to it. See
> Addendum 14 §4.

~~The media and UI layer never comes into existence.~~ `DiskReaderTask`, `VCUpdateTask`, `USBDeviceTask`,
`ATAWorkLoopTask`, `TrackCacheReadTask`, `SearchHelperThread`, `ImagePresentationEngine`,
`iMAImageCacheThread`, `StreamCacheReadTask`, `PhotoCopyTask` and a dozen more all carry inline name
labels and real prologues in the image, and ~~**not one of them has a TCB**~~. `t_graphicsManager` sits
in its event pump at priority 31 with nothing ever posted to it. That is not a second bug; it is what
`APPLEBOOT` not finishing looks like — it blocks two calls short of the end of its own body:

```
0028450c  bl 0x001afd74
00284510  mov r1, #1
00284514  bl 0x001b02d8     <- inside this, forever
0028452c  bl 0x000d05c0
00284538  b  0x000a6860     ... and APPLEBOOT would have terminated, its work done
```

**Bypass #6 is no longer unreachable, and it is no longer last.** Addendum 6's retirement condition —
"once RetailOS uploads the firmware it has already loaded" — is now a specific, addressed piece of
hardware: whatever sits at `0x60009000` and moves 64 KB from SDRAM into the `0x30000000` window, and
whatever posts the completion. Model that, or short-circuit it by posting `0xe0` from the emulator,
and `APPLEBOOT` runs to its own last line for the first time.

---

## Addendum 8: posting `0xe0` is not enough — ~~RetailOS needs the co-processor to answer~~

> **Title retracted 2026-08-13 by Addendum 10.** The first half stands and is load-bearing: posting
> `0xe0` alone really is not enough, and the ablation really did take the machine from 45 tasks to
> 62. The conclusion drawn from it did not. §7's third pillar — "the very next thing `APPLEBOOT`
> does is send a request and wait for a reply" — rested on a call that was never disassembled;
> `0x0016b0b0` creates a thread, and the semaphore it waits on is posted by ordinary ARM code.
> **Nothing measured requires the co-processor to answer.** Read §§1–6 for the ablation; read
> Addendum 10 before believing §7.

Measured 2026-08-13 on `retail-boot.sh --clock=5`, `BUDGET=2000000000 --stop-when-idle=150000000`
except where a 600 M/40 M budget is named. **This is ledger bypass #17, an ablation** — a deliberate,
flagged, off-by-default lie told to see what is behind the door. Nothing here is a fix.

Addendum 7 ended with: *"Model that, or short-circuit it by posting `0xe0` from the emulator, and
`APPLEBOOT` runs to its own last line for the first time."* Posting `0xe0` was done. **`APPLEBOOT`
does not run to its last line**, and the reason it does not is the finding.

### 0. What the flag does, and how it checks itself

`--force-vc-upload` makes `KS_pend` at `0x000a6924` return `0` instead of blocking, for semaphore
`0xe0` only. It is keyed on the pend rather than on a memory value because the wait is reached by a
*tail* `B` from the counting acquire at `0x000a0ebc` — there is no call frame to patch, and the
transfer object's address is heap-dependent while the instruction is not. Returning is the whole
implementation: the wrapper's frame is not built until the next instruction, and `r0 = 0` is what
the kernel would have left in the request struct's `+0x04` slot, which `0x000a6938` pre-zeroes.

Its own positive control is the arrival count. The unablated run pends on `0xe0` **exactly once**;
so does the ablated one:

```
--- bypass #17: pends satisfied without a producer: 1 ---
  sem=0xe0  returned to lr=0x00159c70  @51764562
```

`@51764562` with `lr=0x00159c70` is the same instruction, to the instruction, that the baseline
`--enterlog=0x000a6924` records. The two runs are identical up to that point.

### 1. The frame walk, which is how every "blocked on what" below was measured

Addendum 7 §1 established that a saved frame's last word is the resume PC. That makes the whole
system's blocking state a pure function of a `--save-region=sdram` dump: for each TCB, take `+0x10`
(saved SP), read word 15 for the resume PC, look up which wrapper it returns into, and read the
request struct the wrapper built — `frame[1]` is `r0`, and for `KS_pend` its `+0x08` is the
semaphore. The instrument's control is that it reproduces Addendum 7 §2 exactly on the baseline
dump: task 9, `entry=0x002844e0`, `state=0x40`, resume `0x000a694c`, **`sem=0xe0`**, tick 473.

### 2. Stage 1: the wait is satisfied, and `APPLEBOOT` immediately starts spinning instead

`0xe0` unblocks and the chunking loop does advance — `0x0028704c` is reached for the first time and
a second 64 KB chunk is submitted:

| arrival | baseline | `--force-vc-upload` |
|---|---|---|
| `0x00287030` submit, from loop entry `0x00286ff4` | x1 | x1 |
| `0x00287030` submit, from the back-edge `0x0028704c` | — | **x1** |
| `0x0028704c` next-chunk | — | **x1** |
| `0x00284518` return from `bl 0x001b02d8` | — | — |

and then it stops again, without blocking. Task 9 is `state=0x0` — runnable, not pended — with
`tick=39367`, the highest in the system. **71.2 % of the steady state is a 48-byte window:**

```
profile: 2343750 samples over 73 buckets   (window [100 M, 250 M))
  0x00159bb0   35.6%   0x00159ba0   22.2%   0x00159bc0   13.4%
```

### 3. What it spins on: a drain barrier only the engine can clear

`0x001599fc` (submit) calls the buffer allocator at `0x00159b88`, which is entered twice and never
leaves the second time:

```
00159b9c  ldr   r3, [r0, #0x10]
00159ba0  mov   r2, #0x0          <- retry target
00159ba4  mov   r1, #0x0
00159ba8  add   lr, r0, r1        <- scan four in-use bytes
00159bac  ldrb  lr, [lr, #0x18]
00159bb0  cmp   lr, #0x0
00159bb4  movne r2, #0x1          ; any non-zero -> still busy
00159bb8  add   r1, r1, #0x1
00159bbc  cmp   r1, #0x4
00159bc0  blt   0x00159ba8
00159bc4  cmp   r2, #0x0
00159bc8  bne   0x00159ba0        <- spins until ALL FOUR read zero
00159bcc  add   r0, r0, r3
00159bd0  strb  r12, [r0, #0x18]  ; only then claim a buffer
```

This is not a queue with backpressure; it is a **drain barrier**. It waits for every outstanding
buffer to be retired, and the only thing that retires one is the completion path. `--dump` of the
channel at the end of the ablated run shows why it never exits — `+0x18` is not zero:

```
13ee0834  01 00 00 00  00 00 00 00  02 00 00 00  88 f1 ea 13
                                    ^^ +0x18 = 02 00 00 00
```

Six byte-writes ever land in that object, all from `0x00159d68`, and none of them is a clear.

### 4. And a busy-wait at priority 15 is worse than a block

The baseline's `APPLEBOOT` blocks, which yields. The ablated one spins, which does not — at priority
15, above most of the device layer. The system goes *backwards*:

| | baseline | stage 1 only |
|---|---|---|
| tasks in the TCB array | 45 | **44** (`USB MSC`, pri 51, never created) |
| ATA commands | 96 | 90 |
| code buckets executed | 12 009 | 11 272 |
| RTXC tick at the end of the run | 32 861 | 9 886 |

That last row is also the trap in comparing the two at all: instructions and simulated time stop
being interchangeable the moment the machine stops sleeping, so "fewer buckets" is partly starvation
and partly the run covering 3.3× less simulated time for the same instruction budget.

### 5. Stage 2: model the retire as well, and the OS builds itself

`--force-vc-retire` zeroes the four bytes on the spin edge — at `0x00159bc8` with `r2 != 0`, the
branch that was about to loop. Keying it on the retry rather than the loop head means a genuinely
free ring is untouched, so it changes nothing on the fast path. It fires **three times**, and with
both halves on, `APPLEBOOT` gets past the wall:

- **`0x00284518` is reached** — `bl 0x001b02d8` returned, which it has never done before — and so is
  `0x0028452c`, the second-to-last call of the body.
- **45 tasks become 62.** Seventeen ids that do not exist in the baseline at all. Named from their
  creation descriptors, and absent from the same scan of the baseline dump:
  **`MP3ExampleTask`** (pri 52) · **`TcMemManagerThread`** · **`iMAImageCacheThread`** ·
  **`CIapIncomingProcessThread`** (pri 49) · **`StreamCacheReadTask`** (pri 60). These are the media
  and UI layer Addendum 7 §7 recorded as never coming into existence.
- **ATA commands 96 → 256; DMA 72 transfers / 8 113 664 bytes → 302 / 15 658 496.** It reads another
  7.5 MB off the disk.
- **Code buckets 12 009 → 27 904.**
- **The display does not light up.** BCM halfwords written 129 876 → 129 964, frame updates 2 → 2.
  Reads rise 32 → 56 — it asks the co-processor something and is answered by our synthesised
  constant.

`0x00284538` is still not reached: `APPLEBOOT` does not terminate. It blocks again, and *where* is
the whole point.

### 6. The new wall is `0xea`, and it is a request for an answer

```
0x000a6924 lr=0x000cd7e0  r0=0x000000ea  @52033637
```

Walking task 9's stack in the stage-2 dump gives the path, every candidate preceded by a `BL`:

```
0x108c6fa8  0x00284530   <- bl at 0x0028452c   APPLEBOOT's second-to-last call
0x108c6fa0  0x000d05d0   <- bl at 0x000d05cc   0x000d05c0 calls 0x000cd7c8(0, 0)
0x108c6f90  0x000cd7e0   <- bl at 0x000cd7dc
0x108c6f74  0x000a694c   <- KS_pend
```

and `0x000cd7c8` is six instructions long and says exactly what it is:

```
000cd7c8  stmdb sp!, {r0, r1, r4, lr}
000cd7cc  ldr  r0, [pc, #0x10]   ; 0x1081eaf4
000cd7d0  bl   0x000d1e5c        ; take the request lock
000cd7d4  bl   0x0016b0b0        ; issue the request
000cd7dc  bl   0x000d1e78        ; ldr r0,[r0]; b 0x000a0ebc -- wait for the reply
000cd7e0  ldmia sp!, {r2-r4, pc}
```

> **Retracted 2026-08-13 — the two right-hand comments are wrong, and Addendum 9 replaces them.**
> `0x0016b0b0` was never disassembled; `; issue the request` was inferred from the shape. It is a
> **task creation** (`0x0011c808("MP3ExampleTask", 0x0016b044, …)`), `0x000d1e5c` **creates** the
> semaphore rather than taking a lock, and the thing that posts `0xea` is the spawned thread itself
> at `0x0016b094`. This listing also drops `000cd7d8 ldr r0, =0x1081eaf4`. Nothing in the triple
> touches the co-processor.

`0x000d1e78` is the same counting-acquire helper that fed the `0xe0` pend. Its object is
`[0x1081eaf4] = 0x13ef29a0`, and it has the same two fields in the same shape:

```
13ef29a0  ffffffff 000000ea     count -1, sem 0xea
```

**`0xea` is never signalled**: task 9 is still `state=0x40`, pended on it, at the end of a
570 764 931-instruction run whose RTXC clock reached tick 222 060 — 422× further into simulated time
than the tick 526 at which it blocked.

### 7. The verdict: World B. The co-processor has to answer

The question this ablation existed to settle was whether RetailOS needs *the transfer acknowledged*
or *the co-processor to actually answer*. Three independent measurements say the second:

1. **Satisfying the event was not sufficient.** `0xe0` is one of at least two completion signals for
   a single 64 KB chunk. The other is a set of in-use bytes in SDRAM, polled in a busy-wait, and no
   event post of any kind can clear them — they are written by the completion path or not at all.
2. ~~**Having faked both, the very next thing `APPLEBOOT` does is send a request and wait for a
   reply.** Not a status check that could be satisfied with a plausible constant — a lock, an issue,
   and a counting wait, on a semaphore in the same family, in the same driver.~~
   **Retracted 2026-08-13 (Addendum 9).** The "issue" is `bl 0x0016b0b0`, which creates
   `MP3ExampleTask`; the wait is for that thread, which posts `0xea` itself at `0x0016b094`. This
   pillar named a call that sends nothing, and it was the only one of the three that argued the
   co-processor must *answer* rather than merely *transfer*.
3. **The one thing it did get back was ours, and it was not enough.** BCM reads rose 32 → 56 and
   nothing downstream moved: still 2 frame updates, still no new BCM writes.

So emulating the VideoCore is **not** declinable on the "it only wants an ack" theory. That theory is
now measured and dead. What is *not* yet established is how much of the VideoCore must be emulated:
nothing here shows RetailOS validating the uploaded bytes, and the request at `0x0016b0b0` may have a
small, enumerable reply vocabulary. **Between "post an event" and "run the vmcs firmware" there is a
large unexplored middle**, and the next move is to read what `0x0016b0b0` actually writes into the
`0x30000000` window and what shape of reply the acquire's other side expects.

### 8. Correction: the transfer engine at `0x60009000` IS programmed

Addendum 7 §5 said "the engine is never programmed", from a whole-run `--watch-range=0x60008000:0x8000`
that reported 222 byte-writes, all GPIO. That is reproducible — and wrong. On the **same build and
the same recipe**, `--storeaddr` over the same block reports the engine being programmed by the
command dispatcher at `0x0028e2xx`, immediately before the pend:

```
0x0028e288 -> [0x60009000] = 0x04000000  @51763466
0x0028e2ec -> [0x60009014] = 0x02000000  @51763491
0x0028e300 -> [0x6000901c] = 0x02000000  @51763496
0x0028e288 -> [0x60009020] = 0x04000000  @51763741      (and 50 more)
```

Two instruments, one run, opposite answers. The mechanism, found by running the narrow range with
and without an unrelated flag: `read32`/`write32` hoist the `count()` call — which is the only thing
that feeds `watch_range` — behind `accounting || page_log || !read_addrs.is_empty()`, and
`watch_range` is not in that list. So **`--watch-range` saw only byte writes**, unless some other
flag happened to arm the path. `write8_inner` calls `count()` unconditionally, which is why the GPIO
block (byte writes) showed up and the engine (word writes) did not — and why the "130 649 writes to
`0x60007000`" offered as a positive control proved nothing: those are byte writes too.

Fixed by naming every consumer in both hoists. Control: `--watch-range=0x60009000:0x1000` alone
reported **0** writes before the fix and **208** after, the same 208 that `--readlog` used to arm by
accident; and the baseline run is unchanged to the instruction (112 007 170, 12 009 buckets, 96 ATA
commands), so the fix is purely observational.

**This is the eighth instrument bug caught by carrying a control, and the first one caught by
disbelieving a control.** The rule that failed here is not "carry a positive control" — §5 did carry
one. It is that a control only proves what it exercises: `0x60007000` and `0x60009000` differ in
access *width*, and the instrument was width-sensitive.

### 8b. The audit this fix owed, run 2026-08-13

`NEXT.md`'s standing rule is that a new instrument's first job is to re-run the conclusions the old
one produced. Done here for the first time as a deliberate pass rather than as a side effect, over
every absence-shaped claim in `research/`, `NEXT.md` and the loader's comments. Two things it turned
up that §8 above understates:

- **`--watch-range` was not the only casualty. `input_probe` was missing from the hoist in `read32`
  as well as `write32`**, so `--input-regs` — which answers "which addresses does the firmware read
  that nothing ever wrote", the entire premise of [research/09](09-what-the-hardware-must-supply.md)
  — saw only byte accesses in both directions. It was the worse-affected of the two and nobody had
  noticed, because its output is a list of addresses rather than a claim about one.
- **The blindness is specific to *mapped regions*.** Addresses answered by a device window go through
  `read8`/`write8`, which call `count()` unconditionally, so those were always visible. That is why
  the bug survived so long: most MMIO looked fine. SDRAM, IRAM and the region-backed parts of the
  `0x60000000`/`0x70000000` blocks were where it hid.

The audit's method, worth reusing: **rebuild the pre-fix binary and run it against the current
machine with identical flags**, so instrument and machine are separated. Without that, every
difference is confounded by the second DMA controller having landed in between. Results are recorded
in place — research/09's delegate table and its stale-heap sibling are retracted, its input-register
table is superseded and its conclusion survives, research/03 §47/§48/§51/§54 carry corrections, and
Addendum 7 §6's mailbox is re-confirmed with matched controls.

## Addendum 9: the engine at `0x60009000` is a second DMA controller, and it is modelled

Measured 2026-08-13 on `retail-boot.sh --clock=5`. Two budgets are quoted and each is labelled,
because Addendum 8's ablation used the larger one and a like-for-like comparison is the point:
**600 M/40 M** is the baseline recipe, **2 G/150 M** is the ablation's.

This is **not** an ablation. It is a device model, default-on, no flag. The `--force-vc-upload` /
`--force-vc-retire` pair from Addendum 8 is not involved and is not needed.

### 1. What it is

`0x60009000` is the channel array of a **second PP502x DMA controller**, register-for-register
identical to the one Rockbox's `pp5020.h` names — and the header is once again the whole answer.

| | Rockbox's controller | this one |
|---|---|---|
| master control | `DMA_MASTER_CONTROL 0x6000a000` | `0x60008000` |
| channel array | `DMA0_BASE_ADDR 0x6000b000`, stride `0x20` | `0x60009000`, stride `0x20` |
| channels | 4 | **2** |
| completion line | `DMA_IRQ 26` | **27** |

Both are built by one RetailOS driver object at `0x001da160`, which is what identifies the
undocumented one:

```
001da164  ldr r1, [pc, #0x224]   ; 0x001da394 = 0x60008000
001da1a4  str r1, [r0, #0x20]                       <- controller A base
001da2a0  str r0, [r4, #0x30]    ; 0x001da3a4 = 0x6000a000   <- controller B base
```

and then walks each one's channels with the *same* arithmetic — `base + 0x1000 + n*0x20` — clearing
bit 31 of `+0x00` on each:

```
001da1c0  ldr r1, [r4, #0x20]        ; 001da2b4  ldr r1, [r4, #0x30]
001da1c4  add r1, r1, r5, lsl #5     ; 001da2b8  add r1, r1, r5, lsl #5
001da1c8  add r9, r1, #0x1000        ; 001da2bc  add r6, r1, #0x1000
001da1e8  bic r1, r1, #0x80000000    ;             ... same
001da214  cmp r5, #2                 ; 001da308  cmp r5, #4
```

`bic #0x80000000` is Rockbox's `DMA_CMD_START`. Two channels here, four there.

### 2. The register map, and the code that proves each field

| offset | name | evidence |
|---|---|---|
| `+0x00` | `CMD` | bit 31 `START`, bit 30 `INTR`, bit 27 `RAM_TO_PER`, bit 26 `SINGLE`, bits 15..0 `size-4` |
| `+0x04` | `STATUS` | bit 31 `BUSY`, bit 30 `INTR`, bits 15..0 remaining |
| `+0x10` | RAM address | `str r2, [r12]` at `0x0028dfd4`, r12 = `chan+0x10` |
| `+0x14` | RAM config | width in bits 30..28 (`bic #0x70000000; orr #0x20000000` at `0x0028e594`) |
| `+0x18` | peripheral address | `str r1, [r2]` at `0x0028dfe0`, r2 = `chan+0x18` |
| `+0x1c` | peripheral config | same 3-bit width field, `0x0028e5c0` |
| master `+0x00` | `MASTER_CONTROL` | bit 31 enable — `0x001d9848` writes `0x80000000` |
| master `+0x04` | `MASTER_STATUS` | per-channel bits from bit 24 up |

The size encoding is the sharpest single confirmation. Rockbox writes
`DMA0_CMD = CONFIG | (size - 4) | DMA_CMD_START`; RetailOS does the identical arithmetic in
registers at `0x0028dff8` — `sub r2, r3, #0x4`, r3 = the chunk length — and the store that lands is
`[0x60009000] = 0xcc00fffc` for a 64 KB chunk. `0xfffc = 0x10000 - 4`. Decoded, `0xcc00fffc` is
`START | INTR | RAM_TO_PER | SINGLE | (65536-4)`: every bit named by the header, none left over.

**The peripheral address does not advance.** The chunk loop at `0x00286ff4` walks the source forward
64 KB per iteration (`add r5, r5, r8, lsl #2`, r8 = `0x4000`) and re-uses one destination word it
loads from `[sp, #0xc]` and never updates. `0x30000000` is the co-processor's host *port*, which
keeps its own auto-incrementing write pointer — so a fixed peripheral address is not a convenience,
it is the only reading under which four chunks do not overwrite each other.

### 3. Why it looked unprogrammed, found independently and twice

Addendum 7 §5's "nothing is ever written to `0x60009xxx`" is retracted above; this work hit the same
wall from the other side and found the same cause. `--storeaddr` over the two blocks — which hooks
`note_store_pc` at the *top* of `write32`, before the fast-path hoist — reported 52 word stores in
the run `--watch-range` called silent, with the PCs and values quoted in §2. After the fix, the two
instruments agree: `--watch-range=0x60009000:0x80` reports **208 byte-writes** naming
`0x0028dfd4` and `0x0028dfe0`, the same stores. Two instruments, two code paths, one answer.

### 4. Three unmodelled things, not one

Modelling the engine alone moved 64 KB and then stopped. The upload needs all three, and each was
found by fixing the previous one and re-measuring:

1. **The engine.** Unmodelled, the writes landed in dead backing store, nothing moved, no completion.
2. **`STATUS` is read-to-clear.** Rockbox states it outright — the first line of its FIQ handler is
   a bare `DMA0_STATUS;` carrying the comment *"Clear any pending interrupt"* — and RetailOS agrees
   by omission: its ISR at `0x001d9be0` loads `[chan+0x04]`, tests bit 30, dispatches, and never
   writes the register back. With a latch that needed an explicit acknowledgement, that ISR
   re-entered itself **132 725 times** on one completion (`[0x60009004] read by 0x001d9bf4 x132725`),
   hit the re-entrancy guard at `0x001d9e20` every time, and starved the machine: buckets fell
   12 009 → 11 287 and ATA 96 → 90.
3. **The forced-interrupt registers.** `INT_FORCED_STAT/SET/CLR` at `0x60004014/18/1c` are named by
   `pp5020.h`, used by Rockbox nowhere, and are RetailOS's deferred-work mechanism. Its DMA ISR ends
   by writing `INT_FORCED_SET = 1 << 13` at `0x001fc840`; the handler for soft-IRQ `0x0d` at
   `0x001d9f64` then tail-branches to the completion drain at `0x001da058`. Unmodelled, that store
   fell into dead memory: the ISR ran, posted, and nothing ever collected. **This is why the upload
   stopped after exactly one chunk** — the post is per-completion, so chunk 2 was never submitted.

### 5. The completion line is 27, and that is measured

Nothing published names it. The driver object holds four candidate masks at `+0x10..+0x1c`
(`1<<24`, `1<<13`, `1<<26`, `1<<27`) and the run enables 26, 27 and 13. Sweeping all four with
`--pp-dma-irq=N` settles it: with **27** raised, the profile grows a 123 284-sample peak at
`0x001d9e20` and `[0x60009004]` acquires 132 725 reads from `0x001d9bf4` — the interrupt controller
routed line 27 into *this controller's* ISR, which read *this channel's* status. Nothing else did.
Bit 26 is Rockbox's `DMA_IRQ` for the other controller and bit 13 is the soft-IRQ of §4.3, which
accounts for three of the four masks; `1<<24` is enabled nowhere in the run and remains unexplained.

> **An instrument bug this sweep caught.** With both controllers pointed at IRQ 26, the second one's
> "nothing pending" cleared the first one's completion every service tick, and the run came out
> byte-identical to one with no DMA model at all — which reads exactly like a clean acknowledgement.
> The line is now accumulated across controllers before being applied.

### 6. Result: `APPLEBOOT` gets past the wall, on the real path

At **600 M/40 M**, versus the same build with the line masked:

| | baseline | DMA modelled |
|---|---|---|
| `vmcs.bin` uploaded | 0 | **4 chunks, 201 216 bytes** — `0x13eaf188`, `+0x10000`, `+0x20000`, `+0x30000` |
| BCM halfwords written | 129 876 | **230 572** |
| `KS_pend(0xe0)` | 1, forever | **4, each satisfied** |
| drain barrier `0x00159b88` entered | 1 (spins) | **4 (returns)** |
| in-use ring at `0x13ee0824+0x18` | `[2,0,0,0]` | **`[0,0,0,0]`** |
| tasks in the TCB array | 45 | 52 |
| ATA commands | 96 | 119 |
| code buckets | 12 009 | 22 177 |
| unmapped | 0 | 4 reads, 1 page |

At **2 G/150 M**, the budget Addendum 8's ablation used, against that ablation:

| | baseline | ablation (bypass #17) | DMA modelled |
|---|---|---|---|
| tasks | 45 | 62 | **62** |
| ATA commands | 96 | 256 | **256** |
| ATA DMA | 72 / 8 113 664 B | 302 / 15 658 496 | **302 / 15 658 496** |
| code buckets | 12 009 | 27 904 | **27 985** |
| BCM halfwords written | 129 876 | 129 964 | **230 572** |

The device model reproduces the ablation **exactly** on every count the ablation measured, with no
lie told — and the last row is what the ablation could not do: the co-processor is actually handed
its firmware. The 81 extra buckets are the DMA driver's own ISR and completion path, which the
ablation bypassed by construction.

**`0x00284538` is still not reached.** `APPLEBOOT` blocks in `KS_pend` on **`0xea`**, and its stack
is `0x000cd7e0 · 0x000d05d0 · 0x00284530` — inside `bl 0x000d05c0` at `0x0028452c`, its **last**
call, one instruction from the branch that would terminate it. That is the identical wall Addendum 8
§6 reached from the ablation, reached here honestly, and it is now the frontier.

### 7. Negative results, recorded because they were predicted

- **The COP is not required for this.** The brief's live hypothesis was that the second core
  programs an engine the CPU never touches. The CPU programs it itself, in full, 1 200 instructions
  before the pend. `wake_cop` and bypass #7 are untouched by this work and remain open on their own
  merits — but they are not implicated in the `vmcs` upload, and no part of this needed them.
- **The inter-processor mailbox stays untouched**, exactly as Addendum 7 §6 measured. Nothing here
  changed that, which is the expected result now that the transfer has a modelled owner.
  Independently re-verified 2026-08-13 with width- and region-matched positive controls; see the
  note under that bullet.
- **Modelling both completion paths was not necessary.** Addendum 8 needed a second ablation
  (`--force-vc-retire`) to clear the four in-use bytes at `channel+0x18`, because faking the event
  bypasses the driver that owns them. A real engine needs no equivalent: RetailOS's own ISR does its
  own bookkeeping, and the ring drains to `[0,0,0,0]` without the emulator touching it. **This is the
  argument for modelling hardware over short-circuiting software, stated as a measurement.**
- **The 160 bytes are not missing.** 201 216 uploaded against a 201 376-byte file looks short by 160.
  It is not: the tail path at `0x00287068` rounds the remainder *up* to 16 bytes
  (`add r0,r4,#0xf; asr #4; lsl #4`), so a 4 768-byte remainder would have transferred 4 768. It
  transferred 4 608, which means the driver was handed 201 216. The model moved exactly what it was
  asked for; the 160-byte difference is between the file and the request, and is unexplained.
- **A new unmapped access appeared**, 4 reads at `0xea00007a` from `pc 0x000a0bd0`, `lr 0x002893f0`.
  The address is a `b` instruction word read as a pointer — an uninitialised vtable on a path that
  never used to execute. Four reads, no writes; noted, not chased.

## Addendum 10: `0x0016b0b0` is not a request — it starts a thread, and the co-processor vocabulary is 26 verbs

Addendum 8 §6 read the triple at `0x000cd7c8` as "take lock, **issue a request**, wait", and §7 built
the World-B verdict on it: *"the very next thing `APPLEBOOT` does is send a request and wait for a
reply."* The middle call was never disassembled — the comment `; issue the request` was an inference
from the shape, and the shape was wrong.

`0x0016b0b0` is a **task creation**. Nothing in that triple touches the co-processor.

### 0. The instrument, and what it verifies itself against

Reading forty instructions cost a 600 M-instruction boot, because `trace --disasm` can only show
code a *run* has already placed in memory. The firmware image is flat — `OSOS_correct.bin` lands
byte-identical at `0x10000000` and is mirrored at 0 — so `tools/eapp-loader/src/bin/dis.rs` answers
every static question from the file, in milliseconds, sharing `arm7tdmi::disasm` with the
interpreter so the two can never disagree about what an encoding *is*.

`dis --verify` reproduces the six instructions Addendum 8 §6 recorded **from a live run**:

```
  ok   000cd7c8  e92d4013  stmdb sp!, {r0, r1, r4, lr}
  ok   000cd7cc  e59f0010  ldr    r0, =0x1081eaf4
  ok   000cd7d0  eb0011a1  bl       0x000d1e5c
  ok   000cd7d4  eb027635  bl       0x0016b0b0
  ok   000cd7dc  eb0011a5  bl       0x000d1e78
  ok   000cd7e0  e8bd801c  ldmia sp!, {r2-r4, pc}
  self-check PASSED
```

Same file-offset→address mapping, same decoder, same literal-pool base as every answer below, on the
exact code path under investigation. It also recovers the instruction §6 dropped —
`000cd7d8  ldr r0, =0x1081eaf4`, the reload of the object pointer before the wait.

### 1. What the three calls actually are

```
0016b0b0  stmdb sp!, {r1-r3, lr}      ; three words of outgoing stack arguments
0016b0b4  ldr  r0, =0x1081eaf8
0016b0b8  mov  r1, #0x18
0016b0bc  bl   0x001d320c
0016b0c0  bl   0x001d2e44
0016b0c4  ldr  r0, =0x1081daa4
0016b0c8  mov  r3, #0x64             ; 100
0016b0cc  ldr  r0, [r0, #0x0]
0016b0d0  mov  r2, #0x1
0016b0d4  mov  r1, r0, lsl #4
0016b0d8  stmia sp, {r1-r3}
0016b0dc  ldr  r1, =0x0016b044       ; the task body
0016b0e0  mov  r3, #0x10
0016b0e4  mov  r2, #0x0
0016b0e8  add  r0, pc, #0x14         ; -> 0x0016b104 = "MP3ExampleTask"
0016b0ec  bl   0x0011c808            ; create it
```

and the two neighbours are a matched pair on one object:

| | | |
|---|---|---|
| `0x000d1e5c` | `bl 0x000a0c38; str r0, [r4]` | **create** a semaphore, store the handle at `0x1081eaf4` |
| `0x000d1e78` | `ldr r0, [r0]; b 0x000a0ebc` | **pend** on it |
| `0x000d1e70` | `ldr r0, [r0]; b 0x000a0c84` | **post** it |

So `0x000cd7c8` is not lock/request/wait. It is **create-semaphore / spawn-thread / wait-for-thread**
— an ordinary start-up rendezvous, entirely inside RetailOS.

And the poster is the spawned thread itself. `0x0016b044`, the body handed to the create call, ends:

```
0016b090  ldr  r0, =0x1081eaf4       ; the same object APPLEBOOT is pended on
0016b094  bl   0x000d1e70            ; post it
```

**`0xea` is signalled by ordinary ARM code, not by the co-processor.** The correction to §7 is not
a nuance: the third of its three pillars — *"the very next thing it does is send a request and wait
for a reply"* — names a call that sends nothing.

### 2. ~~Where MP3ExampleTask actually stops, measured to the instruction~~

> **RETRACTED 2026-08-13 by Addendum 11. Every "NEVER" in this section is an artefact of the stop
> condition, not a property of the firmware.** `0x001ebe9c` returns; phase 1 returns; all 24 registry
> entries run; all five phases run. What ends the run 34 M instructions early is `--stop-when-idle`,
> whose test is *"no code bucket executed for the first time in N instructions"* — a **novelty**
> test, not a CPU-halt test. The call chain under `0x001ebe9c` ends in a bounded 65 536-iteration
> scan over code it has already run once, which by construction produces no novelty and reads as
> idle. The measurements below are all reproducible and all correctly report what happened *inside
> the window*; only the word "never" is wrong. See Addendum 11 for the matched control.

`--enterlog`, positive control carried at every step (each run watched an address a previous run had
already measured as reached, in the same task, on the same recipe):

```
0x0016b044  entered            the task body runs
0x0016b080  bx r1, r1=0x001d28e0   the virtual call, target as read statically from vtable 0x0066a510+0x1cc
0x001d28e0  entered            five startup phases, called in sequence
0x001d28f8  bx r1, r1=0x001d2ae8   phase 1 of 5
0x001d2ae8  entered
0x001d28fc  NEVER               phase 1 never returns
```

`0x001d2ae8` walks a 24-entry registry at `[0x1081da80+4]`, stride 12, calling `entry+8`:

```
0x001d2afc  x11   loop body        count r0 = 0x18 = 24
0x001d2b14  x11   about to call    0x0019e82c 0x001935b0 0x0012cc70 0x0013ac90 0x002393d8
                                   0x001ba3e4 0x0014b72c 0x0023e230 0x001c1e04 0x0017fa3c
                                   0x001d8dd8
0x001d2b1c  x10   returned
```

~~**Ten of twenty-four complete; the eleventh does not return.**~~ **Eleven of twenty-four complete
*within this window*; all 24 complete when the window is widened** (Addendum 11). `0x001d8dd8`
allocates 0x134 bytes and tail-calls the constructor `0x001d9430`, and bisecting that constructor's
twelve return points in one run puts the *window's edge* on a single instruction:

```
001d94a0  str  r0, [r4, #0x18]     reached   @53396440
001d94a4  ldr  r1, [r4, #0x128]
001d94a8  bl   0x001ebe9c          <- returns @157277834, 34 M instructions past this run's stop
001d94ac  bl   0x0019ec64          reached @157277835
```

~~It is not a busy-wait: the machine reaches `Idle after 123 466 093 instructions`, so the task is
blocked~~ — **it *is* a busy-wait, and this sentence is the error that cost a day.** `Idle` names a
novelty stall; the machine was executing at full rate. The second half stands and was the clue:
it is *not* blocked on `KS_pend` at `0x000a6924` — a whole-run log of that address shows no new pend
site after `0x001d9430` is entered. Nothing blocked it, because it was not blocked.

### 3. The vocabulary: 26 verbs, and only 4 issued in a whole boot

RetailOS *does* speak GENCMD to the BCM, through three varargs wrappers that all funnel into
`0x000e9358(format, va_list)`. Their ABIs differ and the difference matters: `0x002874f0` takes the
format in **r0** (send, no reply), while `0x00287664` and `0x00287110` take **r0 = reply buffer,
r1 = its size, r2 = format** — a register-blind backward scan for "the nearest string-shaped literal"
reports the `mode=%s` *argument* `"fill"` at `0x00163bc4` as if it were a command. `dis --callfmt`
is spelled `TARGET:REG` for that reason.

38 call sites. Every command string in the image, swept independently by prefix:

| # | command | # | command |
|---|---|---|---|
| 1 | `audio_enable %d` | 14 | `mp_seek %d` |
| 2 | `display_control %d dac=0` | 15 | `mp_selectplay passthru:%s %d` |
| 3 | `display_control %d dac=1 encoding=%d` | 16 | `mp_setvol 0` |
| 4 | `display_control %d freeze=%d` | 17 | `mp_step %d` |
| 5 | `display_control %d model=%d power=%d` | 18 | `mp_stop` |
| 6 | `display_control 2 dac=0` | 19 | `pm_set_policy min` |
| 7 | `end_application %d` | 20 | `pm_show_stats 0 10 10 90 16` |
| 8 | `load_application %s` | 21 | `power_control videocore %d` |
| 9 | `mp_get_stats audio` · `mp_get_stats video` | 22 | `power_management get_info` |
| 10 | `mp_get_status` | 23 | `power_management set_policy manual 27` |
| 11 | `mp_pause` | 24 | `set_vll_dir %s` |
| 12 | `mp_play` | 25 | `ss_get_status` · `ss_region` · `ss_selectplay` · `ss_stop` |
| 13 | `mp_region display=%d dest=…` (3 spellings) | 26 | `ss_tranprop %s %s` · `ss_trantime %ld` · `ss_trantype %s` |

**35 distinct format strings, 26 distinct verbs.** All of them ASCII text with `%d`/`%s`/`%ld`
substitutions into a caller-supplied reply buffer of 0x20 or 0x100 bytes. There is no verb here that
requires computing anything — the largest reply any caller has room for is 256 bytes of text.

And across a full ablated boot, **four are issued**, cross-checked by two independent counters
(`--enterlog` on the wrappers, and the BCM model's own `commands kicked`):

```
0x001afc2c  set_vll_dir <path>                      @51835849
0x00164ae8  "display_control %d freeze=%d"          @51836642
0x00164eec  "display_control %d model=%d power=%d"  @52241732
0x001b0190  pm_set_policy min                       @65103159
```

**All four return `r0 = 0xffffffff`, and RetailOS carries on.** `0x000e9358` early-outs with −1 when
`[0x108235d8+4] < 0` — the channel-not-open path — and the wrapper only reads a reply on success, so
`0x002872c0` is never entered. The OS tolerates total GENCMD failure without blocking. **Whatever
stops the boot at `0x001ebe9c` is therefore not a missing GENCMD reply.**

### 4. The reply mechanism is `ipodloader2`'s, exactly

`0x00287a6c` is the BCM read path, and it is the `fb.c` idiom instruction for instruction:

```
00287a94  ldrne r6, =0x30060000   / 00287a98  ldreq r6, =0x30020000   address port, per channel
00287aa4  ldreq r5, =0x30030000                                        status port
00287aac  moveq r4, #0x30000000                                        data port
00287ab8  ldrh  r0, [r6] ; tst #1 ; beq -8                             bit 0 = ready
00287ac4  strh  r10, [r6] ; mov r0, r10 lsr #16 ; strh r0, [r6]        address, low half then high
00287ae8  ldrh  r2, [r5] ; tst #0x10 ; beq -8                          bit 4 = read-ready
00287af4  ldrh  r2, [r4]                                               data
```

and the write path at `0x00287698` polls `0x30070000` bit 7 (busy) then bit 6 (ready). The run's own
latch log shows the two-halves-low-then-high write of address `0x000001f8`, and the read histogram —
**with its 6-row cap removed, 56 of 56 reads now accounted for** — ~~lands entirely inside the command
block~~ **lands 48 of 56 inside the command block; the other 8 are the two words at
`0x10000c00`, as the very listing below shows** (corrected 2026-08-14, Addendum 14 §5):

```
internal reads: 20 distinct offsets, 56 of 56 accounted for
  0x000001f0..0x0000020e   (0x1f8 x5, 0x1fc x9, 0x200 x5, the rest x1)
  0x10000c00..0x10000c06   x2 each
```

`0x1F8` polled and `0x1FC` read to acknowledge is precisely the documented sentinel/ack pair. The one
difference from `ipodloader2` is the block's base: RetailOS uses **0** where the loader used
`0xE0000`, and `0xE0000` is where `vmcs.bin` is uploaded. The reply envelope carries a magic
`0xf1a55a1f`, checked at `0x00287360` against the first word of a 16-byte header.

### 5. ~~The uploaded bytes are never read back~~ — the regions overlap, so this cannot be measured as stated

> **PARTIAL, corrected 2026-08-14 (Addendum 14 §5). The conclusion is probably right; the
> measurement offered for it is not, and it contradicts its own quoted numbers.** The claim below is
> "not one of the 56 reads falls in either write run", where one of the write runs is
> `0x00000000..0x0001787e`. Every one of the 48 command-block reads is at `0x1f0..0x20e`, which is
> **inside** that range. The BCM model holds one flat internal address space (`Bcm::mem`, a single
> `BTreeMap`), so the protocol block at `BCMA_COMMAND 0x1f8` / `BCMA_STATUS 0x1fc` sits *within* the
> upload's target range — on the current baseline that run is `0x00000000..0x0003129e`, all 201 376
> bytes of `vmcs.bin`. Read-back and reply-polling are therefore not separable by address here.
> **What is measurable, and true:** no read falls in `0x000e0000..0x0010581e`, and every read outside
> the documented `0x1f0..0x20f` block is one of the 8 at `0x10000c00..0x10000c06`. Also note the two
> write runs below are **labelled the wrong way round** — `0x000e0000` is the framebuffer and
> `0x00000000` is the upload, as Addendum 12 §1 has it.

The write runs cover `0x000e0000..0x0010581e` (153 632 bytes of `vmcs.bin`) and
`0x00000000..0x0001787e` (96 384 bytes). ~~**Not one of the 56 reads falls in either.**~~ This was
briefly unprovable: the report printed a 6-row histogram summing to 38 directly under a header
saying 56, and the 18 missing reads are exactly where a read-back would have hidden. Fixed to print
every offset with a reconciliation line — the same instrument, same region, same access width, now
complete.

### 6. Verdict: the co-processor is a week, and it is not what is blocking

Addendum 8 §7's World-B conclusion stands only in its weakest form. Of its three pillars, the third
is retracted outright: `0x0016b0b0` sends nothing. What survives is that the *transfer* has two
completion signals and our BCM model satisfies neither on its own — a transfer-engine problem, not a
firmware-execution one.

Against that, everything measured here points the other way. The vocabulary is 26 text verbs with
≤256-byte text replies; four are issued in an entire boot; **all four fail and the OS does not
care**; the uploaded firmware is never validated; and the protocol is the one already documented in
`ipodloader2`. Nothing observed requires the VideoCore to run a single VLIW instruction.

~~**The boot is blocked at `0x001ebe9c`, eleven entries into a 24-entry registry, and that address has
no co-processor involvement.**~~ **Corrected 2026-08-13 (Addendum 11): the boot is not blocked there
at all — `0x001ebe9c` returns, all 24 registry entries run, and the real wall is four levels further
on, at `KS_pend(0xd1)` inside a recursive view-tree builder.** The load-bearing half of this section
is unaffected and is confirmed by that progress: it still has **no co-processor involvement**, and
chasing Nucleus PLUS would still be chasing something that is not in the way.

## Addendum 11: `0x001ebe9c` returns — "never returns" was the stop condition, not the firmware

Measured 2026-08-13 on `retail-boot.sh --clock=5`. **No flag, no ablation, no model change**: the
same binary, the same disk, the same budget. The only variable is `--stop-when-idle`.

### 1. The matched control

Both runs are `BUDGET=600000000 --clock=5` on the same build, differing in one number:

| watched | `--stop-when-idle=40000000` | `--stop-when-idle=400000000` |
|---|---|---|
| how the run ended | `Idle after 123 469 613` | `BudgetExhausted after 599 999 952` |
| `0x001d2b14` — registry entries dispatched | **11** | **24** |
| `0x001ebe9c` entered | 1 | 1 |
| `0x001ebf5c` — its own `ldmia sp!, {r4, pc}` | **0** | **1** |
| `0x001d94ac` — the caller's next instruction | **0** | **1** |
| ATA commands | 119 | 256 |

`bl 0x001ebe9c` is entered at @53 402 622 and **returns at @157 277 834** — 103.9 M instructions
later, and 33.8 M instructions past the point where the baseline recipe stops looking. Nothing about
the firmware differs between the two columns. This is the whole finding.

### 2. Why a run that is executing at full rate reports `Idle`

`--stop-when-idle=N` ends the run when **no code bucket has executed for the first time** in N
instructions. That is a *novelty* test. It was built for the opposite problem — a booted RetailOS
spends four fifths of a long run in its idle loop — and for that it is exactly right. It cannot
distinguish "the machine is waiting" from "the machine is grinding through a long loop over code it
has already run", because neither produces novelty.

Bisecting `0x001ebe9c` one level at a time lands on a loop of precisely that shape. The chain, each
step measured by watching the instruction after every `BL` in the body and reading which ones arrive:

```
0x001d94a8  bl 0x001ebe9c    -> 0x001ebeb4  bl 0x000a2310   (49-instruction init, 26 calls)
0x001ebeb4  bl 0x000a2310    -> 0x000a23fc  bl 0x000b4a64   (allocates 0x2d30, builds a subsystem)
0x000a23fc  bl 0x000b4a64    -> 0x000b4f08  bl 0x00112194   (337 instructions, 58 call sites)
0x000b4f08  bl 0x00112194    -> tail `bx` through vtable+0x1c0 = 0x001d04d4
0x001d04d4  bl 0x0019e79c    -> 0x0019e810  bl 0x002102a4
0x002102a4                   -> six instructions and a tail `b 0x000ff2ec`
```

and `0x000ff2ec` is a scan over a 0x10000-bit bitmap: index 0…0xfffe, and every iteration re-scans
0x800 words of a second bitmap from word 0 looking for the first non-zero. It costs ~3 400
instructions per iteration and ~226 M instructions in total, all inside code the run walked once at
@83 465 765. `--profile-window=100000000:123469613` puts **68 % of the run's last 23.5 M
instructions** in two 16-byte buckets:

```
0x000ff500   45.3%  166123      the scan's  bne / add / cmp / bne
0x000ff4f0   22.7%   83420      the scan's  ldr / cmp
```

2:1, which is 4 instructions to 2 — the loop, and nothing else.

The TCB dump agrees and would have said so for free: task 48 is **`state = 0x0`** — runnable — with
`tick = 20856`, the highest in the system, and resume PC `0x000ff500`, an instruction inside that
loop. Every genuinely blocked task in the same dump is `state = 0x40` with a resume PC of
`0x000a694c`. **A `state = 0x0` task with the highest tick in the array is the machine telling you it
is busy.** That distinction was available in the dump Addendum 10 already had.

### 3. The instrument fix, and its control

`Stop::Idle`'s doc comment has always said "the machine is still running"; the *printed* line said
`-> Idle after 123469613 instructions`, and that is what got read as a halt. The printer now names
which of the two it was, using the one number that separates them: a machine that is waiting asks the
core to sleep, so **zero CPU sleeps across the trailing window means it was busy**.

```
A  600 M / idle 40 M:   last new code @83469613;  40000000 instructions since,      0 CPU sleeps
                        <- BUSY, not blocked: raise --stop-when-idle
B  2 G  / idle 400 M:   last new code @871809167; 400000000 instructions since, 871501 CPU sleeps
```

Matched pair, same build, same recipe, differing only in the window: the run that produced the wrong
conclusion reports **0**, the genuinely quiescent one reports **871 501**. The baseline is unchanged
to the instruction — 119 ATA commands, 4 DMA transfers / 201 216 bytes, idle @123 469 613, 4 unmapped
reads at `0xea00007a` — so the change is purely observational.

### 4. What the boot actually does now

At `BUDGET=2000000000 --stop-when-idle=400000000`, with **no ablation and no new device model**:

| | 600 M / 40 M | 2 G / 400 M |
|---|---|---|
| module-registry entries run | 11 of 24 | **24 of 24** |
| startup phases of `0x001d28e0` | 1 of 5 entered | **5 of 5 entered, 4 returned** *(now 5 — below)* |
| tasks in the TCB array | 52 | **62** |
| ATA commands | 119 | **256** |
| ATA DMA | 8 113 664 B | **15 658 496 B** |
| code buckets | 22 177 | **28 028** |
| RTXC tick at the end | 32 861 | **580 435** |
| `APPLEBOOT` reaches `0x00284518` | no | **yes, @51 844 657** — and `0x0028452c`, its last call |

`vmcs.bin` still uploads in 4 chunks / 201 216 bytes and the four GENCMDs still return `0xffffffff`,
unchanged. **This is the same 62-task machine Addendum 8 §5 reached by ablation and Addendum 9 §6
reached by modelling the DMA controller — reached here by doing nothing at all except looking
longer.**

> **The phase row is now 5 of 5 returned, re-measured 2026-08-14 on the post-`55854a4` machine** —
> R4, because the model moved under it. `0x001d28e0` dispatches its five phases through vtable slots
> `+0x1d0`/`+0x1d4`/`+0x1d8`/`+0x1dc`/`+0x1e0` at `0x001d28f8`, `0x001d290c`, `0x001d2920`,
> `0x001d2938`, `0x001d294c`, and returns by a tail `b 0x001403f4` at `0x001d2954`.
> `--enterlog` on all five dispatch sites plus `0x001d2950` (phase 5's return address), on the 4 G
> baseline with `0x0016b044` live as the control, gives **x1 at every one of the six** — so the fifth
> phase returns too and the whole startup driver runs to its tail call. Same run:
> `0x001d2b14` — the module-registry dispatch — **x24**, all from `lr=0x000a6450`.

### 5. The real wall: `KS_pend(0xd1)` in a recursive view-tree builder — *"a genuine block" is WRONG; see Addendum 15*

> **Retracted in place 2026-08-14.** The semaphore and the stack below are right, and so is "this is
> not a repeat of §2". Everything that reads as *deadlock* is wrong: `0x0018942c` is entered **272**
> times in a 600 M run and **325** by the 1.27 G idle stop, and `0xd1` is signalled **21** times, not
> once. It is a **retry loop** on an ATA `WRITE DMA` our disk refuses, sampled at the one point it
> spends 99.9 % of each cycle. Two one-line changes each clear it. See Addendum 15.

Task 48 — `MP3ExampleTask`, priority 52 — is `state = 0x40`, `KS_pend`, **semaphore `0xd1`**, at
tick 1 138 789 of a 2 371 809 167-instruction run. Its stack, walked for BL-preceded return
addresses, is the same six-deep chain in both the 2 G and the 6 G dump:

```
0x0016b044  MP3ExampleTask body
0x001d28e0  five startup phases
0x001d293c  <- phase 4 = vtable+0x1dc = 0x0016b148, entered @250 208 950, does not return
0x0016b16c  <- bl 0x0019dad8, vtable+0x9c of the 0x0066fa6c singleton
0x0019db14  <- bl 0x0021a4f4 at 0x0019db10                     ^ 'View' / "****" literals
0x00141b34 · 0x002020d4 · 0x0017dafc · 0x002020d4 · 0x0017dafc · 0x0016c6ac
            six nested `bl 0x0021a4f4`, each re-entered through 0x0017e658
0x00143158  <- bl 0x0018942c   = `add r0,r0,#0x5c; b 0x000d1e78` — pend on the object's own semaphore
0x000a694c  KS_pend
```

The matching post is `0x00189444`: it clears `[this+0x4a]`, and **only if `[this+0x3c]` is zero**
does it `add r0,r0,#0x5c; b 0x000d1e70` — post the same semaphore; otherwise it `bx`es to
`[this+0x3c]` as a completion callback instead. Two call sites in the whole image, `0x00189558` and
`0x0018a364`, and the surrounding literals are `"free"` / `"busy"` / `"Type"` / `"Str "` — a
request/completion rendezvous on a pooled resource, entirely inside RetailOS.

`0xd1` **is** signalled at least once, at @76 240 652 from `lr=0x0018955c` — that same
`bl 0x00189444`. So the mechanism works; what has not happened by @2.37 G is one more completion.

This one is a genuine block and not a repeat of §2, and the discriminator is the point: task 48 is
`state = 0x40` with resume `0x000a694c`, and the 1.5 G-instruction trailing window carries **871 501
CPU sleeps**. Widening the budget from 2 G to 6 G moves nothing — `0x0019db14`, `0x0016b084`,
`0x0016b094` and `0x00284538` are all still unreached at 2 371 809 167 instructions, and
`APPLEBOOT` is still pended on `0xea` at tick 527, waiting for the `MP3ExampleTask` that is waiting
on `0xd1`.

### 6. The rule this cost

Addendum 8 §8 learned *"a control only proves what it exercises."* This is its twin, and it is about
the **stop condition** rather than the instrument: **a measurement window is part of the measurement,
and "never" is a claim about the window until the window is varied.** Four published conclusions —
"phase 1 never returns", "ten of twenty-four complete", "`bl 0x001ebe9c` never returns", "some other
RTXC primitive owns it" — are all a single unvaried parameter. The cheapest possible control was to
run the same command twice with a different `--stop-when-idle`, and it takes five seconds.

`NEXT.md`'s flag table said *"use 40 000 000 or more … a smaller value truncates the boot silently"*.
That was right and it was not enough: **40 M is itself too small**, and the failure mode it warns
about is exactly the one it then walked into. The table now says so.

---

## Addendum 12: ~~RetailOS's own framebuffer~~ — the frame is the boot ROM's, and this section is retracted

> **RETRACTED 2026-08-14 by the Addendum 14 audit. The framebuffer is not RetailOS's, the two frame
> updates are not RetailOS's, and the section title was wrong.** The control this section carried
> (research/03 §10's `diag` dump) proves the *readout geometry* and nothing about *authorship* — it
> is the third time in this project that a control has been believed past what it exercises. The
> control it needed costs one command: dump the same framebuffer at the ROM→RetailOS handoff.
>
> ```
> --stop-at=0x10000000:1  -> StopPoint after 46 397 133 instructions
>   bcm: 4 commands kicked, 2 frame updates
>   bcm: 129876 halfwords written, 32 read
>     0x000e0000..0x0010581e   76816 halfwords   <- the whole framebuffer, already written
>   bcm dump 0x000e0000 320x240 -> fb-rom.ppm    (2922 non-zero pixels of 76800)
>
> full 600 M baseline
>   bcm dump 0x000e0000 320x240 -> fb-full.ppm   (2922 non-zero pixels of 76800)
>
> cmp fb-rom.ppm fb-full.ppm  ->  byte-identical, 230 415 bytes
> ```
>
> Both frame updates, all four kicked commands and every one of the 76 816 halfwords at `0x000e0000`
> are written **before RetailOS executes a single instruction**, and RetailOS does not change one
> pixel in the following 553 M. The 2 922 antialiased pixels §2 describes are the boot ROM's logo —
> which is exactly what **Addendum 6 said**, and this section retracted it without a control.
> RetailOS's whole contribution to `0x30000000` is the `vmcs.bin` upload: 100 696 halfwords into
> `0x00000000..0x0003129e`, and 24 reads.
>
> **What survives:** §1's *numbers* (they are reproduced to the halfword), and the observation in §4
> that a stalled view-tree builder predicts a blank panel — which is now unopposed, since the panel
> is blank rather than fragmentary. **What does not:** "RetailOS reaches the display", "Two frame
> updates kicked *by RetailOS*", "the host side of the display path is live under RetailOS", and §2's
> attribution of the strokes.

Measured 2026-08-13, immediately after Addendum 11 raised the stop window. `retail-boot.sh
--clock=5 --stop-when-idle=400000000 --bcm-dump=0xE0000:140:F0:...`, budget 600 M.

### 1. ~~RetailOS reaches the display~~

```
bcm: 4 commands kicked, 2 frame updates
bcm: 230572 halfwords written, 56 read, 177508 internal words held

internal write runs (largest first):
  0x00000000..0x0003129e  100688 halfwords (201376 bytes)   <- vmcs.bin
  0x000e0000..0x0010581e   76816 halfwords (153632 bytes)   <- 320 x 240 x 2 = 153600
  0x10000400..0x10000402       2 halfwords
  0x10000c00..0x10000c02       2 halfwords
```

Two frame updates kicked *by RetailOS*, and a framebuffer-sized write run at exactly the address
Rockbox documents. Every previous frame this project has produced came from `diag`
([research/03](03-rtxc-and-the-video-coprocessor.md) §10) — a self-contained flash image, no OS.

### 2. What is in it

2 922 non-black pixels of 76 800 — white with an antialiasing ramp (`0xefebef`, `0xf7f3f7`,
`0xe7e3e7`, `0xd6d3d6`), arranged as a few near-horizontal diagonal strokes across the top ~20 rows.
Not a screen. Not noise either: antialiased greys are what a text or vector rasteriser emits, and a
solid fill or a scan-out of garbage would look like neither.

> **Answered 2026-08-14, and §3 below is right about the instrument and wrong about the stride.**
> These 2 922 pixels are **the Apple boot logo**, 62 wide and 78 tall, lying unplaced in the
> co-processor's command-parameter buffer along with the 8-word rectangle header that says where it
> goes. The stride *is* wrong — 62 halfwords where the panel wants 320 — but the wrongness is not
> the instrument's, and §3's control correctly rules the instrument out. It is the **model's**: the
> co-processor was handed a `LCD_UPDATERECT` command and this model executed nothing. Refold the
> same bytes at 62 and the logo is legible; implement the command and it lands centred at
> (129,81)-(190,158), scoring **2 916** — the six missing being the header's own non-zero words.
> [research/14](14-the-apple-logo.md).

### 3. The control — this rules out the explanation that would have made it worthless

Diagonal streaks in a framebuffer read out at the wrong stride are the classic instrument artefact,
and after four instrument bugs in this project that is the first hypothesis to kill, not the last.

**It is killed by an existing measurement, at no cost.** research/03 §10 dumped `diag`'s framebuffer
from **the same address, the same 320×240 geometry, and the same `--bcm-dump` code path**, and got
legible four-language text with 71 685 non-zero pixels. The readout is therefore proven correct at
this geometry by a positive control that predates the question. What lands here is what the firmware
wrote.

### 4. It agrees with the wall, which is the part worth keeping

Addendum 11 puts `MP3ExampleTask` in `KS_pend` on `0xd1`, six frames deep in a **recursive view-tree
builder** in startup phase 4. A view tree that never finishes building is a screen that never
finishes compositing — so fragments of stroke geometry on an otherwise black panel is precisely the
image that blocker predicts. Two independent measurements, one prediction.

**What this is not.** It is not "the display works", and it is not evidence about bypass #6: the
co-processor's replies are still synthesised, and ~~56 of 56~~ **48 of 56** internal reads remain in
`0x1f0..0x20f` — the other 8 are at `0x10000c00..0x10000c06`, and the upload's own write run
*contains* the command block, so "never reads back a byte of what it uploaded" is not separable by
address (Addendum 10 §5's correction). ~~It establishes one thing — the host side of the display
path is live under RetailOS, not only under `diag`.~~ **It establishes nothing about RetailOS's
display path: the frame is the boot ROM's, byte for byte. See the retraction at the head of this
addendum.**

---

## Addendum 13: two "unmodelled" devices are already being driven, blind

Measured 2026-08-13 on the corrected recipe (`--stop-when-idle=400000000`). Both of these were
filed as *"unmodelled, not blocked, do later."* Neither description survives.

### 1. `OptoTask` is created and dispatched — the entry it was said not to have

Addendum 7 §6 recorded *"`OptoTask` is declared and never created … no TCB has either"* and set it
aside as "not on the display path." `--enterlog=0x00284f90`, with `0x0016b044`
(`MP3ExampleTask`'s body, known-reached) carried as a positive control in the same run:

```
0x00284f90  lr=0xeeeeee13  r0=0x00000013 r1=0x11111113 r2=0x22222213 r3=0x33333313  @49678867
0x0016b044  lr=0x000e1b2c  r0=0x00000001 r1=0x13e6ae18 r2=0x0016b044 r3=0x20000093  @52249160
```

The `0xeeeeee13` link register and the `0x11111113`/`0x22222213`/`0x33333313` argument fill are
RTXC's **task-entry pattern**, not a call frame: this is a task being dispatched, not a function
being invoked. `OptoTask` runs, 2.6 M instructions *before* `MP3ExampleTask` does.

`DEV_OPTO` is the click wheel's enable bit ([research/05](05-the-chip-inventory.md) §"Click wheel").
So RetailOS's click-wheel task is executing against `0x7000c140`, which we answer as **zero** —
a wheel that is permanently untouched, never held, and at position 0.

### 2. The audio codec is being configured, and the address had us looking for the wrong count

`--registry` on the same run:

```
i2c: 3704 transfers logged, by device address:
  dev 0x10  1829     dev 0x11  1823     dev 0x34  52
  dev 0x34 reg 0x54 x5 · reg 0x6f x4 · reg 0x06 x3 · reg 0x6b x3
```

**`0x34` is the WM8758.** The bus log reports the 8-bit address; `0x34 >> 1 = 0x1a`, which is the
codec's documented 7-bit address, and `0x10`/`0x11` are likewise the PCF50605 we already model.
NEXT.md's "432 transfers" was a different run's figure at a different address encoding; on this
recipe it is **52**, and the register numbers are the interesting part — a real map to check a model
against, for free, on a boot we already run.

### 3. Why this changes the queue rather than lengthening it

An unexercised device model cannot be verified, and this project has been burned four times by code
that looked correct because nothing exercised it. That objection is the reason to defer the COP and
USB — nothing asks for them, so a model would be unfalsifiable the day it was written.

**It does not apply to these two.** Both are already driven by live firmware, which means a model
gets a verdict on the run that already exists. They are the only two devices in the inventory with
that property today.

Whether either feeds the `0xd1` blocker is **untested** — no claim is made here that it does.

---

## Addendum 14: the truncation audit — 23 absence claims re-measured, 9 wrong, 2 partial

Run 2026-08-14. `--stop-when-idle=40000000` was, in effect, this project's default measuring recipe,
and Addendum 11 showed it ends every run at ~@123 M — a *novelty* test reading a bounded loop as
idle. Addendum 11 named four conclusions it had cost. This is the deliberate pass over the rest, on
the standing rule that a fixed instrument's first job is to re-run the conclusions the old one
produced.

**Baseline for every number below**, verified before starting and unchanged after:

```
BUDGET=600000000 ./tools/ipod-boot/retail-boot.sh --clock=5 --stop-when-idle=400000000
  -> BudgetExhausted after 599999952        ata commands: 256   <- the LOG CAP, not a count (Add. 15 §7)
  pp dma: 4 transfers, 201216 bytes         unmapped: 4 reads, 0 writes across 1 pages
  cargo test --release: 15 passed (3 lib + 12 integration)
```

Every measurement in this pass was taken on one build. `lib.rs` and `trace.rs` were being edited by
other work while it ran, so the baseline was re-run on the changed tree at the end: the two logs are
**byte-identical**, 599 999 952 instructions, 27 985 code buckets, 886 159 CPU sleeps. Nothing below
is confounded by that. (The suite has since grown to **20** — that concurrent work added five tests;
15 is the count this pass started against, and it is recorded as measured rather than updated,
because the number that matters here is the boot's, and the boot did not move.)

### 0. The ledger

**23 claims audited — 12 survived, 9 wrong, 2 partial.** Every wrong one is retracted in place at its
own site; the rows are here so the count can be checked rather than taken.

| | claim | where | verdict |
|---|---|---|---|
| 1 | RetailOS's lifetime contribution to `0x30000000` is **12 reads and 30 writes** | Add. 6 | ❌ 24 reads, 100 696 halfword writes |
| 2 | `vmcs.bin` is "read off disk, parked, and **never uploaded**"; bypass #6 "**unreachable**" | Add. 6 | ❌ uploaded in full, 4 chunks |
| 3 | the 320×240 frame "is the ROM's own logo, and **RetailOS has drawn nothing at all**" | Add. 6 | ✅ byte-identical PPM at handoff and at 600 M |
| 4 | "**RetailOS reaches the display**"; two frame updates kicked *by RetailOS* | Add. 12 §1–§2 | ❌ both are the boot ROM's |
| 5 | `APPLEBOOT` blocks once "and is **never woken**" | Add. 7 §3 | ⚠️ woken; re-blocks on `0xea` |
| 6 | `OptoTask` "declared and **never created** … **no TCB** has either" | Add. 7 §6 | ❌ dispatched @49 678 867; TCB 18 |
| 7 | inter-processor mailbox `0x60001000..0x6000102f`: **zero reads, zero writes** | Add. 7 §6, Add. 9 §7, ledger #7 | ✅ but the old control had saturated the log — §8 |
| 8 | ten named tasks, "**not one of them has a TCB**" | Add. 7 §7 | ❌ 7 of 10 created |
| 9 | `t_graphicsManager` sits in its pump "with **nothing ever posted to it**" | Add. 7 §7 | ✅ `tick = 216` of 236 975 |
| 10 | "**four** [GENCMDs] are issued in an entire boot" | Add. 10 §3 | ✅ `bcm: 4 commands kicked` |
| 11 | "the uploaded bytes are **never read back** — **56 of 56** in `0x1f0..0x20f`" | Add. 10 §5, Add. 12 §4 | ⚠️ 48 of 56; the ranges overlap — §5 |
| 12 | no read falls in `0x000e0000..0x0010581e` | Add. 10 §5 | ✅ |
| 13 | the font registry miss is "**gone** … **zero** on retail" | `NEXT.md` 1 | ❌ **337 lookups, 319 of them Podium Sans 18** |
| 14 | "LBA 22169 is **absent** from the complete retail LBA set" | `NEXT.md` 1 | ❌ *(was ✅)* — **read, at command #342 of 671**; "all 256 commands" is the log's cap, not a count. See Addendum 15 §7 |
| 15 | "**28** are never written by the firmware at all" | research/09 | ❌ 29 — `0x60006038` |
| 16 | every address the original table named is still in the list | research/09 | ✅ none dropped at the wider window |
| 17 | `0x6000d13c` `GPIOL_INPUT_VAL` — 3 560 reads, **never written** | research/09 | ✅ |
| 18 | `0x7000c140` the click wheel — 60 reads, **never written** | research/09 | ✅ |
| 19 | `0x6000d03c` `GPIOD_INPUT_VAL` — **never written** | research/09 | ✅ |
| 20 | `PP_VER1`/`PP_VER2` — **never written** | research/09 | ✅ |
| 21 | "**no named task is ever created**" | research/03 §1, §13 | ❌ 27 named creations, 62 TCBs |
| 22 | "**Zero USB register accesses** in two billion instructions" | research/03 §50 | ✅ only unmapped page is `0xea000078` |
| 23 | `USBStatusTask`/`USBTaskTimeTask` are "**the only two RTXC tasks that never start**" | research/03 §50 | ❌ `USBDeviceTask` @49 705 952 and `USB MSC` @52 178 917 both have TCBs |

Survivors: rows 3, 7, 9, 10, 12, 14, 16–20 and 22. Wrong: 1, 2, 4, 6, 8, 13, 15, 21, 23. Partial: 5 and 11. **Nine wrong is
the deliverable**; finding none would have been the surprising result.

**Four of the nine are not truncation at all.** Rows 1, 2, 4 and 13 are conclusions nobody re-ran
after Addendum 9 modelled the second DMA controller — the machine moved under them. The audit found
them because it re-measured claims rather than re-reading them, which is the only reason to run one.

### 1. What the scope actually was, and why the count is 23 rather than 300

`research/` holds several hundred absence-shaped sentences, and most of them cannot have been
truncated: they are **static** — a byte that is not in an image, a name that is not in a datasheet, a
string that is not in a PE. Only claims produced by *running the emulator on a boot* are in scope,
and of those, only the ones a stop condition could have cut short. Three further groups were read and
set aside deliberately:

- **Already retracted in place.** research/03 alone carries eleven, research/07 more than twenty.
  They were checked for a retraction block and skipped.
- **Prototype-path measurements.** research/03 §§41–53 and much of research/02 were measured on
  `cold-boot.sh`'s prototype NOR, which self-resets 157 times and never mounts `rsrc`. Per the rule
  in `retail-boot.sh`'s header — every number stays attributable to the configuration it was measured
  on — a retail measurement does not retract them. Where a retail run says something different, it is
  noted as superseding, not as refuting.
- **Claims about the *window itself*.** Addendum 11 §5 already varied 2 G against 6 G for the `0xd1`
  wall. Re-running that was not the cheapest thing available and it had already been done properly.

### 2. WRONG — the framebuffer is the boot ROM's (Addendum 12, retracted; Addendum 6 vindicated)

The largest correction, and it is not a truncation casualty — it is a *missing control*. Addendum 12
read "230 572 halfwords written, 2 frame updates, a 320×240 write run at `0x000e0000`" off the end of
a full run and attributed all of it to RetailOS. Dumping the same framebuffer at the ROM→RetailOS
handoff (`--stop-at=0x10000000:1`, @46 397 133) and at 600 M produces **byte-identical PPM files**,
2 922 non-black pixels in both. Addendum 6's "RetailOS has drawn nothing at all" was right, and
Addendum 12 retracted it on the strength of a control that only ever proved the readout geometry.

Addendum 6's other three sentences are wrong for the opposite reason — they predate the second DMA
controller. RetailOS's real contribution to `0x30000000` is **100 696 halfwords and 24 reads**, all
of it the `vmcs.bin` upload, against the "12 reads and 30 writes" recorded there.

### 3. WRONG — `OptoTask` has a TCB, and the fields were compared wrong

Addendum 13 had already caught the "never created" half. The "no TCB has either" half was checked
here by dumping the array (`--dump=0x0087198c:0x1400`, stride `0x3c`): TCB 18, entry `0x00284e58`,
stack `0x10874abc`, size `0x400`, `state = 0x0000`, priority 4, `tick = 236 974` of 236 975. The
registry's `0x10874ebc` is the stack **top**; `0x10874abc + 0x400` is the same stack. The task is not
merely created — it is *runnable at the end of the run*, driving a click wheel we answer as zero.

### 4. WRONG — 7 of 10 "no TCB" tasks are created, 4 of them past the old stop

`--enterlog=0x0011c808` is RTXC's task-creation entry with `r0` = the name pointer, so one run
enumerates every named task with a timestamp. 27 creations; names resolved out of
`OSOS_correct.bin`. Against Addendum 7 §7's list of ten:

| task | created |
|---|---|
| `USBDeviceTask` | @49 705 952 |
| `VCUpdateTask` | @50 441 379 |
| `ATAWorkLoopTask` | @50 494 071 |
| `TrackCacheReadTask` | @53 608 893 |
| `StreamCacheReadTask` | **@174 340 885** |
| `SearchHelperThread` | **@174 462 018** |
| `PhotoCopyTask` | **@249 931 182** |
| `DiskReaderTask`, `ImagePresentationEngine`, `iMAImageCacheThread` | still absent |

The four bold-adjacent ones are past @123 M and were structurally invisible to the old recipe. The
other three were lost to the DMA controller instead. Also created and not previously named:
`TimeManager`, `EventManager`, `TimerEventManager`, `KeyRepeatTimer`, `WindowManager`,
`ATAWorkLoopIRQTask`, `VCIH`/`VCFS`/`VCHR`/`VCPD`, `USB MSC`, `LoadDataTasks`, `VideoDaisyTask`.

`t_graphicsManager` in the same sentence **survives**: TCB 10, `KS_receive`, `tick = 216` of 236 975.

### 5. PARTIAL — "the uploaded bytes are never read back"

48 of 56, not 56 of 56, and the 8 stragglers are at `0x10000c00..0x10000c06`. Worse for the claim:
`Bcm::mem` is a single flat map, so the protocol block at `0x1f8`/`0x1fc` lies **inside** the
upload's own write run (`0x00000000..0x0003129e`). Read-back and reply-polling are not separable by
address in this model. The load-bearing half — RetailOS never validates the firmware, and never
reads `0x000e0000..0x0010581e` — is untouched.

### 6. WRONG — the font registry miss is *not* gone on the retail path

`NEXT.md` item 1 recorded, as one of "two things it is NOT, both measured": *"the font registry miss
is **gone** (447 lookups on the prototype, **zero** on retail, from byte-identical code)."*

`--enterlog=0x00221e24`, positive control `0x0016b044` in the same run (entered @52 249 160):

```
337 lookups.  By key (name, size, style):
  319   r1=0x004b1360  r2=0x12  r3=1     <- "Podium Sans", 18 pt      first @77 274 736
    5   r1=0x004b1360  r2=0x10  r3=1
    5   r1=0x004b1318  r2=0x0a  r3=0
    3   r1=0x004b1360  r2=0x1c  r3=1
    3   r1=0x004b12c4  r2=0x07  r3=1
    1   r1=0x004b1360  r2=0x0e  r3=1
```

That is §6's key, on the retail path, 319 times. **The other half of that sentence survives and is
what makes this interesting: LBA 22169 is still absent from the complete retail LBA set** — checked
against all 256 ATA commands. RetailOS asks for Podium Sans 18 pt three hundred and nineteen times
and never reads a font file.

This one is not a window artefact either: the first lookup is at @77 M, well inside the truncated
window. It is a *machine* casualty — the claim was recorded when the boot still stopped at @51.8 M on
the `vmcs` upload, and nobody re-ran it after Addendum 9 modelled the engine. It is filed here
because the audit is what found it, and because it moves §§5–8 of this file from "prototype-only,
superseded" back onto the retail path.

### 7. WRONG — research/09's "28 never written" is 29, and the 29th is undocumented

`--input-regs=0x60000000:0x11000000` at both windows, same build, same machine — the only variable is
the stop condition:

| | `--stop-when-idle=40000000` | `--stop-when-idle=400000000` |
|---|---|---|
| read-before-write rows | 72 of 204 touched | **73 of 205** |
| never written at all | 28 | **29** |
| new address | — | **`0x60006038`**, 8 reads, first pc `0x00000560` |

`0x60006038` is unnamed in Rockbox's `pp5020.h` — it sits between `PLL_CONTROL` (`0x60006034`) and
`PLL_STATUS` (`0x6000603c`). Its reader is a five-instruction leaf with three call sites
(`0x000a501c`, `0x001a4904`, `0x0026bdbc`):

```
0000055c  ldr r0, =0x60006000
00000560  ldr r1, [r0, #0x38]
00000564  ldr r0, [r0, #0x38]
00000568  eor r0, r1, r0, lsl #16
0000056c  bx  lr
```

**Read the same register twice and mix the two halves** — the shape of a free-running counter or an
entropy source. We answer 0 both times, so the mix is identically 0 on every call. Four calls, and
none of them happen before @123 M.

### 8. SURVIVED — the inter-processor mailbox, but the old control had saturated the log

`research/04` #7 is the one bypass in the ledger held open by an absence claim, so it got the most
care. It survives: `--readlog` + `--storeaddr` over all twelve words of
`0x60001000..0x6000102f` record **zero reads and zero writes** across the full 600 M baseline.

**The re-confirmation recorded under Addendum 7 §6 does not establish that, and it is worth saying
why.** `Memory::read_log` is capped at 2 000 000 entries. Its control, `0x60005010` (`USEC_TIMER`),
is read 9 588 012 times in a full run — so the first attempt at this measurement returned exactly
`--- reads of watched addresses: 2000000 ---`, the cap, consumed entirely by the control, with the
cap reached shortly after @123 M — the same control stands at 1 753 380 when the old recipe stopped at @123 469 613. **Roughly two thirds of the run was unobserved by an instrument
that reported a clean zero.** That is the ninth instrument ceiling in this project and the same shape
as bypass #10's interrupt storm: a round number that means *saturated*, not *this many*.

`0x60005010` was also the wrong **code path**. `page_is_plain` disqualifies the USEC_TIMER page, so
it is served by `read8`, which counts unconditionally and was never affected by the `count()` hoist
bug. `0x60001000` is a plain page in `mmio-6` and goes through `read32`'s hoist. A control that
cannot exercise the failure mode proves nothing about it — the same lesson as Addendum 8 §8, one
layer down.

Re-run with controls chosen to be live, word-width, plain-page and **low-rate**:

```
--- reads of watched addresses: 74 ---          (74, not 2 000 000: no saturation)
  [0x60004020] read by 0x000cd340  x3   first @599999951   <- live at the last instruction
  [0x6000603c] read by 0x40001504  x6   first @243965
  [0x70000000] read by 0x000005bc  x30  first @46397527    (+5 more sites, 33 total)
  ... no row for any address in 0x600010xx

--- stores by watched instructions: 13 ---
  0x001fc9a4 -> [0x60004024] = 0x00000010  @48634977        (+12 more, all the control)
  ... no store to any address in 0x600010xx
```

Controls fire from @88 375 to @599 999 951. **The mailbox really is silent**, now on evidence that
covers the whole run.

### 9. The other survivors, and one that is scoped rather than wrong

- **`NEXT.md`: "LBA 22169 is absent from the complete retail LBA set."** Survives — checked against
  all 256 commands, and now load-bearing in a way it was not, per §6 above.
- **Addendum 10 §3: "four GENCMDs issued in an entire boot."** Survives — `bcm: 4 commands kicked`.
- **research/03 §50: "zero USB register accesses."** Survives on the register half: the PP502x USB
  block at `0xc5000000` is covered by no region, so any access would be reported unmapped, and the
  only unmapped page in a 600 M baseline is `0xea000078`. **The task half is now wrong** —
  `USBDeviceTask` @49.7 M and `USB MSC` @52.2 M both have TCBs. A device with a driver running
  against it and no register traffic at all is a more interesting state than "nothing asks for USB".
- **research/09's conclusion** ("what we still invent from nothing is five addresses") survives as a
  conclusion; the count is six with `0x60006038`.
- **research/03 §1: "no named task is ever created."** Wrong — 27 named creations. It was already
  contradicted by §41 of its own file and never annotated, and §13 of that file promotes it as *"the
  one negative result in this project that has survived scrutiny."* It has not. Retracted there.

### 10. The rule this pass adds

Addendum 8 §8: *a control only proves what it exercises.* Addendum 11 §6: *a measurement window is
part of the measurement.* This pass found both failing again, in the same week, and adds a third that
is really a corollary of the first:

> **A control that saturates the instrument is worse than no control**, because it produces a clean
> number from a log that stopped recording. Before believing a zero, check the *total* the instrument
> reports against its own cap — `read_log` and `store_pc_log` both stop at 2 000 000, `watch_range`
> at 4 096, `write_log` at 8 192. Prefer the **quietest** control that is provably live end-to-end
> over the loudest one available.

And a smaller one, from §3 and §6: when a claim compares two records, **check that the fields being
compared mean the same thing** (stack top against stack base), and when a machine changes under a
claim, **re-run the claim, not only the conclusions the change was about**. Four of the eight wrong
answers here were not truncation at all — they were conclusions nobody re-measured after the second
DMA controller landed.

---

## Addendum 15: `0xd1` is an ATA `WRITE DMA` our disk refuses in silence — and the wall is a retry loop, not a deadlock

Measured 2026-08-14 on `retail-boot.sh --clock=5 --stop-when-idle=400000000`. Addendum 11 §5 named
the semaphore and read the block correctly as *not* a repeat of its own §2. It then read it as a
deadlock, and that is the part that is wrong: the machine is not stuck, it is **retrying a disk
write every ~3.9 simulated seconds, forever**, and the sampled `KS_pend(0xd1)` is where it spends
99.9 % of each cycle. Two independent one-line changes — neither an ablation, neither a bypass —
each clear it, and `APPLEBOOT` reaches `0x00284538`.

### 1. The object, named

`0x0018942c` and `0x00189444` are two thunks in a table of one-instruction accessors at
`0x001893c0..0x001894a0`; the class they belong to is constructed at `0x001894a0` (vtable
`0x0066bd88`, `bl 0x000d1e5c` on `this+0x5c` — create a semaphore), destroyed at `0x001894cc`, and
allocated `operator new(0x60)` by three derived constructors at `0x00142fc0`, `0x001430e4` and
`0x00143218`. Field map, read off its own setters at `0x001454f0..0x00145790` and its initialiser at
`0x00145548`:

| field | what | how known |
|---|---|---|
| `+0x0c` | unit | `0x00145784` `strb` |
| `+0x10` | flags — bit 19 = "do not wait" | tested at `0x00143144` `tst r0, #0x80000` |
| `+0x1a` | **ATA command byte** | `0x001454f0`; measured values below |
| `+0x20` | device select, `0xff` until set | `0x001456dc`, default at `0x00145558` |
| `+0x24` | timeout, always `0x2710` | `0x001455fc`, `ldr r1, =0x00002710` at `0x00226e80` |
| `+0x28` | buffer pointer | `0x0014577c` |
| `+0x3c` | completion callback, else 0 | branched on inside `0x00189444` |
| `+0x40` | status | `0x00189498` sets it, `0x0014576c` reads it |
| `+0x4a` | busy byte | cleared first thing in `0x00189444` |
| `+0x5c` | **its own RTXC semaphore** | `+0x5c` in both `0x0018942c` and `0x00189444` |

`--enterlog=0x001454f0` gives the command byte at every construction. In a 600 M run, **378**
requests, four commands, and the caller of each is unambiguous:

```
245  lr=0x002269c0  cmd 0xc8   READ DMA          <- the read builder at 0x00226970
110  lr=0x00143040  cmd 0xef   SET FEATURES
 22  lr=0x00226ee4  cmd 0xca   WRITE DMA         <- the write builder at 0x00226e80
  1  lr=0x00142f0c  cmd 0xec   IDENTIFY DEVICE
```

So the "resource lock or pool of size 1" is an **ATA taskfile request**, and the `'free'`/`'busy'`
FourCCs at `[driver+0x14]` are the disk driver's state tag. `0x001430f8` is its synchronous submit:
issue through `[[this+4]+0x10]`, and *if* `[req+0x3c] == 0` and `[req+0x10] & 0x80000 == 0`, wait on
the request's own semaphore, then return `[req+0x40]`.

### 2. `0xd1` is that request's semaphore, and the identity is measured, not inferred

`0x000d1e5c` allocates 8 bytes, zeroes `[0]`, and asks RTXC service 3 to write a semaphore **id**
into `[4]`; `0x000a0ebc`/`0x000a0c84` are a *counting* pend/signal pair that only load `[obj+4]` and
enter the kernel on the branch where a task actually blocks or is actually woken.

```
--storeaddr=0x13ee218c,0x13ee2124
  0x000d1e68 -> [0x13ee218c] = 0x13ee2120   @50571232     ; req+0x5c = the semaphore object
  0x002806d8 -> [0x13ee2124] = 0x000000d1   @50571175     ; semobj+4 = the RTXC id
```

`0xd1` belongs to heap object **`0x13ee2130`**, and that object is the disk driver's single
in-flight request slot: `0x001894f0` reads it out of `[driver+0x10]`, and `r1 = 0x10872364` — the
driver — is constant across all 272 pends.

### 3. The pairing the queue asked for, and it closes exactly

Three runs, each carrying `0x0016b044` (`MP3ExampleTask`'s body, arrival `x1` at `@52 249 160` in
every arm — a control that fires once and therefore cannot saturate anything):

| watched | count | what it is |
|---|---|---|
| `0x0018942c` | **272** (267 on `0x13ee2130`, 5 on `0x13ee1f08`) | arrivals at the wait |
| `0x00189558` | **377** (266 on `0x13ee2130`: 245 status 0, **21 status 3**) | the completion post |
| `0x0018a364` | **0** | the abort/flush post — never taken, in a run the control proves ran |

Arrivals are not blocks: the semaphore is counting, so a pend whose completion already landed never
enters the kernel. `[semobj+4]` is loaded *only* on the two branches that do, which makes
`--readlog=0x13ee2124` an exact census of them:

```
[0x13ee2124] read by 0x000a0ed4  x22   first @57697637     ; KS_pend  — blocking pends on 0xd1
[0x13ee2124] read by 0x000a0c9c  x21   first @76240648     ; KS_signal — kernel wakes of 0xd1
[0x13ee2120] read by 0x000a0830  x267 / by 0x000a07f4 x266 ; the counter — pends / posts
```

**22 blocking pends, 21 wakes.** And `--enterlog=0x0014570c` says the same run built exactly **22**
`WRITE DMA`s — LBA 65 580 ×1, 65 792 ×5, 32 894 ×16. Twenty-two and twenty-two: *every* pend that
ever blocked on `0xd1` is a disk **write**, every one of the 21 that finished finished by **timeout**
(status 3), and the twenty-second is outstanding when the window ends. No read ever blocks.

**The request that never completes is a 1-sector ATA `WRITE DMA` (`0xCA`) to LBA 32894.** The first
one to hang is the write to LBA 65 580, built `@57 692 042`, blocked `@57 697 637`, abandoned by
timeout `@76 240 613` — and *that* timeout is the `0xd1` signal at `@76 240 652` which Addendum 11
recorded as the only one. There were 21.

### 4. It is a retry loop, and the discriminator Addendum 11 used cannot see the difference

`0x0018942c` is entered **272** times by 600 M and **325** times by the 1.27 G idle stop. The cadence
is ~19.6 M instructions — 3.9 s at `--clock=5`, the driver's own timeout — and each cycle is the
same four steps:

```
@577697804  0xCA  WRITE DMA  lba 32894           <- issued
@577703420        KS_pend(0xd1)                  <- blocks
@597148581        completion, status 3           <- 19.45 M later: TIMED OUT
@597151796..597168574   5 x 0xEF SET FEATURES    <- the driver resets the device
@597184792  0xCA  WRITE DMA  lba 32894           <- and tries again
```

871 501 CPU sleeps in the trailing window is therefore *true and not diagnostic*: a task that sleeps
3.9 s out of every 3.9 s looks exactly like a task that sleeps forever. Addendum 11 §6 taught that a
measurement window is part of the measurement; this is the same lesson one level up — **`state =
0x40` is a sample, and a sample cannot distinguish "blocked" from "blocked again".** The cheap
discriminator was free and sitting in the same instrument: *watch the pend and count it twice.*

### 5. Where LBA 32894 is, and what RetailOS is trying to write

MBR partition 2 is type `0x0c`, start LBA **32768**, 16 744 448 sectors. Its FAT32 BPB reads 512
B/sector, 8 sectors/cluster, **`BPB_RsvdSecCnt` = 126**, 2 FATs, `BPB_FATSz32` = 16 321, root cluster
2, FSInfo at reserved sector 1.

```
32768 + 126   = 32894   <- first sector of FAT #1      the write that never completes
32768 + 16447 = 49215   <- first sector of FAT #2      its mirror
32768 + 32812 = 65580   <- a directory sector of iPod_Control/iTunes
32768 + 33024 = 65792   <- cluster 34, iPod_Control/Device/Accessories
```

Run the same boot on a **writable copy** of the same image and 41 sectors change. Diffed against the
pristine image, RetailOS is doing first-boot volume bootstrapping:

- FSInfo — `FSI_Free_Count` and `FSI_Nxt_Free`, at offsets 488 and 492 of part-rel sector 1;
- FAT #1 sectors 0, 813 and 6149, and all three FAT #2 mirrors;
- creates `Contacts` (cluster 35), `Calendars` and `Notes` in the root, and
  `iPod_Control/Device/Accessories`;
- deletes `iPod_Control/iTunes/IC-Info.sid`;
- writes `Contacts/ipod_created_instructions.vcf` (104 B) and `Contacts/ipod_created_sample.vcf`
  (676 B), whose bodies read `begin:vcard / version:3.0 / org:<<<instructions display name>>> …`
  and `fn:<<<name>>> / title:<<<title>>> / adr;type=work …`.

Those are the two placeholder vCards a restored 5G writes on its first boot. **The blocked "resource"
is not a lock and not a pool — it is the disk, and RetailOS is stuck one sector into formatting its
own Contacts folder.**

### 6. Two defects, each on its own sufficient — the A/B

The recipe opens the disk **read-only** (`retail-boot.sh` passes no `--disk-writable`), so
`Ata::command`'s `0xca | 0x35` arm takes its else-branch: `status = ERR`, `error = 0x04` (ABRT) —
**and no `irq_pending`**. A real ATA device asserts INTRQ when it aborts a command. Ours does not, so
the driver is not told the write failed; it is told nothing, and only its own 3.9 s timer ends the
wait.

Four arms, `BUDGET=2500000000`, same image, same flash, differing in those two things only. The two
binaries come from **one frozen source snapshot** built twice (the shared tree was being edited by
other work mid-session); the pre-fix build reproduces the session-start baseline byte for byte at
600 M — 27 985 buckets, 907 IDE IRQs raised, `ata dma: 303 transfers, 15691264 bytes`, 273 arrivals,
`pp dma: 4 transfers, 201216 bytes`, 4 unmapped reads — so the arms are matched.

| | `--disk-writable` | abort raises INTRQ | stop | buckets | ata dma | `0x0019db14` | `0x0016b094` | `0x00284538` |
|---|---|---|---|---|---|---|---|---|
| **A** baseline | no | no | Idle @1 271 809 167, 871 501 sleeps | 28 028 | 322 / 16 313 856 | — | — | — |
| **B** | **yes** | no | Idle @1 560 127 029, 2 187 347 | 36 161 | 614 / 32 130 560 | @810 445 861 | @1 000 880 206 | @1 011 429 993 |
| **C** | no | **yes** | Idle @1 080 809 269, 2 211 022 | 35 609 | 484 / 27 815 424 | @218 810 613 | @408 991 860 | @419 532 867 |
| **D** | **yes** | **yes** | Idle @1 610 306 101, 2 199 467 | 38 291 | 660 / 33 637 888 | @870 718 070 | @1 061 153 039 | @1 069 131 064 |

`0x0019db14`, `0x0016b084`, `0x0016b094` and `0x00284538` are the four addresses Addendum 11 §5
recorded as *"still unreached at 2 371 809 167 instructions"* with the budget widened from 2 G to
6 G. **All four are reached in three of the four arms.** The recursive view-tree builder returns,
`MP3ExampleTask` runs to its own end and posts `0xea`, and `APPLEBOOT` reaches `0x00284538` — which
is `NEXT.md` item 3's stated settle condition, and it was never gated on the co-processor.

So the answer to "which of (a)–(d)" is **(c) an instrument/recipe error, compounded by (b) a model
error**, and neither of the other two:

- **not (a) missing hardware.** The whole chain is ATA, and the ATA controller, its DMA path and its
  interrupt are modelled and exercised 671 times in this very run.
- **not (d) a missing file.** The sectors RetailOS wants are present; it wants to *write* them.
- **(c)** the recipe never passes `--disk-writable`, so every write is refused. The correct pattern
  already exists three files away: `flash-update.sh` `cp -c`s the image into `$WORK` — an APFS clone,
  instant, costing only the blocks written — and runs writable on the clone. `retail-boot.sh` should
  do the same. It must be a clone, not the pristine image: every number in `research/` is
  attributable to that file being unmodified.
- **(b)** the refusal is *silent*. One line in `Ata::command`'s write-abort branch, and the same
  omission on the read-error and unknown-command branches:

  ```rust
  self.status = ATA_DRDY | ATA_DSC | ATA_ERR;
  self.error  = 0x04;          // ABRT
  self.irq_pending = true;     // <- missing; a real drive interrupts when it aborts
  ```

  Arm C is that one line and nothing else. It is the *fastest* arm to the wall's far side
  (@218 810 613, four times sooner than arm B) because RetailOS handles a prompt write failure
  gracefully and carries on, where a writable disk makes it do the real filesystem work.

Not landed here: the tree was being edited by two other agents while this ran, and a model change
wants its own re-run of everything the old model produced. Both fixes are one line each and the
evidence is above.

### 7. `ata commands: 256` is a saturated counter, and one absence claim dies with it

`Ata::command` records commands under `if self.commands.len() < 256`. The "256 ATA commands" that has
served as this project's baseline fingerprint since Addendum 9 is the **cap**, not a count. Raised in
the snapshot to 1 000 000, the same baseline run at 2.5 G reports:

```
ata commands: 671      322 x 0xc8   286 x 0xef   56 x 0xca   4 x 0x20   3 x 0xec
  writes:  lba 65580 x1   lba 65792 x5   lba 32894 x50
```

and arm C reports **625** — 484 `0xC8`, 117 `0xCA`, only **16** `0xEF` (the error-recovery storm is
gone), and **one `0xE0` STANDBY IMMEDIATE**, which is a boot that finished, not one that gave up.

The casualty: **LBA 22169 — the font file — *is* read, as command #342 of 671**, in both arms.
Addendum 14 row 14 marked "absent from the complete retail LBA set" ✅ *"checked against all 256
commands"*; the log had stopped recording 415 commands earlier. Retracted at its row. This is
precisely the rule that pass itself paid for — *a control that saturates the instrument is worse than
no control* — and the audit's own baseline block carried the saturated number in it.

### 8. The font-registry lead, killed

A parallel measurement found 337 font-registry lookups on the corrected recipe, 319 of them the key
`("Podium Sans", 18, 1)`, first at `@77 274 736`, and proposed the miss as `0xd1`'s producer: a view
tree that cannot measure text is a plausible shape for a resource taken and never released.

**It is not the producer, and three measurements say so independently.** (i) All 22 blocking pends on
`0xd1` are accounted for, one per `WRITE DMA`, with none left over for a font. (ii) The first of them
blocks at `@57 697 637` — **19.6 M instructions before the first font lookup**, so the wall predates
the font path entirely. (iii) Two one-line changes in the ATA write path, touching nothing that
knows what a font is, each let the recursive builder return. The font-registry miss may well be a
real defect; it is not this one, and its "no font file is ever read" half is dead per §7.

### 9. Predictions that measured out to nothing

Recorded because the convention is load-bearing and both were good hypotheses.

- **"Fixing the silent abort will not clear the wall — the write still fails."** Wrong, and
  informatively so. RetailOS treats a prompt write error as recoverable and boots straight past it
  (arm C). The firmware's own error handling was never the problem; it was never given an error.
- **"The view tree finishing will finish the screen."** Addendum 12 §4 predicted that the fragmentary
  frame was the blocked builder's. Arm D — wall cleared, builder returned, `APPLEBOOT` terminated —
  dumps `bcm: 4 commands kicked, 2 frame updates` and **2 922 non-zero pixels of 76 800**, identical
  to the baseline. Nothing changed. Independent corroboration of Addendum 14 §2's retraction: that
  frame is the boot ROM's, and RetailOS has still drawn nothing.

### 10. The rule this one cost

Addendum 11 §6: *a measurement window is part of the measurement.* This is its exact twin one level
in: **a resting state is a sample, and "blocked" is a claim about one instant until the same point is
counted twice.** `state = 0x40` at 2 G and `state = 0x40` at 6 G is not two observations of a
deadlock; it is one observation, taken twice, of a loop whose period nobody measured. The cheapest
control was `--enterlog` on the pend with two different budgets — 272 against 325 — and it costs one
command, exactly as varying `--stop-when-idle` did.

And a second, smaller: **a saturating log answers "absent" with the same words it uses for "not
recorded".** `ata commands: 256`, `read_log`'s 2 000 000, `--enterlog`'s 400-row print — three caps
in one instrument, and two absence claims in this file were built on two of them. A cap should be
reported as a cap.

---

## Addendum 16: the click wheel, modelled — and the only thing reading it is Apple's bootloader

Addendum 13 §3 scheduled this one on a specific argument: an unexercised device model cannot be
verified, but `OptoTask` is dispatched `@49 678 867` on a run we already have, so a wheel model gets
a verdict instead of an alibi. It got one. Three of them, and two are negative.

Measured 2026-08-14. Every number below is from one privately-built binary run four ways on
`BUDGET=600000000 retail-boot.sh --clock=5 --stop-when-idle=400000000`.

### 1. The register map, second-sourced against Apple's own driver

Rockbox's `button-clickwheel.c` gives the data register, the `0x800000ff == 0x8000001a` frame check,
96 clicks per rotation and the bit-30 touch flag. It does **not** describe a transceiver, because
Rockbox only ever consumes what the transceiver pushes. RetailOS does the other half, and reading it
turned a one-register fact into a four-register device:

| address | what it is | evidence |
|---|---|---|
| `0x7000c100` | bit 31 **transmit start**; bits 30..29 **arm the receiver** | ROM `0x4000e5ec` and RetailOS `0x002813ec` both `orr #0x60000000` to re-arm; RetailOS `0x00283fcc` `orr #0x80000000` to send, `0x00284028` `bic` to drop it. Rockbox's init word `0xc00a1f00` and ISR-tail word `0x400a1f00` are the same register, and our boot leaves it holding `0x600a1f00` |
| `0x7000c104` | bit 31 **transmit busy**; bit 26 **receive ready**, write-1-to-clear; bit 27 likewise | RetailOS spins on bit 31 at `0x00283ffc`, polls bit 26 at `0x00283ed8`, acknowledges by *setting* bit 26 at `0x002813e4`. Rockbox acknowledges the same way (`inl(…) \| 0x0c000000`) |
| `0x7000c120` | **transmit data** — the command word | RetailOS `0x00283fc8` `str r0, [r5, #0x120]` with `r0` = `0x8000023a`. Absent from every published map |
| `0x7000c140` | **receive data** — the packet | Rockbox `CLICKWHEEL_DATA`; RetailOS `0x00281364`, `0x00283f04`; ROM `0x4000e59c` |

**Two packet shapes, and RetailOS decodes both** at `0x00281350`:

```text
00281370  and r12, r0, #0xbc0000ff
00281374  cmp r12, #0x8000001a      ; the streaming frame
00281380  mov r12, r0, lsl #18      ; buttons  = bits 13..8
00281384  tst r0, #0x40000000       ; touched
0028139c  and r0, r12, r0, lsr #16  ; position = bits 25..16, masked
002813b8  ldr lr, =0x8000023a       ; else the queried button frame
002813bc  bic r12, r0, #0x7f000000
002813c0  bic r12, r12, #0xff0000   ; r0 & 0x8000ffff, buttons at bits 20..16
```

Apple's mask `0xbc0000ff` is *stricter* than Rockbox's `0x800000ff` — it additionally requires bits
29..26 clear — and is satisfied by exactly the frames Rockbox accepts. The two sources agree; the
stronger one is Apple's, because it is the code this emulator runs.

**The interrupt is IRQ 40** — bit 8 of the controller's second bank. Rockbox's `button_init_device`
enables `CPU_HI_INT_EN = I2C_MASK` with `I2C_IRQ = 32+8`; RetailOS masks the same bit around its
polled read (`mov r9, #0x100; str r9, [r10, #0x128]`, `r10 = 0x60004000`). The wheel shares the I²C
block's address space *and* its line.

**The boot ROM and RetailOS ship the same driver, byte for byte.** `0x4000e540` in IRAM and
`0x00283ea0` in RetailOS are the same forty instructions: same `0x8000023a`, same five retries, same
`0x5dc` (1 500 µs) timeout, same permutation of buttons into a mask, same closing `eor r0, r4, #0x3f`.
Two stages, one routine — which is why a single model answers both.

Licence discipline: Rockbox was read for register semantics and nothing was copied from it. The
register facts above are, in any case, now attested by Apple's own binary independently.

### 2. The model was wrong first, and the firmware is what said so

The first version answered a command **inside the store that started it**. It passed its unit test,
and on a real boot it produced *3 word reads of `0x7000c140`, none of them with a frame waiting* —
because Apple's sender acknowledges (`orr #0x0c000000` at `0x00284054`) about thirty instructions
*after* starting the transmit, and that acknowledgement wiped the answer before the caller ever
looked. A device that replied synchronously would make Apple's shipping firmware time out on every
query, which it demonstrably does not on real hardware. So the reply has to arrive after the ack: a
real round trip to the PSoC, modelled as `OPTO_REPLY_USEC = 100` against the driver's own 1 500 µs
patience.

This is the **second** time this emulator has made the synchronous-completion mistake — the first was
the drive's `IDE_COMPLETION_USEC`, whose comment says the same thing in the same words. The general
form: *when firmware arms a wait and only then acknowledges, a device that finishes inside the store
is racing code that was written assuming it could not.*

### 3. What the model measurably changes

`--clickwheel` models the registers; `--wheel=SCRIPT` injects a sequence; `--wheel-no-irq` ablates the
line. The script below is 60 steps — touch, twelve clicks forward, release, a Menu press, twelve
clicks back, a Select press, hold and release, a twenty-four-click scroll — spread from `@50 M` to
`@302 M`, and it is printed in full before the run so a log reproduces itself.

| | control | `--clickwheel`, no events | + 60 events | + 60 events, `--wheel-no-irq` |
|---|---|---|---|---|
| code buckets executed | 27 985 | **27 988** | **13 102** | 27 988 |
| instructions | 599 999 952 | 599 999 952 | 599 999 952 | 599 999 952 |
| simulated µs | 949 751 362 | 949 718 498 | **119 999 990** | 949 718 498 |
| irqs asserted / taken | 1 154 841 / 237 329 | 1 154 872 / 237 332 | **8 594 012 / 1 804 436** | 1 154 872 / 237 332 |
| ata commands | 448 | 448 | **96** | 448 |
| pp dma | 4 / 201 216 B | 4 / 201 216 B | 4 / 201 216 B | 4 / 201 216 B |
| unmapped | 4 reads | 4 reads | 4 reads | 4 reads |
| word reads of `0x7000c140` | 15 | **3, all with a frame waiting** | 3 | 3 |

Controls, matched in width, region and code path: `0x7000c01c` — the I²C status register, same
`0x7000c000` block, same 32-bit width — is read 10 918 times in every run, and `0x0016b044`
(`MP3ExampleTask`'s body) is entered in every run. Neither instrument is dark.

**The model does not merely get read; it changes what the reader does.** Six 16-byte code buckets
are reached that the control never reaches and none are lost, and three of them are Apple's
`0x4000e580` / `0x4000e5c0` / `0x4000e5d0` — the poll-loop tail and **the button-decode block, which
only executes on a frame that passed the validity test**. The read count falls 15 → 3 for the same
reason: the ROM calls its query three times and retries five times per call, so fifteen reads is
three calls that failed every attempt, and three reads is three calls that succeeded on the first.
`0x4000e570` (the receive-ready poll) rises 15 → 85, which is the driver genuinely waiting out the
round trip instead of finding a stale bit already set.

### 4. Finding one: in a 600 M boot, the only thing that reads the wheel is Apple's bootloader

`--readlog=0x7000c140` attributes every read to `0x4000e59c`. RetailOS reads that register **zero
times**, in all four configurations. Its streaming decoder at `0x00281350` and its polled query
wrapper at `0x00283ea0` are never entered — `--enterlog` on both, with `0x0016b044` carried as a
reached control in the same run, reports the control and nothing else.

What RetailOS *does* do, exactly once, at `@49 680 320` — which is `OptoTask`'s dispatch — is run its
opto init at `0x00283e30`: write `0x600a1f00` to `0x7000c100`, clear `0x7000c104` bits 27..26, set
`CPU_HI_INT_EN = 0x100` (IRQ 40), set `0x7000c104` bit 24, and transmit **`0x8001052a`**.

### 5. Finding two: with the line ablated, sixty injected events change nothing at all

`--wheel-no-irq` with the full script is **identical to `--clickwheel` with no script** in every
machine number: same 27 988 buckets, same 1 154 872 interrupts, same 949 718 498 µs, same 448 ATA
commands. 63 frames posted, 59 of them overwritten unread. The delta is zero, and it is zero for a
locatable reason rather than an unknown one: nothing polls the register, so a device that only fills
it is talking to no one.

This is the answer to "does injecting input do anything yet." It does not — **yet** — and the
measurement says so plainly rather than leaving it to be assumed.

### 6. Finding three: with the line live, the boot wedges — and the wedge names its own cause

Firing the same script with IRQ 40 enabled asserts a level the firmware never acknowledges. The line
goes up at `@50 M` and stays up: 1 804 436 interrupts taken, the machine stops sleeping entirely
(simulated time collapses from 950 s to exactly `budget / clock`), and the boot reaches 13 102 code
buckets against 27 988 and 96 ATA commands against 448.

`--profile` names it. The three hottest addresses are `0x00127610` at 1 804 435 samples — one per
interrupt taken — and `0x001fc620` / `0x001fc720` at 1 697 004 and 1 696 872. `0x001fc5c0` is the
demux:

```text
001fc5c4  mov  r7, #0x8            ; eight groups of bits
001fc5e8  add  r1, r6, r5, lsl #2
001fc5ec  ldr  r0, [r1, #0x8]      ; the handler slot for this source
001fc5f0  cmp  r0, #0x0
001fc5f4  beq  0x001fc618          ; …empty: fall through and return
001fc720  subs r7, r7, #0x1
001fc728  bne  0x001fc5d4
```

So: **RetailOS enables the wheel's interrupt before it registers a handler for it.** The demux walks
to bit 8, finds a null slot, returns without touching `0x7000c104`, and — the line being a level, as
the write-1-to-clear acknowledgement requires — is re-entered immediately, forever.

> **The observation is right; the explanation is ours, not RetailOS's — see Addendum 17 §7.** No
> handler is *ever* registered for source 40, and none is needed: the object-table demux is not the
> path the wheel takes. `--enterlog` on the installed IRQ service routine `0x00277128` reports the
> log's 65 536 cap, every arrival from the ARM exception entry at `0x00289e98`; that routine reads
> `CPU_HI_INT_STAT` itself and routes bit 8 straight to the decoder at `0x00281350`. It never gets
> there because `0x002771a4` gates its whole hi-bank arm on `tst r4, #0x40000000` — bit 30 of the
> **low** bank, which Rockbox names `HI_IRQ` and which is the "the second bank has something"
> aggregate. **Our model never raises it.** The wedge above is that omission: the ISR skips the wheel,
> delegates to the object demux, which has no slot for 40 and therefore never acknowledges. One line
> in `Machine::service_interrupts` clears it, and the A/B is Addendum 17 §8.

Stated carefully, because this is exactly the kind of result that gets over-read: the wedge is a
fact about RetailOS's state at `@50 M` in *this* boot, which is already blocked elsewhere. It is not
evidence that the interrupt model is wrong, and it is not evidence that the wheel cannot work. It is
evidence that **input injected before the handler is installed has nowhere to go**, which is a
scheduling constraint on every future experiment with this flag.

### 7. What is not modelled, said plainly

- **`0x8001052a` is unanswered.** Both Apple stages send it from an init routine that arms the
  receiver and enables IRQ 40 immediately beforehand, which is a strong hint that its reply is what
  puts the transceiver into autonomous streaming mode. We have no evidence for what it replies, so
  the model **refuses** rather than inventing — the count and the command are printed. Until that is
  settled, "RetailOS never reads a frame" cannot be attributed: it may be waiting for a handshake we
  declined to complete.

  > **Settled, and the attribution goes the other way — Addendum 17 §8.** With bit 30 of
  > `CPU_INT_STAT` modelled, RetailOS reads **every** frame posted (17 of 17, none dropped), enters
  > its decoder 14 times, and `SerialOptoTask` wakes from `KS_pend(0x7f)` for the first time. So
  > "RetailOS never reads a frame" was *our* interrupt model, not the handshake. The handshake is
  > still unanswered and is now the wheel's live blocker: the woken task re-sends `0x8001052a`
  > thirty times in one run.
  >
  > **Settled — and it was never a handshake. Addendum 21.** `0x052a` is a *setter* with a payload
  > byte, sent once by this init and periodically by an unrelated caller; the hint above was half
  > right, in that the command puts the wheel into reporting mode, but there is no reply to it and
  > nothing in either Apple stage reads one. The "thirty re-sends" are not the woken task retrying —
  > `SerialOptoTask` transmits nothing at all — and answering it correctly, with silence, moves no
  > display counter.
- **No autonomous frames without a script.** A real wheel streams while a finger rests on it; this
  model posts a frame per scripted event and nothing in between.
- **Transmit is instantaneous**, so `0x7000c104` bit 31 is never observably set. The register file's
  other bits are ordinary storage, and the ~0x40 bytes of the block that are not one of the four
  registers stay backing memory so `--input-regs` still reports them.
- **Hold** clears frame bit 31 *and* `GPIOA` bit `0x20` (active low), which is the line
  `button_hold()` actually reads. It does **not** clear `DEV_OPTO`, which is what Rockbox does on
  hold; the firmware is left to do that itself if it wants to.
- **Snapshots do not carry the wheel.** `--restore` plus `--wheel` replays the script from step one
  against a machine already past every anchor, so all steps fire at once. The injection is for runs
  from reset.

### 8. Two method notes this cost

**A shared working tree is not a stable measurement substrate.** A control run that had been
byte-identical to the pre-change baseline came back, twenty minutes later, with 35 577 code buckets
instead of 27 985 and a boot that ran materially further — from source that `git diff` said contained
only my own additions. Forcing a rebuild from the same nominal tree restored the original numbers
exactly, which means a concurrent agent's in-flight edit had been compiled into my binary and then
reverted. Every number in this addendum was re-measured from a **private snapshot of `tools/`** built
into a private target directory. `cargo build` finishing in four seconds is not evidence that the
tree it read was the tree you wrote. (See also the ledger's shared-tree entry, and the commit that
swept this model's source into someone else's change.)

**The baseline's "256 ATA commands" was a saturated log**, corrected mid-session to 448 by the
uncapped counter. It is worth noting what that did to the comparison above and what it did not: the
control's 448 and the wedged run's 96 are now both true counts, and 96 was always below the cap and
therefore always real. The direction of that delta was never in doubt — only its size.

### 9. The flag

```text
--clickwheel                   model the four registers instead of answering them with zero
--wheel=SCRIPT                 inject a sequence (implies --clickwheel)
--wheel-click-instr=N          instructions between the frames of a rotate (default 20000,
                               which at --clock=5 is 4 ms per click)
--wheel-no-irq                 model the registers but never raise IRQ 40

SCRIPT := STEP[,STEP…]
STEP   := ('@' N | '+' N) ':' ACTION      @N at instruction N; +N is N after the previous step
ACTION := touch | release | hold | unhold
        | rotate=[+-]N | down=BTN | up=BTN | press=BTN
BTN    := select | menu | play | prev | next        (also center/left/right/ffwd/rew/pause)
N      := digits, `_` ignored, optional `k` or `M`
```

Anchored in **instructions, not microseconds**, because simulated time here is dominated by the idle
task's sleeps — a 600 M-instruction boot reaches 950 s of `usec`, and 120 s when it stops idling —
so a microsecond anchor is not comparable across runs. `rotate` and `press` are expanded into their
individual steps by the parser, before anything runs, and the whole expanded schedule is printed:
the schedule in a log is the schedule that executed, and it can be pasted back into `--wheel=`.

The run report prints frames posted, frames dropped unread, word reads of DATA and how many found a
frame waiting, transmits started, commands refused, interrupt edges, the final `CTRL`/`STATUS`, and
whether the firmware ever set `DEV_OPTO` and `INIT_BUTTONS` — so "the model changed nothing" and
"nobody enabled the device" can never be confused for one another.

---

## Addendum 17: the census — 61 tasks, and the two rendezvous nobody ever arrives at

Measured 2026-08-14 on `retail-boot.sh --clock=5 --stop-when-idle=400000000`, `BUDGET=4000000000`,
which reproduces the session-start baseline to the instruction: **Idle @1 610 256 821**, 38 262 code
buckets, 770 ATA commands, `ata dma: 660 transfers, 33 637 888 bytes`, `pp dma: 4 transfers,
201 216 bytes`, `ide irq: raised 1607, delivered 686, acked 712`, `bcm: 4 commands kicked, 2 frame
updates`, `cpu sleep: 2 685 679 halts`, 4 unmapped reads at `0xea00007a`, `cargo test --release`
20 passed. Every run below is that command plus one instrument.

The idle is real. What this addendum adds is *what the idle is made of*: every task, what each one is
waiting for, and — for the two that matter — who would deliver it and why nothing does.

### 1. The instrument: the frame walk, as a census rather than a sample

Addendum 7 §1 established that the whole scheduler is a pure function of memory: TCB array at
`0x0087198c` stride `0x3c`; the kernel trampoline at `0x00084644` saves `{cpsr, r0-r12, lr, lr}`, so
word 15 of a saved frame is the resume PC; every resume PC is one instruction past the
`bl 0x00084644` inside one of the service wrappers at `0x000a613c..0x000a69cc`; and `r0` in that same
frame is the request block the wrapper built on the task's own stack. That walk has now been done by
hand three times (Addendum 7 §2, Addendum 8 §1, Addendum 11 §2) and each time it produced the one or
two tasks the session was already chasing. `tools/eapp-loader/src/bin/tcb.rs` does it for all of them,
off a `--save-region=sdram` file, in 40 ms.

Two things in it are worth naming because both were wrong first.

**The wrappers do not share a request layout, so the argument offset is derived, not tabulated.**
The scan walks each wrapper from its prologue to its `bl`, tracking the `mov r0, #imm` that sets the
service number and where the caller's `r0` was stashed *while it was still the caller's `r0`*.
`KS_pend` puts its argument at `req+0x08`, `KS_receive` at `+0x0c` via an `stmia {r0, r1}`, `KS_delay`
at `+0x04`. Without the liveness test, `KS_delay`'s later `str r0, [sp, #0x30]` — a scratch pointer
computed into `r0` four instructions earlier — reads as the argument and puts the answer eight bytes
wrong. There are **37** entries into the trampoline and **two** distinct `KS_pend` wrappers
(`0x000a6924` and the timed `0x000a697c`), which a hand-written table would have missed.

**Names come from records that pair a name with an entry point, never from adjacency.**
`extract_symbols`'s pattern A is the weakest source and is overridden by the device table at
`0x00880f8c` (5 words, `{stack, entry, name, n, prio}`) and by the boot-task descriptors the creation
code at `0x000d3b60` memcpy's to `0x108c77b4` (6 words, `{id, prio, stack, size, entry, name}` — the
field order Addendum 7 §2 read by hand, confirmed again here by every priority matching its TCB).
`t_csa`'s entry `0x00284a98` is a two-instruction thunk, not a stack-push prologue, and requiring one
silently dropped it.

**Its control is that it reproduces Addendum 7 §2 exactly, on five independent points**, none of
which it is told: `t_csa` → mailbox 2 · `t_ppfs` → mailbox 3 · `t_graphicsManager` → mailbox `0x16` ·
`DiskMgrTask` → mailbox `0x14` · `t_power` → sem `0x49`. Those four mailbox numbers are the ones a
wrong argument offset would get wrong, and they come out right.

### 2. The census

`tcb _out/sdram-idle.bin --free`. 61 tasks in use — 1 runnable, 47 in `KS_pend`, 8 in `KS_waitm`,
5 in `KS_receive` — plus one terminated slot:

```
 id name                     pri  state entry           tick blocked in    on
  0 -                        127    0x0 0x00000000    717820 RUNNABLE      at 0x000af4a8   <- the idle loop
  1 -                          2   0x40 0x00284f84       539 KS_waitm      mask 0x1081dc38
  2 -                          5   0x40 0x000f162c    301005 KS_waitm      mask 0x149e8334  PowerManager+0x48
  3 -                         11   0x40 0x000f162c         7 KS_pend       sem 0x47         PowerManager+0xafc
  4 -                         10   0x40 0x000f162c         7 KS_pend       sem 0x48
  5 t_csa                      5   0x20 0x00284a98    198496 KS_receive    mailbox 0x02
  6 t_device                  29   0x40 0x00284aa4    696600 KS_waitm      mask 0x108236ec
  7 t_ppfs                    30   0x20 0x0028533c       539 KS_receive    mailbox 0x03
  8 t_power                    1   0x40 0x00285190    717819 KS_pend       sem 0x49         <- alive
  9 APPLEBOOT                126  0x100 0x002844e0    199062 svc 0x18                       <- TERMINATED
 10 t_graphicsManager         31   0x20 0x00284ea0       216 KS_receive    mailbox 0x16     <- see §4
 11 -                          4   0x40 0x000f162c         6 KS_pend       sem 0x62
 12 -                          5   0x40 0x000df11c         7 KS_pend       sem 0xb1         ICAPTPCameraIOTask
 13 -                         49   0x40 0x000e1b10    717427 KS_pend       sem 0xb2         APPLE_MDFW   <- alive
 14 -                         52   0x40 0x000e1b10       567 KS_pend       sem 0xb3         EventManager
 15 -                         52   0x40 0x000e1b10    717423 KS_pend       sem 0x136        EventManager <- alive
 16 -                         52   0x40 0x000e1b10       567 KS_pend       sem 0xf7         Finder+0x430
 17 -                         52   0x40 0x000e1b10       568 KS_pend       sem 0xf8         EventManager
 18 FirewireTask               4   0x40 0x00284e58    197627 KS_pend       sem 0xb4
 19 -                         49   0x40 0x000e1b10       565 KS_pend       sem 0xbf         <- OptoTask's old slot
 20 SerialOptoTask             4   0x40 0x0028558c        66 KS_pend       sem 0x7f         <- see §6
 21 BacklightTask              4   0x40 0x0028453c    210133 KS_waitm      mask 0x004fc0a8
 22 CNATask                    5   0x40 0x00284a54    197628 KS_pend       sem 0xb6
 23 USBPowerSense              5   0x40 0x002856a4    197707 KS_pend       sem 0xb8
 24 DiskMgrTask               10   0x20 0x00284b0c    696600 KS_receive    mailbox 0x14     <- alive
 25 HoldSwitchTask             3   0x40 0x00284f30    197627 KS_pend       sem 0xba
 26 TopPlugTask                4   0x40 0x0028564c        68 KS_pend       sem 0xbb
 27 HPhoneDetTask              4   0x40 0x00284eb0      1068 KS_pend       sem 0x87
 28 LowBattDebounceTask        4   0x40 0x00284a18        69 KS_pend       sem 0xbc
 29 AccessoryDetectTask        4   0x40 0x00284338        70 KS_pend       sem 0xbd
 30 RTCTimerMgr                4   0x40 0x00285460        70 KS_pend       sem 0x88
 31 AlarmTask                  4   0x40 0x00284374        70 KS_waitm      mask 0x1087a700
 32 WatchdogTask               2   0x40 0x002856f0    710070 KS_pend       sem 0x8c         <- alive
 33 PCFPowerMgr               10   0x40 0x00284ff4        70 KS_pend       sem 0xbe
 34 USBRoleManager            10   0x40 0x000b95a4    197707 KS_pend       sem 0xb9
 35 AsyncPiezo                 4   0x40 0x00285060    197675 KS_pend       sem 0x95
 36 -                          7   0x40 0x00103084    199061 KS_waitm      mask 0x1087d31c
 37 -                          7   0x40 0x00103078    199061 KS_waitm      mask 0x1087e324
 38 RsistrAccsryMgr            7   0x40 0x000febf8    701385 KS_waitm      mask 0x004fc1c0  <- alive
 39 USBAudioTask               3   0x40 0x000c75cc       574 KS_pend       sem 0xf3
 40 -                         15   0x20 0x000d6c68       539 KS_receive    mailbox 0x05
 41..61  twenty-one pooled tasks, all entered through the thread trampoline 0x000e1b10, each
         KS_pend on its own event-queue semaphore.  Bodies recovered from the stack walk:
           42 sem 0x132 · 43 sem 0xc3  ATAWorkLoopIRQTask      45 sem 0xe4  VCIH
           46 sem 0xe6  VCFS            47 sem 0xe8  VCHR      48 sem 0x137 MP3ExampleTask <- alive
           49 sem 0xc2  AudGetInterface  53 sem 0x10e ImagePresentationEngine
           58 sem 0x12f SearchHelperThread                     61 sem 0xc7  Finder
```

The run ends at RTXC tick 717 820. Six tasks are still being scheduled at the end (ticks ≥ 696 000):
the idle loop, `t_power`, `WatchdogTask`, `RsistrAccsryMgr`, `t_device`/`DiskMgrTask`, and the pooled
tasks 13/15/48. **`MP3ExampleTask` is task 48, it is alive, and it is sitting in its own
`TimerEventManager` pump** — the application layer is running an event loop and being ticked. That is
not a machine that failed to start. It is a machine with nothing to do.

Everything from 18 to 39 is a device task waiting for hardware that is not there — no accessory, no
headphone, no USB host, no alarm. A real iPod on a bench sits in exactly that state. **Two entries do
not fit that description, and they are the two the display and the wheel run through.**

### 3. Three things the census settles for free

- **`APPLEBOOT` terminated, at tick 199 062.** TCB 9 is `state = 0x100`, priority 126 — the free-slot
  priority — with its entry still `0x002844e0` and its last saved frame resuming in service `0x18`,
  the termination wrapper at `0x000a6860`. Addendum 15 §6 inferred this from an arrival at
  `0x00284538`; the TCB says it outright, and says when.
- **`OptoTask` ran once and its slot was recycled — Addendum 14 §3's TCB attribution is wrong.**
  `--enterlog=0x00284f90` fires exactly once, `@49 678 867`, with RTXC's task-entry register fill
  (`lr=0xeeeeee13, r0=0x13, r1=0x11111113 …`), whose low byte is the task id: **19**. TCB 19 at idle
  is a pooled thread on sem `0xbf`. Addendum 14 §3 identified TCB **18** as `OptoTask` because
  `0x10874abc + 0x400 = 0x10874ebc` matched the registry's stack for `OptoTask` — but TCB 18's *entry*
  is `0x00284e58`, which the device table at `0x00880f8c` pairs by pointer with the name
  `"FirewireTask"`. The stacks merely abut. The sentence *"RetailOS's click-wheel task has been
  executing against `0x7000c140` for the whole boot"* dies with it, and it was already contradicted by
  Addendum 16 §4's zero reads.
- **The mailbox mechanism works; only `0x16` is silent.** `DiskMgrTask` is blocked in `KS_receive` on
  mailbox `0x14` with tick 696 600, and `t_csa` on mailbox 2 with tick 198 496. Same primitive, same
  wrapper, same code path, different number. That is a matched positive control for §4 that costs no
  run at all.

### 4. The display: mailbox `0x16` has never been sent to, in 1.61 G instructions

`t_graphicsManager`'s entry `0x00284ea0` is `bl 0x00188f98` (singleton init) then `b 0x00189060`, its
pump; the pump calls `0x00188f60`, which is

```
00188f60  stmdb sp!, {r4-r6, lr}
00188f6c  mov r0, #0x16
00188f70  mov r1, #0x0
00188f74  bl  0x000a66a4      ; KS_receive(mailbox 0x16)
00188f78                      ; <- the resume address the census reads off TCB 10's stack
```

and the only thing in the image that posts to that mailbox is `0x00189008`:

```
00189028  ldr r0, =0xaaaa0001      ; the message tag
00189030  stmia r1, {r0, r5-r7}    ; a four-word message
00189038  mov r0, #0x16
0018903c  mov r3, #0xab
00189040  mov r2, #0x4
00189044  bl  0x000a6770           ; KS_send(mailbox 0x16, …)
```

One run, `--enterlog=0x00189008,0x00188f60,0x0016b044`:

| watched | arrivals |
|---|---|
| `0x00188f60` — the receive | **1**, `@50 448 522` |
| `0x00189008` — the send | **0** |
| `0x0016b044` — control (`MP3ExampleTask`'s body, fires once) | 1 |

**`t_graphicsManager` has gone round its pump exactly once in the entire boot and has been blocked on
its first message ever since.** Nothing has asked RetailOS's display server to do anything. That is
the silent panel, stated as a count rather than as an inference from pixels.

### 5. Why: the view's draw target is NULL, and the one instruction that sets it never runs

`0x00189008` has exactly one caller, `0x00180f04`, inside the function at `0x00180e54`; that has
exactly one caller, `0x001ae0fc`, inside `0x001ae09c`. And `0x001ae09c` is the fork:

```
001ae0e8  bl  0x00180ce8           ; setDrawTarget(view, target)  =  str r1,[r0,#0xa0]; bx lr
001ae0ec  ldrb r0, [r4, #0x104]
001ae0f4  ldr  r0, [r4, #0x100]
001ae0f8  beq 0x001ae104
001ae0fc  bl  0x00180e54           ; draw AND post to mailbox 0x16
001ae104  bl  0x00180cf0           ; draw only
```

Both draw entry points open with the same two-condition gate — `0x00180cf0` at `+0x08` and
`0x00180e54` at `+0x08`, instruction for instruction:

```
00180cf8  ldr   r0, [r0, #0xa0]    ; the draw target
00180d00  cmp   r0, #0x0
00180d04  ldrneb r0, [r4, #0x9d]
00180d08  cmpne r0, #0x0
00180d0c  beq   0x00180e4c         ; either is zero: return, having drawn nothing
```

Two runs bisect it. First, `--enterlog=0x001ae09c,0x00180e54,0x00180cf0,0x0016b044`:

```
0x0016b044 x1        (control)
0x00180cf0 x70       from lr=0x00181040, first @1 069 253 747, continuing to the end
0x00180e54 x0
0x001ae09c x0
```

Then, watching the instruction on each side of the gate,
`--enterlog=0x00180cf0,0x00180d10,0x00180e4c,0x00189184,0x0016b044`:

```
0x00180cf0 x70       arrivals at the function
0x00180e4c x70       arrivals at the bail-out
0x00180d10 x0        arrivals at the first instruction past the gate
0x00189184 x1        the graphics-manager init helper, once
0x0016b044 x1        (control)
```

**Seventy for seventy.** Every draw attempt RetailOS makes — and it makes one every ~2–4 simulated
seconds from `@1.069 G` onward — returns without touching a pixel. `0x00180cf0` and `0x00180e4c`
appearing in the same run with the same count is the instrument proving itself live at both ends.

`--storeaddr` on the field itself closes it. The two view objects are `0x13c9bf7c` and `0x13c9c2bc`
(vtable `0x0066b94c`, read out of the enterlog's `r0` and confirmed against the SDRAM dump), so
`+0xa0` is `0x13c9c01c` and `0x13c9c35c`. Watching those, with each object's own neighbouring word
`+0x9c` as a control matched in width, region and allocation — 148 stores logged against a 2 000 000
cap, so nothing saturated:

```
0x001ac84c -> [0x13c9c01c] = 0x003716c4   @859873988   ; the heap allocator's block header
0x001810ec -> [0x13c9c01c] = 0x00000000   @859895797   ; the CONSTRUCTOR:  str r1,[r0,#0xa0], r1 = 0
0x001810f8 -> [0x13c9c018] = 0x00000001   @859895800   ; the same constructor: strb r1,[r0,#0x9c]
                                                       ; and nothing after, for 750 M instructions
```

`0x001810c4` is the constructor; it writes the vtable, then `[+0xa0] = 0`, `[+0x9d] = 0`,
`[+0x9c] = 1`. Both fields are still those values in the idle dump. **The view is never attached to
anything to draw into, because `0x001ae09c` — the only caller of the setter `0x00180ce8` — is never
entered.**

So the display chain is closed end to end and its break is a single named function:

```
0x001ae09c  never entered
   -> 0x00180ce8  [view+0xa0] never assigned  -> 70 draw attempts, 70 bail-outs
   -> 0x00180e54  never entered
      -> 0x00189008  never entered  -> mailbox 0x16 never sent to
         -> t_graphicsManager  parked since tick 216 of 717 820
```

**What could not be established: why `0x001ae09c` is never entered.** It has six call sites
(`0x001acce8`, `0x001acdc4`, `0x001ad7a4`, `0x001adcb4`, `0x001ae04c`, `0x001ae2d0`) and none of them
was measured. That is the next command, not a conclusion.

### 6. The wheel: `KS_pend(0x7f)`, and the complete chain from an interrupt to the task

`SerialOptoTask` is nine instructions long before it blocks:

```
0028558c  stmdb sp!, {r4-r8, lr}
00285590  bl  0x002659d4           ; if it returns 0, terminate
002855a4  bl  0x00283e20           ; the opto init
002855a8  ldr r6, =0x7000c000
002855b8  mov r0, #0x7f
002855bc  bl  0x000a6924           ; KS_pend(0x7f)     <- TCB 20 has been here since tick 66
002855c0  ldr r0, [r4, #0x8]       ; the resume address the census reads
```

`--iscan=0xe3a0007f` over the image finds eleven `mov r0, #0x7f`, and **exactly one of them is
followed by an RTXC call: this one.** No instruction anywhere names `0x7f` to `KS_signal`. The
producer is not an instruction — it is a *return value*, and the whole chain is in the image:

```
00283e20  the opto init: enable 0x60006000 bit 16; CTRL 0x7000c100 := 0x600a1f00; clear STATUS
          bits 27..26; CPU_HI_INT_EN (0x60004124) := 0x100  <- IRQ 40; STATUS bit 24; transmit
          0x8001052a.  It registers no handler, and returns 0.

00289e60  the ARM IRQ exception entry — banked-register save, kernel stack 0x109368d8,
00289e94    bl 0x00277128 , and `ldmia sp!, {r0-r12, lr, pc}^` to return from the exception

00277128  ldr r4, [0x60004000]      ; CPU_INT_STAT
0027713c  ldr r5, [0x60004100]      ; CPU_HI_INT_STAT
002771a4  tst r4, #0x40000000       ; <- THE GATE
002771a8  beq 0x00277200            ;    not set: delegate to the object-table demux and stop
002771dc  tst r5, #0x100            ; hi-bank bit 8 = IRQ 40 = the click wheel
002771e4  bl  0x00281350            ; the streaming decoder
002771f4  mov r0, r6 ; b 0x000a5fa0 ; RTXC's ISR epilogue, which signals the id it is handed

002813d8  …decoder tail: mark the event byte, ack STATUS bit 26, re-arm CTRL bits 30..29,
002813f8  mov r0, #0x7f             ; <- and RETURN 0x7f
002813fc  ldr pc, [sp], #0x4
```

`0x00281350` returns the exact semaphore `SerialOptoTask` is pended on. That is the pairing this
addendum was asked for, and it closes without a gap.

### 7. Why nothing posts it: `CPU_INT_STAT` bit 30 is an aggregate, and we never set it

Two independent facts, both measured:

**The handler table has no entry for the wheel, and never gets one.** The interrupt controller object
is at `0x1086f788` — found by scanning SDRAM for `[obj+0x574] == 0x60004024` and
`[obj+0x578] == 0x60004124`, unique in 64 MB, then confirmed the other way: the IRQ vector at
`0x0012763c` loads `0x1084be48`, and `[0x1084be48 + 4]` is that pointer. Its tables are
`obj+0x08+4·src` (handler objects), `obj+0x108+4·src` (raw handlers) and `obj+0x208+4·id`
(logical id → source). At idle, **8 of 64 sources have a handler** — src 0, 2, 4, 13, 23, 26, 27, 39 —
and source 23's `0x000e09e4` is the IDE interrupt the run reports delivering 686 times, which is the
positive control that the decoding is right. **Source 40 is null in both tables.**
`--enterlog=0x001fc730,0x001fc79c` says it always was: **8 registrations in the whole 1.61 G boot**,
all between `@48.6 M` and `@51.8 M`, logical ids `{0, 2, 4, 0x0d, 0x18, 0x1b, 0x1c, 0x26}`, and never
again. `[obj+0x208 + 4·0x29] = 0x28`, so the wheel is logical id 41 — not among them.

**But the object table is not the path the wheel would take.** `--enterlog=0x00277128` reports
**65 536 arrivals — the log's exact cap, so this is a floor and not a count** — every one from
`lr=0x00289e98`. The raw-`INT_STAT` dispatcher *is* the installed IRQ service routine, and it reaches
the wheel decoder directly with no registration involved. It only never gets there because of
`0x002771a4`: `tst r4, #0x40000000`.

Bit 30 of the low bank is not a source. Three attestations:

1. **Rockbox** `firmware/export/pp5020.h` names it, immediately above the `32+n` sources:
   `#define HI_IRQ 30` / `#define HI_MASK (1 << HI_IRQ)` — with `I2C_IRQ (32+8)` four lines below,
   which is Addendum 16's IRQ 40.
2. **Apple's ISR** tests it *before* it looks at `CPU_HI_INT_STAT` at all.
3. **Apple's object demux** at `0x001fc52c` keeps `0x40000000` in `[intc+0x528]` and does
   `bics r1, r0, r1` — it masks that bit out of the low status before demuxing, which is a statement
   that nothing lives there. And no handler is registered for source 30, in a run where eight are.

Our model never raises it. `lib.rs`:

```rust
self.mem.write32(CPU_INT_STAT, pending & enabled);          // sources 0..31 only
self.mem.write32(CPU_HI_INT_STAT, pending_hi & enabled_hi);
```

So a click-wheel frame raises hi-bank bit 8, the CPU takes the interrupt, Apple's ISR reads a low
status with bit 30 clear, skips its entire hi-bank arm, and hands the interrupt to the object demux —
which finds a null slot for source 40, returns without touching `0x7000c104`, and is re-entered
immediately because the line is a level. **That is Addendum 16 §6's wedge, and this is its cause.**

### 8. The A/B — and what it does *not* fix

One line, against three arms of the same recipe at `BUDGET=4000000000`:

```rust
let hi_agg = if pending_hi & enabled_hi != 0 { 1 << 30 } else { 0 };
self.mem.write32(CPU_INT_STAT, (pending & enabled) | hi_agg);
```

| | baseline | + HI aggregate | + HI aggregate, 14 wheel events from `@60 M` |
|---|---|---|---|
| stop | Idle @1 610 256 821 | Idle @1 610 232 373 | Idle @1 610 274 677 |
| code buckets | 38 262 | 38 265 | 38 334 |
| ata commands | 770 | 770 | 770 |
| ata dma | 660 / 33 637 888 | 660 / 33 637 888 | 660 / 33 637 888 |
| pp dma | 4 / 201 216 | 4 / 201 216 | 4 / 201 216 |
| bcm | 4 kicked, 2 frames | 4 kicked, 2 frames | **4 kicked, 2 frames** |
| frames read by RetailOS | — | — | **17 posted, 0 dropped unread, 17 reads all with a frame waiting** |
| `0x00281350` entered | 0 | 0 | **14**, from `lr=0x002771e8` |
| `SerialOptoTask` tick | 66 | 66 | **2 144** |
| `0x001ae09c` entered | 0 | 0 | **0** |
| `0x00189008` entered | 0 | 0 | **0** |
| `t_graphicsManager` tick | 216 | 216 | **216** |

Read the middle column first: **the fix is inert without wheel traffic.** 24 448 instructions out of
1.61 G, three code buckets, and every device counter identical. Read the right column next. RetailOS's
streaming decoder at `0x00281350` — entered **zero** times in all four configurations Addendum 16
tried — is entered fourteen times; every posted frame is read instead of 59 of 63 being overwritten
unread; there is no wedge; and `SerialOptoTask` wakes from `KS_pend(0x7f)` for the first time in the
project's history. The chain in §6 runs end to end.

**And the screen is still blank.** `0x001ae09c` and `0x00189008` are still zero, `t_graphicsManager`
is still on tick 216, and the framebuffer still reports the boot ROM's `4 commands kicked, 2 frame
updates`. Behind the cleared gate, the wheel's *next* blocker is the one Addendum 16 §7 already named
and refused to invent: `SerialOptoTask` now runs, gets no answer to `0x8001052a`, and re-sends it —
30 times in this run.

**Not landed.** Addendum 15 §6's rule applies verbatim: a model change wants its own re-run of
everything the old model produced, and the baseline fingerprint in this file and in `NEXT.md` is
`Idle @1 610 256 821 / 38 262 buckets`. The patch is one line, its evidence is above, and landing it
is a slice with a re-measurement in it. `tools/` is at the pre-fix source as committed, and
`cargo test --release` is 20 passed.

### 9. The ranking, and the answer

**The prompt's own hypothesis is the thing this addendum falsifies.** It asked for a candidate that
explains *both* the silent display and the ignored wheel. There is no such candidate. §8 is the
control: the one change that completely explains and clears the ignored wheel moves nothing at all on
the display, in the same run, with every display counter unchanged. They are two independent walls.

So, ranked:

1. **The single most likely gate into the UI phase is `0x001ae09c` never being entered.** It is the
   only route to `[view+0xa0]`, and through it the only route to `t_graphicsManager`. Everything
   downstream of it is alive and correct: the view objects exist, a redraw is requested every 2–4
   simulated seconds, the display server is up and blocked on its mailbox, and the mailbox primitive
   demonstrably works for `0x14` and `0x02`. One function, six call sites, none measured.
   **Category: unknown, and honestly so** — it is not (a), because nothing here is hardware; the
   candidates left are (b) a model error upstream that makes some predicate false, or (d) a genuine
   dependency on something absent from the disk image, and this pass cannot choose between them.
2. **`CPU_INT_STAT` bit 30 — category (b), a model error**, second-sourced three ways, one line,
   with a matched A/B. It fully explains the ignored wheel and Addendum 16 §6's wedge. It is not the
   UI gate.
3. **The reply to `0x8001052a` — category (a)/(d)**, unchanged from Addendum 16 §7 and now the wheel's
   live blocker rather than a hypothetical one. **Wrong on both counts — Addendum 21.** It is
   category (b), a model that classified a write as an unanswered question, and it was never a
   blocker: nothing in either Apple stage waits for a reply to it.

### 10. Predictions that measured out to nothing

- **"A single delivery explains both the blank screen and the dead wheel."** Wrong, and §8 is the
  measurement that says so. Worth stating because the framing was mine as well as the prompt's, and it
  shaped two hours of chasing a shared cause that does not exist.
- **"The raw `INT_STAT` dispatcher at `0x00277128` is dead code."** Wrong, and it was a *sampled
  profile* that said so: `--profile-window` over the 400 M-instruction trailing window puts
  `0x00289e90` — the bucket holding `bl 0x00277128` — at **21** samples against `0x00127610`'s
  **416 815**, a ratio of 1:20 000, and I read that as "not the live vector." An arrival counter on
  the same address in the same recipe reports the log's 65 536 cap. A profile that samples every 64th
  instruction can alias a short fixed-length path into near-invisibility; **a sampled profile is not a
  census, and cannot support an absence claim.** This is the same lesson as Addendum 15 §7's saturated
  `ata commands`, arriving from the opposite direction — there a cap read as a count, here a sample
  read as a census.
- **"`[view+0xa0]` is set and later torn down."** Wrong. The `--storeaddr` log's last two entries look
  like a set-then-clear 21 809 instructions apart, and the earlier of the two, `0x001ac84c`, is a heap
  allocator writing a block header into memory that was still free. Disassembling it — rather than
  reading its shape — showed `bic r2, r2, #0xc0000003` two instructions above. Addendum 15's rule 4,
  paid for again.
- **"The wheel handler is registered by code that runs later"** (Addendum 16 §6's closing inference).
  Not wrong so much as beside the point: no handler is *ever* registered, and none is needed. The
  decoder is reached by the ISR's own hard-coded arm.

### 11. Two method notes this one cost

**A `grep` for a byte pattern containing a NUL cannot work, and fails silently by matching something
else.** Three searches in this session were run as
`LC_ALL=C grep -abo $'\xa0\x00\x84\xe5'` — and command substitution strips the NUL, so the pattern
becomes three bytes and matches a different instruction entirely. It reported **zero** stores to
`[rN, #0xa0]` in an image where there are 114, and it reported a clean absence for
`mov r0, #0x7f`-followed-by-`KS_signal` that I nearly published. It was caught only by running the
same search for an offset whose answer I already had on screen — `str r0, [r4, #0x48]` at
`0x001d9904`, which it also reported as zero. `dis --iscan=WORD[:MASK][:FOLLOW]` replaces it: a masked
word scan through the same decoder the interpreter uses, with register wildcards, and its control is
that same known store.

**The census's cheapest finding was one nobody asked for.** TCB 9 being `state = 0x100` with a resume
in the termination service answers "did `APPLEBOOT` finish" in a field read, and TCB 20's tick of 66
answers "has the wheel semaphore ever been posted" without an instrument at all — a task's `tick` is a
signed statement about when it last ran, and for a task that blocks in one place it is a complete
history. Both were sitting in a dump that already existed.

---

## Addendum 18: Wall A's boundary is one instruction — `0x001adf58`, six arrivals, never taken

Measured 2026-08-14 across eleven runs of `retail-boot.sh --clock=5 --stop-when-idle=400000000`,
`BUDGET=4000000000`. **The baseline moved under this pass and every "never" below is against the new
one.** Commit `55854a4` landed Addendum 17 §8's `CPU_INT_STAT` bit-30 aggregate in
`tools/eapp-loader/src/lib.rs`, so the fingerprint is now the "+ HI aggregate" column of that table,
not the one printed everywhere else in this file:

```
Idle @1 610 232 373      38 265 code buckets       770 ata commands
ata dma 660 / 33 637 888   pp dma 4 / 201 216       bcm 4 kicked, 2 frame updates
ide irq raised 1607, delivered 685, acked 712       cpu sleep 2 686 784 halts
4 unmapped reads at 0xea00007a                      cargo test --release: 20 passed
```

All eleven runs reproduce that to the instruction. `NEXT.md` and Addendum 17's own prose still carry
the pre-fix `1 610 256 821 / 38 262 / delivered 686`; that is stale, not a discrepancy.

### 1. The command from Addendum 17 §5, run — and it does not settle it

*"Settled when the six call sites of `0x001ae09c` … have been measured and the one that should fire
is named."* One run, all six watched, `0x0016b044` as control:

| watched | arrivals |
|---|---|
| `0x001acce8` · `0x001acdc4` · `0x001ad7a4` · `0x001adcb4` · `0x001ae04c` · `0x001ae2d0` | **0**, every one |
| `0x0016b044` — control | 1 |

`--enterlog` fires on **PC arrival**, not on the branch being taken, so this is stronger than "the
branch was not taken": for three of the six the enclosing instruction was never *reached*. The
answer is not one of the six. The dead region is bigger than §5 mapped, and the question moves up.

### 2. The class, named: a descriptor-built list widget with three implementations, chosen by one byte

`0x001ae09c` is not a function that some caller forgot. It is **one of three bodies** of the same
operation, and a single byte at `[this+0xb1]` picks which. Four separate dispatchers in the image
agree on the encoding, and disassembling any of them shows it:

```
001acca8  ldrb r1,[r0,#0xb1]   0 -> 0x001ad7ac   1 -> 0x001adb04   2 -> 0x001ad70c   ; "show"
001accc8  ldrb r1,[r0,#0xb1]   0 -> 0x001acfc0   1 -> 0x001ad304   2 -> 0x001ae09c   ; "value changed"
001ad2e0  ldrb r3,[r0,#0xb1]   0 -> 0x001acdd0   1 -> 0x001ad234   2 -> 0x001acd88   ; "scroll by n"
001adffc  ldrb r0,[r4,#0xb1]   0 -> 0x001ae118   1 -> 0x001acbd0   2 -> 0x001ae09c   ; vtable paint
```

Where the byte comes from is measured, not inferred. The class is built by the factory `0x001aca80`
(`mov r0,#0x118` — a 280-byte object) whose fourth argument is a **static descriptor record**, and
the constructor `0x001ae3b8` ends with

```
001ae448  ldrb r0, [r5, #0x50]     ; r5 = the descriptor
001ae44c  strb r0, [r4, #0xb1]     ; the mode
```

`--enterlog=0x001aca80,0x001ae44c`:

```
0x001aca80  lr=0x0021a5cc  r3=0x004d1b24   @246 483 088     -> 0x001ae44c r0=0  (mode 0)
0x001aca80  lr=0x0021a5cc  r3=0x004d1b80   @859 931 670     -> 0x001ae44c r0=2  (mode 2)
```

Two instances in the whole boot, and `--wordref=0x0066daf4` over a 64 MB SDRAM dump returns four
hits — `0x001ae458` and `0x001ae4a8`, the constructor's own literal pool in the mirrored image, and
**two objects**, at `0x13c9bd28` and `0x13ca9e70`. So that is a census, not a sample. Reading the descriptors
straight out of the image agrees on the second source: `[0x004d1b24 + 0x50] = 0x00`,
`[0x004d1b80 + 0x50] = 0x02`.

The descriptors are a **static table compiled into the firmware**, not resource-image data:
`--wordref=0x4e6f6e65` (the `'None'` FourCC every record carries at `+0x4`) finds **799** of them,
running from `0x004cc188`. Each record holds `{parentId, 'None', id:u16, …, mode:u8}`; the two that
matter are id `0x509c` (mode 0) and id `0x50a1` (mode 2). Both ids are confirmed a third way, by
`--storeaddr` on the objects' own `+0x1c`: written **once each**, by the one instruction
`0x0021afc8  strh r1,[r4,#0x1c]`, out of `[descriptor+0x8]`.

What the class *is* falls out of `0x001acfc0`: `[this+0x114]` is a data source with a count at
`+0x30`, a page size at `+0x50` and a scroll offset at `+0x60`, and `[this+0xac]` is the current
index. **It is a scrolling list.** `0x001ae09c` is list-rendering strategy 2 of 3, and it is the only
one of the three that reaches the display server.

### 3. Correction to Addendum 17 §5 — `0x00180ce8` has FIVE callers, not one

§5 says *"`0x001ae09c` — the only caller of the setter `0x00180ce8` — is never entered."* `--xref`
says otherwise:

```
=== branches to 0x00180ce8 ===
  0x0016e7b8  bl     0x0016f3b4  bl     0x0016f3e0  b
  0x001ad1c4  bl     0x001ae0e8  bl                       5 total
```

The empirical claim survives; the reason attached to it does not. One run watching the setter and
all five of its callers, with `0x00180cf0` as the control — matched in call mechanism, in region and
in code path, being the *sibling* draw entry point of the same class:

| watched | arrivals |
|---|---|
| `0x00180ce8` — the setter itself | **0** |
| `0x0016e284` · `0x0016e7ac` · `0x0016e7b8` · `0x0016f3b4` · `0x0016f3e0` · `0x001ad1b8` · `0x001ad1c4` · `0x00180ea0` · `0x00180f74` | **0**, every one |
| `0x00180cf0` — control | 70, last at `@1 597 176 656` |

So the correct statement is stronger than §5's: **no instruction in the image ever calls the setter
at all**, and the control was still firing 13 M instructions before the stop.

While disassembling that: `0x00180e54`'s tail is `strb r5,[r4,#0x9d]; str r5,[r4,#0xa0]` with `r5 = 0`
— the draw-and-post path **clears** the draw target on the way out. So `[view+0xa0]` would read as
zero in an idle dump of a *healthy* machine too, and Addendum 17's `--storeaddr` result is
consistent with drawing having happened. It is the arrival counters, not the field, that carry the
absence.

`0x00180e54`'s uniqueness does hold, and it is what makes this whole chain load-bearing:
`--xref=0x00189008` → 1 (`0x00180f04`, inside `0x00180e54`); `--xref=0x00180e54` → 1 (`0x001ae0fc`,
inside `0x001ae09c`); `--wordref` on both → 0, so neither is reachable through a function pointer.
**Mailbox `0x16` has exactly one road to it and it runs through `0x001ae09c`.**

### 4. The boundary, as two counts in one run

Walking up from the six dead sites reaches live code in two hops. `0x001ae214` is the class's
`handleEvent` — vtable slot `+0x30`, the slot the event dispatcher calls at
`0x0014a318..0x0014a328` — and it **runs**:

```
0x001ae214  x6    all from lr=0x0014a32c, r0 ∈ {0x13ca9e70, 0x13c9bd28}
```

Bisecting its type switch in a second run says all six are the same type:

| arm | event type | arrivals |
|---|---|---|
| `0x001ae230` | 4 or 5 — **key / scroll** | **0** |
| `0x001ae27c` | > 13 | **0** |
| `0x001ae28c` · `0x001ae2b0` · `0x001ae2bc` · `0x001ae2d0` | 16 — the other route to `0x001ae09c` | **0** |
| `0x001ae2d8` | **13** | **6** |
| `0x001ae318` | 15 | 0 |
| `0x001ae3a4` | unhandled | 0 |

**In 1.61 G instructions this widget has received six events and all six were type 13.** It has never
been handed a key or scroll event at all.

Type 13 tail-calls vtable slot `+0xf0` = `0x001adf34`, twelve instructions long, and that is the last
live code on the chain:

```
001adf34  stmdb sp!, {r4, lr}
001adf38  ldrsh r2, [r1, #0xc]      ; the event's code
001adf3c  ldrsh r3, [r0, #0x1c]     ; MY id
001adf40  mov   r4, #0x0
001adf44  cmp   r2, r3              ; <- gate 1
001adf48  ldreq r1, [r1, #0x8]      ; the event's parameter
001adf4c  ldreq r2, =0x5656616c     ; 'VVal'
001adf50  subeqs r1, r1, r2         ; <- gate 2
001adf54  moveq r4, #0x1
001adf58  bleq  0x001accc8          ; <- THE BOUNDARY
001adf5c  mov   r0, r4
001adf60  ldmia sp!, {r4, pc}
```

The block is straight-line predicated code, so PC arrives at every instruction in it on every call —
which makes the register capture a complete record. `--enterlog=0x001adf44,0x001adf50`, all six
arrivals:

```
@979 921 037   r2=0x000000a9  r3=0x0000509c        @979 924 074   r2=0x000000a9  r3=0x000050a1
@1 074 247 730 r2=0x00004296  r3=0x0000509c        @1 074 251 072 r2=0x00004296  r3=0x000050a1
@1 074 260 391 r2=0x00005082  r3=0x0000509c        @1 074 263 733 r2=0x00005082  r3=0x000050a1
```

Three distinct notification codes — `0xa9`, `0x4296`, `0x5082` — against two widget ids, `0x509c`
and `0x50a1`. **Six deliveries, zero matches, and gate 2 is never even evaluated.** A final run
watching the branch and its target together states it as a pair:

| watched | arrivals |
|---|---|
| `0x001adf58` — the `bleq` | **6** |
| `0x001accc8` — its target | **0** |
| `0x00180cf0` · `0x0016b044` — controls | 70 · 1 |

Two addresses one instruction apart in control flow, in the same run, one live and one dead. That is
the boundary, and it needs no interpretation: **live code looked at a notification, saw it was
addressed to somebody else, and returned.**

### 5. …and behind the boundary, two more NULLs with one writer each

Passing the gate would not have drawn anything, and this is worth saying plainly because it is the
difference between "the trigger is missing" and "the trigger is one of several missing things".
`0x001ae09c` opens with `ldrb r0,[r0,#0x105]; cmp r0,#0; beq <return 0>`, and `--storeaddr` on that
byte in both objects, with each object's neighbouring `+0x104` as a matched control, logs 23 stores:

```
0x001ae42c -> [0x13ca9f75] = 0   @246 502 554     ; the CONSTRUCTOR, both objects,
0x001ae42c -> [0x13c9be2d] = 0   @859 956 153     ; once each, and never again
0x001ae428 -> [0x13ca9f70] = 0   @246 502 553     ; [this+0x100] — the view pointer
0x001ae428 -> [0x13c9be28] = 0   @859 956 152
```

(The control's traffic is all `pc 0x00000100/0x00000108` — the memcpy in low memory scribbling XML
through the same bytes *before* the object was allocated there. It proves the watch is live at those
addresses; it says nothing about the object.)

`--iscan` names every instruction that could undo either zero, and there is one of each inside the
class:

```
--iscan=0xe5c00105:0xfff00fff    strb rN,[rM,#0x105]   0x001ad198 (teardown, writes 0)
                                                       0x001ad794 (writes 1)   <- the only one
                                                       0x001ae42c (constructor, writes 0)
--iscan=0xe5800100:0xfff00fff    str  rN,[rM,#0x100]   0x001ad784, 0x001ae428  (43 image-wide,
                                                                                2 in this class)
```

Both live in the same eight instructions, and those eight instructions are `0x001ad70c`'s tail:

```
001ad768  bl 0x0019ec64            ; the view factory singleton
001ad774  ldr r2,[r1,#0xbc]        ; ask it for
001ad778  ldr r1,=0x000050a3       ;   the view with descriptor id 0x50a3
001ad77c  bx  r2
001ad780  cmp r0,#0x0
001ad784  str r0,[r4,#0x100]       ; <- the view pointer
001ad788  ldmiaeq sp!,{r4-r6,pc}   ; NULL -> give up, draw nothing
001ad790  bl  0x00149f28           ; register as an event listener
001ad794  strb r6,[r4,#0x105]      ; <- the "I have something to draw" flag, r6 = 1
001ad79c  strb r5,[r4,#0x104]
001ad7a4  b   0x001ae09c           ; and paint
```

So `0x001ad70c` is not one of six equal call sites. **It is the function that manufactures every
precondition `0x001ae09c` tests, and then jumps to it.** Naming what would make RetailOS draw is
naming that one function.

### 6. `0x001ad70c` is the mode-2 arm of `setVisible`, and the base implementation is busy

`0x001ad70c` has exactly one branch to it, `0x001accc0` inside the dispatcher `0x001acca8`;
`0x001acca8` has exactly one, `0x001ae080` inside `0x001ae070`; and `0x001ae070` is
`--wordref`'d to `0x0066db98` = **the class's vtable slot `+0xa4`**, whose tail
`b 0x0021ada8` is that slot's base implementation. The same shape holds for the paint slot `+0x70`:
override `0x001adffc`, base `0x0021acac`.

That gives the matched control this pass needed — same virtual slot, same dispatch mechanism, same
run, base versus override:

| slot | base implementation | this class's override |
|---|---|---|
| `+0xa4` "show" | `0x0021ada8` — **68 arrivals, 11 distinct call sites** | `0x001ae070` — **0** |
| `+0x70` "paint" | `0x0021acac` — **566 arrivals, 5 distinct call sites** | `0x001adffc` — **0** |

The framework shows and paints hundreds of objects per boot. It never shows or paints this one. And
the constructor half of the same class is unambiguously alive: `0x0021af84`, the descriptor-driven
base constructor these two objects are built through, is entered **1 942 times from 18 distinct call
sites** in the same boot — the widget system builds nearly two thousand objects and shows none of
this class.

### 7. The live / dead ledger

Every row below is an arrival count from a run in this pass carrying `0x00180cf0` (70) and
`0x0016b044` (1) as controls; nothing here is inferred from a profile or a shape.

**Live** — `0x0021af84` 1942 · `0x0021afcc` 1942 · `0x0021acac` 566 · `0x00181004` 569 ·
`0x0021ada8` 68 · `0x00180cf0` 70 · `0x001adf34` 6 · `0x001adf44` 6 · `0x001adf50` 6 ·
`0x001adf58` 6 · `0x001ae214` 6 · `0x001ae2d8` 6 · `0x001aca80` 2 · `0x001ae44c` 2 ·
`0x00197b0c` 4 · `0x0014a2c8` 22 305.

**Dead, all zero** — the whole attach/present half of the list widget: `0x001accc8` · `0x001acca8` ·
`0x001acbd0` · `0x001acd88` · `0x001ad188` · `0x001ad1b8` · `0x001ad1c4` · `0x001ad1e0` ·
`0x001ad2e0` · `0x001ad70c` · `0x001ad794` · `0x001ad7ac` · `0x001adb04` · `0x001adffc` ·
`0x001ae070` · `0x001ae09c` and all six of its call sites · `0x001ae118` · `0x001ae230` ·
`0x001ae27c`. The draw plumbing: `0x00180ce8` and all five callers · `0x00180e54` · `0x00189008` ·
`0x00180ea0` · `0x00180f74`. The notification producers: `0x00196eb0` · `0x00196f30` ·
`0x0019700c` · `0x001972f4`. The value model: `0x001be000` · `0x001be0f8` · `0x001be234`. And a
second, independent screen class at `0x0016exxx` (its view at `[this+0x45c]`, its graphics context at
`[this+0x428]`, two of the five setter call sites its own): `0x0016d4b8` · `0x0016e224` ·
`0x0016e4c4` · `0x0016e5a8` · `0x0016e668` · `0x0016e778` · `0x0016f310` · `0x0016f3c4` ·
`0x001d8734`.

### 8. The only code that could have posted a matching notification, and it is dead too

`--wordref=0x0000509c` and `--wordref=0x000050a1` find the two ids in exactly one place outside the
descriptor table: four adjacent literal pools at `0x00196f24`, `0x00196f9c`, `0x00197078` and
`0x00197364`, each sitting beside a pool word `0x5656616c` — `'VVal'`, gate 2's magic. All four
functions have the same body:

```
00196eb0  stmdb sp!, {r4, lr}
00196eb8  ldr  r0,[r0,#0x18]        ; the bound value model
00196ebc  bl   0x001be000           ; get
00196ec0  sub  r1, r0, #0x1         ; step DOWN by one
00196ec8  bl   0x001be2cc           ; set
00196ed4  ldrb r0,[r0,#0x58]        ; the model's kind: 1, 2 or 3
00196ed8  cmp  r0,#1 -> r2 = 0x509c
00196eec  cmp  r0,#2 -> r2 = 0x509f
00196f00  cmp  r0,#3 -> r2 = 0x50a1
00196f1c  bx   r3                   ; postNotification(this, 'VVal', r2)
```

They are **step-the-value-and-notify** handlers — increment, decrement — and `0x50a1` is what they
post when the model's kind is 3. All four measured **zero**, as did `0x001be000` / `0x001be2cc`'s
neighbours `0x001be234` and `0x001be0f8`, in a run whose controls both fired.

The widget's *other* road to `0x001ae09c` closes on the same thing from the other side:
`0x001ae300` (key code `0x57`/`0x58`) → `0x001ad1e0` → `0x001ad2e0` → mode 2 → `0x001acd88` →
`0x001acdc4  bl 0x001ae09c`. Both roads are input roads.

### 9. The answer, and the category

**The boundary is `0x001adf58`, and the live side is deciding that the notification it just received
is addressed to another widget.** `0x001ae214` → `0x001adf34` runs six times; `0x001accc8` and
everything under it runs never; the codes are `0xa9`, `0x4296`, `0x5082` and the ids are `0x509c`,
`0x50a1`.

**What would make it draw is `0x001ad70c`** — the mode-2 arm of the widget's `setVisible`, which asks
the view factory for descriptor `0x50a3`, stores it at `[this+0x100]`, sets the `[this+0x105]` flag
that is `0x001ae09c`'s first gate, and jumps to `0x001ae09c`, which binds `[view+0xa0]` to the
widget's own graphics context at `+0xd8`, draws, and `KS_send`s mailbox `0x16`. Nothing calls
`0x001ae070`, in a run where the base implementation of that same vtable slot is called 68 times.

**Category: (b) or (d), and this pass still cannot choose** — but it can say something Addendum 17
could not, and it cuts against that addendum's headline. Every trigger this widget has is an *input*
trigger: two of the four routes into `0x001ae09c` are key/scroll handlers, and the notification that
would take the third is posted only by increment/decrement handlers. In 1.61 G instructions the
widget has received **no key or scroll event of any kind** (`0x001ae230`, `0x001ae27c`: zero).
Addendum 17 §8 concluded the two walls are independent; what it *measured* is narrower — that the
`CPU_INT_STAT` bit-30 fix **alone** moves no display counter. In that run `SerialOptoTask` woke and
then spent itself re-sending `0x8001052a` unanswered, so no decoded wheel event ever became a UI
event. **The link from a working wheel to this widget has never been exercised**, and §8 does not
rule it out. That makes the `0x8001052a` reply — category (a)/(d), unchanged since Addendum 16 §7 —
a candidate for Wall A as well as Wall B, which is not what this file has been saying.

> **Measured, and it is not a candidate for either — Addendum 21.** `0x052a` is a setter with no
> reply. With it modelled, `0x001ae230` is still **0** in a run where nineteen wheel events reach
> the event queue: 4 button posts at `0x000ada4c` and 15 `'Weel'` posts at `0x000cd6a0`. The link
> from a working wheel to the event system was *already* exercised and already works; what has
> never been exercised is the link from those events to **this widget**, and §10's experiment 1 was
> written on the assumption that the delivery was blocked upstream. It is not.

**No one-line fix falls out and nothing was landed.** There is no wrong bit here to flip: the finding
is an absence in RetailOS's own control flow, not a register our model reports incorrectly.

### 10. What would settle it

Two experiments, in order of cost:

1. **Answer `0x8001052a`.** If a modelled reply lets `SerialOptoTask` deliver decoded frames into the
   event system, re-run the sweep in §7. `0x001ae230` going non-zero would connect the walls;
   staying zero would confirm §8's independence at full strength.
2. **Ablate gate 1.** `--restore=` is applied *before* `--poke=` in `trace.rs`, so a snapshot taken
   past the OSOS DMA can be restored with `0x001adf44` patched to `cmp r2,r2`, forcing the six
   notifications through `0x001accc8`. Predicted outcome, stated in advance so it is falsifiable:
   **still no draw** — `0x001ae09c` returns at its first instruction because `[this+0x105]` is zero
   and only `0x001ad794` can change that. If it *does* draw, §5's reading of the constructor is
   wrong.

### 11. Predictions that measured out to nothing

- **"One of the six call sites of `0x001ae09c` should have fired."** All six are zero, including the
  three that are `b` rather than `bl` and therefore were never even reached. The framing — that a
  caller exists which merely took a wrong branch — was wrong: the entire half of the class that
  attaches a view and shows it is dead, and so is a whole second screen class beside it.
- **"`[this+0xb1]` is 0 for both widgets, so mode 2 is never selected."** Read straight off the idle
  SDRAM dump, where `[obj+0xb0]` happens to look like a pointer and made the byte offsets look
  wrong. Measured at the instruction that writes it, the two modes are **0 and 2** — the mode-2 path
  is exactly one of the two objects, and reading a live field out of a dead-object dump nearly cost
  the finding. Two independent sources now agree (the store, and the descriptor byte in the image).
- **"The widget ids come from the `rsrc` image."** No. 799 descriptor records are compiled into
  `OSOS` itself from `0x004cc188`, and `--enterlog` on the factory names the two byte-for-byte:
  `0x004d1b24` and `0x004d1b80`. This file is called *the resource image* and the temptation to route
  every unexplained id through `rsrc` is strong; the table is in the firmware.
- **"`Demo` names this subsystem."** `extract_symbols` reports `Demo+0xc4` for the notification
  producers, and there are `"Demo"` strings at `0x00197860` and `0x0016d8ac`. Both sit in constant
  pools between an epilogue and the next prologue — pattern-A adjacency, exactly the source
  Addendum 17 §1 said to distrust. There is no evidence this is a demo-mode subsystem and the name
  is not used anywhere above.
- **"`t_graphicsManager` is one of several ways to the panel."** `--wordref=0x00180e54` and
  `--wordref=0x00189008` are both **0**, so neither is reachable through a function pointer, and each
  has exactly one branch to it. Mailbox `0x16` has one road.

### 12. The method note this one adds

**`--enterlog` on a predicated block is a register probe, not a branch counter — and that is a
feature.** ARM's conditional execution means `0x001adf44` through `0x001adf58` all arrive on every
call regardless of the comparison, so watching five consecutive addresses in that block yielded five
identical counts and looked useless. It is the opposite: because arrival is unconditional, the
captured `r0-r3` at `0x001adf44` are a *complete* record of both operands of the gate on every
single call — six for six, no sampling, no cap. The same property is what makes
"`0x001adf58` x6 / `0x001accc8` x0" a self-controlling statement: one instruction apart, one live and
one dead, in one run. A branch counter would have reported `0` for both and named nothing.

---

## Addendum 19: working input reaches the ISR and dies before the widget — `0x8001052a` is the join

Measured 2026-08-14, immediately after Addendum 18 named `0x001adf58` as Wall A's boundary and
observed that *every* trigger on the display's last live function is an **input** trigger.

Addendum 17 §8 concluded that the wheel fix "moves nothing on the display", and that was read — by me,
in a report to the operator — as **two independent walls**. Addendum 18 §"Corrections owed" narrowed
it: §8's A/B ran in a machine where `SerialOptoTask` woke and then re-sent `0x8001052a` unanswered,
so **the link was never exercised**. This addendum exercises it.

### 1. The measurement

`retail-boot.sh --clock=5 --clickwheel` with seven scripted events well past the idle point
(`@1650M:touch,+20M:rotate=+12,+20M:rotate=+12,+20M:rotate=-6,+20M:release,+20M:press=select,+20M:press=menu`),
budget 2.5 G, watching the whole display chain with `0x0016b044` as a positive control:

```
0x0016b044  x1      control — fires
0x001ae214  x6      the widget's handleEvent
0x001adf34  x6      the gate: event.code == this->id && event.param == 'VVal'
0x001adf58  x6      the boundary: bleq 0x001accc8, never taken
0x001ad70c  0       manufactures every precondition the draw tests
0x001ae09c  0       the mode-2 body that reaches mailbox 0x16
```

**Six, and six is the same number the run with no wheel traffic at all produces** (Addendum 18: "ran
6 times, all six events type 13"). Same caller, `lr=0x0014a32c`, in both. Seven wheel events —
touch, three rotations, release, and two button presses, every one of them read by Apple's decoder
per Addendum 17 §8 — contribute **zero** additional widget events.

### 2. What that establishes

The input path is live from the pins to `SerialOptoTask` and **dead from there upward**:

```
wheel frames -> Apple's ISR 0x00277128 -> decoder 0x00281350 -> SerialOptoTask wakes
     -> sends 0x8001052a -> UNANSWERED, re-sent 30 times in one run
          -> no event ever posted upward
               -> the widget's 6 events remain the same 6 it gets with no wheel at all
                    -> 0x001adf58 never branches -> nothing draws
```

`0x8001052a` is therefore not a loose end on the wheel. **It is the join between the working input
path and the dead display path**, and it is the only unknown standing in both.

> **Both halves of that diagram are wrong below the first line — Addendum 21.** `0x8001052a` is a
> *setter* (`0x052a`, payload byte at bits 23..16) and the hardware sends no reply to it;
> `SerialOptoTask` never re-sends it, and the "30 times" is a periodic caller with nothing to do
> with the receive path. And events *are* posted upward: in the same recipe, watching the two
> posters rather than only the display chain gives `0x000ada4c` **x4** and `0x000cd6a0` **x15**, the
> latter putting a `'Weel'` message into the UI event queue. What is severed is the last hop, from
> those events to *this widget* — which is where Addendum 18 §4 already put it. `0x8001052a` was not
> the join, and there is no unknown standing in both walls.

### 3. What this does NOT establish

That answering it makes RetailOS draw. Addendum 18 is explicit that a matching notification alone is
insufficient — `0x001ae09c` would return at its first instruction on `[this+0x105]`, and only
`0x001ad70c` sets that flag, and nothing calls `0x001ad70c`. Two absences remain in the chain and
this measurement closes neither. It establishes only that the wheel→widget link is severed at a
named place, and that the previously published "two independent walls" is too strong: **not proven
independent — never connected.**

### 4. Correction, in the reporting rather than the research

The "two independent walls" framing was mine, in a report to the operator, extrapolated from §8's
narrow and correct A/B. §8 measured that the bit-30 fix *alone* moves no display counter. That is
true and remains true. It does not license "independent", because the run it measured had the join
unanswered. The error is the same shape as this project's other two: **a control that proves one
thing, quoted as proving a wider one.**

---

## Addendum 20: the list widgets are built and never shown — Wall A is higher than the dispatch table

Measured 2026-08-14. Addendum 18 established that `0x001ae09c` is list-rendering strategy 2 of 3,
selected by `[this+0xb1]`, and named four dispatchers that agree on the encoding. It left open which
of the three bodies *should* have run. That framing assumed a dispatcher ran at all.

### 1. The measurement

`retail-boot.sh --clock=5 --stop-when-idle=400000000`, budget 4 G, `0x0016b044` as positive control:

| watched | what it is | arrivals |
|---|---|---|
| `0x001ae3b8` | the widget **constructor** | **2** (`lr=0x001acab0`) |
| `0x001acca8` | the **"show"** dispatcher | **0** |
| `0x001ad7ac` · `0x001adb04` · `0x001ad70c` | show, modes 0 / 1 / 2 | **0**, every one |
| `0x001adffc` | the **vtable paint** dispatcher | **0** |
| `0x0016b044` | control | 1 |

### 2. What it means

**Both instances are constructed and neither is ever shown or painted.** The question is not which of
the three list-rendering strategies should have run — no dispatcher is entered, so the mode byte
never gets read. `0x001ad70c` is not "a function nothing calls"; it sits under a *"show"* that is
never issued to this widget at all.

That also disposes of the shape Addendum 18 left implied — that `setVisible` reaching 68 other
widgets from 11 sites while missing this one meant this widget's arm was somehow skipped. Its show
entry point is untouched. Nothing is choosing mode 0 or 1 over mode 2; nothing is choosing.

### 3. Where this puts Wall A

The wall is **above the widget layer**: something decides which screen exists and tells its widgets
to appear, and that decision has never been made. A real iPod draws its main menu without being
touched, so "no input has arrived" (Addendum 19) does not by itself explain a screen that never
shows — unless RetailOS's first screen is itself posted by an event we never deliver.

**Settled when** the caller of `0x001acca8` is identified statically and measured, and the live/dead
boundary above it is named — the same walk Addendum 18 did one level down. `0x001acab0`, the
constructor's caller, is the thread to pull: it builds these widgets, so it knows they exist.

**What this does not establish.** Whether this is connected to Addendum 19's severed input link.
Both are live hypotheses and neither has been tested against the other.

## Addendum 21: `0x8001052a` is a write, and the answer is silence — the wheel reaches the event queue

Measured 2026-08-14, immediately after Addendum 19 named `0x8001052a` "the join between the working
input path and the dead display path, and the only unknown standing in both."

It is not an unknown, and it is not a question. **`0x052a` is a setter with a one-byte payload, the
hardware sends no reply to it, and Apple's own code says so five different ways.** The command was
never the join. Answering it — correctly, with silence — moves no display counter, and the run that
proves that also shows the input chain is alive four functions further up than Addendum 19 measured.

Baseline for this pass is unchanged and reproduced first:

```
Idle @1 610 232 373      38 265 code buckets       770 ata commands
ata dma 660 / 33 637 888   pp dma 4 / 201 216       bcm 4 kicked, 2 frame updates
ide irq raised 1607, delivered 685, acked 712       cpu sleep 2 686 784 halts
4 unmapped reads at 0xea00007a           cargo test --release: 21 passed (20 + the new one below)
```

### 1. The command, disassembled: `set(bool)`, and the payload is a byte at bits 23..16

`--wordref=0x8001052a` over `OSOS_correct.bin` returns **two** words, not one. The second is the
whole API:

```text
00283e10  ldr r1, =0x8000052a
00283e14  orr r0, r1, r0, lsl #16      ; command = 0x8000052a | (payload << 16)
00283e18  b   0x00283fa0               ; the sender
```

Three instructions and a tail branch. It has no stack frame, so it **cannot** read a reply. Its two
callers are one-liners of the same shape — `0x000bbdb0` is `mov r0,#1; b 0x00283e10` and
`0x000b4638` is `mov r0,#0; b 0x00283e10` — and a third caller assembles the same two words by hand:

```text
000b2ce0  cmp r0, #0x0
000b2ce4  ldreq r0, =0x8001052a        ; payload 1
000b2ce8  ldrne r0, =0x8000052a        ; payload 0
000b2cec  b     0x00283fa0
```

`--wordref` finds exactly two `0x8001052a` and exactly two `0x8000052a` in 7.5 MB, and all four are
these senders' literal pools. So the opcode is the low 16 bits, `0x052a`; bits 23..16 are a payload
byte; and the only two values the firmware ever sends are 1 and 0.

The frame layout this implies is the one the other two shapes already use — `bit 31` framing, the
low bits the opcode, the middle bits the payload. `0x8000023a`'s reply carries its buttons at
bits 20..16, in the same field `0x052a` carries its argument.

### 2. Nothing reads a reply — and the boot ROM is the cleanest proof

Five call sites send `0x052a`. **None of them reads `0x7000c140` afterwards**, and three of them
physically cannot:

| site | what it does after the transmit |
|---|---|
| `0x00283e10` | `b` to the sender — no frame, no return path of its own |
| `0x000b2ce0` | same |
| `0x00283e20` (the opto init both Apple stages ship) | `mov r0,#0; ldmia sp!,{r4,pc}` |
| ROM `0x000c9714` | see below |
| ROM `0x000c9634` | byte-identical to the above but for one pool word |

The ROM's copy is the one that settles it, because it is written without a driver around it:

```text
000c9740  ldr r2, =0x8001052a
000c9744  ldr r0, =0x7000c120        ; TX
000c9748  str r2, [r0]
000c974c  sub r0, r0, #0x1c          ; -> 0x7000c104   STATUS |= 0x0c000000
000c975c  sub r0, r0, #0x4           ; -> 0x7000c100   CTRL := 0xc00a1f00  (start + arm)
          …GPIO B bit 4 low, then high…
000c9798  ldr r1, =0x00002710        ; 10 000 iterations of a bare decrement loop
000c97f0  ldmia sp!, {r2, r3, pc}
```

**Write, start, spin a fixed count, return.** It never touches `0x7000c140`, never tests a status
bit, and never learns whether anything came back. Its twin at `0x000c9634` differs in exactly one
word — `0x8000052a` — and the two are called from a power-down helper (`0x000c870c`) and a power-up
sequence (`0x000c88f4`) respectively, in the ROM's own diagnostic.

The third proof is structural, and it is the reductio. The image contains exactly **two** frame
parsers: the ISR decoder `0x00281350` and the polled query `0x00283ea0`. Both accept only
`(f & 0xbc0000ff) == 0x8000001a` or `(f & 0x8000ffff) == 0x8000023a`. `--wordref=0x0000052a` over the
whole image is **0** — no instruction anywhere compares anything against this opcode. A
`0x052a`-shaped reply would therefore fall into the decoder's third arm, set the bad-frame flag at
`[0x1081d998+1]`, and drive `SerialOptoTask` into its receiver-reset path at `0x00285608` —
**seventy-odd times per boot**, on firmware that demonstrably works on real hardware. A device that
replied here would be a device Apple's own driver treats as broken.

Empirically, from the same run: `--readlog=0x7000c140` attributes every one of the 39 word reads to
two addresses, `0x00281364` (RetailOS's decoder, x36) and `0x4000e59c` (the boot ROM, x3). There is
no third reader to consume a reply.

**So the right answer to `0x8001052a` is nothing at all, and the model was already producing the
right bytes for the wrong reason** — refusal-because-unknown rather than silence-because-write-only.
What changes below is that the device now *knows* the command, and the difference is falsifiable.

### 3. What the payload means, second-sourced twice

Payload 1 is "report", payload 0 is "stop reporting". Three independent handles agree:

1. **The accessory-mode byte.** `0x00266b18` writes a mode at `[0x1081de40+1]` and, only when it
   changes, sends payload **1** for mode 0 and payload **0** for modes 1–2 (mode 3 sends nothing).
   `SerialOptoTask` reads that same byte through `0x00266b08` at `0x002855f8` and runs the scroll
   accumulator `0x000dd018` **only while it is 0** (`0x00285600  bleq 0x000dd018`). Payload 1 is
   exactly the state in which RetailOS bothers to decode wheel positions.
2. **The RetailOS power state machine.** `0x001d8198` sends payload 1 on the arm that arms 10 s and
   120 s timers; `0x001d8418` sends payload 0 on the arm that arms a 500 ms one. Wake and sleep.
3. **The ROM pair in §2** — payload 0 on power-down, payload 1 on power-up.

### 4. The "re-sent 30 times" is not a retry, and this matters

Addendum 17 §8 recorded that the woken `SerialOptoTask` "re-sends `0x8001052a` thirty times in one
run," and Addendum 19 built its diagram on that reading. `SerialOptoTask` does no such thing.
Its whole body is nine instructions to `KS_pend(0x7f)` and a loop that never transmits; the init at
`0x00283e20` is called **once**, and `--enterlog` confirms one arrival per boot (`lr=0x002855a8`).

The other sends come from `0x000bbdb0` — the `mov r0,#1` wrapper — on a cadence of roughly one per
20 M instructions, from a caller that has nothing to do with the wheel's receive path. In the 1.61 G
baseline that is **30 sends**; in the 2.5 G run it is 73. The count is a function of run length, not
of whether the previous one was answered. **A repeated command is not evidence of an unsatisfied
one**, and reading it as a retry is what made this look like a live blocker for two addenda.

### 5. The model, and the A/B both ways

`ClickWheel::transmit` now recognises opcode `0x052a`, records the payload as `reporting`, replies
with nothing, and stops counting it as a command we have no evidence for. Autonomous frames are
gated on `reporting`, which **starts off** — a wheel nobody has spoken to has not been told to
report. Refused frames are counted separately from posted ones, so "the script did nothing" and
"the script was refused" can never be confused. The polled query is ungated; it is an explicit
question and the hardware answers questions.

The gate is the falsifiable half, and the boot supplies its own trigger: the **boot ROM** sends
`0x8001052a` at `@238 346` (`--storeaddr=0x7000c120`, `pc = 0x4000e654`), long before RetailOS's own
at `@49 680 335`.

| arm | script | frames posted | suppressed | decoder `0x00281350` |
|---|---|---|---|---|
| A — baseline, no wheel at all | — | — | — | 0 |
| G — `--clickwheel`, no script | — | 3 (the ROM's own query replies) | 0 | 0 |
| B — 36 steps `@1650 M` | after both enables | **39, 0 dropped, 39 reads all with a frame waiting** | 0 | 36 |
| D — 36 steps `@40 M` | after the ROM's enable, before RetailOS's init | 39, **35 dropped unread**, 3 reads | 0 | **0** |
| D2 — 36 steps `@100 k…160 k` | **before** the ROM's enable | **0** | **36** | 0 |
| E — `@200 000:touch, +50 000:rotate=+4` | straddles `@238 346` | 4 | **1** | 0 |

Read A and G first: **the change is inert.** A reproduces the fingerprint to the instruction —
`Idle @1 610 232 373`, 38 265 buckets, 770 ATA commands, `delivered 685`, `pp dma 4 / 201 216`,
4 unmapped reads. G lands where Addendum 17 §8's middle column does (`Idle @1 610 238 773`,
38 268 buckets — the same three extra buckets the wheel's registers have always cost), reports
**0 unknown commands** where it used to report 30, and suppresses nothing.

Read B next: it is **byte-identical** to the same recipe run against the pre-change binary. `diff`
of the two run reports is two lines — the per-PID temp disk name, and the unknown-command line this
addendum exists to retire.

Then read E, which is the whole A/B inside a single run. The `touch` at `@200 000` is refused; the
four clicks from `@250 047` are posted; the boundary between them is the instruction at which the
boot ROM writes `0x8001052a` to `0x7000c120`. One variable, one run, and the variable is Apple's own
command.

D is worth keeping for a different reason: 39 frames posted and **35 overwritten unread**, with the
decoder entered zero times. Those events were legal — the ROM had enabled reporting — and still went
nowhere, because RetailOS's `SerialOptoTask` had not run yet. Addendum 16 §6's scheduling constraint
survives this change untouched; the gate is a second, earlier constraint, not a replacement.

`cargo test --release` is **21 passed**. The new test asserts the three consequences with matched
controls: the set posts no frame *and* the recognised query still does on the very next transmit;
the set is not counted as unknown; and the same script step is refused before the enable, posted
after it, and refused again after `0x8000052a`.

### 6. What it changes above `SerialOptoTask`: nothing at the widget — and Addendum 19's diagram is wrong

`0x001ae214` sees **six** events, all type 13, exactly as it does with no wheel traffic at all. That
part of Addendum 19 stands and is reconfirmed here in four separate runs.

Everything else in Addendum 19's diagram does not. It said:

```text
     -> sends 0x8001052a -> UNANSWERED, re-sent 30 times in one run
          -> no event ever posted upward
```

Both lines are false, and the second is the expensive one. One run, `0x0016b044` as the control
(x1), events at `@1650 M`:

| watched | what it is | arrivals |
|---|---|---|
| `0x00281350` | Apple's ISR decoder | **36** |
| `0x000c953c` | the button-edge dispatcher `SerialOptoTask` calls on every wake | **36** |
| `0x000dd018` | the scroll accumulator (wrap at 0x60 = 96 clicks, delta into `[state+0x10]`) | **32** |
| `0x000ada4c` | post a **button** event | **4** |
| `0x000adaa8` · `0x000adae4` · `0x000adb08` | …past its predicate, into the delivery call | **4 · 4 · 4** |
| `0x000cd6a0` | post a **wheel** event | **15** |
| `0x000adb84` | …its `'Weel'` arm — `0x5765656c` into a 0x1c-byte message via `0x00151a40` | **15** |
| `0x000adb74` | the other arm | 0 |
| `0x001ae214` | the widget | **6** — the same 6 |
| `0x001ae230` | its key/scroll arm | **0** |

**Nineteen UI events are posted by wheel traffic and the widget receives none of them.** The input
path is not severed at `0x8001052a`, or at `SerialOptoTask`, or at the decoder, or at the event
poster. It is live from the pins to a message in the event queue, and the widget at `0x001ae214` is
simply not the thing those messages are addressed to — which is the same finding Addendum 18 §4
reached from the other end, when it caught live code reading a notification and returning because it
was for somebody else.

So the honest statement of where Wall A stands is unchanged in substance and much narrower in
scope: **the widget is not on the receiving end of the wheel's events, and `0x001ad70c` — which
manufactures every precondition the draw tests — is still called by nothing.** Answering the
handshake did not close either absence, and this addendum does not claim it did.

### 7. Corrections owed

- **Addendum 16 §7** — *"`0x8001052a` is unanswered … a strong hint that its reply is what puts the
  transceiver into autonomous streaming mode."* Half right and usefully so: it is the **command**
  that puts the wheel into reporting mode, not a reply to it. The refusal to invent a reply was the
  right call and is now retired on evidence rather than on nerve.
- **Addendum 17 §8** — *"`SerialOptoTask` … re-sends `0x8001052a` thirty times in one run."* The
  count is real; the attribution is not. `SerialOptoTask` sends it once. See §4.
- **Addendum 17 §9 / Addendum 18 §9** — the reply to `0x8001052a` is listed as a live category
  (a)/(d) blocker and, in Addendum 18, as "a candidate for Wall A as well as Wall B". It is neither.
  It is category (b) — a model that classified a write as an unanswered question — and closing it
  moves nothing on either wall.
- **Addendum 19 §2** — the chain diagram's last three lines are wrong. §6 above is the replacement.
- **research/05 §"Click wheel"** — *"The one command still unanswered is `0x8001052A`."* Updated in
  place.

### 8. Predictions that measured out to nothing

- **"Answering the handshake will let `SerialOptoTask` deliver decoded frames into the event
  system."** It already did. The frames were being decoded and the events were being posted before
  this pass started; nobody had watched `0x000ada4c` or `0x000cd6a0`. Addendum 18 §10's experiment 1
  was written on the assumption that the delivery was blocked, and its predicted outcome
  ("`0x001ae230` going non-zero would connect the walls") is now measured: `0x001ae230` is **0** in a
  run where nineteen wheel events reached the queue. The walls are not connected by this route.
- **"The `0x052a` reply is what the 1500 µs timeout is waiting for."** No. That timeout lives in
  `0x00283ea0`, the *button query*, and in the sender's busy-bit spin. Nothing waits on `0x052a`.
- **"Injecting before RetailOS's opto init at `@49.68 M` will be refused by the gate."** Wrong by a
  wide margin, and it is the boot ROM that says so: the ROM enables reporting at `@238 346`, so an
  event at `@40 M` is posted, ignored, and overwritten. The gate's real boundary is 200× earlier than
  predicted, which is why arm E exists.
- **"`0x000cd658` is the scroll event."** No — it is a sixteen-entry ring of microsecond deltas at
  `0x10882394`, the scroll-velocity history. The event is `0x000cd6a0`, four instructions further on.
  Disassembled rather than inferred from position in the trace, per Addendum 15's rule 4.

### 9. What is still not derivable, and why

**The wire-level effect of payload 0.** Every observation above is consistent with two readings —
that payload 0 stops all autonomous frames, or that it stops only *position* reporting and leaves the
buttons streaming. RetailOS cannot discriminate them: in the modes where it sends payload 0 it also
stops running the scroll accumulator, so it would behave identically either way, and no boot in this
project ever sends payload 0 (all 30–75 sends per run are payload 1). The model takes the stronger
reading — all frames gated — because it is the one that makes the setter *do* something observable
and therefore the one that can be caught being wrong. If a future capture shows a real 5G still
reporting buttons after `0x8000052a`, this is the line to revisit.

**Whether the PSoC acknowledges on the wire at all.** Silence is what Apple's software requires; it
is not a claim about the SPI bus. If the part does emit something, no code in either Apple stage
could tell.

### 10. The method note this one adds

**"Unanswered" is a property of the model, not of the firmware, and the two get conflated by the
report that prints it.** The run report has said *"73 of them commands we have no evidence for:
0x8001052a"* for two addenda, and that line was read — by three passes in a row, including this
one's own prompt — as *the firmware is waiting for an answer*. It never said that. It said we had
not classified the command. The counter that would have caught it earlier is the one this pass
added: a command that is *recognised* and deliberately unanswered has to be a different number from
a command that is merely unrecognised, or the instrument cannot distinguish a device that refused
from a device that does not understand.

The corollary is the cheaper lesson: **the boot ROM is a second implementation of every driver
RetailOS ships, written without a scheduler around it, and it is far easier to read.** Forty
instructions of straight-line register stores at `0x000c9714` answered in five minutes a question
that two addenda had left open, because a routine with no task, no semaphore and no callback cannot
hide a wait.

---

## Addendum 22: Ghidra, wired at last — and construction and showing are two separate passes

2026-08-14. Ghidra 12.1.2, `GhidraMCP-7.0.0.jar` and `tools/ipod-boot/ghidra-mcp-headless.sh` have
all been present for the whole project and **nothing had ever used them**. A headless server was in
fact already running with `OSOS_correct.bin` loaded and analysed. Now registered:

```
claude mcp add ghidra -- uv run --project resources/vendor/ghidra-mcp bridge-mcp-ghidra
```

The bridge is stdio MCP; the server is a plain REST API on `127.0.0.1:8089` and can be driven with
`curl` without any MCP client at all — `get_xrefs_to`, `get_function_by_address`,
`decompile_function_by_address`. **Fragility worth knowing: the bridge lives under `resources/`,
which is gitignored, so this registration points into untracked material and will not survive a
rebuild of that tree.**

### 1. What it answered in ten minutes

**`0x001acca8`, the "show" dispatcher, has exactly one caller in the whole image**: `0x001ae080`,
inside `FUN_001ae070`. And `0x001ae070` is itself referenced exactly once, as **DATA**, from
`0x0066db98`. The class's vtable base is `0x0066daf4` (Addendum 18 §2), and
`0x0066db98 − 0x0066daf4 = 0xa4` — the `setVisible` slot.

So *show* is not reachable by any direct call anywhere in 7.5 MB. It is reachable **only** through a
virtual `setVisible` on this class. That is why walking the call graph upward from it terminates
immediately, and it is the structural reason Addendum 20's measurement came back all-zero.

**The construction path is a different pass entirely.** `0x001acab0` — the lr Addendum 20 recorded at
the constructor — sits inside `FUN_001aca80`, the factory. Its caller is `FUN_0021a4f4`: the
**recursive view-tree builder**, the same function `MP3ExampleTask` was six frames deep in when it
blocked on `0xd1` (Addendum 11). Decompiled:

```c
void FUN_0021a4f4(int param_1, undefined4 param_2, code *param_3, code *param_4, int param_5)
{
  ...
  iVar3 = (**(code **)(*piVar4 + 0x48))(piVar4, &local_24, 4, ...);   // register by id
  if (iVar3 != -1) {
    (**(code **)(*piVar4 + 0x3c))(piVar4, iVar3, &uStack_2c);
    (*local_28)(param_1, param_2, uVar2, param_3, param_4);            // run the build callback
  }
}
```

`local_24` is loaded from `*(ushort *)param_3` — the descriptor's leading halfword, which
[Addendum 18](#) §2 established is the widget **id**. So the builder registers each widget in an
id-keyed table and then invokes its build callback.

### 2. What this establishes

**Building a widget and showing it are two separate passes, and only the first has run.** The build
pass completed — both instances exist, twice-constructed, registered by id. Nothing in that pass
calls `setVisible`, and `setVisible` is the only route to *show*.

This retires the framing of Addendum 20 §3 ("something decides which screen exists and tells its
widgets to appear, and that decision has never been made") from a hypothesis to a structural fact
about the code: the decision is a **separate pass over the built tree**, and it is that pass we have
never observed running.

### 3. Where it points

Addendum 18 §9 measured the **base** implementation of vtable slot `+0xa4` firing **68 times from 11
sites**. Those 11 sites are the show pass. **Settled when** one of them is named and the list it
iterates is identified — and then why these two widgets are not in it.

### 4. Method note: what Ghidra is and is not for

Every decisive finding in this project has been an **arrival counter**. Ghidra cannot produce one —
it says who *could* call a thing, never who *did*, and `0x001ae070`'s single DATA xref is exactly the
kind of fact that reads as "unreachable" until you know it is a vtable slot. Used the other way
round it is transformative: **Ghidra proposes candidates, the emulator says which ones fire.** The
pairing matters more than either half, and `from-idle.sh` now makes the emulator's half a 3-second
question instead of a 110-second one.

---

## Addendum 23: the show pass walks a child list at `+0x78` — our widgets are in nobody's

> Still current, and folded into [research/12](12-how-retailos-draws.md) §2 with the vtable-slot
> correction Addendum 24 §1 makes to §4 below.

2026-08-14, Ghidra + `from-idle.sh`. Addendum 22 established that building and showing are separate
passes. This names the show pass.

### 1. The 11 call sites, and where they cluster

`--enterlog=0x0021ada8` (the base visibility slot) over the full boot, `0x0016b044` as control:

```
0x00141f74 x2
0x0017db20 x15   0x0017db58 x10   0x0017dbb0 x8   0x0017dbd8 x8     <- 41
0x0017e898 x6    0x0017e8d0 x6    0x0017e928 x3   0x0017e950 x4     <- 19
0x001ae714 x2    0x001bd228 x4
                                                          68 total, 11 sites
```

Sixty of the sixty-eight come from two functions in `0x0017dxxx`/`0x0017exxx`. Our class's override
`0x001ae070` remains **0**.

### 2. What the show pass does

`FUN_0017db04` decompiled:

```c
void FUN_0017db04(int param_1, int param_2, undefined4 param_3, int *param_4)
{
  ...
  FUN_001e2828(&iStack_18, param_1 + 0x78);                       // iterator over [container+0x78]
  while (FUN_001e27e0(&iStack_18, &local_10) != 0) {
    (**(code **)(*local_10 + 0xa0))(local_10, 1);                  // visibility slot on each child
  }
  FUN_001e2838(&iStack_18);
  ...
}
```

It takes a container, iterates the collection at **`[container + 0x78]`**, and calls the visibility
slot on every element. The `param_2 != 0` arm passes `1`, the other passes `0` — show and hide are
the same walk with a different argument.

### 3. What that makes Wall A

Showing is not a decision made *about* a widget. It is a walk over a **parent's child list**. So
there are exactly two ways for a widget never to be shown, and this measurement does not yet
separate them:

1. the widget was never added to any container's `+0x78` collection, or
2. it was added, and **its container is never itself shown**.

Both are consistent with everything measured so far, and (2) recurses — the same question one level
up. **Settled when** the two instances' membership is read out of a live `--save-region=sdram` dump:
find the containers whose `+0x78` collection holds them, if any, and walk upward until either a
container is found that the show pass does reach, or the chain ends in an orphan.

### 4. A discrepancy left standing rather than smoothed over

Addendum 18 §6 identifies `0x0066db98` as vtable slot **`+0xa4`** and `0x0021ada8` as that slot's
base implementation. The call site here dispatches through **`+0xa0`**. Both cannot be right about
the same function, and the arrival counter is unambiguous that `0x0021ada8` is what `0x0017db20`
reaches. Either the vtable base is off by one slot or the class overrides an adjacent entry;
`--wordref` on the vtable region settles it in one query and it has not been run. Recorded because
an off-by-one in a vtable index is exactly the kind of error that survives by being nearly right.

---

## Addendum 24: Wall A is a stuck visibility state — `flags & 0x1800 == 0x1800`, and `setVisible` is inert in both directions

> **The mechanism below is correct and still stands. Its framing does not** — this is not why the
> screen was blank; 566 paints happen regardless (Addendum 25), and the output stage is a different
> stage entirely. It remains the reason *that subtree* is not shown, and that question is open. The
> current description of the pipeline this sits inside is
> [research/12](12-how-retailos-draws.md) §2 and §9①.

2026-08-14, Ghidra MCP + `from-idle.sh`. This closes the mechanism. It does not yet name the writer.

### 1. Correcting Addendum 18 §6 — the slots were off by one

Read from the vtable at `0x0066daf4` (`read_memory`, 48 bytes) rather than inferred:

| slot | this class | other classes |
|---|---|---|
| `+0xa0` | `0x00219284` | `0x0021ada8` |
| `+0xa4` | `0x001ae070` (→ the "show" dispatcher) | — |

Addendum 18 §6 called `0x0021ada8` "the base implementation of slot `+0xa4`". It is not: it is what
*other* classes carry at **`+0xa0`**. The discrepancy Addendum 23 §4 recorded rather than smoothed
over was real, and this is its resolution. **`+0xa0` is `setVisible(bool)`; `+0xa4` is the
visibility-changed hook it calls.**

### 2. The gate, decompiled

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

`this[8]` is `[obj + 0x20]`. **If `flags & 0x1800` is neither `0x800` nor `0x1000`, both arms return
and `+0xa4` never fires — silently, with no error path.**

### 3. Both widget chains are stuck at exactly that

Object pointers taken from the constructor's `r0` (`--enterlog=0x001ae3b8`: `0x13ca9e70` @246 483 436,
`0x13c9bd28` @859 932 015), parent chains walked through `[obj+0x10]` in a live `--save-region=sdram`
dump at the 1.61 G idle:

```
mode 2:  13c9bd28 HIDDEN  -> 13cc3a2c flags 0x5a00 NEITHER -> 10a10f74 VISIBLE -> 10a112ac VISIBLE
mode 0:  13ca9e70 HIDDEN  -> 13caac10 HIDDEN -> 13cac250 HIDDEN
                          -> 13cac2f8 flags 0x5a00 NEITHER -> 13d3b5a4 VISIBLE -> ... -> 10a112ac VISIBLE
```

Both chains cross an object of the **same class** (vtable `0x0066b298`) whose flags are `0x5a00`:
`0x5a00 & 0x1800 = 0x1800` — **both visibility bits set at once**. `setVisible` on it is inert in
both directions, so the show pass (Addendum 23) never descends past it to the children.

That it is a *state* and not a class trait is settled in the same dump: `0x10a10cd8` is the same
vtable with `0x4a00` — `& 0x1800 == 0x800`, properly VISIBLE. `0x5a00` differs from `0x4a00` by
exactly `0x1000`.

### 4. What must be true, and what to look for next

`FUN_0021a0fc(obj, bits)` is `[obj+0x20] = ([obj+0x20] & 0xffffe7ff) | bits` — it **clears both**
bits before setting one, so it can never produce `0x1800`. Some **other** writer OR'd `0x1000` into
`[obj+0x20]` without going through it.

**Settled when** that writer is found and it is known why it runs on these two objects. Two shapes
worth distinguishing: a legitimate "transition in progress" marker whose completion never came, or a
straightforward missing clear.

### 5. Prediction that measured out to nothing

I expected the two list widgets themselves to be in the `NEITHER` state — that was the hypothesis
this run was built to test. They are cleanly `HIDDEN` (`0x5211` / `0x5201`), which is precisely the
state that *permits* a show. Nothing is wrong with the widgets; the obstruction is three and one
levels above them respectively.

---

## Addendum 25: RetailOS paints 566 times and presents zero times — Wall A is the output stage, not visibility

> **Two of this section's claims are retracted by Addendum 26, measured the same day.** The headline
> — paints many, presents never — **survives and is confirmed**. What does not:
>
> 1. **"RetailOS never touches the VideoCore" (§ preamble) is wrong**, and it is wrong for the reason
>    rule R6 exists: `--watch-range`'s log caps at 4096 entries and the bootloader's own firmware
>    upload fills it before RetailOS has executed an instruction. RetailOS uploads `vmcs.bin` itself
>    at @51 753 290 and runs the whole co-processor bring-up. Addendum 26 §1.
> 2. **`[obj+0xa0]` is not a draw target.** It is the *image a full-screen image view displays*, and
>    "why does nothing assign it" has the prosaic answer that no photo is ever opened. The output
>    stage is a different function entirely. Addendum 26 §2–3.
>
> The `--watch-range` line is left in place below rather than deleted, because the shape of the
> error is the point. *(That cap was fixed on 2026-08-14: the same command now reports 423 450
> byte-writes across 5 words and names every writing PC, so the instrument refutes its own claim —
> see the README's "What the sweep changed".)*
>
> **The headline — paints many, presents never — is described as a working pipeline in
> [research/12](12-how-retailos-draws.md) §1 and §3.**

2026-08-14. Prompted by the operator asking why a freshly booted RetailOS should need per-object
visibility coaxing at all. The premise-check was the right move and it reframes Addendum 24.

### 1. RetailOS *is* painting

`--enterlog=0x0021acac` (the base paint slot), full boot, control live:

```
0x00139428 x66    0x0017e80c x452   0x001ab84c x2   0x0023b094 x1   0x002448f8 x45
                                                              566 arrivals, 5 sites
```

`FUN_0021acac` decompiles to a real painter — gradient fills interpolated per scanline, four edge
lines, corner handling, text — drawing into a graphics context built by `FUN_00211bd4`. **So the
claim "RetailOS never draws" was always wrong.** It draws constantly. What it never does is
*present*.

### 2. The output stage never runs

`t_graphicsManager` has been parked in `KS_receive` on mailbox `0x16` since tick 216 of 717 820
(Addendum 17). The only sender is `FUN_00189008`, whose sole static caller is `FUN_00180e54` — and
`FUN_00180e54` gates on `[obj + 0xa0]`, the **draw target**:

```c
bVar4 = *(int *)(param_1 + 0xa0) != 0;
if (bVar4 && *(char *)(param_1 + 0x9d) != '\0') {
    ...
    FUN_00189008(uVar3, auStack_19c, &local_1b0, 0);      // -> mailbox 0x16
}
```

Measured this run, `0x0016b044` as control: `0x00180e54` **0 arrivals**, and all four static callers
of the `[obj+0xa0]` setter `0x00180ce8` — `0x0016e778`, `0x0016f3c4`, `0x0016f310`, and
`FUN_001ae09c` — **0 arrivals each**. The presenter does not run and bail; it never runs.

### 3. The loop closes back on the widget, which is not a coincidence

`get_xrefs_to 0x00180e54` returns exactly one reference: `0x001ae0fc`, inside **`FUN_001ae09c`** —
the mode-2 list-rendering body this investigation has been circling since Addendum 18. So that widget
is not an arbitrary one the census happened to land on: **it statically contains the only path in
RetailOS from a view to mailbox `0x16`.**

**Caveat, and it is the standing trap of this tool:** Ghidra xrefs are *static*. Virtual and
function-pointer calls do not appear, and this codebase dispatches through vtables everywhere — the
same blindness that made `0x001ae070` read as unreachable in Addendum 22 until its DATA reference was
recognised as a vtable slot. "One caller" here means one *direct* caller. It is evidence, not proof,
that no other presentation path exists.

### 4. What this changes

Addendum 24's stuck `0x1800` state is real and does block that subtree — `FUN_00255b50` is
`(flags & 0x1800) == 0x800` and it is the first line of both the painter and the show walk. But it is
**not** the explanation for a blank screen, because 566 paints happen regardless. The correct
statement of Wall A is one level more general and does not mention any particular widget:

> **RetailOS renders and never presents. Its paint stage runs 566 times; its output stage runs zero
> times; nothing has ever been sent to mailbox `0x16` in 1.61 G instructions.**

**Settled when** it is known what assigns a view's draw target `[obj+0xa0]` — a surface, allocated
from somewhere — and why nothing does. That is a question about display initialisation, not about
widget state, and the operator's instinct that a fresh boot should simply show things is what
relocated it there.

*(Answered by Addendum 26: the field is not a surface, the question was aimed sideways, and the real
output stage stops four functions lower on a co-processor that never publishes its service
directory.)*

---

## Addendum 26: `[obj+0xa0]` is a photo, not a surface — and the output stage stops on a co-processor that never answers

> **§1–§3 stand. §4's cause is retracted** — the words at `0x1f0` were never zero; our own
> `Bcm::read8` halved them on the way to the CPU (Addendum 29 §1). **A model defect looks exactly
> like missing hardware**, and this is the case that named that lesson. Current description:
> [research/12](12-how-retailos-draws.md) §3–§5 and §9③.

2026-08-14, Ghidra + five full boots. Addendum 25 asked what assigns a view's *draw target*
`[obj+0xa0]` and why nothing does. The field is not a draw target, the question was aimed one module
sideways, and answering it properly walks straight into ledger bypass **#6**. Four things settle
here, and two of them are retractions.

### 1. Retraction: "RetailOS never touches the VideoCore" is an instrument ceiling

`Machine::write*` records `--watch-range` hits under `self.watch_range_log.len() < 4096`
(`lib.rs:923`). Apple's bootloader fills `0x000e0000..0x0010581e` — 76 816 halfwords of framebuffer,
plus its own traffic — **before RetailOS executes an instruction** (Addendum 12 §2's handoff dump is
that same control). So the log is full long before the window of interest opens, and the report
prints only the *first* PC per word, attributing every address to whoever touched it first.
Reproduced with Addendum 25's own flags:

```
retail-boot.sh --clock=5 --stop-when-idle=400000000 --watch-range=0x30000000:0x100000
  writes into the watched range: 4096 (byte granularity, oldest first)     <- exactly the cap
    0x30000000  4066 byte-writes, first from pc 0x4000ec14
    0x30010000     4                          0x4000eb80
    0x30030000    16                          0x4000e978
    0x30070000    10                          0x4000e9b8
```

`4096` is not a count, it is a **saturation flag** — rule R6, again. Worse: the correct statement was
already in this file. Addendum 12's retraction block reads *"RetailOS's whole contribution to
`0x30000000` is the `vmcs.bin` upload: 100 696 halfwords"*. Addendum 25 published the opposite
thirteen sections later, from a capped log, and nothing caught it.

Arrival counters say it plainly. All BCM traffic funnels through three functions — `0x00287998`
(bring-up), `0x00287be8` (write block), `0x00287a6c` (read block), the only three carrying the
`0x3000xxxx` literals — plus `0x00286fb4`, the bulk helper `0x00287be8` hands its port pointer to.
Full boot, `0x0016b044` as control:

| watched | arrivals | what |
|---|---|---|
| `0x00287698` | **1** @51 753 290, lr `0x001affc4` | the `VMCS.BIN` uploader — RetailOS's own |
| `0x00287998` | 2 | BCM bring-up (`a1 81 91 02 12 22 72 62` → `0x30030000`) |
| `0x00287be8` | **5** | one of `0x312a0` bytes to internal 0, four of 4 bytes |
| `0x00286fb4` | **1** @51 754 855, `r0=0x30000000 r2=0x31200` | the bulk half — chunks it 0x10000 at a time onto the DMA engine, which is exactly Addendum 9's `pp dma: 4 transfers, 201216 bytes` (`0x10000`×3 + `0x1200` rounded to 16 = 201 216) |
| `0x00287a6c` | 3 | 16 bytes at `0x1f0`, 4 at `0x10000c00`, 4 at `0x1f8` |
| `0x0016b044` | 1 | control |

**RetailOS uploads the co-processor firmware itself and runs the whole bring-up handshake.** What it
never does is push a *frame*: the only large write in 1.61 G instructions is `vmcs.bin`.

### 2. `[obj+0xa0]` is the image a full-screen image view displays

The class is the one whose factory is `FUN_001810c4` (vtable `0x0066b94c`), built on the same
descriptor base `FUN_0021af84` as the rest of the widget system. `FUN_00180ce8` is
`*(obj+0xa0) = arg; bx lr`, and it has four static writers — two setting, two clearing:

| site | what it stores |
|---|---|
| `FUN_0016f310` | `owner+0x428` = `{ dataPtr, 0, <32-byte header> }`, header `memcpy`'d from a database record |
| `FUN_001ae09c` | `owner+0xd8`, filled by `FUN_001be0f8(db, dst, index, 3, &ok)` |
| `FUN_0016f3c4` | `0` |
| `FUN_0016e778` | `0` |

Both setters do the same thing: **fetch record type `3` from a media database and hand the resulting
descriptor to a full-screen image view.** The readers confirm the shape — `FUN_00180cf0`
dereferences `[img+0x14]` as an *orientation* (`0`, or `0x10e` = 270°, whose arm rotates the
destination rect through `FUN_0021175c`), `[img+0x18]`/`[img+0x1c]` as an offset pair, `[img+0x24]`
as a size, then builds a graphics context with `FUN_00211bd4` and blits with `FUN_00210114`. A draw
*target* has none of those fields; a stored photo has all of them. The owner module carries the
strings `'Sub-LCD'` and `'Sub-TV'` at `0x0016fc68`, queries its database by index, and arms 800 ms /
2000 ms timers around each fetch; `ss_get_status` is a real exported name in the image at
`0x001c7a2c`, and `slideshow.vll` exists.

**So the answer to "why does nothing assign it" is that no photo is ever opened.** The boot ends at
the main menu with no user navigation and no photo database, so the controller that would fetch a
picture never runs, so nothing hands the image view an image. Nothing is broken. Category **(c)** —
the field was mis-identified, and a correct chain was walked from a wrong premise for one addendum.

Mailbox `0x16` is real but is not "the output stage" either. Its pump at `0x00189060` decodes tag
`0xaaaa0001` to index 0 and calls `FUN_00177864`, which dispatches a virtual `+0xc` on *either*
`FUN_001647c0()+8` (the display server) or `FUN_00163b0c()+8`, chosen by `[msg+0x10]`. It is the
display server's **async blit-request channel**, used by the image view when it wants the server to
composite rather than drawing CPU-side — which is exactly why the only path to it runs through an
image view, and why it is silent when no image view has an image.

### 3. The real output stage, and the exact place it stops

RetailOS's `lcd_update` is `FUN_00164f44`. It uploads only the dirty scanlines, then tells the
co-processor to show them, then flips:

```c
FUN_00287be8(back->bcmAddr + stride*y0, cpuFB + stride*y0, stride*(y1-y0), 1);  // upload
FUN_00286b6c(back+0x20, ...);                                                   // show
tmp = ctx->back; ctx->back = ctx->front; ctx->front = tmp;                      // flip
```

Above it: `FUN_001650f8` (lock → `FUN_00164cb8` → present → signal) ← `FUN_000c3004` ← a callback
trio registered by `FUN_0017eeb0` via
`FUN_000f223c(obj+0x24, 0, 0x0017ef98, 0x000cab18, 0x000c3004)`. `0x0017ef98` marks damage,
`0x000c3004` flushes it. Both run. One full boot, control live:

| watched | arrivals |
|---|---|
| `0x001205d0` — the startup module that owns the screen | 1 @49 383 791 |
| `0x0017eeb0` — display init, registers the callbacks | 1 @49 384 232 |
| `0x000f223c` — the registration | 1 @49 551 718 |
| `0x001647c0` — display-server singleton | 173, from six sites |
| `0x001650f8` — **flush** | **42**, all from `FUN_000c3004` |
| `0x00164f44` — **present** | **0** |

So the display server exists, is initialised, has its damage callbacks wired, and is asked to flush
**forty-two times**. It declines every time. `FUN_001650f8` presents only if
`FUN_00164cb8(server, layer)` holds, and that needs `[server+0x6c4] == FUN_00164610(server, layer)`
— a *bound* layer. The bind is `FUN_00164878`, and the counter names the reason in its arguments:

```
0x00164878  lr=0x0017efe4  r0=0x10882c3c  r1=0xffffffff   x42
```

**The layer index is `-1`.** `FUN_00164610` returns 0 for any index ≥ 11 (unsigned compare), so the
bind returns 0, forty-two times, silently.

It is `-1` because the single attempt to create a layer failed:

```
0x001649ac  lr=0x001b0390   @51 841 828     create layer   (from FUN_001b02d8)
0x00164450  lr=0x00164a30   @51 841 905     create surface
0x00286ca8  lr=0x001644f8   @51 841 951     allocate it on the co-processor
0x0028861c                  0 arrivals      <- the RPC was never sent
```

`FUN_00286ca8` opens on `if (-1 < *DAT_00286d70)`. `DAT_00286d70` is `0x1082359c`, and `--storeaddr`
over a whole boot finds **three writes in 1.61 G instructions, all of them the BSS initialiser**:

```
0x00084394 -> [0x1082359c] = 0xffffffff  @46 548 504     the RPC channel index
0x00084394 -> [0x10831980] = 0x00000000  @46 606 844     the service-directory base
0x000843a8 -> [0x108d3bd4] = 0x00000000  @47 271 002     the service directory itself
```

Never written again. So `FUN_00286ca8` returns `-1` without touching the bus, `FUN_00164450` returns
0 before reaching its `FUN_00287be8`, `FUN_001649ac` returns `-1`, and every layer operation for the
rest of the boot carries `-1`.

### 4. Why the channel index is `-1`: the co-processor never publishes its service directory

> **Retracted in part, 2026-08-14 — see Addendum 29 §1.** The chain below is right and the cause is
> not. "The words come back zero" is false: internal `0x1f8` held `1` at the moment of the read, and
> the model's `Bcm::read8` corrupted it on the way to the CPU by popping the data FIFO twice per
> halfword. The gate that actually rejected was `(w3 & 3) == 0`, one test later.

Three functions look for a service, all with the same shape — walk eight `u16` offsets at
`0x108d3bd4`, add the base at `*0x10831980`, read 16 bytes of co-processor memory at each, match a
tag:

| scanner | tag | consumer |
|---|---|---|
| `FUN_00286aa8` | **2** | surface allocation — `FUN_00286ca8`, and through it every layer |
| `FUN_00287194` | **1** | |
| `FUN_00288978` | **7** | |

That directory is filled by exactly one function, `FUN_00288058`, and only on its success path:

```c
FUN_00287a6c(&w0, 0x1f0, 0x10, 0);                  // 16 bytes of co-processor memory
if (rc == 0 && w2 == 1 && w3 != 0 && (w3 & 3) == 0) {
    *(DAT_002880f8 + 8) = w3;                       // -> 0x10831980, the base
    FUN_00287a6c(0x108d3bd4, w3, 0x10, 0);          // -> the 8 service offsets
    ...
}
```

`FUN_00288058` runs once, @51 774 010. `FUN_00286aa8` runs once, @51 775 728, and returns
`r0 = 0xffffffff`. The BCM read histogram — uncapped, and it says so: `internal reads: 20 distinct
offsets, 56 of 56 accounted for` — shows `0x1f0`/`0x1f2`/`0x1f4`/`0x1f6` read exactly once each, and
the `internal write runs` list shows **nothing was ever written anywhere near `0x1f0`**, by the
bootloader or by RetailOS. The words come back zero, `w2 != 1`, the directory stays zeroed, no
service is ever found.

On real hardware that block is written by `vmcs.bin` **executing**. Our BCM model is a memory and a
protocol: it stores halfwords, decodes the address latches, answers `CONTROL` with a fixed `0x52`.
It does not run the firmware, so it publishes nothing, so RetailOS asks eight times, finds nothing,
and disables its own display path with no error message anywhere.

That is ledger bypass **#6**, and the category is **(a)/(d)** — a genuine dependency on a device we
do not have. End to end:

```
vmcs.bin uploaded, bring-up handshake completes      @51.75 M   our synthesised replies satisfy it
FUN_00288058 reads 0x1f0 -> zeros, w2 != 1           @51.77 M   service directory stays empty
FUN_00286aa8 scans 8 slots, finds no tag-2 service   @51.78 M   channel index stays -1
FUN_00286ca8 early-returns -1, no RPC sent           @51.84 M   no surface on the co-processor
FUN_00164450 -> 0, FUN_001649ac -> -1                @51.84 M   no layer
FUN_00164878 binds layer -1                          x42        no bound layer
FUN_001650f8 flushes                                 x42        FUN_00164cb8 false every time
FUN_00164f44                                         x0         no frame ever leaves the CPU
```

### 5. The operator's hypothesis, tested and killed as stated

> *If the bootloader leaves display state behind at handoff — a surface address, a framebuffer
> descriptor, a "display present" flag — then our synthesised replies may have produced wrong or
> missing handoff state, and RetailOS's display init bails on it.*

**It does not hold, because RetailOS inherits nothing.** It re-runs `FUN_00287998` (the full
`0x30030000` bring-up sequence), re-uploads all 201 376 bytes of `vmcs.bin` to internal address 0,
and then re-discovers every service from scratch by reading co-processor memory. There is no handoff
word, no inherited surface, no flag. Every global the display path depends on is written once by the
BSS initialiser at @46.5–47.3 M — *before* the bring-up — to `-1` and `0`.

The adjacent claim survives, and is now measured rather than hypothesised: **bypass #6 is tied to
this wall**, just not through handoff state. It is tied through a read of co-processor memory that
only a running co-processor can have filled.

### 6. The control the bar demands

Addendum 14 §2's control, re-run at the full 1.61 G idle rather than at 600 M (rule R7 — vary the
stop condition):

```
--stop-at=0x10000000:1 --bcm-dump=0xE0000:140:F0:handoff.ppm   2922 non-zero pixels of 76800
--stop-when-idle=400000000 (Idle @1 610 232 373), same dump    2922 non-zero pixels of 76800
cmp handoff.ppm idle.ppm  ->  byte-identical, 230 415 bytes
```

**RetailOS changes not one pixel in 1.6 billion instructions.** That is exactly what the chain above
predicts, and it is the only statement about the screen this project should make until a frame
actually leaves `FUN_00164f44`.

Baseline re-measured on every run in this section and unchanged: Idle @1 610 232 373, 38 265 code
buckets, 770 ata commands, `pp dma: 4 transfers, 201216 bytes`, `ide irq … delivered 685`, 4 unmapped
reads, `cargo test --release` **21 passed**. No emulator source was modified.

### 7. Predictions that measured out to nothing

- **"`FUN_00164450` fails on the pixel format."** Its four accepted formats are `0x565`, `0x1888`,
  `0x2565`, `0x3888`, and a format mismatch was the cheapest explanation for a surface that never
  gets made. Wrong: `FUN_00286a1c` — the pure-arithmetic descriptor initialiser *downstream* of that
  test — was reached, so the format was accepted. It fails one call later, on the co-processor.
- **"The display init never runs."** `FUN_0017eeb0` was the obvious suspect for "display
  initialisation, not widget state". It runs, at @49 384 232, and completes: two bitmap descriptors
  built, three callbacks registered. Everything above the co-processor is alive.
- **"Something writes the channel index and gets it wrong."** `--storeaddr` on all three globals over
  a full boot found three writes, all BSS. Nothing gets it wrong; nothing tries.

### 8. Settled when

The mechanism is settled; what is not built is the co-processor. Making a frame appear needs the
model to answer, in order: a 16-byte block at internal `0x1f0` whose third word is `1` and whose
fourth is a valid pointer; an 8-entry `u16` service directory at that pointer; a 16-byte descriptor
per service tagged `1`, `2`, `7`; and then the ring protocol behind `FUN_0028861c` (request) /
`FUN_00288434` (reply) / `FUN_002881fc` well enough that `FUN_00286ca8` returns a surface handle and
a co-processor-side address. That is a bounded reverse-engineering job on `0x00286aa8`–`0x00288a88`,
not a patch, and **no fix falls out of this session** — inventing the layout would produce a machine
that advances further and means nothing, which is the failure mode this file exists to prevent. The
honest next step is to read that protocol out of `vmcs.bin`'s own strings and relocations, or out of
Rockbox's `lcd-video.c` if it turns out to speak the same dialect.

**Settled when** a frame RetailOS drew is dumped from `0xE0000` and differs from `handoff.ppm`.
**Met 2026-08-14 — Addendum 29 §7.** 76 607 non-zero pixels against 2 922, and the four artefacts
listed above are built behind `--bcm-registry`. §3 of this addendum stands unchanged and is the
measurement that made the derivation possible; only §4's *cause* was wrong.

---

## Addendum 27: `vmcs.bin` is a Nucleus PLUS application, and the `0x1f0` block really is runtime-only

2026-08-14. `vmcs.bin` has sat at the centre of the last open bypass for the whole project and had
**never been extracted to the host**, because nothing could read `rsrc`. `ipod-boot rsrc` now
can — pure parsing, no `hdiutil`, no mounting.

### 1. The volume, in full

```
IPODRESO.URC                                       0
RESOUR~1/FONTS/CJK.TTF                       3 446 748
RESOUR~1/FONTS/HELVET~1.TTF                     56 816     HELVET~2.TTF   81 440
RESOUR~1/FONTS/MONOHO~1..4.TTF          22 772 … 37 592
RESOUR~1/FONTS/PODIUM~1.TTF                    126 524     <- Podium Sans, the 319 lookups
RESOUR~1/VIDEOC~1/BOOT/RENDER~1.BIN            104 540
RESOUR~1/VIDEOC~1/BOOT/VMCS.BIN                201 376
RESOUR~1/VIDEOC~1/LIBRARY/{AACDEC,H264DEC,MPG4DEC,MPLAYER,PASSTH~1,SLIDES~1}.VLL
```

Note the **two different `vmcs.bin`**: this 201 376-byte one from `rsrc`, and a 101 728-byte one in
the NOR at `resources/derived/fw/flsh/vmcs.bin`. The bootloader uploads the flash copy; RetailOS
uploads this one. Anything measured against "the" vmcs must say which.

### 2. It runs Nucleus PLUS

`strings` finds 525 runs, and the informative ones are not strings at all — they are **u32 magic
constants read backwards in a byte dump**: `KSAT` `AMES` `TNVE` `UEUQ` `EMIT` `RSIH` `ANYD` =
**TASK · SEMA · EVNT · QUEU · TIME · HISR · DYNA**. Those are Nucleus PLUS control-block signatures,
`HISR` and `DYNA` especially — high-level interrupt service routine and dynamic memory pool are
Nucleus's own vocabulary. `GENCMD` appears in plain text.

So the co-processor runs a **documented commercial RTOS** hosting a command server. That is a much
better position than an opaque blob: Nucleus PLUS control-block layouts are published, so structures
found at runtime can be identified rather than guessed.

The first six words of the file are `0x0b4a 0x0b46 0x0b36 0x0b3e 0x0b3a 0x0b42` — six pointers into
`0x0b36..0x0b4a`, exactly 4 apart when sorted. A vector or entry table, listed in a non-sequential
order. Code proper starts at `0x200`.

### 3. The `0x1f0` block is zero in the file — confirmed, not assumed

The upload lands at co-processor internal address 0, so **file offset `0x1f0` is the `0x1f0` RetailOS
reads**. Measured: `0x1f0..0x1ff` is all zero in the file; the first non-zero byte after it is at
`0x200`. *(Re-verified 2026-08-14 on the **`rsrc`** copy specifically, since Addendum 28 §3 turned
out to have measured the NOR one: `0x1f0 = 0x1f4 = 0x1f8 = 0x1fc = 0`, and `0x200 = 0xfc402f78`,
`0x204 = 0x2f790001` — the bytes that Addendum 29 §1 reconstructs the corrupted read from.)* Addendum 26's conclusion stands from the other side — that block is populated by the
firmware at run time, and there is **no model bug to find there**. Our BCM returns zeros because
zeros are genuinely what was uploaded.

### 4. What this means for the route, and the limit of Ghidra here

**Ghidra cannot decompile this.** It is VideoCore II object code and Ghidra ships no VC02 processor
module; the public Broadcom VideoCore documentation covers **VideoCore IV**, a later generation, for
the Raspberry Pi. Static analysis of `vmcs.bin` is therefore limited to strings, tables and byte
patterns — which is exactly how §2 and §3 were obtained, and it was worth doing, but it will not
yield the service-directory layout by reading the code that writes it.

That leaves the layout derivable from only one place: **RetailOS's own reader**. Which is the method
that has already worked twice — `0x8001052a` (Addendum 21) and the ATA abort — and the same
distinction applies. *Derived from the parser* is a model. *Tuned until the symptom disappears* is a
bypass and belongs in [research/04](04-bypass-ledger.md).

> **Amended 2026-08-14 — still true, but bounded.** The Alphamosaic patents (notably `US7036001B2`)
> disclose the architecture and give the encoding shape: **80-bit full / 48-bit compact vector
> instructions, 16-bit scalar with 32- and 48-bit variants**. Rockbox's `dreamlayers` reached the same
> patents in 2009 while looking at this exact chip's code. That is not a disassembler and does not
> change the conclusion above, but "variable-length 48/80-bit" is a much better starting point than
> "opaque". See [research/11](11-the-videocore-runtime.md) §8.

---

## Addendum 28: the codec libraries are ELF for EM_VIDEOCORE, and they name the entire runtime API

2026-08-14, immediately after Addendum 27. The `.VLL` files in `rsrc` are **standard ELF shared
objects** — `e_machine = 0x5f`, which is the officially assigned **EM_VIDEOCORE (95)**. Their symbol
tables are readable with ordinary ELF tooling; **no VC02 disassembler is required to read them**,
which is what makes this tractable after Addendum 27 concluded that reading the code is not.

Their **undefined** symbols are the payload: what each library imports is, by definition, what the
co-processor's runtime **exports**.

> **Prior art, noted 2026-08-14.** The ELF identification is not new. Rockbox's `dreamlayers` posted
> it in **2009** ([FS#9787](https://web.archive.org/web/20240228164605/https://www.rockbox.org/tracker/task/9787)),
> with the extraction recipe and the `nm -D` / `objdump -x` route: *"Those are ELF DLLs which get
> loaded into the BCM."* What is new here is what the symbols say. Credit where it is due.

### 1. The surface, measured

```
AACDEC.VLL     5 imported  120 exported      MPLAYER.VLL   125 imported   68 exported
H264DEC.VLL   22 imported  360 exported      PASSTHROUGH     12 imported   11 exported
MPG4DEC.VLL   22 imported  358 exported      SLIDESHOW      107 imported   53 exported
                                    union: 183 distinct runtime symbols
```

Grouped by prefix, the runtime is:

| prefix | n | what it is |
|---|---|---|
| `dispman_*` | 12 | **DispmanX** — `object_create/add/remove/delete`, `resource_create/delete`, `update_start/end`, `rect_set`, `display` |
| `vc_image_*` | 21 | image surfaces — blt, convert, resize, reshape, pitch, YUV |
| `dma_*` | 14 | transfer queues, chains, 2D memcpy, callbacks |
| `vclib_*` | 13 | VRF obtain/release/check, dcache flush, timers |
| `vmcs_*` | 10 | `queue_message`, `create_task`, `create_timer`, `display`, `get/set_cookie` |
| `gencmd_*` | 6 | **`register` · `deregister` · `execute` · `param` · `decode_fourcc` · `decode_int`** |
| `hostreq_*` | 3 | `notify`, `read_iphoto_block`, `rendertext` — the co-processor calls **back to the ARM host** |
| `TCC_/TCT_/TCS_/TCF_/SMC_/SMS_/EVC_/TMT_` | ~~21~~ **25** | Nucleus PLUS internals, confirming Addendum 27 by a second route. *(Recount 2026-08-14: 9 + 3 + 2 + 1 + 4 + 1 + 4 + 1 = **25**. See [research/11](11-the-videocore-runtime.md) §2.8.)* |
| `audio_*`, `univ_*`, `filesys_*`, `powerman_*`, `pds_*`, `vll_*`, `malloc_*` | | services, plus a C library (`fopen`, `sprintf`, `qsort`, `dlopen`/`dlsym`) |

### 2. What this settles about the architecture

**GENCMD is a registry, not a fixed command set.** Services call `gencmd_register`; the host's
`gencmd_execute` dispatches by name. ~~That *is* the "service directory" Addendum 26 found RetailOS
failing to read~~ — *(**wrong, corrected 2026-08-14 in Addendum 29 §4**: the block at `0x1f0` is a
**transport** directory — eight channels, each a ring pair with a numeric type tag. GENCMD is **one
service on one of them**: tag 1, opcode 1, a printf-formatted command string in and a text stream
back, sent by `FUN_000e9358`. `gencmd_register` registers a command *name* with that service and has
nothing to do with how a channel is located.)* — it is populated at run time by whichever services
registered, which is exactly why `0x1f0` is zero in the file (Addendum 27 §3) and why only a running
`vmcs.bin` fills it.

> ⚠️ **Corrected 2026-08-14 — the struck clause conflates two different structures.** The GENCMD
> registry is matched **by name, in text**. What Addendum 26 measured RetailOS failing to read at
> `0x1f0` is an 8-entry directory of 16-byte descriptors matched **by numeric tag** (1 / 2 / 7) — a
> *channel* directory for an RPC transport, one level below GENCMD, not GENCMD itself. The half of
> the sentence that survives is the important half: both are runtime-populated, which is why the file
> is zero there. See [research/11](11-the-videocore-runtime.md) §5.4.

**The display model is DispmanX.** Addendum 26 described RetailOS binding an RPC channel, then a
*layer*, then uploading dirty scanlines to a *surface*. In DispmanX vocabulary those are a display,
an **object** (`dispman_object_create`/`add`), and a **resource** (`dispman_resource_create`), inside
an `update_start`/`update_end` bracket. The chain it measured and the API named here are the same
thing described from the two ends.

**This matters strategically more than anything else found today:** DispmanX is **publicly
documented**. The Raspberry Pi userland exposes the same API — `vc_dispmanx_element_add`,
`vc_dispmanx_resource_create`, `vc_dispmanx_update_start` — for VideoCore IV, a later member of the
same lineage. Addendum 27 closed the door on reading `vmcs.bin`'s code; this opens a different one:
we do not need to read it, because **the semantics of the interface it presents are published.**

### 3. Two limits, recorded rather than glossed

- **`render.bin` is not ELF** — it is a flat image like `vmcs.bin`, so it yields no symbols. Only the
  `.VLL` libraries carry tables.
- ~~**None of these names appear in `vmcs.bin`.** Confirmed by direct search for `gencmd_register`,
  `dispman_object_create`, `vmcs_queue_message`, `hostreq_rendertext`: **zero hits each.** The linkage
  is resolved by ordinal or by a hash table, not by string, so the symbol names are a specification of
  the API and **not** a way to locate anything inside the firmware image.~~
  **RETRACTED 2026-08-14, same day, and it is Addendum 27 §1's own warning walked into one section
  later: this was measured on the wrong `vmcs.bin`.**

### 3b. Retraction: the names are all in `vmcs.bin`, in a name table at `0x201dc`

There are **two** `vmcs.bin` — the 101 728-byte one in the NOR (`resources/derived/fw/flsh/`) and the
201 376-byte one in `rsrc`. Addendum 27 §1 says so explicitly, three sections above. §3 above
searched the NOR copy. Re-measured on the `rsrc` copy — the one RetailOS uploads, and the one whose
file offset `0x1f0` §3 of Addendum 27 is about:

```
strings vmcs.bin | grep -cE '^(gencmd_|dispman_|hostreq_|vmcs_|vc_image_|dma_|vclib_)'
    NOR copy  (101 728 B)   0
    rsrc copy (201 376 B)   82
```

The names begin at file offset **`0x201dc`** and run to about `0x213xx`, as **prefix-grouped,
alphabetically-sorted runs of NUL-terminated identifiers** separated by other data — not one
unbroken table, which is worth stating because the first pass at this section said it was:
`dispman_display` … `dispman_update_start` (12, at `0x20357`–`0x2043c`), `gencmd_decode_fourcc` …
`gencmd_register` (6, at `0x2064f`–`0x206a4`), `hostreq_notify` / `hostreq_read_iphoto_block` /
`hostreq_rendertext` (3, at `0x206c0`–`0x206e9`), then the `vc_image_*` block from `0x20a54`. Every
run is in ASCII order within itself. Matched against §1's union: **149 of the 183 symbols the six
`.VLL` libraries import are present in `vmcs.bin` as strings.** The 34 that are not are the
C-library and compiler-runtime imports (`fopen`, `qsort`, `dlopen`, `__ldivs`, `__lmul`, …) — i.e.
the names that resolve into a *different* module, which is itself consistent with these runs being
the runtime's own export directory.

**What this changes:** the last sentence of the retracted bullet is exactly backwards. The linkage is
name-keyed, the table is in the image, and a symbol name **is** a way to locate something inside
`vmcs.bin` — which matters because Addendum 27 §4 had concluded that the layout was derivable only
from RetailOS's reader, on the strength of Ghidra having no VC02 module. Reading the code is still
out; reading the *table* is not, and an ordered name table with a fixed stride adjacent to it is the
usual shape of an export directory.

**Why it slipped through, and it is not a new failure mode:** the check was a four-string `grep`
against a file the author had in hand, and both files are called `vmcs.bin`. The warning against
exactly this is one screen earlier in the same document, written by the same session. *Adjacency in
the file is not adjacency in attention* — R4's rule about re-running conclusions applies to
conclusions reached ten minutes ago, not only to ones reached last week.


> **Two agents retracted this bullet independently, within an hour, and agreed.** The second adds one
> fact this section did not have and it matters for the route: **the image carries the export table
> but *not* the registry.** Searching the `rsrc` copy for `0x50`-strided records tagged `{1, 2, 7}`
> returns **0 hits**, and `0x1f0..0x1ff` is zero in that copy too. So the name table is real and
> useful, and it is still not where the registry lives — that had to come from RetailOS's reader
> after all (Addendum 29).
>
> **And the counts disagree, which is worth recording rather than reconciling.** This section says
> **149 of 183**; the independent check said **183 of 183**. The difference is the test: 149 is
> whole-token matching, 183 is substring matching — and substring over-counts, because `free`,
> `rand` and `memcpy` occur inside longer identifiers. **149 is the better number and 183 was mine.**
> A control chosen to prove presence (`vc_image_malloc`, found in both copies) does not detect a test
> that is too permissive; that needs a control chosen to prove *absence*, and there wasn't one.

### 4. What is still not known

Every name above is a symbol, not a wire format. The GENCMD registry's on-the-wire layout at
`0x1f0` — how many entries, what each record holds, how a service is matched — is **not** given by
this and remains derivable only from RetailOS's reader, per Addendum 27 §4. What changed is that we
now know what the fields *mean*, which is the difference between decoding a structure and guessing at
one.

---

## Addendum 29: the block at `0x1f0` is a channel table, every field is in the reader — and RetailOS draws

> **This addendum is the last link in a chain, not a description of the result.** For "how RetailOS
> draws", read [research/12](12-how-retailos-draws.md) — the ARM-side pipeline end to end, with §6's
> chosen-rather-than-derived list carried into its §8. This section remains the derivation and the
> evidence.

2026-08-14, immediately after Addendum 28. The layout is derived, the model presents it, and the
bar this file has been holding since Addendum 12 is met: **`--bcm-dump=0xE0000` now differs from the
handoff dump, because RetailOS composited 41 frames into it.** Two published conclusions retract on
the way, one of them Addendum 26's central diagnosis.

### 1. Retraction: the words at `0x1f0` were never zero — the instrument was halving them

Addendum 26 §4 says *"the words come back zero, `w2 != 1`, the directory stays zeroed."* The second
half is right and the first half is wrong, and the difference is the whole wall.

`FUN_00288058` is `FUN_00287a6c(sp, 0x1f0, 0x10, 0)` then `ldr r0,[sp,#8]; cmp r0,#1`. Halted on
that instruction, `--bcm-peek` says what the co-processor's memory actually holds:

```
retail-boot.sh --clock=5 --stop-at=0x00288084:1 --bcm-peek=0x1f0:4
  0x000001f0 = 0x00000000
  0x000001f4 = 0x00000000
  0x000001f8 = 0x00000001     <- w2, and the test is `== 1`
  0x000001fc = 0x00000001
registers at halt:  r0 = 2f01fc78
```

**The memory holds `1`. The CPU was handed `0x2f01fc78`.** `Bcm::read8` served each half of an
`ldrh` from a fresh `read16`, and `read16` on the data port is a FIFO pop — so one halfword the host
asked for cost **two** internal halfwords, and the byte it returned came from a different word each
time. The delivered halfword was `(mem[a+2] >> 8) << 8 | (mem[a] & 0xff)`. Predicted from the
co-processor's own bytes at `0x200`/`0x204`, which the peek prints:

```
mem[0x200]=0x2f78 mem[0x202]=0xfc40 mem[0x204]=0x0001 mem[0x206]=0x2f79
  ((0xfc40>>8)<<8 | 0x2f78&0xff) = 0xfc78
  ((0x2f79>>8)<<8 | 0x0001&0xff) = 0x2f01      -> 0x2f01fc78     measured r0 = 0x2f01fc78
```

Byte-exact. The 16-byte read at `0x1f0` drained `0x1f0..0x20f` — which is why Addendum 26's own read
histogram shows `0x200`–`0x20e` being read at all, sixteen halfwords for a sixteen-**byte** request,
and nobody asked why. The write direction had buffered the pair correctly since it was written
(`write8` holds the low byte until the high one arrives); only the read direction did not.

One line, symmetric with the write side. Category **(c)** — a model defect, not a missing device,
and it had been mis-attributed to the device for one addendum. `0x1f8` and `0x1fc` were *already*
being set to 1 by the model's `on_write(0x10000400)` synthesis, so the co-processor had been
answering "firmware up" correctly the whole time and the answer never survived the bus.

With the read path fixed, `w2 == 1` passes and the boot fails one gate later, on
`(w3 & 3) == 0` — `w3` is `1`, and `1` is not a 4-byte-aligned pointer. That is a real failure: the
model had no directory to point at.

### 2. The layout, read out of the reader

Five functions define it, and nothing below is chosen except where §6 says so.

**The header at `0x1f0`** — `FUN_00288058`:

| off | width | meaning | test |
|---|---|---|---|
| `+0x00` | u32 | read, never examined | — |
| `+0x04` | u32 | read, never examined | — |
| `+0x08` | u32 | firmware/registry ready | must be **exactly 1** |
| `+0x0c` | u32 | address of the channel directory | non-zero, `& 3 == 0` |

`+0x08` is internal `0x1f8`, which is also what `FUN_00287964` polls — and `FUN_00287698`, the
`VMCS.BIN` uploader, **writes zero to `0x1f8` and then spins until it reads back non-zero**
(`do { iVar3 = FUN_00287964(); } while (iVar3 == 0);`). Rockbox's `lcd-video.c` `bcm_init` does the
identical dance — `bcm_write32(BCMA_COMMAND, 0)` … `while (bcm_read32(BCMA_COMMAND) == 0) yield();`
— so the handshake is publicly attested. RetailOS asks for one thing more than Rockbox does: the
value must be `1`, and the next word is a pointer.

**The directory** — `FUN_00286aa8`, `FUN_00287194`, `FUN_00288978`, all with the same shape:
**eight `u16` slots** at `w3`. Slot value `0` means "no service"; otherwise it is a byte **offset
from `w3`** to a record. The scanner reads 16 bytes at `w3 + slot[i]`, and the **matching slot's
index `i` is the channel id** — `*puVar1 = uVar4`, and that is the `-1` at `0x1082359c` Addendum 26
found never being written.

**The record**, 0x50 bytes, pulled down whole by `FUN_002882c0`
(`FUN_00287a6c(state + ch*0x50, slot + *base, 0x50, 0)`):

| off | width | meaning | who reads it |
|---|---|---|---|
| `+0x00` | u32 | read, never examined | all three scanners |
| `+0x04` | u16 | **service tag** — 1, 2 or 7 | the scanners; the match is on this |
| `+0x06` | u16 | TX ring start (offset from base) | `FUN_0028871c`, `FUN_00288398` |
| `+0x08` | u16 | TX ring end | same |
| `+0x0a` | u16 | RX ring start | `FUN_00288434`, `FUN_00288374` |
| `+0x0c` | u16 | RX ring end | same |
| `+0x10` | u16 | **TX read** pointer — co-processor writes, host polls | `FUN_00288928` pulls it down |
| `+0x20` | u16 | **TX write** pointer — host writes | `FUN_00288800` pushes it up |
| `+0x30` | u16 | **RX read** pointer — host writes | `FUN_0028850c` pushes it up |
| `+0x40` | u16 | **RX write** pointer — co-processor writes | `FUN_002885cc` pulls it down |

Two rings, four pointers, each pointer alone in its own 16-byte block so that either side can update
its own without touching the other's. Ring pointers are byte offsets **from the same base as the
directory** — `FUN_0028871c` writes to `wr + *DAT_002887fc`, and `DAT_002887fc` is `0x10831980`, the
word `FUN_00288058` filled from `0x1fc`. Wrap is explicit: `if (wr == txEnd) wr = txStart`. Occupancy
is RetailOS's own `FUN_000f5834(lo, hi, rd, wr)` = `wr >= rd ? wr - rd : (hi - rd) + (wr - lo)`, and
the writer keeps `0x10` bytes free so full never reads as empty.

The whole protocol is **16-byte granular**: the header is 16, the payload is padded to 16
(`bic r6, r0, #0xf` on `len + 0xf`), the ring slack is 16, and the four pointer blocks are 16 apart.
Every byte outside the first `u16` of each block is padding, which is why so many fields above are
"read, never examined".

### 3. The wire format — and the magic is in the firmware, not just in the reader

`FUN_0028861c(channel, opcode, len, payload, event)` builds a 16-byte header:

```
+0x00 u32  0xf1a55a1f            *DAT_002886a0
+0x04 u32  sequence              (prev + 1) & 0x7fffffff, one counter per channel at 0x108d3b34
+0x08 u32  opcode
+0x0c u16  payload length, UNPADDED
+0x0e u16  0
```

then the payload rounded up to 16, then `FUN_00288800` to ring the doorbell. The reply is the same
shape, and that is not an inference: `FUN_002872fc` reads exactly `0x10` bytes, **rejects the reply
outright if word 0 is not the magic** (`if (local_28 != DAT_002874ec) return 0xffffffff`), and takes
its length from word 3's low `u16`, with `0xffff` meaning "stream until terminated". Six independent
display call sites then read exactly `0x20` bytes and take the word at `+0x10` — header plus one
16-byte payload, payload word 0 = result.

**Cross-checked against the co-processor's own image.** `0xf1a55a1f` appears **six times** in the
201 376-byte `rsrc` `vmcs.bin`, little-endian, at `0xe41a 0xe5d4 0xe618 0xe740 0xe7e6 0xe82a` — odd
addresses, so they are immediates inside VideoCore instructions rather than aligned data. The format
derived from RetailOS's writer and the constant embedded in the firmware's code agree. Control for
that search: `b'gencmd_register'`, already proven present at `0x206a4`.

### 4. The opcode table, and a correction to Addendum 28 §2

Eight functions call `FUN_0028861c`. Seven of them are the tag-2 display client:

| fn | opcode | request | reply consumed | reached in a boot |
|---|---|---|---|---|
| `FUN_00286f34` | 1 | — | `+0x10` | 41 |
| `FUN_00286eb4` | 2 | — | `+0x10` | 41 |
| `FUN_00286b6c` | 3 | 0x20 (handle + two points + four rects) | `+0x10` | 41 |
| `FUN_00286c24` | 4 | 0x10 (a handle) | `+0x10` | 40 |
| `FUN_00286ca8` | 8 | 0x20 (type, w, h, pitch, address) | `+0x10` handle, **`+0x14` address** | 2 |
| `FUN_00286d74` | 9 | 0x10 (a handle) | `+0x10` | 0 |
| `FUN_00286df8` | 0x10 | 0x10 | `+0x10` | 0 |

`FUN_00164f44` — the present path — issues 1, then 4 if a previous object is live, then `0x10`, then
uploads the dirty scanlines straight to the surface's co-processor address, then 3. That is
`update_start` / `object_remove` / `object_add` in DispmanX shape, and opcode 8 taking
`(type, width, height, pitch)` and returning `(handle, address)` is `resource_create`. The
correspondence is **proposed**, not derived; a parallel agent is mapping it against the published API.

The eighth caller settles what GENCMD actually is. `FUN_000e9358` does `vsnprintf(buf, 0x100, fmt,
args)`, takes `strlen(buf)`, and sends **opcode 1 with `strlen + 1` bytes of NUL-terminated text** on
the channel at `0x108235d8` — which is `DAT_00287244`, the struct `FUN_00287194` fills when it
matches **tag 1**. And `FUN_002872fc`, the reply reader with the `0xffff` streaming length, reads the
same channel.

So Addendum 28 §2's *"GENCMD is a registry … that **is** the service directory Addendum 26 found
RetailOS failing to read"* is **wrong, and the correction matters**. The directory is the
**transport** layer: eight channels, each a ring pair with a numeric type tag. **GENCMD is one
service on one of them** — tag 1, opcode 1, a printf-formatted command string in, a text stream back.
`gencmd_register` registers a *command name* with that service; it has nothing to do with how a
channel is found. Tag 2 is the display; tag 7 is a third service (`FUN_00288978`, two event groups
and buffers of 0x520/0x520/0x34/0x34) that is not identified here.

### 5. Correction to Addendum 28 §3, re-measured

Addendum 28 §3 said none of the 183 runtime symbol names appear in `vmcs.bin`. Re-measured by
parsing the `.VLL` ELF symbol tables and testing each name as a byte string against both copies:

```
rsrc  vmcs.bin  201 376 bytes:  183 of 183 present   gencmd_register at 0x206a4
NOR   vmcs.bin  101 728 bytes:    2 of 183 present   gencmd_register ABSENT
control 'vc_image_malloc':       FOUND in both
```

Two faults, and the control is what exposes them: the search hit the **NOR** copy — the one the
*bootloader* uploads, which Addendum 27 §1 explicitly warns is a different file — and it used
`grep -c` on binary data, which reports 0 rather than a count. `vc_image_malloc` is present in both
copies, so an instrument that worked would have said so. **A zero from a tool not verified on that
data type is not a measurement**, and this is the same family as the `--watch-range` cap in
Addendum 26 §1.

What the correction buys, tested: the `rsrc` image carries a string block from `0x20100`, 194
printable runs, prefix-grouped and sorted, and **156 of them are pointed at by an aligned `u32`
elsewhere in the image** — a real name-indexed export table. What it does **not** buy is the
registry: searching the image for three `0x50`-strided records carrying tags `{1, 2, 7}` at `+4`
returns **0 hits**, and `0x1f0..0x1ff` is all zero in the `rsrc` copy exactly as it is in the NOR one.
The directory is built at run time, so Addendum 27 §3's conclusion survives on the correct file, and
RetailOS's reader remains the only source for the layout. The two sources agree where they overlap
(§3's magic); they do not conflict anywhere.

### 6. The model — and what in it is chosen rather than derived

`--bcm-registry`, off by default. On, `on_write(0x10000400)` — the same trigger that already
synthesised "firmware up" — publishes the header, an eight-slot directory with tag 2 in slot 0, and
one 0x50-byte record; a write to record `+0x20` drains every complete request from the TX ring and
appends a reply to the RX ring.

**Derived**: every field offset and width in §2, the `== 1` and 4-alignment tests, the eight-slot
count, the tag at `+4`, the ring bounds and the four pointers, the wrap rule, the 16-byte header, the
magic, the sequence counter, the unpadded-length-with-padded-transfer rule, the 16-byte reply
payload, and which reply word each caller consumes.

**Chosen, and it could be wrong without the machine noticing**:

- where the base lives (`0x40000`) and how big the rings are (8 KiB each). The reader constrains the
  base only to be non-zero and 4-aligned, and the rings only to fit in a `u16` offset from it.
- **surfaces are allocated from `0xE0000` upward.** Rockbox calls `0xE0000` `BCMA_CMDPARAM` and puts
  the panel image there; Apple's bootloader fills exactly `0xe0000..0x10581e`, one 320x240 RGB565
  frame. So the choice is consistent with the published map — but the reply format says the
  co-processor returns *an* address, not *which*, and if it is wrong the frame lands somewhere else
  and this section's pixel claim is about the wrong buffer.
- handles are a counter. Nothing in the reader constrains them beyond non-zero.
- non-8 opcodes reply with a handle in payload word 0. Six call sites read that word; none of them
  branch on it in any path reached here, so the value is unconstrained by measurement.

There is no timing model: the reply is placed synchronously, inside the doorbell write. RetailOS
tolerates that because `FUN_002883d4` refreshes the co-processor's write pointer (`FUN_002885cc`)
before it blocks — but a real co-processor answers later, and any bug that only appears when the
reply is late will not appear here.

### 7. What happened

Full boot, `--clock=5 --stop-when-idle=400000000`, budget 4 G:

```
0x00288058  reads 0x1f0, w2 == 1, w3 aligned            directory accepted
0x00286aa8  finds tag 2 in slot 0                       channel index 0, not -1
0x00286ca8  x2  opcode 8                                two surfaces, two addresses
0x00164450  x2                                          both accepted; 307 200 bytes of bitmap uploaded
0x001649ac  x1  returns 0                               a layer, double-buffered
0x00164878  x41  r1 = 0x00000000                        bound, every time (was -1, x42)
0x001650f8  x41                                         flush
0x00164f44  x41                                         PRESENT  (was 0)
0x00286f34 / 0x00286eb4 / 0x00286b6c   x41 each         update_start / update_end / object_add
bcm gencmd: 165 requests answered, 0 dropped
```

```
--bcm-dump=0xE0000:140:F0   registry off  ->  2 922 non-zero pixels, byte-identical to handoff.ppm
                            registry on   -> 76 607 non-zero pixels, DIFFERS
```

The frame is the iPod 5G **"Charged"** screen — title bar, centred green battery, plug glyph,
anti-aliased text. Nothing in the model draws; every pixel of it came out of RetailOS's own
compositor through `FUN_00164f44`'s scanline upload. **That is the bar this file set in Addendum 26
§8, and it is met.**

The back buffer is a second, independent check that the allocator's *second* reply was used too:
`--bcm-dump=0x106000` is also 76 607 non-zero pixels and **byte-identical to the front** — which is
what a double-buffered static screen should look like, and is not what an accidental single
allocation or a stale bootloader frame would look like.

The 4 unmapped reads at `0xea000078` also disappear — they came from `0x000a0bd0` dereferencing a
pointer on the failed-service path, and that path no longer runs.

### 8. A/B, both ways

| | baseline (pre-fix) | read fix only | read fix + registry |
|---|---|---|---|
| Idle | **@1 610 232 373** | @1 610 279 157 | @1 609 736 757 |
| code buckets | **38 265** | 38 266 | 38 518 |
| ata commands | **770** | 770 | 706 |
| ide irq delivered | **685** | 695 | 628 |
| pp dma | **4 / 201 216 B** | 4 / 201 216 B | 104 / 5 225 216 B |
| unmapped reads | **4** | 4 | 0 |
| bcm halfwords written | **230 572** | 230 572 | 2 749 468 |
| bcm halfwords read | **56** | 28 | 5 420 |
| `0xE0000` non-zero px | **2 922** | 2 922 | 76 607 |
| `cargo test --release` | **21** | 21 | 24 |

The pre-fix column was **re-measured, not quoted**: with the old read path restored the boot
reproduces Addendum 26's baseline to the instruction — Idle @1 610 232 373, 38 265 buckets, 770 ATA,
685 IRQs delivered, 4 unmapped, 2 922 pixels. So the fix is the only variable, and it costs 46 784
instructions and one code bucket while the halfword-read count halves exactly as predicted.

The registry column changes a lot, and honestly should: 41 frames of compositing is 5.2 MB of DMA and
2.7 M halfwords the machine never used to move. It still idles, and it idles 542 400 instructions
*earlier*. The three new tests (§9 of the test file) carry their own controls — a write/read round
trip for the FIFO, "no reply before the doorbell" for the responder, and a zeroed `0x1f0` before the
firmware is started for the registry.

### 9. Predictions that measured out to nothing

- **"The co-processor never wrote `0x1f8`, so `w2` reads 0."** This was Addendum 26's conclusion and
  it was carried into this session as the thing to fix. The model had been writing `1` there since
  the `on_write` synthesis was added; the value was destroyed on the way to the CPU. Two sessions
  aimed at the wrong artifact.
- **"The `rsrc` `vmcs.bin` holds a static service directory."** If services are named in the image, a
  template might be too. Searched for `0x50`-strided triples tagged `{1,2,7}` at `+4`: **0 hits**.
  `0x1f0..0x1ff` is zero in that copy as well. Built at run time, as Addendum 27 §3 said.
- **"Publishing a registry with no responder will hang the display task."** Expected, and the reason
  the responder was written at all — but never measured, because the responder went in first. The
  claim is untested and is recorded here as untested.
- **"`FUN_00286aa8` returning `0xffffffff` in the arrival log means it still failed."** That column is
  `r0` at *entry*, which is an incoming argument, not a return value. The channel index is the thing
  to read, and the proof it changed is that `FUN_0028861c` fired at all.

### 10. What is still not known

- **Tag 7.** `FUN_00288978` matches it and nothing here identifies the service. Its consumer
  allocates two event groups and four buffers (0x520, 0x520, 0x34, 0x34).
- **Opcodes 9 and `0x10`.** Never reached in a boot that ends at the charging screen, so their reply
  shape is derived from the call sites only.
- **Record `+0x00` and `+0x0e`.** Read by every scanner, examined by none. They may be a name
  pointer, a version, or padding; nothing in RetailOS discriminates.
- **What a real reply's payload words 2 and 3 hold.** No caller reads past `+0x14`.
- **Whether `0xE0000` is where the co-processor would really have put the surface.** §6.
- **The name-indexed export table at `0x20100`.** 156 of 194 strings are pointed at; the table's
  record shape is not decoded here, and it is the obvious next thing to read now that the `rsrc`
  copy is known to carry it.

~~**Settled when** the frame that reaches `0xE0000` is the main menu rather than the charging screen —
which is a question about what RetailOS decides to draw, not about whether it can.~~
**Settled 2026-08-14 by Addendum 30.** The frame is the main menu. RetailOS was drawing the charging
screen because we were telling it a charger was plugged in.


### 3c. Sharper than §3b: it is an export table, not just a run of strings

Measured independently by the agent that wrote [research/11](11-the-videocore-runtime.md), and it
supersedes §3b's description without contradicting it. The `rsrc` copy carries a real **export table
at `0x2160C`**: **183 records of `(u32 code_addr, u32 name_ptr)`, sorted for binary search and
`(0,0)`-terminated.** The strings §3b found at `0x201dc` are what its `name_ptr` fields point at.

**Every symbol named in §1 therefore has a known address inside an image we hold** —
`gencmd_register` at `0xc49a`, `dispman_object_create` at `0x713a`, `hostreq_rendertext` at `0xd05e`.

This also settles §3b's recorded count disagreement: **183 is right and 149 was an artefact of
whole-token string matching**, because 34 of the names live only as `name_ptr` targets that the
string scan's grouping missed. Both earlier numbers were measurements of the instrument.

And it explains the NOR copy's 2-of-183 cleanly rather than by exception: that copy is an
**`M25 Diagnostics` build with no display stack at all**, which is why the bootloader can drive a
screen with it and RetailOS could not have.

**The reference document for this API is now [research/11](11-the-videocore-runtime.md)** — the full
183-symbol table with addresses, the DispmanX model, an evidence-tiered mapping to the documented
VideoCore IV API, the GENCMD command vocabulary read out of the image, and the bring-up sequence.

---

## Addendum 30: the charging screen was ours — RetailOS draws its main menu

2026-08-14, the same day as Addendum 29. **The frame at `0xE0000` is the iPod main menu.** Title bar
`iPod`, the six rows `Music / Photos / Videos / Extras / Settings / Shuffle Songs`, disclosure
chevrons, `Music` selected, a green battery in the corner with no plug in it. Getting there took no
new device: it took **not lying to the firmware about whether a charger was plugged in**, and the
thing standing between us and telling the truth was a defect in our own PMU.

Three things retract on the way, one of them an instrument that has been a no-op every time it has
ever been used.

### 1. RetailOS's charger sense is one GPIO bit, and we were holding it down

`--enterlog=0x00282b70` on a registry-on boot, grouped by caller (the grouping is uncapped; the
400-row detail print is not, and 268 arrivals fits inside it):

```
0x00282b70 from lr=0x00265448  x130     r0 = 0x63
0x00282b70 from lr=0x00265368  x129     r0 = 0x10
0x00282b70 from lr=0x00264fdc  x1       r0 = 0x20
0x00282b70 from lr=0x00265240  x1       r0 = 0x13
0x002218ac from lr=0x00164014  x7
                                        first arrival of pin 0x63 @50 415 625
```

`FUN_00282b70(pin, u32 *out)` is Apple's generic GPIO input read, and the pin encoding is in the
function rather than inferred:

```
port  = (pin >> 3) - 1                  A=0 … L=11
group = port / 4                        0 -> 0x6000d000, 1 -> 0x6000d080, 2 -> 0x6000d100
addr  = 0x6000d000 + group*0x80 + (port % 4)*4 + 0x30        (+0x30 = INPUT_VAL)
bit   = pin & 7
```

so **pin `0x63` is `0x6000d13c` bit `0x08` — `GPIOL`, the main/FireWire charger line** — and pin
`0x10` is `0x6000d034` bit `0x01`, `GPIOB`, "charging". Both are exactly the constants Rockbox's
`power-ipod.c` carries for `IPOD_VIDEO`, arrived at from Apple's binary without consulting it.

The predicate is four instructions of `FUN_00265424`:

```
0026542c  mov   r4,#0x1a          ; older board: GPIOC bit 2 — "C2 is firewire power"
00265430  bl    0x00265a74        ; board-variant test
00265438  movne r4,#0x63          ; this board: GPIOL bit 3
00265444  bl    0x00282b70
0026544c  rsbs  r0,r0,#0x1        ; out = 1 - v          ACTIVE LOW
```

`map_hardware` left `0x6000d13c` at the region default of **zero**. Zero on an active-low line is
*asserted*. **We were telling RetailOS, 130 times a boot, that it was plugged into a wall charger.**
`GPIOB` was already seeded `0x01` — not charging — and reading `FUN_00265274`, the composite
power-source state machine, that pair should give state 4 ("on power, not charging") rather than
state 6 ("charging"): `bl 0x00282b70` with pin `0x10`, then `moveq r4,#0x6 / movne r4,#0x4`. The
title said **"Charged"** rather than "Charging". *That last step is read off the code, not measured —
what is measured is the picture and which flag changes it.* The screen was a faithful report of the
machine we had built.

**RetailOS never asks the PMU about this.** `--storeaddr=0x7000c00c` records the I²C
register-pointer write with its PC, and split by PC over a whole boot:

```
0x4000acac (bootloader)   reg 0x34 x1777, plus a one-off init sweep of 0x00,0x02,0x05,0x09,0x1b…0x3a
0x00282fb0 (RetailOS)     reg 0x30 x374   reg 0x2e x215   reg 0x0a x109   reg 0x25 x3
```

The ADC pair and the RTC block, and nothing else. Positive control in the same log: the WM8758's
registers `0x63/0x68/0x6b/0x6c/0x6f` appear from the same PC, so the instrument is not blind to
RetailOS's I²C. The PMU register the bootloader hammers 1 777 times is **the bootloader's**, and it
is polled 1 776 / 1 773 times with and without a charger — identical, therefore not the difference.

### 2. The defect: a completed conversion that reported zero

`GPIOL = 0x08` — nothing plugged in, physically correct — had been tried before and refused to boot
([research/09](09-what-the-hardware-must-supply.md)), which is why the lie was left in place. That
document's conclusion was *"with no charger the bootloader checks the battery, and our PMU cannot
answer that check convincingly … the acceptance condition is **not** identified."* It was our
converter, and here is the transaction, from `--storeaddr=0x7000c00c` and
`--readlog=0x7000c00c,0x7000c010` in one run:

```
@2 491 813  write ptr 0x2e            ADCC1, start bit — conversion begins
@2 492 067  write ptr 0x30
@2 492 179  read  data0 = 0x00        ADCS1
@2 492 185  read  data1 = 0x00        ADCS2 — ready CLEAR
@2 492 252  write ptr 0x30            poll again
@2 492 364  read  data0 = 0x00        ADCS1
@2 492 370  read  data1 = 0x80        ADCS2 — ready SET
```

The bootloader recombines `ADCS1 << 2 | (ADCS2 & 3)` = **0**. A flat cell, on every conversion, for
as long as this model has had a PMU.

The cause is one line. `busy` counted down inside `read_reg(0x30)`, and `read_reg(0x30)` answered
`0` while it was counting:

```rust
0x30 => { if self.busy > 0 { self.busy -= 1; 0 } else { self.regs[0x30] } }
0x31 if self.busy > 0 => self.regs[0x31] & !0x80,
```

Apple reads the pair as a **single two-byte I²C transfer**. The ADCS1 byte spent the last tick of
the countdown and was served from the in-flight state; the ADCS2 byte, two microseconds later in the
same transfer, was served from the completed one. **One transfer straddling both states**, and the
only value the firmware was ever allowed to accept was the synthetic zero.

That also explains research/09's otherwise baffling bisect — *only* `--pmu-force=0x30=0xff` **and**
`0x31=0x83` **together** boot. `force` short-circuits ahead of the `match`, so `0x30=0xff` alone
never decrements `busy` and the ready bit never sets; `0x31=0x83` alone still hands ADCS1 the zero.
Neither is a fact about Apple's firmware. Both are facts about that function.

Category **(b)**, model error — and the fourth time this project has attributed one to missing
hardware. It is the rule the working notes already carry: *before concluding "the hardware must
supply X", check that we are reading it correctly.*

### 3. The fix, and what in it is derived

Result registers are result registers. Nothing else changed.

- `convert()` computes the value, clears **ADCS2 bit 7** and ADCS3 bit 0, stores the value in
  `pending`, sets `busy = 2` — and **does not publish**. `ADCS1`/`ADCS2` keep the previous result,
  which is what a converter's output latch does while a new conversion runs.
- `transfer()` decrements `busy` **once per read transfer, before any byte of that transfer is
  served**, and `latch()`es when it reaches zero. A conversion advances on its own; a host reading a
  register is when it finds out, not what makes it happen.
- `read_reg` no longer knows `busy` exists, so a multi-byte read cannot straddle the transition.

**`busy = 2` is unchanged.** The magnitude is the one thing in the old model that could have been
tuned to make the symptom go away, and it was left exactly where it was — what changed is what it
counts and what it destroys. Two new tests cover it — a two-byte poll of `ADCS1`/`ADCS2` driven
through the real I²C controller registers, and a `--pmu-adc` round trip — each carrying a positive
control matched in width, device and code path (the RTC pair, written and read back the same way).
`cargo test --release` goes 24 → 26.

### 4. What was actually gating, now that it can be measured

`--pmu-adc=CH=VALUE` **has never worked.** It pushed into `m.mem.pmu` — the device that existed
*before* `--pmu` builds one, which on every recipe is `None` — and the `m.mem.pmu = Some(pmu)` three
lines below then replaced it with a chip whose `adc_values` was empty. It printed
`pcf50605 ADC channel 0x3 answers 0x03ff` while doing nothing. research/09's *"sweeping channel 3
alone from `0x200` to full scale **never** lets the boot proceed, and neither does channel 0, channel
4, nor all three together"* is therefore **not a measurement**: the device never saw any of it.
`--pmu-force` sat immediately below and pushed into the right object, which is why forcing worked and
why the difference read as a fact about the firmware. Category **(c)**.

Fixed, and with a calibrated instrument — 120 M-instruction boots, positive control `ch3 = 0x200`
must load `osos`, negative control `ch3 = 0x040` must not; at the 30 M budget tried first the
positive control failed, which is the only reason the budget is 120 M:

| channel | swept | result |
|---|---|---|
| `0x0` `BATVOLT_RES` | `0x000`, `0x001` | boots — **no gate at all** |
| `0x4` `BATTEMP` | `0x000`, `0x3ff` | boots — **no gate at all** |
| `0x3` `ADCIN1_SUBTR` | `0x040`…`0x3ff` | **gates**, and the edge is exact |

```
ch 3 = 0x07f   ->  halts        ch 3 = 0x080   ->  BOOTS
```

**The acceptance condition research/09 asked for is `ADCIN1_SUBTR >= 0x080`** — 128 of 1023, one
eighth of full scale, an exact power of two, which reads as a bit test rather than a millivolt
comparison and is consistent with a subtractor-mode reading whose offset has already been removed.
The model's catch-all for that channel is `0x200`, four times the threshold; that number is still
unmotivated, but it is now known to sit *where* relative to the only edge that exists.

### 5. `GPIOL` tells the truth now, and it is the emulator's default

`map_hardware` seeds `0x6000d13c 0x00000008` — bit 3 set (no main/FireWire charger), bit 4 clear (no
USB charger). A bare iPod, matching `GPIOA = 0x20` and `GPIOB = 0x01` beside it. research/09's
*"deliberately left reporting charger present — a known, documented lie"* is **retired**, and its
stated retirement condition is met by §4 rather than waived.

The third value is measured too, and it fails differently: **`GPIOL = 0x18`** (no main charger, USB
charger present) stalls the bootloader after **801 code buckets, @238 849, 0 ATA commands, 0
halfwords to the co-processor**, spinning on

```
40003640  ldrne r0,[r4,#0x28]   ; r4 = 0x70000000
40003644  bicne r0,r0,#0x800    ;   a reset pulse on bit 11
4000364c  ldr   r0,[r4,#0x28]
40003650  tst   r0,#0x80
40003654  beq   0x4000364c      ;   …and then wait for bit 7, forever
```

That is a **USB block bring-up** — told a USB charger is attached, the bootloader resets the
controller at `0x70000028` and waits for a ready bit we do not model. Category **(a)**, missing
hardware, a different and much earlier failure than the battery gate, and not pursued here.

### 6. The screens

All four dumps are `--bcm-dump=0xE0000:140:F0`, same address, same recipe, one flag apart.

| `GPIOL` | registry | what is on the screen |
|---|---|---|
| `0x00` charger present | off | the bootloader's frame, 2 922 px, byte-identical to `handoff.ppm` |
| `0x00` charger present | on | **"Charged"** — title bar, centred green battery, plug glyph. 76 607 px |
| `0x08` nothing plugged in | on | **the Language list** — `English` selected, 日本語, Čeština, Dansk, Deutsch, Español, Français, Ελληνικά, Italiano, and a green battery with no plug. 75 267 px |
| `0x08` + `rotate=+16` | on | the same list, **highlight on 日本語** — the widget re-renders, the title bar does not. 75 289 px |
| `0x08` + one `select` | on | **the main menu** — `iPod`, Music / Photos / Videos / Extras / Settings / Shuffle Songs, chevrons. 75 791 px |

The last two are a scripted click wheel and nothing else:

```
retail-boot.sh --clock=5 --stop-when-idle=400000000 --bcm-registry --clickwheel \
  --wheel=@1500M:touch,+2M:press=select,+2M:release \
  --bcm-dump=0xE0000:140:F0:menu.ppm            BUDGET=3000000000

  ... --wheel=@1500M:touch,+2M:rotate=+16,+5M:release       -> the highlight moves one row
```

**Wall A is gone.** Addendum 20 recorded *"the list widgets are built and never shown"*; Addendum 24
called it a stuck visibility state; Addendum 25 corrected that to an output stage that never
presented. All three were describing the missing co-processor of Addendum 29. What is on the screen
now is a list widget, drawn, and then a *different* list widget after a click wheel event walked the
whole path from `0x7000c140` through the ISR, the event queue, the widget and the compositor. The
click wheel work of Addendum 16 and the input chain of Addenda 19/21 are validated end to end by one
picture.

### 7. A/B, both ways, and the controls

Every row is `retail-boot.sh --clock=5 --stop-when-idle=400000000`, budget 4 G, measured today. The
pre-fix column is a **rebuild of the committed model**, not a quotation.

| | Idle @ | buckets | ata | unmapped | `0xE0000` px |
|---|---|---|---|---|---|
| **pre-fix, registry off, `GPIOL=0`** | 1 610 279 157 | 38 266 | 770 | 4 | 2 922 = handoff |
| post-fix, registry off, `GPIOL=0` | 1 553 933 365 | 38 229 | 770 | 4 | 2 922 = handoff |
| post-fix, registry off, `GPIOL=8` **(the new default)** | 1 562 789 429 | 38 220 | 770 | 4 | 2 922 = handoff |
| pre-fix, registry on, `GPIOL=0` | 1 609 736 757 | 38 518 | 706 | — | 76 607 "Charged" |
| post-fix, registry on, `GPIOL=0` | 1 553 471 669 | 38 481 | 706 | — | 76 607, **byte-identical to the pre-fix dump** |
| post-fix, registry on, `GPIOL=8` | 1 812 316 856 | 38 476 | 706 | — | 75 267 Language |
| post-fix, registry on, `GPIOL=8`, one click | 1 943 899 715 | 39 107 | 706 | — | 75 791 **main menu** |
| pre-fix, registry on, `GPIOL=8` | halt @8 053 570 | 1 420 | **0** | — | 76 540 bootloader partial |

The pre-fix row reproduces Addendum 29 §8's middle column to the instruction (1 610 279 157 / 38 266)
and its right-hand column exactly (1 609 736 757 / 38 518), so the machine is the one that file was
written on and the fix is the only variable.

**The control-arm invariants all survive**: 770 ATA commands, 4 unmapped reads, and a registry-off
framebuffer byte-identical to `handoff.ppm` — checked against a `--stop-at=0x10000000:1` dump taken
on the fixed binary at the unchanged 46 397 133 instructions. `--rdval=0x6000d13c=0x08` and the
seeded default produce **byte-identical** menu dumps.

**The Rockbox oracle is byte-identical, pre-fix and post-fix** — `diff` over the whole run log
reports no difference. That control exists because a GPIO change once cut Rockbox from 29 frame
updates to 2 (research/09); it did not this time.

`cargo test --release`: **26 pass** (24 before, plus the two in §3).

### 8. Predictions that measured out to nothing

- **"The PMU is telling RetailOS it is docked and charged."** The framing this session started from,
  and it is wrong in its mechanism and right in its conclusion. RetailOS never reads a charger
  register from the PCF50605 — only the ADC pair and the RTC. The lie was a GPIO bit, not a chip.
- **"The screen might be a disk-mode or USB artefact."** Nothing supports it, and one flag that is
  not a USB flag flips the screen. The static reading agrees: the discriminator for the
  "Do Not Disconnect" case is a *host link* — `FUN_0009e664`, a FireWire link or the USB device task
  — which is a different predicate from the charger sense of §1, and the charge screen sits on the
  power-only branch. That second half is read off the code and was not separately measured, because
  by the time it mattered the A/B had already answered the question.
- **"There is no battery threshold — the gate was purely the handshake."** Believed for about twenty
  minutes on the strength of a run where `--pmu-adc` appeared to lower every channel to 375 mV and
  the boot proceeded anyway. The flag was a no-op (§4). With it fixed the same experiment halts, and
  the threshold is exact. **The reason it was caught is that the run's own ADC histogram still
  printed `0x2c0`/`0x200`** — an instrument that reports what it actually did is worth more than one
  that reports what it was asked to do.
- **"A 30 M-instruction boot is enough to answer did-it-load-`osos`."** The positive control failed
  at that budget. 120 M.
- **"`--bcm-dump` non-zero pixel counts measure how much was drawn."** They do not: these screens are
  white, so 75 000–76 800 of 76 800 is the range for *any* full-screen UI. Three different screens
  score 76 607, 75 267 and 75 791. **Only the image distinguishes them**, and the count is useful for
  exactly one thing — telling a composited frame from the bootloader's 2 922.

### 9. What this opens

- ~~**The `select` that reaches the main menu passes through the language list, and nothing here
  confirms the list can be *scrolled*.** One rotation and a second dump would settle it.~~
  **Settled in the same session.** `--wheel=@1500M:touch,+2M:rotate=+16,+5M:release` moves the
  highlight from `English` to `日本語` — the selection bar is on the second row and the title bar is
  unchanged, so the widget re-rendered rather than the screen being replaced. 21 frames posted, 21
  read, 0 dropped, 18 of 18 script steps fired. ~~**Sixteen wheel clicks moved the list one row**,
  which is Apple's own detent ratio and not something this model chose; it is worth knowing before
  anyone writes a script expecting one click per item.~~ **Sixteen is not a ratio and this sentence
  was read as one** — corrected in [research/13](13-do-the-games-load.md) §2.1–2.2. The same 16-click
  burst moves one row and then three rows *in the same run on the same list*, because what a burst
  moves depends on whether the finger ever came off the wheel. The reproducible unit is a whole
  gesture: `touch, rotate=+8, release` is exactly one row, and one to four clicks in a fresh gesture
  move nothing at all.
- **`FUN_00265a74` is a second invented input.** It gates whether the GPIOL pins are used at all —
  `(FUN_00265164() >> 16) ∈ {0xA, 0xB, 0xC}`, cached from a descriptor at `0x10882048 + 0x84`. Get
  the hardware id wrong and RetailOS reads GPIOC/GPIOD instead and none of §1 applies. It happens to
  be right; it is not *known* to be right.
- **`0x70000028` bit 7** is now a named, bounded missing device: the USB block's ready bit, and the
  only thing between us and a `GPIOL = 0x18` boot.
- **The UI string pool has no index table.** `"Charging"` (`0x4b21a4`) and `"Charged"` (`0x4b21b0`)
  have zero code xrefs and zero 32-bit pointers anywhere in the 7.5 MB image; a search for an offset
  table matching the pool's own delta signature, at every alignment and width, returns nothing. The
  pool at `0x4b07b8` is followed by a **1 065-entry monotonic u32 table at `0x4b47d8`** indexing an
  ~84 KB blob at `0x4b587c` that reads as UI bytecode with big-endian operands — so the screens are
  probably *interpreted*, not compiled, which would explain the absence cleanly. Unproven.
- **Nine RTXC tasks have no functions defined in the Ghidra database**, and their names are in the
  task table at `0x0025d63c`: `PCFPowerMgr` `0x00284ff4`, `USBPowerSense` `0x002856a4`,
  `LowBattDebounceTask` `0x00284a18`, `TopPlugTask` `0x0028564c`, `AccessoryDetectTask` `0x00284338`,
  `FirewireTask` `0x00284e58`, `HPhoneDetTask` `0x00284eb0`, `DiskMgrTask` `0x00284b0c`,
  `WatchdogTask` `0x002856f0`. `0x00284000..0x00286000` is valid ARM code with zero analysed
  functions in it.

## Addendum 31: the snapshot carries the clock now — and the screen that comes back on its own is the charger's, not the clock's

2026-08-14, the same day as Addenda 29 and 30, and directly downstream of 30. Two jobs: a snapshot
format that lost the simulated clock, and a symptom — *RetailOS reaches the main menu and reverts to
the "Charged" screen after a few seconds of no input* — that the clock was the obvious suspect for.

**The clock defect was real and is fixed. It is not the cause of the revert.** The revert is
reproduced, bracketed, and belongs to something else entirely: it happens only when a charger is
attached, it happens on a cold boot with no snapshot anywhere near it, and it is RetailOS returning
to its charging screen after ~165 s of simulated inactivity — which is what a 5G on a wall socket
does. Three further things fell out of measuring it, and two of them are defects in our own tools.

### 1. `Machine::snapshot` saved a derived number and not the state behind it

`Memory::usec` is **not stored state**. The run loop recomputes it every instruction:

```rust
self.mem.usec = (self.executed / self.instr_per_usec.max(1)) as u32 + self.mem.slept_usec;
```

Version 3 of the snapshot format saved `usec` and not `slept_usec`. So the restored clock survived
exactly zero instructions: the first one recomputed it against an accumulator of 0. Measured on the
standard idle snapshot, at `--clock=5`:

```
the snapshot says          3 036 268 993 µs
one instruction later      321 775 678 µs        a step of −2 714 493 315 µs
```

**44 minutes of simulated time, backwards, on every restored run, silently.** And the direction is
the trap: firmware measures an interval as `now - start` in *unsigned* 32-bit microseconds, so it
does not see a negative number. It sees **+1 580 473 981 µs — twenty-six minutes of elapsed time**,
arriving in one instruction. Every timeout RetailOS was holding is expired at the moment of restore.

The fix is one field and a version bump: the magic goes `IPODSNP3` → `IPODSNP4`, `slept_usec` is
written next to `usec`, and a v3 image is **refused** rather than read with a zero in the new slot —
because reading it would restore precisely the machine the fix exists to abolish, and would do it
without a word.

**The check is now the identity, not a threshold**, and that distinction is the whole lesson: the
old defect is not recognisable by size or by sign. `ipod-gui` prints, on every restore:

```
restore: the simulated clock round-trips — 3036268993 µs, and `executed / 5 + slept_usec` agrees,
         so the next instruction will not move it.
```

A first version of that line compared the clock before and one *slice* later and complained at any
step over 1 000 µs. It fired on a healthy restore — 250 000 instructions at the idle point advance
the clock 1 725 688 µs through the idle task's sleeps, which is the machine working correctly. A
tolerance tight enough to catch the defect would have cried wolf on every run; the identity catches
it exactly and never otherwise.

**Tests: `cargo test --release` in `eapp-loader` goes 30 → 33** — `tests/load_and_trace.rs` 27 → 30 —
and the round-trip test is built so that "always passes" and "passes correctly" are distinguishable.
*(45 after merging the `ipod-film` work, which brought twelve unit tests of its own; the three below
are this session's.)*

- The fixture is a machine that has genuinely slept — a two-instruction idle loop writing CPU_CTRL's
  sleep bit with a 10 ms timer armed. It runs 5 000 instructions and accumulates **789 984 µs** of
  sleep, so 99.9 % of its clock is in the field the old format dropped. Two premise assertions
  refuse to let that go untested: `slept > 0`, and `usec != executed / instr_per_usec`. On a fixture
  that had never slept the test would pass against either format.
- `dropping_the_sleep_accumulator_moves_the_clock_backwards` is the negative control: restore, zero
  the field the way v3 did, run one instruction, and assert the clock falls back by **exactly** the
  accumulator that was dropped.
- `a_version_3_snapshot_is_refused` has its own positive control — the same bytes with the current
  magic must still restore, so what is refused is the version and not the image.
- **And the ablation was run for real**: with `restore` patched to discard `slept_usec`,
  `a_snapshot_round_trips_the_simulated_clock` fails with `the sleep accumulator was not carried:
  left 0, right 789984`. A test nobody has watched fail is a test nobody has watched.

### 2. The full-boot baseline is unchanged, to the byte

`BUDGET=4000000000 tools/ipod-boot/retail-boot.sh --clock=5 --stop-when-idle=400000000`, before and
after the format change, same binary tree, same disk image:

| | Idle @ | buckets | ata | unmapped |
|---|---|---|---|---|
| pre-fix | 1 562 789 429 | 38 220 | 770 | 4 reads, 0 writes, 1 page |
| post-fix | 1 562 789 429 | 38 220 | 770 | 4 reads, 0 writes, 1 page |

`diff` over the two whole run reports shows **one differing line**, the per-run temp disk path
(`ipod-retail-boot-13249.img` vs `-16451.img`, the shell's `$$`). This is the expected result and it
is worth stating why it is not evidence of anything: **a cold boot never restores**, so a change
confined to `snapshot`/`restore` cannot reach it. The baseline's job here is to prove the change was
*confined*, not to prove it works.

**What changes for restored runs**, plainly: every restored machine now runs with a simulated clock
about 2.7 billion µs *higher* than it used to, from the first instruction. Any number in `research/`
that was measured from a restored snapshot **and depends on elapsed simulated time** may move. The
audit of that is short, because so little is anchored in `usec`: every wheel script in this file is
anchored in *instructions* (`WheelStep::at`, by explicit design — see the type's doc comment), the
enter/store/read logs are instruction-anchored, and the ATA and BCM counters are event counts. The
one class that is not is any timeout RetailOS itself computes, and §3 and §4 below are the first
measurements of that class ever taken on a machine whose clock is continuous. Snapshots are cached
under a hash of the emulator's own source, so every existing one re-mints on the next run rather
than being restored into the new model — the hybrid-machine failure `from-idle.sh` documents at
length does not arise here.

### 3. The revert: reproduced, and it is the charger

Six cold arms, each a full boot with `--bcm-registry` and the click wheel, one press of Select at
instruction 1 500 000 000 — the anchor Addendum 30 §6's own `--wheel=@1500M:…` script uses — and
then the panel at `0x000e0000` sampled at fixed instruction counts with nothing touching the
machine. The only variable between the halves of the table is `GPIOL` bit 3, the mains-charger sense
of Addendum 30 §1.

| arm | before | +4 M | +8 M | +32 M | +33 M | +40 M | +100 M | +800 M |
|---|---|---|---|---|---|---|---|---|
| **bare iPod, Select** | 75 267 list | | 75 267 | | | 75 396 | **75 791 menu** | **75 791 menu** |
| bare iPod, no input | 75 267 | | 75 267 | | | 75 267 | 75 267 | 75 267 |
| **charger, Select** | **76 607 Charged** | 75 267 UI | 75 267 | 75 267 | **76 607 Charged** | 76 607 | 76 607 | 76 607 |
| charger, no input | 76 607 | | 76 607 | 76 607 | 76 607 | 76 607 | 76 607 | 76 607 |

Digests, not just counts — Addendum 30 §8 already retired pixel counts as a way of telling screens
apart. `846eae07…` is the Language list, `208e012f…` the main menu, `6304ad7c…` the Charged screen,
`01149fcb…` the UI as it appears on the charging machine. Every arm writes a PNG per sample to
`_out/probe-*-*.png`.

**On a bare iPod the menu does not revert.** It came up at +100 M and was still there 700 M
instructions later — and because the idle task's sleeps run the clock far ahead of the instruction
count, that is **about 32 minutes of simulated time** on one screen with no input. Pressing Select
*later*, at 2 000 000 000 — past the 1.81 G idle rather than before it — gives the same result out
to 2.8 G. So it is not a matter of the press landing at the wrong moment.

**With a charger attached the revert is exact.** The UI is up at +4 M (2 756.930 s simulated) and
still up at +32 M (2 916.419 s); by +33 M (2 919.060 s) the Charged screen is back, byte-identical
to the one that was there before the press. The press was at 2 752.992 s, so:

> **The UI survives 163.4 s of simulated inactivity and is gone by 166.1 s.**

The matched control — same boot, same charger, same sampling instants, no press — never leaves the
Charged screen at all, so the +4 M change is the input's and the +33 M change is the timeout's.

That is the operator's symptom, and it is **authentic behaviour**: a 5G on a wall socket returns to
its charge display when left alone. It has nothing to do with snapshots, and the clock A/B in §4
shows it has nothing to do with the clock either. It also explains why `tools/ipod-gui/README.md`
described the machine as timing "back out to the charging screen" after a scroll — that document was
written on the machine of *before* Addendum 30, where `GPIOL` was held low by our own region default
and every boot therefore believed it was plugged in. `--charger` now exists in `ipod-gui` to put a
machine back into that state deliberately, which is how the table above was made.

### 4. The clock jump, isolated: it changes nothing on the screen

`--clock-v3` zeroes `slept_usec` **after** a restore that carried it correctly, so the two arms share
one snapshot file — literally the same bytes on disk — and differ in one assignment. Restored idle
machine, no input, panel sampled at seven instants over 800 M instructions:

| | clock at the restore point | panel, all seven samples |
|---|---|---|
| carried (`IPODSNP4`) | 3 036 268 993 µs | 75 267, `846eae07…`, unchanged |
| **zeroed (`--clock-v3`)** | **321 775 678 µs** | 75 267, `846eae07…`, unchanged |

**Identical.** A 44-minute backwards discontinuity in the simulated clock does not, on its own, make
RetailOS redraw anything. The hypothesis this session started from — *the backwards jump makes the
idle timer compute a nonsense elapsed time and drop to the charging screen* — is **wrong**, and it is
wrong in an instructive way: on the machine where a charging screen exists at all, the timeout fires
after 165 s whatever the clock did, and on the machine where it does not exist, no amount of clock
damage produces one.

### 5. Restored and cold still answer the same input differently — and the clock was not why

The observation the GUI recorded and could not isolate — *"a restored machine and a cold-booted one
at the same instruction count answer the same input differently"* — survives the clock fix. It is
now measured under one variable, and it splits into two separate things.

**First: on a bare iPod a restored machine's wheel is deaf, and nothing re-arms it.** A snapshot does
not carry the click wheel, so a restored machine starts with the `0x052a` reporting gate shut and
`service_clickwheel` suppresses every autonomous frame. `ipod-gui`'s README says the firmware
re-sends `0x8001052a` "about once every 20 M instructions", so the gate reopens within a second. On
the current machine it does not:

```
500 M instructions after a restore:  reporting OFF, 0 `0x052a` commands, 3 293 code buckets executed
```

Zero. `--probe=menu` on a restored bare machine reports `ABANDONED … the gate never opened within
400 M instructions` and pushes nothing, because pushing would only have incremented
`frames_suppressed`.

The 20 M figure was measured on the charger-present machine, and there it is right: the same cold
boot with `--charger` sends **28** `0x052a` commands before 1.5 G, the last at 1 481 103 395, and a
machine restored from it has the gate open **1.75 M instructions** later. Addendum 30 §1 read the
reason off the code without knowing it would matter here — `0x001d8198` sends payload 1 on the arm
carrying the 10 s/120 s timers, `0x001d8418` sends 0 on the 500 ms arm — so the enable is a
*power-state transition*, not a heartbeat. A machine plugged in transitions constantly; a bare one
sitting on a list does not transition at all. Two consequences had to be fixed rather than described:

- **`ipod-gui --selftest` could not run on the current model.** Stage 0 waited for that gate with no
  bound, on a machine where it had always opened within 3.5 M instructions. It now abandons after
  400 M with the reason printed. A test that hangs is indistinguishable, from outside, from a slow
  one.
- **The self-test's two arms were never anchored to the same moment.** It waits for the gate and
  *then* acts — and the gate opens during the boot in a cold machine and after the idle point in a
  restored one, so the cold and restored arms delivered their gesture hundreds of millions of
  instructions apart, at completely different points in RetailOS's startup. The README's "only after
  a restore does the machine wake into the Language list" was measured across that confound.
  `--probe` exists because of it: every arm acts at a fixed instruction anchor.

**Second, and this is the real one: with the charger attached the wheel *is* armed, the input
arrives identically, and the screen still does not change.** One snapshot file, taken at
1 500 000 000 on a charger boot; one arm restores it, the other keeps running from the cold boot that
wrote it; both press Select at the same anchor with the same four events; both sample the same
instants:

| | decoder | edge | scroll | button ev | wheel ev | panel at +4 M | at +32 M | at +40 M |
|---|---|---|---|---|---|---|---|---|
| **cold** | 4 | 4 | 2 | 2 | 1 | **75 267 UI** | 75 267 UI | 76 607 Charged |
| **restored** | 4 | 4 | 2 | 2 | 1 | 76 607 | 76 607 | 76 607 |

**Every input counter is identical.** The frame reaches Apple's ISR decoder four times, the edge
dispatcher four times, and RetailOS's own event system twice as a button event and once as a wheel
event — the same numbers, in the same order, at the same instruction counts. And then one machine's
panel changes and the other's does not.

#### It is one page flip, and the restored machine draws the identical picture

The counters said the co-processor was written to in both arms — **2 825 308 halfwords** in the cold
session (which includes the whole boot's uploads) and **282 400** in the restored one, which began at
1.5 G and therefore has nowhere else to have come from. 282 400 halfwords is 3.7 frames of a 320x240
RGB565 surface. A machine that has drawn nothing does not write a third of a megabyte of pixels.

So the probe was made to sample **both** surfaces — the front at `0x000e0000` and the back at
`0x00106000` — and the answer is unambiguous:

| | front `0x000e0000` | back `0x00106000` |
|---|---|---|
| **cold**, +4 M to +32 M | **75 267, `01149fcb…` — the UI** | 76 607, `6304ad7c…` Charged |
| **restored**, +8 M to +32 M | 76 607, `6304ad7c…` Charged | **75 267, `01149fcb…` — the UI** |

**Same picture. Same digest, to the pixel. The other buffer.** And it times out on the same schedule
— by +40 M both arms are back to the Charged screen on whichever surface they were using.

That retires the alarming version of the sentence this section started from. A restored machine and a
cold one do **not** answer the same input differently in any behavioural sense: they answer it
identically, draw the same frame, and differ by one page flip in which surface that frame lands in.
What differs is not RetailOS. It is that **nothing in this model represents which surface the panel
scans out** — `--bcm-dump`, `--selftest` and the window all read `0x000e0000` because that is where
Apple's bootloader put the first surface, and a machine whose double-buffer is on the other phase is
therefore invisible to every instrument this project owns. The GUI now counts both surfaces and says
so when the one on screen is static while the other is moving; `_out/probe-*-back.png` is written
beside every sample.

Three of the obvious explanations were excluded on the way, and they stay excluded:

- **Not the clock.** §4, and this arm prints `the simulated clock round-trips … `executed / 5 +
  slept_usec` agrees` before it starts.
- **Not the input path.** The counters above are identical.
- **Not the PMU's host-written state.** `Machine::restore` never touches `Memory::pmu`, so a restored
  machine runs with the `Pcf50605` that `build()` made — power-on defaults, an RTC block of zeroes,
  everything the boot wrote to it gone. `--ablate=pmu` does that to a *cold* machine at the moment of
  the press, discarding what 2 361 reads' worth of traffic had built, and the run is
  **indistinguishable** from the un-ablated one: same screens, same digests, the same simulated clock
  to the microsecond at all five samples. *This negative is narrower than it looks and the width
  matters* — a fresh `Pcf50605` answers ADC conversions identically by construction, so what the
  ablation actually destroys is only what the host had written, which is exactly the state a restore
  loses, and nothing more. It is not a demonstration that the flag can change any behaviour at all.

What a snapshot still omits, for the record, since one of these is the likeliest reason the phase
differs: the **click wheel** (the whole device, registers included), the **co-processor's
allocator** — `next_handle` and `next_surface` are not among the four scalars `Bcm::save_scalars`
carries, so a restored co-processor hands out handles and surface addresses it has already handed
out — the **NOR** and **external memory bus** state machines, and `read_toggle`'s alternation state.
The allocator is the one to try first now that the failure is known to be at the *surface* level. It
was not tried here: putting it in the snapshot changes every restored run again, and this session
has already changed them once.

### 6. MENU+SELECT is delivered, decoded, and ignored

`ClickWheel::buttons` is a five-bit mask and has always been able to report two buttons at once, so
the hardware's hard-reset chord is expressible without changing the model. Cold boot, Select at
1.5 G to reach the main menu, then **MENU and SELECT held together for 400 M instructions** — at
`--clock=5` and with the idle task's sleeps that is minutes of simulated time, against the six to ten
seconds a real 5G wants — and released:

| | decoder | edge | button events | panel through the hold | after release |
|---|---|---|---|---|---|
| **MENU+SELECT held** | 8 | 8 | 6 | 75 791 main menu | 75 791, unchanged at +800 M |
| SELECT alone held | 7 | 7 | 4 | 75 791 main menu | **75 403, a different screen** |

**The machine does not reset.** It does not re-enter the reset vector, it does not redraw, it runs on
to 2.3 G instructions with the same menu on screen. The positive control is in the same row: eight
arrivals at Apple's ISR decoder and six button events say the pair *reached RetailOS's event
system* — this is not a chord that failed to be delivered.

The matched control is the informative half. Holding SELECT **alone** for the same span, released at
the same instant, does change the screen — a menu row activating, as a long press of Select on a
list should. So the pair is not merely unhandled; on this firmware, having MENU down alongside it
suppresses what SELECT alone would have done. That is one arm's worth of evidence for a reading, not
proof of one.

Static search agrees that nothing tests the pair. In the query frame RetailOS reads, the five buttons
sit at bits 16..20, so SELECT|MENU is `0x110000`; across `cmp cmn tst teq and bic sub eor orr mov`
over all 641 479 disassembled instructions of `OSOS_correct.bin` that immediate appears **zero**
times. (Positive control on the same instrument: `0x1100` — the same pair in the *streaming* frame's
bit order — returns 24 hits, so the search works. See §8 for what those turned out to be.)

**On real hardware the chord is caught below the firmware** — the wheel's PSoC or the PMU, neither of
which this project models; `ClickWheel` is a transceiver that posts frames, and there is no path in
it from `buttons` to anything that could reset a machine. So the honest UI is the one now in
`ipod-gui`: the chord delivers the buttons and is labelled as delivering them, and the controls that
actually restart the machine are labelled as emulator controls. A button captioned MENU+SELECT that
secretly rebuilt the `Machine` would be the window claiming a hardware behaviour we have measured to
be absent.

### 7. Power off and cold boot, from the window

`ipod-gui`'s emulator thread is now a loop over **power cycles** rather than a single session.
Powering off drops the `Machine` outright — CPU, all 64 MB of SDRAM, the co-processor's surface —
publishes a dark panel and zeroed statistics, and waits. Powering on builds a new machine and enters
at address 0 through `call_with(0, …)`, which is the same entry `retail-boot.sh` makes; it is not a
restore wearing the name.

- The **drive survives**, because that is what survives a real power cycle. A second boot finds the
  volume RetailOS built on the first.
- Only the **first** session may restore a snapshot or write one. A session reached by powering back
  on has been asked for a cold boot explicitly, and a snapshot taken during it would have been taken
  against an already-written drive — quietly changing what every later restore restores.
- Input queued at the old machine is discarded with it.
- A machine that stops on its own (`Stop::Lost`) keeps its reason on screen and the power controls
  live, which is exactly when someone wants them.

`--power-cycle-at=N` is the self-check, because "the GUI can cold boot" is a claim until a second
boot fingerprint says so. Restore at 1.6 G, cut power at 1.65 G, and let it run:

```
restore: the simulated clock round-trips — 3036268993 µs …
power cycle: cutting power at 1650249952 instructions, 3297161879 µs simulated, 75267 non-black
headless: Idle after 1755959163 instructions
  ata commands: 573        36 348 code buckets        bcm: 4 kicked, 2 frame updates
  framebuffer 0x000e0000: 75267 non-black       wheel: reporting ON, 5 `0x052a` commands
```

The instruction count **restarts from zero** and climbs to a fresh idle: this is a boot, not a
resume, and a restore wearing the name would be caught by that number alone. And the second boot is
demonstrably not the first: **573 ATA commands and 36 348 code buckets against the first boot's 706
and 38 479**, because the drive it woke up to already had the volume RetailOS built before the power
was cut. The machine is new; the disk is not. That pair of numbers is what makes the power cycle a
measurement rather than a screenshot.

### 8. `ipod-gui` did not compile at `HEAD`

Found by building it. `report_headless` reads `d.command_count` on `Ata`, and that field no longer
exists: the `Capped<T>` work (research/12) replaced it with `commands: Capped<…>`, whose census is
`commands.seen()`. The two changes were merged from different branches and nothing rebuilt the
window, so the crate on `main` was broken from the moment it landed — which also means the
`--headless` comparison table in its README (`Idle after 1609725109`, 38 521 buckets, 706 ata) cannot
have been produced by the merged tree. It was produced before the charger fix of Addendum 30, and it
is stale in exactly the way that fix predicts. Re-measured on the merged tree, the window and the
recipe agree exactly again, in the window's own configuration — registry on, click wheel on, no
charger:

```
$ ipod-gui --cold --headless=2000000000     $ BUDGET=4000000000 retail-boot.sh --clock=5 \
Idle after 1812313976 instructions              --stop-when-idle=400000000 --clickwheel --bcm-registry
  706 ata, 38 479 buckets, 0 unmapped       -> Idle after 1812313976, 38 479 buckets, 706 ata
  bcm 4 kicked / 2 frame updates               bcm 4 kicked / 2 frame updates
  framebuffer 75 267 non-black                 bcm dump 75 267 non-zero
```

(Addendum 30 §7's table reads 1 812 316 856 / 38 476 for the same row with the click wheel *off* —
2 880 instructions and three code buckets of difference, which is what modelling the wheel costs.)

Fixed to `commands.seen()`, with the reason in the comment, because the next person to write that
line will reach for a length again.

### 9. Predictions that measured out to nothing

- **"The backwards clock jump causes the revert."** The session's opening hypothesis and the reason
  the clock was fixed first. Isolated with a one-assignment ablation over a shared snapshot: no
  effect on the panel at any of seven instants. The clock defect was real, worth fixing, and not the
  cause of anything anybody had noticed.
- **"A real inactivity timeout that the cold run simply did not reach."** Half right, and the half
  that was wrong is the interesting half. There *is* a real inactivity timeout, it fires at ~165 s of
  simulated time, and the cold run reached it easily — the reason a bare-iPod cold run does not
  revert is that with no charger there is no charging screen to revert *to*, not that it ran out of
  time. Both halves of that were guessed the wrong way round.
- **"`0x1100` in the image is the MENU+SELECT mask."** It is Rockbox's mask for exactly that pair, in
  the streaming frame's bit order, and there are two `cmp r1, #0x1100` sites in RetailOS. Both are
  something else: `0x0026f248` is a `glHint` argument validator (`0x1100`/`0x1101`/`0x1102` are
  `GL_DONT_CARE`/`GL_FASTEST`/`GL_NICEST`, and the string `"glHint"` is in the literal pool four
  words away), and `0x0017dbdc` compares against `0x1100`/`0x1080`/`0x1000` in a chain that returns.
  The four `sub rX, rY, #0x1100` sites are switch dispatches over values ≥ 0x1161. Rule 6, paid
  again: do not infer a call's purpose from its shape — disassemble it.
- **"The `u32` microsecond clock wrapping is a second discontinuity worth chasing."** It wraps every
  4 294.967 s and every long run crosses it — visible three times in the tables above. It is benign:
  unsigned `now - start` is correct across a wrap for any interval shorter than 2³² µs, which is
  every interval RetailOS measures. Noted and dropped.

### 10. What is not established

- **The operator's own session was not reproduced, only its symptom.** The charger arm reproduces
  the described behaviour exactly — UI, then the Charged screen with no input — but which
  configuration their window was actually in (charger-present model, restored or cold) is not
  something this file can determine from here. If it was a *restored* session on the current bare
  machine, §5 says the window would have accepted no input at all; if it was a restored charger
  machine, the picture would have been drawn to the surface the window does not show. Three
  different complaints share the same first sentence.
- **Why the page flip is out of phase after a restore is not identified**, only that it is and that
  the picture is otherwise identical. The `Bcm` allocator is the first suspect and was not tested.
- **Which RetailOS timer fires at ~165 s is not identified.** Addendum 30 §1 names an arm with 10 s
  and 120 s timers at `0x001d8198`; 165 s is neither, and no attempt was made here to tie the
  measured interval to a constant in the image.
- **Whether the wheel's gate reopens on a restored *charger* machine** — the configuration where the
  README's 3.5 M figure was measured — was attempted and mis-run: the charger machine reaches idle
  at 1 553 481 845, *before* the default 1.6 G snapshot point, so a `--headless` mint stops at idle
  and never writes a snapshot, and the "restored" arm silently cold-booted instead. The re-run is at
  `--snap-at=1500000000`. That the cold charger boot sends 31 `0x052a` commands is measured; that a
  restored one gets any is not.
- **Nothing here says the 165 s timeout is Apple's own number rather than an artefact of
  `--clock=5`.** The clock ratio is 15x faster than real silicon per instruction and the idle task's
  sleeps advance it further; a timeout measured in simulated seconds is only as real as that mapping.
