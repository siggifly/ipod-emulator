# Getting the two files

Apple wrote both. This project ships neither, and never will.

You need a **boot ROM** and **something to make a drive from**. Or neither — see
[Skipping the boot ROM](#skipping-the-boot-rom).

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

Either Apple's `.ipsw` (~14 MB) or a drive image you already have. An `.ipsw` is built into a drive
as it lands in the window, or with:

```sh
ipod-boot make-disk iPod_20.1.3.ipsw disk.img
```

**Apple no longer serves these**, so there is no official source to try.

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

## Skipping the boot ROM

Pick a model from a list of 198 — transcribed mechanically from libgpod's table — and the emulator
**synthesises a boot ROM** for it: the identity block a real iPod carries, with a serial and GUID
generated from a seed so the same machine comes back next launch. It then fetches Apple's firmware
itself, verified against a recorded SHA-256.

A synthesised ROM carries an identity and a reset vector, and none of Apple's code. What that costs
is recorded in [research/17](../research/17-the-boot-matrix.md): everything that has to *run* Apple's
bootloader needs a real dump, and everything entered directly does not.

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
