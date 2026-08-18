# Running an OS that is not RetailOS

**Proposed 2026-08-18; steps 1 and 2 shipped the same day, step 3 is the open one.** Written down because the design falls out of parts that
already exist, and because the obvious implementation is the one that would quietly undo the thing
this emulator is for.

RetailOS is not the only thing that ran on this hardware. Rockbox and iPodLinux both drove the
PP5022, both are still maintained enough to build, and people still write new firmware for these
machines. An emulator for a dead device that runs exactly one dead OS is narrower than it needs to
be, and the difference is mostly plumbing.

## What already works

`trace --osos=FILE --boot-osos` runs an arbitrary ARM image today. Rockbox 4.0 boots on it and
scans the disk ([research/06](../../research/06-rockbox-as-oracle.md)). So the emulator can already
do this; the window cannot.

## The obvious implementation, and why not

The direct route is to give the window the same flag: warm-enter an image at `0x10000000` and start
executing. It is small, and it would work.

It is also a **bypass** in the sense this project uses the word — the bootloader never runs, so
whatever the bootloader does for the OS does not happen, and any divergence afterwards is
un-attributable between "the OS does this" and "we skipped the thing that set it up." The bypass
ledger exists because six of those accumulated once already.

## The route that is not a bypass

**Install the OS into the drive image, and cold boot it.** That is what an iPod does, what Rockbox
Utility does on real hardware, and what `ipodpatcher` does to the firmware partition: write the
image into the `osos` entry of the firmware directory, fix the checksum, and let the machine's own
bootloader find it at the address it already looks at.

Nothing new boots. The existing cold path loads it, and a divergence is the OS's.

This also lands exactly on the window's existing shape. Files are already routed **by content**
rather than by which box they were dropped in — a zip is Apple's bundle, exactly 1 MiB is a boot
ROM, anything else large enough is a drive. An OS image is one more case:

| what lands | how it is known | what happens |
|---|---|---|
| `rockbox.ipod`, `bootloader-ipodvideo.ipod` | 8-byte big-endian checksum + ASCII `ipvd` (`tools/scramble.c`, `modelnum = 5`) — and the checksum is **checkable**, so this is identification and not a guess | written into the drive's firmware partition as `osos` |
| a bare ARM image | falls back to asking | same, once confirmed |

The checksum is the good part: unlike "large enough to be a drive", this format states what it is
and lets us verify the claim before acting on it.

## What it must not do

**It must not overwrite the drive the user gave us.** Apple's `osos` is 7.21 MiB of software they
cannot re-download — [Apple no longer serves the IPSWs](../../README.md). Installing an OS has to
produce a *new* drive image, named for what is in it, exactly as building from an `.ipsw` already
does (`built_drive_name()`), leaving the original alone.

That makes the machine list the natural home for this: RetailOS and Rockbox are two drives, not one
drive in two moods, and switching between them is switching machines.

## The chord, and why there is no boot picker

**A real iPod boots whatever is in its firmware partition.** There is no menu asking which operating
system you meant, and there must not be one here either — a picker would be a bypass in spirit: it
would let the window start something the machine's own bootloader never chose, and every divergence
afterwards would be un-attributable between *"the OS does this"* and *"we started it a way nothing
starts it."* Switching OS is switching **drives**, which is what the machine list is for.

The one place a chooser is honest is the one the hardware has: **the hold-a-button-at-power-on
chord.** `diag`, `disk`, `scan` and `logo` are reached on a real 5.5G by holding `SELECT`+`REW` and
friends while it starts, and the window already draws those buttons and delivers them to the
machine. So the interaction is *hold the chord on the wheel while the iPod boots* — not a menu item.
That is the same design rule as everything else here: the way in is the way the device has.

## Honest state of the destination

**Updated 2026-08-18, twice in one day.** Rockbox reaches **its main menu**, takes wheel input, and
opens its file browser onto a volume `put-files` wrote — in its own font, off the emulated disk. The
shutdown that made this section say *"the most likely outcome is a device that boots and then
switches itself off"* is gone: it was the emulator's clock teleporting through idle time, and
`sys_poweroff` went from 315 calls a boot to **none**.

**Cold-booted from disk it still stops after its splash**, so the destination is not finished — but
it is no longer a device that turns itself off, and the failure now has a bounded shape rather than
a symptom.

**iPodLinux** still has no kernel here. `ipodloader2` — the bootloader that would load one — now
**builds and cold-boots**, and immediately misidentifies the chip; see
[research/16](../../research/16-the-third-bootloader.md).

## Order

1. ~~Find out why a shutdown is requested after the disk scan.~~ **Done** — it was the clock, not
   the battery, and not Rockbox.
2. ~~`osos` installation into a **new** drive image, with the `ipvd` checksum verified.~~ **Done** —
   `ipod-boot install-os`, which refuses unless the checksums already in the directory reproduce
   first, and `ipod-boot put-files` for the volume beside it.
3. **Content routing for `.ipod` files, and the machine list showing what each drive holds.** The
   open one, and now the only thing between a person and a second operating system.
