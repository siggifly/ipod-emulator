# The other four images in the flash

`osos` is not the only bootable thing this iPod has. The NOR carries a directory at `0xffe00` of
40-byte records, each tagged `flsh` (stored reversed, `hslf`), describing five images:

| tag | devOffset | length | load addr | entry | checksum |
|---|---|---|---|---|---|
| `disk` | `0x000d3bd0` | 180 784 | `0x10000000` | 0 | **valid** |
| `diag` | `0x000a2cb8` | 200 472 | `0x10000000` | 0 | **valid** |
| `scan` | `0x00089fdc` | 101 596 | `0x10000000` | 0 | **valid** |
| `logo` | `0x000879f8` | 9 700 | `0x10000000` | 0 | **valid** |
| `vmcs` | `0x0006ec98` | 101 728 | `0x10000000` | 0 | **valid** |

All five checksums verify as a plain byte sum. `disk`, `diag` and `scan` open with `ea000006` — an
ARM branch, so they are vector tables and therefore bootable. `logo` opens with `"LoGo"` and `vmcs`
with zeros; those are data.

**The contract is identical to `osos`**: a raw ARM image loaded at `0x10000000` and entered there.
So `tools/ipod-boot/flsh.sh` boots any of them exactly the way RetailOS is booted — and they are
two orders of magnitude smaller, 200 KB against 7.5 MB.

## Diagnostics boots, and then waits to be talked to

`diag` runs. It does **not** remap SDRAM to 0 the way `osos` and Rockbox do — it executes in place at
`0x1000xxxx` — and it stops here:

```
1000c6a0  ldr  r0, [pc, #-0x2ec]   ; 0xb0020000
1000c6a4  ldrh r0, [r0, #0x0]
1000c6a8  tst  r0, #0x1
1000c6ac  beq  0x1000c6a0
1000c6b0  ldr  r0, [pc, #-0x2f8]   ; 0xb0060000
1000c6b4  ldrh r0, [r0, #0x0]
      …
1000c6bc  beq  0x1000c6a0
```

**101 185 488 unmapped halfword reads of `0xb0020000`** from that one instruction, plus writes to
`0xb0030000`, and three related literals: `0xb0020000`, `0xb0040000`, `0xb0060000`. Nothing else in
this project touches `0xb00x0000`, and it appears in no register map we have — Rockbox's `pp5020.h`
included.

Supplying bit 0 of `0xb0020000` clears the first gate and it advances to the second, at
`0xb0060000`. That is a **two-register handshake**, and the shape — a ready bit, a second status, a
write port — is a UART.

### Which makes it less useful than it first looked

The hope was that diagnostics would be Apple's own hardware test suite: same codebase family as
RetailOS, written to report what the hardware is doing, and small enough to read. It may still be
that. But it opens by **waiting for a host to talk to it**, not by testing anything, and the
protocol on that port is unknown. Driving it means inventing the other end of a conversation, which
is the same class of problem as synthesising BCM replies.

`scan` is the more promising of the three — a disk scanner has no obvious reason to need a host —
and it is untested. `disk` is USB mass storage, which this emulator does not model at all
([research/11](11-rtxc-and-the-video-coprocessor.md) §50), so it is the least promising.

## The flash is a prototype's, and its bootloader knows it

The dump's own Internet Archive metadata settles the provenance question:

```
title:    SA JULY 12 2007 ipod video prototype firmware dump
uploader: austin@eliteobsolete.com
date:     2007-07-12
```

Elite Obsolete Electronics — the same source as the board table in
[research/15](15-the-chip-inventory.md). So the placeholder serial `U1234567890`, the blank `HwId`
and the unpublished `HwVr = 0x000b0011` are all explained: **this is a prototype's flash.**

The obvious experiment is to patch `HwVr` to a published retail value and see whether RetailOS
behaves differently. It does not survive contact:

