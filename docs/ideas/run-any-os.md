# Running an OS that is not RetailOS

**Proposed, 2026-08-18. Not in 0.5.** Written down because the design falls out of parts that
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

## Honest state of the destination

Rockbox reaches `Scanning disk…` and reads the volume, then prints *"Battery empty! RECHARGE!
Shutting down…"* and powers off. It does not reach a menu. **iPodLinux is untested, and we do not
have a kernel image to test with** — the tree vendors `ipodloader2`, which is the bootloader that
would load one, not the OS itself.

So shipping this today would ship a feature whose most likely outcome is a device that boots and
then switches itself off. **Finishing the boot is the prerequisite, not the UI.** It is also a good
problem to have: unlike RetailOS, Rockbox has source and symbols, so it names a function.

## Order

1. Find out why a shutdown is requested after the disk scan. It has symbols; this is a debuggable
   question rather than a hex address.
2. `osos` installation into a **new** drive image, with the `ipvd` checksum verified.
3. Content routing for `.ipod` files, and the machine list showing what each drive holds.
