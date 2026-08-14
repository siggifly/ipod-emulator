# Changelog

What changed between releases, and why. Written for someone deciding whether to update, so it
leads with what they will notice rather than with what was refactored.

**Versions.** One number for all four crates, set once in the workspace root. The window compares
its own version against the latest release tag, so the tag and the crate version are the same
statement made twice — bumping the workspace is the whole of bumping.

**A published tag is never moved.** `v0.1.0` was moved once, on the day it was cut, before anyone
had downloaded it — and that is exactly the mistake this rule exists to prevent: had anyone been
running the first build, their copy would have reported itself current while being four commits
behind. Anything published from here gets a new number.

## 0.2.0

### You can change the images without quitting

The setup screen used to be reachable only when no machine was running, and the first successful
pair was remembered — so the second launch went straight into the iPod and never showed that screen
again. Whatever you picked on day one was what you had. **`images…` in the footer** returns to it.
It ends the running machine, because a booted RetailOS read its partition table at boot and has
been writing to that drive since; there is no honest way to hand it a different one.

Nothing is lost that was not reproducible: the snapshot is keyed on the pair of paths, so the one
taken against these images stays valid for them, and a different pair gets its own.

### The cold boot says what it is doing, in both modes

A cold boot spends most of its time on a white screen — the Apple logo is drawn early, and then
RetailOS does a long stretch of simulated work before it draws anything else. User mode showed
nothing at all during that, and a blank window that is busy looks exactly like a blank window that
has hung. The progress bar with a percentage and an estimate had been sitting in the debug panel
the whole time; it is in the footer now, where both modes can see it.

The underlying gap is still there and is still a bug: the boot takes far more simulated time than
hardware does. This makes it visible, not shorter.

### Setup is the front door, and the command line honours it

`ipod-gui`'s setup screen asks for the two files, says what each one actually is, and remembers
them. It used to remember them only for itself: you could finish setup in the window and every
shell recipe would still fail, because `ipod-boot` had never heard of the settings file.

- `ipod-boot` now resolves its NOR dump and drive as **environment → setup screen → repository
  default**, so setting up once in the window is enough for every recipe.
- `--print` says where each path came from, because a recipe with an input you cannot see in its
  command line is one you cannot check.
- **`ipod-boot setup`** asks the same two questions in a terminal, for a machine with no window,
  with the same verdicts and the same file. Answer with an `.ipsw` and it builds the drive.
- The missing-file message used to explain that `resources/` is gitignored — this repository's
  mental model, and a directory a release user does not have and never will. It now names the
  setup screen, the settings file, and the two variables.

### Booting no longer asks for a game

Every recipe demanded an eApp image and refused to run without one, even though a boot enters from
the reset vector and never looks at `0x18000000`. It was only there to fill `trace`'s first
positional, and its default pointed somewhere that existed on one machine. Someone with exactly the
two files the README documents could not boot from the command line at all.

`trace`'s image positional is optional now: a leading positional that parses as an integer is the
budget, and a path is never a bare integer, so the two cannot be confused.

### macOS gets an app bundle

`iPod 5G.app`, so double-clicking it in Finder opens the emulator rather than a Terminal running a
Unix executable. Still no certificate and no notarisation — it is ad-hoc signed, which is what
Apple Silicon requires to run anything at all, and is not the same thing.

The instructions for allowing it were **wrong** and are corrected: macOS 15 removed the
right-click → Open bypass for unsigned apps. Open it, let it be blocked, then allow it once in
System Settings → Privacy & Security.

### Also

- Linux and Windows binaries, both run before publishing rather than merely linked.
- Build paths no longer embed the machine they were built on.
- The update check pointed at a repository that does not exist.

## 0.1.0

First public release. Apple's retail iPod 5.5G firmware booting from the reset vector: the
bootloader brings up SDRAM, talks to the PCF50605 over I²C, uploads firmware to the video
co-processor, reads the partition table, DMAs RetailOS into memory, checksums it and jumps.
RetailOS starts its RTXC kernel and 61 tasks, mounts a FAT12 volume out of the firmware partition,
formats and populates its own FAT32 volume, and draws its menus. The click wheel works. Brick
plays.

macOS binaries only.