| `HwVr` in flash | bootloader reaches `Retail mode` | profile buckets |
|---|---|---|
| `0x000b0011` — as dumped | **yes**, loads `osos` | 6 033 |
| `0x000b0010` — published 5G | **no** | 449 |
| `0x000b0005` — published 5G | **no** | 448 |

**The prototype bootloader requires the prototype `HwVr`.** They are a matched pair: hand this ROM a
retail revision and it decides it is running on hardware it does not recognise, and never loads
`osos` at all. This is the third "it stopped failing because it never got there" of the
investigation, and it was caught the same way — by checking whether the bootloader still completed.

So the prototype hypothesis **cannot be tested one field at a time**. It needs a whole retail NOR,
bootloader and `SCfg` together. That remains the single highest-value artefact this project does not
have.

## A retail NOR — and the prototype was never the problem

The retail dump was **already in this repo, mislabelled**. `resources/reference/ipod-bootrom-archive/`
files it under `A1238`, which is the iPod *classic* 6G's model number; the bytes say otherwise.

| field | prototype (ours) | retail dump |
|---|---|---|
| `SrNm` | `U1234567890` (placeholder) | **`<SERIAL-ROM>`** |
| `HwId` | `ff ff ff ff` (blank) | **`3a 76 01 82`** |
| `HwVr` | `0x000b0011` (unpublished) | **`0x000b0005`** (published retail 5G) |
| `Mod#` | `M8976` | `MA146` |

It is PP502x, not S5L: `PP5020`/`PP5022`/`PP5026` strings sit at byte-identical offsets to our
prototype, and every `flsh` load address is `0x10000000` (the archive's 3G dump uses `0x28000000`,
the PP5002 base — internally consistent). 511 827 bytes differ from the prototype, so it is a
genuinely separate device.

### It rejected our disk, correctly

Booting it against the existing image produced Apple's own multilingual restore screen —
*"Connect to your computer. Use iTunes to restore."* in four languages — and then `b .`. The retail
ROM validates what the prototype ROM waved through.

**Our firmware partition was inconsistent.** Its directory at partition+`0x4200` is a faithful copy
of `Firmware-20.6.3`'s, but the `osos` body had been *relocated* to partition+`0x1c200` by earlier
surgery and the directory never updated — so no image checksummed at the offset the directory
claimed. Writing the pristine 13 895 680-byte firmware over the partition (which it fits **exactly**)
makes `osos` and `rsrc` verify. `aupd` still does not, at any offset.

### Which is when it ran Apple's flash updater

With a consistent partition the retail ROM does not boot `osos` at all. It runs **`aupd`**:

```
Running 'aupd' 0 from 0x10000000
0> iPod CFI Flash Firmware update    offset 0x1C     size 0x2000
1> iPod CFI Flash Firmware update    offset 0x2038   size 0xF8000
84> updatePctOfTotal  : 42
END MARKER - VALID
Device Flash Version: FFFFFFFF     Update Flash Version: 0
```

299 frame updates of a white progress screen.

**This explains [research/12](12-bypass-ledger.md) #12.** That bypass removes the `aupd` directory
entry, and its stated reason was only *"the ROM halts after reading it"*. The real reason is now
visible: **with `aupd` present and the partition consistent, the bootloader runs the updater instead
of the OS.** The bypass is not a workaround for a halt, it is a workaround for a firmware update.

*(Two claims that stood here have been corrected below. `Device Flash Version: FFFFFFFF` is **not** a
CFI query returning nothing — it is a `vers` record lookup at flash `0x4040`, and it does not gate
anything. And `aupd` is **not** plainly executable ARM: `aupd.bin` measures 7.9998 bits of entropy
per byte and contains not one printable string, including the ones the updater demonstrably prints.
It is ciphertext, and the bootloader decrypts it — which is also why its body sums to `0x08299587`
against a directory that claims `0x0b19db1c`. The earlier reading of it as encrypted was right.)*

### And the answer to the question the whole exercise was for

