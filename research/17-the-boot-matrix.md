# The boot matrix — real, synthetic 5G, synthetic 5.5G

**Measured 2026-08-19.** Every row is a run, not a claim. The point of the table is that a
synthesised boot ROM now reaches the same place a real one does, and that where it does not is
named rather than left to be discovered.

## RetailOS

| boot ROM | firmware | outcome |
|---|---|---|
| real 5G dump, cold | `iPod_20.1.3` (updater 20) | boots — copyright banner, 597 M instructions |
| real 5G dump, cold | `iPod_25.1.3` (updater 25) | **70 ATA commands** — the wrong-family signature |
| **synthetic 5G** (`MA146`) | `iPod_20.1.3` | **boots** — 597 M instructions, 7 ATA commands |
| **synthetic 5.5G** (`MA446`) | `iPod_25.1.3` | **boots** — 597 M instructions, 7 ATA commands |

The second row is not a failure: `inspect::family_mismatch` documents exactly that shape — *"a
bundle from the wrong updater family boots, is not recognised as this iPod's own software, and
shows the plug-into-a-computer screen after about 70 ATA commands"*. A 5G ROM with 5.5G firmware
**should** land there, and it does.

## Rockbox

Warm-booted from `rb-main.raw` against a drive carrying `.rockbox`, with `--sysinfo` so the handoff
comes from the ROM under test.

| boot ROM | instructions | ATA |
|---|---|---|
| real 5G dump | 16 980 585 | 72 |
| synthetic 5G | 16 483 392 | 90 |
| synthetic 5.5G | 16 483 392 | 90 |

**The two synthetic runs are identical**, which is informative rather than suspicious: Rockbox is
built for a fixed target and does not read the generation out of the handoff, so a `MA146` and a
`MA446` present it the same machine.

**The real-versus-synthetic difference is not explained.** 72 ATA commands against 90, and about
500 000 fewer instructions. Both reach the same budget without faulting. It is recorded here
because an unexplained difference that nobody wrote down is one that gets rediscovered.

## `ipodloader2` — the row this file did not have

**Measured 2026-08-19**, `ipod-boot loader` against the same drive on all three ROMs:

| boot ROM | ATA commands | unmapped |
|---|---|---|
| real 5G dump (`MA146`) | **3 196** | none |
| synthetic 5G (`MA146`) | **3 196** | none |
| synthetic 5.5G (`MA446`) | **3 196** | none |

**Identical, to the command.** The loader reads the drive, walks the FAT32 volume, loads
`/boot/vmlinux` and jumps, and it does not care which of the three it booted through — which is the
useful result, because it says the generation fields the synthesiser writes are not something this
bootloader consults. The Linux kernel then boots in full and panics; that is
[research/16](16-the-third-bootloader.md) and is not a property of the ROM.

**One real dump exists.** `ipod-resources/roms/` holds exactly one file — `retail_5g_MA146`, a real
5G — and it is what every recipe here uses by default. The 5G and 5.5G rows above are built by
`ipod-boot make-nor --model MA146|MA446`, so "we are testing a 5.5G" is true only of the synthetic
row: **the default machine is a real 5G.**

## Re-measured 2026-08-20, after six ATA fixes — and the old table below is superseded

R4 applies in full: the machine now gets further with two of these firmwares than it ever has, so
every cell of the 2026-08-19 table was measured on a machine that no longer exists. This is what a
re-run says.

| | real 5G dump | synthetic 5G | synthetic 5.5G |
|---|---|---|---|
| **RetailOS**, cold | boots — 611 READ DMA, 4 WRITE DMA | — | — |
| **Rockbox**, warm | **main menu, 3 858 lit pixels** | runs, **0 pixels** | runs, **0 pixels** |
| **ipodloader2** | draws its own console | not yet run | not yet run |
| **iPodLinux** | **boots to ZeroSlackr's userland** | `Lost(0x40020000)` | not yet run |

**Rockbox on a synthetic NOR is a display failure, not a boot failure — and that is measured, not
inferred.** Both arms issue the identical `0xc6 ·  0xc8 x2065 ·  0xec x2 ·  0xef x2`, so Rockbox
loads its whole binary off the disk either way, and both end at **the same instruction** —
`0x00086300`, inside `switch_thread`. It is running its scheduler on both. What differs is that
nothing reaches the co-processor surface at `0xE0000`. Ledger #6 is the neighbourhood.

