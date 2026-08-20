# Bringing your own iPod's files

**Optional.** The emulator synthesises a boot ROM and fetches Apple's firmware itself, and that is
the path the [README](../README.md) describes. This is for when you would rather run *your* iPod.

Two things a synthesised ROM cannot give you, because they are Apple's code and live inside the ROM:

- **Apple's own bootloader**, running from the reset vector
- **the service diagnostics** — `SELECT`+`REW` at power-on

Everything entered directly — RetailOS through the high-level boot, Rockbox, `ipodloader2`,
iPodLinux — runs on a synthesised ROM. What is measured, cell by cell, is in
[research/17](../research/17-the-boot-matrix.md).

Apple wrote both files below. This project ships neither, and never will.

## The boot ROM

A 1 MB NOR dump. Any filename works; the size and the reset vector are what get checked.

**Read it off your own iPod.** This is the only route that involves nobody else's copy of anything,
and the only one guaranteed to match the iPod you have. It takes about five minutes and undoes
cleanly:

1. Install [Rockbox](https://www.rockbox.org/wiki/RockboxUtility) with Rockbox Utility — only
   *bootloader* and *rockbox* need ticking.
2. On the iPod: **System → Debug (Keep Out!) → Dump ROM contents**.
3. Plug it in and copy the `internal_rom_…` file off.
4. Uninstall Rockbox if you want to.

The [flash guide](https://www.rockbox.org/wiki/IpodFlash.html) has the detail.

### If the dump comes out 0 bytes

Reported more than once. The file is written and closed at the *end*, so an iPod reset before it
finishes leaves a correctly named empty file.

The read itself is seconds, not minutes — a wheel still frozen after a minute has **failed**, not
gone slowly. Let it finish, then shut down through Rockbox so the volume is flushed before you
unplug.

### If you are looking for an archived one

**It is filed under the wrong product.** BootROM collections put the iPod Video's dump under
*iPod Classic*, in a directory named `A1238` — which is the Classic 6G's model number. The Video is
`A1136`. Searching for "iPod Video", "5.5G" or "A1136" finds nothing; searching for the Classic
finds it. This cost someone hours, and the same file was mislabelled in this project's own tree.

A **prototype** dump also circulates — `HwVr 0x000b0011`, `Mod# M8976`, blank `HwId`. It will **not**
boot a pristine firmware partition.

## Something to make a drive from

Either Apple's `.ipsw` or a drive image you already have. An `.ipsw` is built into a drive as it
lands in the window, or with:

```sh
ipod-boot make-disk iPod_20.1.3.ipsw disk.img
```

**You almost certainly do not need to find one.** Apple still serves 66 of the 71 releases in the
catalogue, from `secure-appldnld.apple.com` — their own servers, not a mirror — and every one has
been downloaded and hashed here, so it can be verified byte for byte:

```sh
ipod-boot firmware list [filter]          # everything, or matching a model or filename
ipod-boot firmware get 20                 # by UpdaterFamilyID — 20 is the 5G Rev A
ipod-boot firmware get iPod_20.1.3.ipsw   # or by name
```

Downloads are verified rather than trusted — size and SHA-256, both — and nothing is renamed into
place until it verifies, so an interrupted download can never be mistaken for a finished one. The 5
Apple no longer serves say so, rather than failing with a transport error.

Full catalogue and the `FamilyID`/`UpdaterFamilyID` trap: [firmware-catalogue.md](firmware-catalogue.md).

## The two halves are independent

The boot ROM and the drive are **separate questions**. Supply either, both or neither:

| the boot ROM | the drive |
|---|---|
| synthesised — no file needed | built from a fetched `.ipsw` — no file needed |
| your own 1 MB NOR dump | your own `.ipsw` |
| | a drive image you already have, in which case **no `.ipsw` is needed at all** |

A synthesised ROM with your own `.ipsw` works. Your dump with a fetched `.ipsw` works. Your dump
with a drive image and no `.ipsw` anywhere works.

## What must match is the firmware and the iPod

Not "the two files you supplied" — the *firmware family* and the *model*.

Apple ships each model's software under an **updater family**, and an iPod recognises only its own —
the Video takes family 20. A mismatch boots, fails to recognise the drive, and asks to be restored
from iTunes after about **70 ATA commands**, where a match reaches the language picker with **618**.

This is checked whichever way the two arrived, including when one of them was synthesised or
fetched.

The emulator checks before booting rather than after. With no window at all:

```sh
ipod-emulator --check-images --flash=… --disk=…
```

which reports which of the size, the reset vector and the image directory is wrong.

## What the synthesised ROM is, exactly

A reset vector, a mark, and the identity block a real iPod carries — `SrNm`, `HwId`, `HwVr`, `Regn`,
`Mod#`, `DrmV` — with the serial and GUID generated from a seed, so the same iPod comes back on the
next launch. **None of Apple's code**, which is the point: it is 101 live bytes against the real
dump's ~390 KB.

The model numbers come from a table of **198 rows**, transcribed mechanically from libgpod's
`ipod_model_table` rather than retyped. That table is what makes `Mod#` and the capacity right for
whichever iPod you name.

**The window's device picker offers one device, not 198.** That is deliberate — ROADMAP Ⅳ: *a device
drawn in the picker is a promise, and each one appears when it boots, not before.* The table carries
every clickwheel iPod so that adding one is a row rather than a refactor.

### Your own boot picture

A synthesised iPod shows the click-wheel outline while it starts. You can replace it: the setting is
a **path**, not the pixels, so editing the picture is enough and there is nothing to regenerate.

```sh
ipod-boot make-nor --model A146 --seed 5 --preview boot.png out.bin
```

writes out what a given machine will show, which is where
`docs/media/ipod-30-synthetic-nor-boot.png` came from.

### What a synthesised ROM cannot do

`diag`, `disk` and the `aupd` updater are **images inside the NOR** — Apple's code, which is exactly
what is not synthesised. There is no `flsh` directory in a synthesised ROM and the tools say so:

```
ipod-boot flsh: synth-5g.bin has no `flsh` image directory at all
```

Apple's **service diagnostics needs a real dump** and always will. Cold RetailOS does too, because
reaching it means running Apple's bootloader. Everything entered directly — RetailOS high-level,
Rockbox, `ipodloader2`, iPodLinux — runs on either, with identical numbers. Cell by cell:
[research/17](../research/17-the-boot-matrix.md).

## What has actually been tested

Everything in `research/` was measured on exactly one pair of files. That is part of what "alpha"
means here:

| | |
|---|---|
| **NOR** | the retail iPod Video dump — 1 048 576 bytes, `HwVr 0x000b0005`, `Mod# MA146`, non-blank `HwId` |
| **IPSW** | `iPod_20.1.3.ipsw` — `Firmware-20.6.3` inside it is 13 895 680 bytes, exactly 27 140 sectors, exactly the size of the firmware partition |

**The reference hardware is a 30 GB 5G.** Apple gives both revisions the same `FamilyID` of 6 and
separates them by `UpdaterFamilyID`, so *iPod Video* is the honest name for what runs here and
**5.5G is a claim this project has not yet earned** — see [ROADMAP](../ROADMAP.md) §"5G, 5.5G, and
which is the default".