With `aupd` removed, the retail ROM boots RetailOS — `Retail mode`, `Running 'osos' 0 from
0x10000000`, 59 DMA transfers.

| NOR | arrivals at `0x00000000` over 800 M at `--clock=5` |
|---|---|
| prototype | 103 |
| **retail** | **104** |

**The same loop, at the same rate, and the PMF fault at `0x000fb8a4` fires in both.** The prototype
flash — the placeholder serial, the blank `HwId`, the unpublished `HwVr` — is **not** the cause of
RetailOS's blocker.

That closes the largest remaining "maybe it isn't our fault" hypothesis, and it closes it with the
artefact the hypothesis demanded rather than by argument.

## The update runs, and it retires bypass #12 itself

`tools/ipod-boot/flash-update.sh` is the recipe. Retail ROM, `Firmware-20.6.3` written at the MBR's
own partition start so the `!ATA` directory is Apple's — `osos`, `rsrc` **and** `aupd`, nothing
removed — `--nor`, and `--disk-writable`. Two boots:

```
===== boot 1 =====                      ===== boot 2 =====
Running 'aupd' 0 from 0x10000000        Retail mode
0> iPod CFI Flash Firmware update       Running 'osos' 0 from 0x10000000
END MARKER - VALID
Device Flash Version: FFFFFFFF
```

24 ATA commands on the first boot, 96 on the second. Nothing is edited between them: **the updater
edits the disk.**

### The bypass was hand-applying the updater's own last write

The updater's final act is `WRITE SECTORS` (`0x30`) of one sector to **LBA 96** — the firmware
directory it was launched from — followed by `FLUSH CACHE` and `STANDBY IMMEDIATE`. It does not
delete the `aupd` entry; it sets the word at entry+`0x08` from 0 to 1, and the ROM then skips it.

That is exactly what the bypass was doing by hand. The disk carried a second, `aupd`-less copy of
the directory and the machine read that instead. **Read-only, the updater's own bookkeeping write
aborts and the machine updates on every boot forever** — so `--disk-writable` is as load-bearing as
the flash model.

### The ROM will not touch a flash it cannot name

`0x40009f88` — copied verbatim into `aupd` at `0x100051fc` — writes `0xAA`/`0x55`/`0x90` to `0xAAAA`
and `0x5554`, reads a manufacturer/device pair back, resets with `0xF0` then `0xFF`, and looks the
pair up in an eight-row table at flash+`0x1d0e0`. Against a plain memory region the "reply" is
whatever offset 0 holds: `0x1ffe` and `0xea00`, which is the reset branch `b 0x8000` read as two
IDs. No row matches, and the 40 command writes go nowhere.

The table is identical in both dumps. `0x40009eb8` reads its geometry as triples of
`(start, end, sector)` in **2 KiB units** from row+`0x18`, terminated by `0xffff`:

| row | mfr | dev | size | sectors | | 
|---|---|---|---|---|---|
| 0 | `0x00ec` | `0x22b2` | 1 MiB | uniform 64 KiB | Samsung |
| 1 | `0x0001` | `0x226b` | 1 MiB | 16/8/8/32K then 64 KiB | AMD — an AM29LV800B bottom-boot map, exactly |
| 2 | `0x0004` | `0x226b` | 1 MiB | same | Fujitsu |
| 3 | `0x00bf` | `0x273f` | 1 MiB | uniform 4 KiB | SST |
| 4 | `0x00bf` | `0x2781` | 1 MiB | uniform 4 KiB | SST — SST39VF800A |
| 5 | `0x00b0` | `0x0000` | 1 MiB | 8 KiB then 64 KiB | Sharp — **Intel** command set |
| 6 | `0x0020` | `0x0000` | 1 MiB | 8 KiB then 64 KiB | Intel/ST — Intel command set |
| 7 | `0x00bf` | `0x272f` | 512 KiB | uniform 4 KiB | SST |

Row 1's map decoding to a real AM29LV800B is the check that the format was read right, not guessed.

