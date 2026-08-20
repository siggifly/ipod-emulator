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

## They must be a matching pair

Apple ships each model's software under an **updater family**, and an iPod recognises only its own —
the Video takes family 20. A mismatched pair boots, fails to recognise the drive, and asks to be
restored from iTunes after about **70 ATA commands**, where a matching pair reaches the language
picker with **618**.

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
