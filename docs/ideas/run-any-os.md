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

## The plan, in full

**Written out 2026-08-19, after four things were measured that this document was guessing about.**
The shape below is the same one it always argued for — *the way in is the way the device has* — and
every part of it now has a measurement behind it rather than an expectation.

### The two layers, and why they are different

| | how a person gets there | what it is |
|---|---|---|
| **Another operating system** | **install it onto the drive**, then cold boot | Rockbox, iPodLinux. They live in the firmware partition, exactly as on hardware |
| **The boot ROM's own modes** | **hold the chord at power-on** | `diag`, `disk`. They are in the ROM, not on the drive; nothing is installed |

They are not variations on one mechanism, and collapsing them would lose what is honest about each.

### Layer 1 — installing, which is what a person actually does

Both halves already exist as commands, and together they are what Rockbox Utility does:

| command | writes | Rockbox Utility's equivalent |
|---|---|---|
| `ipod-boot install-os SRC.img OS.ipod OUT.img` | the image into the firmware partition's `osos` entry, checksum fixed | `ipodpatcher` |
| `ipod-boot put-files` | `.rockbox/` — or `loader.cfg` and a kernel — onto the FAT32 volume | its file copy |

Then the machine cold-boots it from the reset vector. Nothing new boots; Apple's own bootloader
finds it where it already looks, so a divergence afterwards belongs to the OS.

**Three things stand between that and a window a person can use:**

1. **Content routing for `.ipod` files.** The window already sorts dropped files by what is *in*
   them. A `.ipod` is an 8-byte header — big-endian checksum, then four ASCII characters — over a
   raw ARM image, and the checksum is **verifiable**, so this is identification and not a guess.
   Drop `rockbox.ipod` on the window and it offers to build a new drive with Rockbox installed.

2. **A drive library that says what each drive holds.** RetailOS and Rockbox are two drives, not
   one drive in two moods. Each row states its firmware-partition contents and whether the volume
   carries `.rockbox` or a `loader.cfg`, so the thing a person picks is a *machine*, described.
   This is also what protects Apple's `osos`: 7.21 MiB nobody can re-download, and installing never
   edits the source.

3. **Somewhere to get the files.** The same shape as the Apple firmware work already shipped: a
   small catalogue of Rockbox releases and its bootloader, each recorded by SHA-256, fetched on
   demand, cached, deduplicated and clearable. `firmware.rs` is that machinery and generalises.

### Layer 2 — the chord, which is now measured rather than proposed

This document argued for the chord on principle. **It works.** Apple's boot ROM queries the click
wheel three times before it loads anything — `0x8000023a` at 2.8 M, 18.1 M and 57.4 M instructions —
and holding `SELECT`+`REW` across them makes it choose diagnostics itself:

```
Running 'diag' 0 from 0x10000000
```

with nothing placed by us, and it draws to the same 70 669 non-black pixels a directly-entered
`diag` produces. See [research/07](../../research/07-the-flash-images.md).

**One thing blocks it from being the only way in:** releasing the chord afterwards storms the
interrupt controller — 7 812 499 asserted, 1 taken — where a directly-entered `diag` handles the
same release cleanly. Until that is understood there has to be a second door, and the second door
should be honest about being an instrument.

### The boot picker is an instrument, not a feature

A picker was added to the window on 2026-08-19, and **this document had already argued against
one** — correctly. It belongs in debug mode beside the readout and the framebuffer inspector, as
the window's equivalent of `ipod-boot flsh`: a way to enter an image directly, for looking at it,
which skips the bootloader and says so. It is not how a person runs another operating system, and
when the chord's release storm is fixed it stops being how anybody reaches diagnostics either.

### Where each destination actually is

**Measured 2026-08-19.** This is the part that decides what is worth building next.

| | state | the blocker, named |
|---|---|---|
| **Rockbox** | **main menu, legible**, on a volume `put-files` wrote — 3 953 ATA commands, 23 frame updates, 74 057 non-black pixels | none; it works |
| **Rockbox bootloader, cold** | **chain completes** — Apple's bootloader → Rockbox's → Rockbox main, 113 ATA commands — and then draws **nothing at all**, 0 non-black pixels at 1.5 G | why the same binary that draws a menu from one drive draws nothing from another. The difference is the disk and it has not been isolated |
| **ipodloader2** | **its console draws, legible**, past everything research/16 had ever seen | two bugs in its own source, below |
| **iPodLinux** | the kernel **is here** — `_out/ipl/boot/vmlinux`, 1 531 200 bytes, and `loader.cfg` points at it | ipodloader2 never reads it: 3 ATA commands total, which is `IDENTIFY` + the MBR + one probe |

### The two bugs standing between us and a third operating system

Both are in `ipodloader2`'s own `vfs.c`, both read out of the source it was built from, and neither
is this emulator's:

**The firmware-partition test is inverted.** `vfs.c:193`:

```c
if( mlc_strncmp((void*)(fs_header->fwfsmagic),"]ih[", 4) ) { validoffset = 1; }
```

`mlc_strncmp` returns **0 on a match** (`minilibc.c:452`). So the partition is accepted only when
the magic does **not** match. Both drives here carry `5d 69 68 5b` — `]ih[`, correct — at byte
`0x100` of the partition's first sector, and the loader rejects them and prints
`[0]: Bad iPod FW entry`.

**There is no case for FAT32-LBA.** `vfs.c` handles `case 0x00`, `case 0x83` and `case 0xB`. The
MBR here says partition 1 is type **`0x0C`** — FAT32 LBA — so it falls to `default:` and prints
`[1]: Unknown 0xC2`. (The trailing `2` is a literal in the format string, `vfs.c:274`.)

Two independent reasons a correct drive is invisible to it, and `No valid paritions found!` is the
consequence of both.

## Order

1. ~~Find out why a shutdown is requested after the disk scan.~~ **Done** — it was the clock.
2. ~~`osos` installation into a **new** drive image, with the `ipvd` checksum verified.~~ **Done** —
   `ipod-boot install-os` and `ipod-boot put-files`.
3. **Content routing for `.ipod` files, and a drive library showing what each drive holds.** Still
   the only thing between a person and a second operating system.
4. **A Rockbox catalogue**, so the files come from inside the program like Apple's already do.
5. **Move the boot picker into debug mode**, where an instrument belongs.
6. **Patch ipodloader2's two `vfs.c` bugs** and see whether iPodLinux boots. The diagnosis is done;
   what is left is a carried patch and a run.
7. **Isolate the Rockbox bootloader's black panel** — same binary, two drives, one draws.
