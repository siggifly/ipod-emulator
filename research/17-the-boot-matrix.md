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

## Two blockers found while testing the bootloaders

**`install-os` refuses every drive `make-disk` builds**, and for a different image on each revision:

```
5G   (20.1.3)   `aupd`: directory says 0x0b19db1c, bytes sum to 0x08299587
5.5G (25.1.3)   `osos`: directory says 0x2c7c48f3, bytes sum to 0x2c7f4045
```

That check is deliberate — it reproduces the existing checksums before writing new ones, so a wrong
idea of the layout fails on an unmodified file rather than producing an image the bootloader rejects
seventy ATA commands in. It is working; what it is telling us is that we have two layout problems.

1. **The 5.5G case is the header again.** `FW_SECTOR` is a fixed `512`, which is right for the 5G's
   bundle and wrong for the 5.5G's `0x800`. That is the **third** place this constant is assumed —
   `osos_from_drive` was the second, and `research/02` §Provenance records the first two.
2. **The 5G's `aupd` case is not explained.** Its extent — `dev_offset 0xc3a200` plus `len 0x106400`
   — ends at `0xD40600`, past the `0xD40000` end of Apple's own firmware file, with or without a
   header offset. So the sum reads beyond what was written. Whether the directory is describing
   something the extracted file does not contain, or `dev_offset` means something else for `aupd`,
   is **open**.

**The Rockbox bootloader test was run the wrong way.** `bootloader-ipodvideo.ipod` was warm-booted
through `--osos=`, and burned its whole 200 M budget with **zero** ATA commands on all three ROMs.
That is not evidence about the bootloader: it expects to be *installed in the firmware partition and
entered by Apple's bootloader*, which is a cold boot. The run says nothing until it is redone that
way — and redoing it needs `install-os`, which is blocked above.