We answer as row 4. The dump is 1 MiB, which rules out row 7; of the six left it is the only
**uniform** geometry, so the sector the driver computes and the one we erase cannot disagree, and it
drives the AMD command set the probe already speaks. **Nothing in either dump records which part the
hardware carried** — this is a choice among the eight the ROM accepts, not a measurement.

### `Device Flash Version: FFFFFFFF` is not the blocker, and never was

It is not a CFI query. `0x100013e8` loads the literal `0x00004040`, reads the word there, and takes
the version only if it reads `'vers'`; our flash has a SysCfg `HwId` record at `0x4040` and its own
`'vers'` at `0x8040`, so the device version stays at its `mvn`-initialised `0xFFFFFFFF`. The update
version comes out 0 the same way. The caller keeps the result in `r8` and **proceeds either way** —
`r8` only picks between two arms at `0x100037ec`, and a mismatch selects the *write* arm.

A full 1 MiB reflash issues the CFI query command `0x98` exactly **zero** times. The model answers
one anyway, because the driver object calls itself `Cfi!` and a part that answers autoselect but not
a query is not a part.

### What actually decides: the flash already matched

`0x10001534` walks the record word by word against the flash and returns "identical", and the caller
skips the write. That is not a failure — it is correct. The `aupd` payload in `Firmware-20.6.3` is
**byte-identical to the retail NOR dump** over both regions it would write:

| record | source | destination | size | vs retail dump | vs prototype dump |
|---|---|---|---|---|---|
| 0 | payload+`0x1c` | flash `0x0` | `0x2000` | **0 bytes differ** | 0 bytes differ |
| 1 | payload+`0x2038` | flash `0x8000` | `0xf8000` | **0 bytes differ** | 511 793 differ |

So the retail device is already carrying exactly this firmware, and the authentic outcome is a
no-op followed by the bookkeeping write. `arg0` is the flash destination and `offset` the source
within the payload — the second record covers `0x8000`..`0x100000`, which is the bootloader image.

### Proving the erase and program paths, since the real update declines to use them

Perturb 64 bytes at `0xc0000` in a scratch copy of the retail ROM — inside the `diag` image, which
the boot never executes, and inside record 1's span — and the updater stops declining:

```
nor: 248 sector erases, 507904 words programmed
  cycle 0x30 sector erase   x248        cycle 0xa0 program setup  x507904
  cycle 0x80 erase setup    x248        cycle 0xaa unlock         x508406
  cycle 0x90 autoselect     x6          cycle 0xf0 reset          x255
```

Every number checks against another: 248 = `0xf8000`/4 KiB, 507 904 = `0xf8000`/2 words, and
508 406 unlocks = 507 904 programs + 2×248 erases + 6 identifies. **The flash afterwards is
byte-identical to the pristine retail dump** — the model repaired all 64 perturbed bytes and touched
nothing else.

That run also found a bug in the model, by a route worth recording: the first version tallied every
bus cycle as a command, and the report showed `reset (Intel) x281612` beside a program count less
than half the size of the transfer. A program's *data* cycle is data — a real part latches whatever
is on the bus — and decoding it was swallowing every payload word whose low byte was `0xff`. A
report that only counted erases and programs would have shown 225 130 words written and looked fine.

### The prototype ROM still refuses, and #12 stays open there

`cold-boot.sh` runs the prototype dump, and that ROM reads its firmware partition at **4× the MBR
LBA** — LBA 252 and 284 where the retail ROM reads 63 and 96, in 2 KiB blocks against 512-byte ones.
That is why `ipod8g.img` carries two firmware copies: one at `0x7e00` for the retail ROM and a
hand-built one at `0x1f800` for the prototype, and it is the second one that had `aupd` removed.

