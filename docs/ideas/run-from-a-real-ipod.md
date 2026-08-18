# Running from a real iPod in disk mode

**Parked, 2026-08-18. Not scheduled, and deliberately not built yet.** Written down because the
reasoning is worth keeping and because the obvious implementation is the dangerous one.

An iPod held in disk mode is a USB mass-storage device: `/dev/diskN` on macOS, `/dev/sdX` on Linux,
`\\.\PhysicalDriveN` on Windows, with the firmware partition first and a FAT32 volume after it.
That is the same shape as the image files this emulator already runs on, so "just point it at the
device" looks like a one-line change.

## Why the one-line change is the wrong one

`Ata::open(path, writable)` would take a device node happily. Then RetailOS boots, decides the
volume is not one it recognises, and **formats it** — on the actual iPod, which is somebody's music
and, on the devices this project cares about, an authorisation minted against that device's GUID and
not reproducible. There is no undo, and the emulator would be behaving exactly as designed.

Writable direct access to a real device must therefore be **refused**, not offered with a warning.

Two smaller obstacles sit behind that one:

- **Size.** `metadata().len()` returns 0 for a block device on macOS and Linux alike. Sectors would
  have to come from an ioctl (`DKIOCGETBLOCKCOUNT` / `BLKGETSIZE64`), which is a per-platform code
  path this program does not otherwise have.
- **Permission.** Raw device access needs elevation everywhere. This binary is unsigned on purpose
  (`README.md` — buying a certificate to make a reverse-engineering tool look official is the wrong
  trade), and an unsigned binary asking for root is a worse trade still.

## The three safe shapes, in the order they are worth doing

**1. Import the firmware partition — the fast path, and probably what people actually want.**
"I want this iPod's software" needs partition 0 only: 27 140 sectors, about 13.9 MB. The rest of the
drive is synthesised exactly as `ipsw::build_disk` already does from a bundle. Fourteen megabytes
read, no 80 GB copy, and the result is an ordinary image file the emulator already knows how to run.

**2. Import the whole drive** — "I want a faithful copy of my iPod", including its FAT32 volume,
its `iPod_Control`, and any keybag on it. Up to 160 GB and slow, but it is the only way to preserve
a device's actual state.

**3. Run read-only, with a copy-on-write overlay.** Reads come from the device, writes land in a
sparse overlay file, and the iPod is never written to. This is the one that literally answers "run
directly off a mounted volume", and it is last because it means changing `Ata` — the component every
number in `research/` was measured through. `NEXT.md` R4 is explicit that a change to the machine
invalidates every "never" measured before it, so this is a deliberate re-measurement, not a feature.

## The part that is actually hard

Not the copying. **Identifying which device is the iPod.** A `dd` aimed at the wrong `/dev/diskN`
destroys a disk, and the failure is silent until it is total. So the valuable work is: enumerate,
recognise an iPod by its USB identifiers and partition layout, name it back to the user
("iPod, 80 GB, at /dev/disk4"), and refuse anything that does not look like one.

That argues for a **guided** flow over a privileged one: the program identifies the device and hands
over an exact, verified command to run, then checks the result. It keeps this binary unprivileged,
and it puts the one irreversible step in front of a human who can read what it says.

`safety-and-working-model.md` already forbids `diskutil`, `hdiutil attach` and every partitioning
command in this project, for this reason. `tools/fat-read.py` is the existing precedent: it walks
MBR and FAT32 itself, read-only, and never mounts anything.