**iPodLinux on a synthetic NOR ends at `Lost(0x40020000)`** — a jump one byte past the top of IRAM —
after 12 792 unmapped reads around `0x04716000`, every one of them from a single PC. That is past
the end of the 64 MB SDRAM region, from code running through the low mirror.

**And one cell is not the emulator at all.** `ipodloader2` reads FAT32 partition type `0x0B` and no
other — `vfs.c` has `case 0x83` for ext2 and `case 0xB` for FAT32, and nothing else. Every drive
image here taken off real hardware is `0x0C`, the LBA form, and the loader's own console says so:

```
Detected WinPod MBR
[0]: Bad iPod FW entry
[1]: Unknown 0xC2          <- mlc_printf("0x%X2"), so the trailing 2 is a literal
No valid paritions found!
```

A real 5G with a `0x0C` volume would fail identically on real hardware. `install-linux` refuses
those drives now rather than writing 1 776 files onto a disk that cannot boot.

## Superseded: the whole matrix, measured 2026-08-19

Every cell is a run. `—` is not "untested", it is "cannot, and the row below says why".

| | real 5G dump | synthetic 5G | synthetic 5.5G |
|---|---|---|---|
| **RetailOS**, cold | boots — 599 ATA, full framebuffer | — | — |
| **RetailOS**, high-level | boots — 597 M instructions | **boots** — 7 ATA | **boots** — 7 ATA |
| **Rockbox**, warm | menu — 72 ATA | 90 ATA | 90 ATA |
| **Rockbox**, cold | **menu — 10 304 ATA, 74 057 lit pixels** | **0 ATA, 0 pixels** | **0 ATA, 0 pixels** |
| **ipodloader2** → Linux | 3 196 ATA, no unmapped | **3 196** | **3 196** |
| `diag` | **draws — 70 669 lit pixels** | — | — |
| `disk` | faults after 128 K instructions (USB unmodelled) | — | — |
| `logo`, `vmcs` | not bootable images — payloads, refused by `is_bootable` | — | — |

**Two rows are worth reading twice.**

`ipodloader2` is **identical on all three** — 3 196 ATA commands and no unmapped accesses — so it
consults nothing the synthesiser writes, and the Linux failure downstream of it is not a
generation mismatch. That was a live hypothesis and this kills it.

**Cold Rockbox is 10 304 against 0.** Anything that has to run Apple's bootloader needs a real dump,
because a synthetic ROM has none — see below. The synthetic rows exist only through the high-level
boot, which enters an operating system directly.

## Where the four NOR modes actually ship — and why a synthetic ROM still cannot run them

**Asked and answered 2026-08-19, because the obvious guess is that `diag` and friends come down in
the IPSW and could therefore be given to any ROM.** They do come down in the IPSW. It does not help.

**The IPSW's firmware directory holds three images and none of them is a mode.** Parsed out of
`iPod_20.1.3`'s `Firmware-20.6.3` at `0x4200`:

| image | devOffset | length |
|---|---|---|
| `osos` | `0x00004400` | 7 559 680 |
| `rsrc` | `0x0073a000` | 5 242 880 |
| `aupd` | `0x00c3a200` | 1 074 176 |

The `diag`, `disk`, `logo` and `scan` strings that appear elsewhere in the file are constants inside
code — including an `hslf` at `0x71c754` whose neighbours disassemble to `push {r2-r6,lr}` and
`ldr r0,[r1,#0x10]`, i.e. the routine that *searches* for a flash directory rather than one.

**But `aupd` is 1 074 176 bytes, which is a 1 MB flash image plus a header, and it is encrypted.**
Entropy is **8.00 bits/byte** over the whole image and 7.99 in every 16 KB window sampled, with the
byte histogram flat (4 266 `0xff`, 4 309 `0x00`, against 4 196 expected for uniform). So the flash
content almost certainly *does* ship in the IPSW — inside the updater, which is Apple's own code and
would decrypt it itself. That is a better answer than "the modes are only in the NOR", and it is the
answer to the question people will actually ask.

**It still cannot be used, and the reason is not the payloads.** A synthesised ROM is an identity
card, not a ROM:

| | non-zero bytes |
|---|---|
| real 5G dump | **908 246** — 86.6 % of a megabyte: a bootloader and four self-contained payloads |
| synthetic 5G | **101** — 0.0 % |