Put `aupd` back in that copy, with its body at the block the ROM actually reads (`partition`+
`0xc3a800`, LBA 25296 — measured, not derived), and the prototype ROM reads all 2 104 sectors of the
image and then runs an orderly power-off: `0x40006138` → `0x40003984` → `0x4000159c`, `b .`, without
printing a line. It never enters the image and never prints `Running 'aupd'`. Not diagnosed further.
The plausible reading is that the prototype bootloader cannot decrypt this payload — it is the same
matched-pair behaviour as the `HwVr` result above — but that is a hypothesis, not a measurement.

## The prototype ROM was the blocker, and the retail one was in the repo all along

*2026-08-13. This section supersedes the working assumption that the prototype NOR was a curiosity
rather than a constraint.*

`cold-boot.sh` has defaulted to the prototype dump since the day it was written, for no reason
beyond it being the first one downloaded. Measured on the same day, same budget, same emulator, the
two configurations are not close:

| 600 M instructions, `--clock=5` | prototype NOR + `ipod8g.img` | retail NOR + `ipod8g-retail.img` |
|---|---|---|
| arrivals at address `0` | **314** — 157 self-resets | **2** — the cold reset and the bootloader handoff |
| unmapped accesses | 640 reads at `0xea000078` | **none at all** |
| ATA commands | 77 | **96** |
| ATA DMA | 60 transfers, 7 596 032 B | **72 transfers, 8 113 664 B** |
| irqs asserted | 183 412 | 512 901 |
| reads `rsrc` | never | **yes** |

### The retail path mounts `rsrc` and loads the VideoCore firmware

Its post-handover command sequence, with the LBAs resolved against the volume inventory above:

```
[76] READ DMA 64 sectors  lba 0        MBR
[77]                      lba 32768
[78]                      lba 0
[79]                      lba 64
[80]                      lba 14864    <- the rsrc FAT boot sector, exactly
[81]                      lba 22429    <- RenderServer.bin  (starts 22425)
[82]                      lba 14870    <- the FAT itself
[83]                      lba 22645    <- vmcs.bin          (starts 22633, 99 clusters)
[84] 126 sectors          lba 22646
[85..88]                  lba 22772..22902   more of vmcs.bin
[89]                      lba 23028    <- aacdec.vll        (starts 23033)
[90] STANDBY IMMEDIATE                 <- spins the drive down
```

Mount the volume, walk the FAT, load the co-processor's firmware and its first codec, then park the
disk. That is what a shipping iPod does at boot, and **it is bypass #6's retirement path executing
by itself** — `vmcs.bin` and `RenderServer.bin` are being read off the disk by Apple's own code,
with no help from us.

Two things this settles at once:

- **The 157 self-resets are prototype-only.** They are `BX` to address zero through a null `this`
  (research/20 Addendum 5), and the null comes from a font registry that has nothing in it because
  `rsrc` was never mounted. On the retail path the volume *is* mounted, and the reset does not
  happen. The chain in research/20 §§1–8 is correct and was diagnosing a downstream symptom of the
  wrong bootloader.
- **`0xea000078` was never a real address.** It is RetailOS's own reset-vector word, `b 0x1f0`,
  being read as a vtable base. It appears in exactly the runs that take the null dispatch, which is
  to say the prototype's.

### Why the recipes stay separate

`retail-boot.sh` is a new recipe rather than a change to `cold-boot.sh`'s defaults, so that every
number already recorded in `research/` stays attributable to the configuration it was measured on.
**New work should use `retail-boot.sh`.** The prototype recipe is now the special case — kept
because bypass #12 is still open on that ROM, and because a prototype/retail diff is itself
evidence.

One caution, recorded because it nearly shipped: the first `retail-boot.sh` set `FLASH` and `DISK`
with `: "${VAR:=...}"` and `exec`'d the cold recipe. Plain shell assignment is not exported, so
`exec` passed neither, and the wrapper **silently ran the prototype configuration it exists to
avoid** — printing a plausible, entirely wrong 77-command run. Caught only by diffing its output
against the manual invocation. A recipe is an instrument, and instruments get verified.
