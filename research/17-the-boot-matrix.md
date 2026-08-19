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