Word 0 is a branch in both (`0xea001ffe`, to `0x8000`) because `inspect::flash` checks for one, but
on the synthetic image there is nothing at `0x8000` to branch to. Booted cold it fetches straight
off the end of the chip — 188 reads at `0x00100000`, one megabyte in, from 43 consecutive PCs. **It
has no bootloader, so it cannot run `aupd`, so it cannot be given the modes by the updater either.**

Which is why the synthetic rows in this file are all high-level boots: they enter an operating
system directly and skip the bootloader that does not exist. **Diagnostics, disk mode, the scanner
and the boot logo require a real dump** — not because the bytes are unobtainable, but because the
program that would install them needs a ROM to run in.

### Could Apple's own updater install them? No, and the refusal is circular

The obvious follow-up: `aupd` is the program that writes the NOR, so **enter it directly** and let
Apple's code decrypt its own payload onto a synthetic chip. `--osos-from-disk=TAG` exists now for
exactly this — the firmware directory's images are all firmware images and nothing about the entry
path was `osos`-specific.

It refuses, and the refusal is the confirmation:

```
upd-armed.img: no ARM vector table within 0x4000 of `aupd` at 0xc3a200.
An image entered at its base opens with two branch instructions and this one does not,
so either it is not a 5G/5.5G OS image or it is not stored in the clear.
```

**It is not stored in the clear** — which is what 8.00 bits/byte already said, now confirmed by a
reader that was not looking for encryption. So:

1. the four modes ship in the IPSW, inside `aupd`;
2. `aupd` is encrypted;
3. the thing that decrypts it is Apple's bootloader;
4. Apple's bootloader is in the NOR we are trying to synthesise.

**The requirement is circular by construction**, and no amount of work on this side breaks it. A
synthetic ROM can boot an operating system, and that is the whole of what it can do. `diag`, `disk`,
`scan` and `logo` need a dump from an iPod somebody owns.

## What a synthesised ROM cannot do

**The four NOR modes are not in it, and cannot be.**

```
synthetic       flsh entries: 0
real 5G dump    flsh entries: 4
```

`diag`, `disk`, `scan` and `logo` are self-contained payloads Apple ships inside the flash, indexed
by the `flsh` directory at `0xffe00`. Synthesising the identity block is one thing; synthesising
Apple's diagnostics is not the same kind of task, and this project does not have those images except
inside a dump somebody already owns.

So: **diagnostics mode, disk mode, the disk scanner and the boot logo image require a real dump.**
A synthesised ROM boots an operating system and nothing else. That is worth saying out loud next to
any claim that "all the NOR modes draw", because it is true of a real dump and false of a generated
one.

## What this does *not* establish

**The 5.5G's `HwVr` is still unsourced.** `0x000B0010` came from a code comment. The 5.5G boots with
it, and boots byte-identically with the 5G's `0x000B0005` forced in its place — same instruction
count, same ATA count, same unmapped count. So the boot says nothing about which value is right, and
anyone reading this table should not take "the 5.5G boots" as "the constant is confirmed".

Every other constant in the handoff is measured: `len = 0xf8`, `BoardHwName = "iPod M25"`, the 5G's
Gestalt, the model number, and the `SysCfg` copied in after the struct.

## All three revisions

There are **three** firmware revisions of this iPod, not two, and the middle one had been skipped:

| updater family | revision | RetailOS, high-level boot |
|---|---|---|
| **13** | 5G Initial (Oct 2005) | **boots** — 597 M instructions, 7 ATA |
| **20** | 5G Rev A | **boots** — 597 M instructions, 7 ATA |
| **25** | 5.5G, Late 2006 | **boots** — 597 M instructions, 7 ATA |

13 and 20 are both 5G and share Gestalt `0x000B0005`, so both take the synthetic `MA146`. Our own
reference drive reports `updaterFamily: 13`, so it is an Initial.

## `HwVr` for the 5.5G — upgraded from "a comment" to "published, uncited"

Searched for deliberately. What exists:

- **theapplewiki's *iPod with video* page** assigns `0x000B0010` to Rev B / Late 2006, update family
  25 — which matches `iPod_25.1.3.ipsw`'s own manifest. Published, and **uncited**.
- **Apple's own binary contains the constant.** `ipod-usb`'s reverse engineering of
  `CIpodDevice::GetDeviceType()` records a switch with `0x000B0005 → type 17` and
  `0x000B0010 → type 23`, both inside the iPod-with-Video family. So the value is Apple-recognised
  rather than invented — though that does not by itself say which revision it belongs to.
- **The iPodLinux wiki lists both values as "5G"** without splitting them by revision, and is the
  likeliest origin of the comment this project carried.
- **No measurement exists anywhere.** No retail 5.5G NOR `HwVr`, no 5.5G `SysInfo`. Rockbox and
  `ipodloader2` both compare only the high halfword (`0xB`), so neither can distinguish, and no
  aligned occurrence of any of these words appears in any Apple firmware image — consistent with
  RetailOS switching on the high half only.

**Keeping `0x000B0010` is defensible. Calling it measured is not.**

> *Strengthened 2026-08-19.* A third occurrence, and the first in code that runs **on the iPod**:
> Apple's `diag` image dispatches on the hardware version with exactly three cases —
> `0x000B0005`, `0x000B0010`, `0x000B0011` — each with its own handler, at prototype `diag`
> `0x10003c28`. See [research/07](07-the-flash-images.md). The value is Apple's, not a wiki's.
> Which revision it names is still unmeasured, because no retail 5.5G NOR has been read.

## Two blockers found while testing the bootloaders

**`install-os` refuses every drive `make-disk` builds**, and for a different image on each revision:

```
5G   (20.1.3)   `aupd`: directory says 0x0b19db1c, bytes sum to 0x08299587
5.5G (25.1.3)   `osos`: directory says 0x2c7c48f3, bytes sum to 0x2c7f4045
```

That check is deliberate — it reproduces the existing checksums before writing new ones, so a wrong
idea of the layout fails on an unmodified file rather than producing an image the bootloader rejects
seventy ATA commands in. It is working; what it is telling us is that we have two layout problems.

Both are now settled, and by a **second independent method**: which offset reproduces each image's
recorded checksum. That agrees with the vector-table method and is stronger, because a checksum over
seven megabytes cannot match by coincidence.

| bundle | `osos` / `rsrc` | `aupd` |
|---|---|---|
| `iPod_13.1.3` (5G Initial) | reproduce at **`+0x200`** | matches at **no** offset |
| `iPod_20.1.3` (5G Rev A) | reproduce at **`+0x200`** | matches at **no** offset |
| `iPod_25.1.3` (5.5G) | reproduce at **`+0x800`** | matches at **no** offset |

1. **The header is per-bundle**, and `FW_SECTOR`'s fixed 512 was right for the 5G and wrong for the
   5.5G. It is now *discovered* — `osos`'s own checksum is the oracle — so the tool no longer has to
   be told, and a bundle with a header nobody has seen resolves itself.
2. **`aupd`'s checksum reproduces at no offset in any bundle.** My earlier note here said its extent
   ran past the end of the firmware file; **that was wrong arithmetic** — the file is `0xD40800`,
   not `0xD40000`, and `aupd` fits inside it. The real finding is that whatever Apple sums for the
   updater image, it is not the bytes at `devOffset`. It is systematic across all three bundles, so
   it is a property of the format rather than damage, and `install-os` now exempts it by name and
   says why. Failing on it meant refusing every drive `make-disk` builds.

**And "no room" is not a defect — it is a faithful drive with an armed updater.** `install-os`
refuses to install a bootloader on a drive `make-disk` builds, because Apple's three images fill the
partition. The obvious fix was to widen partition 0 to `DATA_LBA - FIRMWARE_LBA`, and that was
**wrong and has been reverted**: measured on the reference drive, a real iPod's firmware partition is
**27 140 sectors — Apple's firmware to the byte, with no slack at all**. Widening it made our drives
differ from real hardware to work around something that is not a fault. The layout test said so, and
I had been about to update the test to match the code.

What a real post-update iPod has instead is **no `aupd`**: the reference drive carries only `osos`
and `rsrc`, and the megabyte the updater occupies is exactly the room a bootloader goes into. So the
drive to install onto is one whose updater has been consumed — which is what the reference drive is,
and `install-os` works on it first time.

**The Rockbox bootloader test was run the wrong way.** `bootloader-ipodvideo.ipod` was warm-booted
through `--osos=`, and burned its whole 200 M budget with **zero** ATA commands on all three ROMs.
That is not evidence about the bootloader: it expects to be *installed in the firmware partition and
entered by Apple's bootloader*, which is a cold boot. The run says nothing until it is redone that
way — and redoing it needs `install-os`, which is blocked above.
