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

## 0.5.0

**Your iPod remembers, the settings no longer reboot it, and it tells you what you gave it.**

### The drive is yours, and it is written to

The emulator runs **on the drive image you gave it**, the way a real iPod writes to its own disk, so
your settings, your language and your music stay on it. It used to run on a throwaway copy and keep
a second frozen copy beside it — two 8 GB files per pair of images, and an iPod that forgot
everything.

Closing the window **parks the machine**: RAM and a stamp naming the drive go down together, and the
next launch resumes in about three seconds. If anything touched that drive in between — iTunes,
`make-disk`, a second window — the stamp does not match and it cold boots and says why, rather than
restoring RAM onto a drive that has moved.

**Work on a copy** is still there in the settings, and it remembers too now. `--copy` and
`--no-copy` choose for one run. Switching to direct offers the old drives back: the reclaim figure
counts them, where before it protected them for ever because their names matched.

### Settings, not setup — and the iPod keeps running behind them

Opening the settings used to end the machine and walk you through three pages to get back, because
the settings screen and the first-run screen were the same screen: the only way to reach it was to
have no machine.

Case colour, the readout and the update check apply as you change them. **Only the two files and
where the iPod writes need a restart**, and the screen names which changed and offers it; `Done`
leaves it for the next launch. `Esc` closes. Both are refused only while the images do not validate,
so a first run cannot be escaped into an emulator with nothing in it.

### One screen, and it sorts your files for you

**Drop both files anywhere on the window, in any order.** Each is identified by what it contains — a
zip is Apple's bundle, exactly 1 MiB is the boot ROM, anything else large enough is a drive — so
there is nothing to put in the wrong box, and an `.ipsw` builds the drive as it lands instead of
waiting behind its own button. **Choose…** takes both files at once.

The path fields are gone. Files are named for what they are — `iPod Video · Y7TXK`, `iPod software
20.6.3` — with the path on hover.

### It tells you what it found

Every verdict already read these files and threw the findings away unless one failed. **What's in
it** now opens a page per file: the ROM's images, serial, GUID and build string; the drive's
firmware images, whether there is an OS, whether the flash updater is armed.

And it checks the **pair**, which no single file's verdict can: a bundle from the wrong updater
family boots, fails to recognise the drive, and asks to be restored from iTunes after about 70 ATA
commands where a matching pair reaches the language picker with 618. That reads as a broken
emulator. It is now caught before the boot — *"These are not the same iPod — Family 24. iPod Video
takes family 20."*

### One window, and nothing scrolls but the click wheel

Every screen is the same column in the same window. The minimum size is derived from the tallest
page rather than guessed at, and a test lays every screen out with no window and no GPU and fails if
one outgrows it. The old minimum was 520 px against pages needing up to 678.

### The readout replaced the instrument panel

The resizable right-hand panel is gone, and what was in it split three ways. **Power, restart and
the two-thumb holds belong to an iPod**, so they sit under the device in every mode — in user mode
you previously could not restart the machine at all. **Conditions** — halted, on hold, drawing to
the surface nobody is looking at — are one line each in every mode. What remained was measurement,
and `D` draws it over the device without changing the window's shape.

### Rockbox boots here

**Rockbox 4.0 gets to `Scanning disk…` and reads the volume** — 2 393 ATA commands where it
previously issued none. Two missing device models were in the way, both found by disassembling the
spin and both matched to Rockbox's own source:

- **A USB clock that never reported ready.** `usb-fw-pp502x.c` sets `INIT_USB` and then spins on
  bit 7 of `0x70000028` with no timeout. That bit now follows the enable. Apple's firmware reads
  the address **zero** times in a 600 M boot, measured before the change was written.
- **The battery ADC was on the wrong channel.** Rockbox's `adc_init` names channel 2 as the
  battery; we answered it with the 3 000 mV catch-all for unknown channels.

It still ends at *"Battery empty! RECHARGE! Shutting down…"* — and that is **not** the battery:
forcing the ADC to full scale changes nothing, so the trigger is elsewhere. RetailOS is unchanged
to the digit across both fixes. See [`docs/ideas/run-any-os.md`](docs/ideas/run-any-os.md) for what
running another OS as a *feature* would take.

### An empty ROM dump now says so

Rockbox's **Dump ROM contents** can leave a correctly named file with nothing in it — reported from
a real 5.5G, and the failure looks like success until something tries to read it. `--check-images`
used to answer *"cannot read this file: failed to fill whole buffer"*, because it read the file
before it measured it. It measures first now, and an empty dump gets the sentence written for it:
the file is empty, the dump wrote nothing, and a reset before it finishes leaves exactly this.

### Fixed

- **`--headless`, `--selftest`, `--probe` and `--power-cycle-at` could not open a drive.** Which
  drive the machine writes to was decided inside the window, so every path without a window pointed
  at a working copy nothing had made.
- **The window called itself `ipod-emulator`.** It names the machine now — `iPod Video (5G / 5.5G)`
  — which is the thing a second model would change. Deliberately without the old `— RetailOS`: the
  OS is whatever the drive holds, and this window already boots a drive that holds something else.
- **Three boot scripts pointed at a ROM directory the resource reorganisation had removed.**
  `flsh.sh`, `rockbox.sh` and `warm-boot.sh` defaulted to a path that no longer existed; all five
  scripts' defaults now resolve.
- **Building from a second `.ipsw` silently overwrote the first.** Every build landed on one path.
  Drives are named for the software in them now, keyed on version and CRC, so the same bundle
  resolves to the same file and a different one cannot land on it.
- **The storage figure skipped the largest files in the folder** once built drives moved into a
  subdirectory — `dir_size` stopped at the first level.
- The frozen drive can no longer be restored over your own image by a mis-wired path: reaching that
  branch requires copy mode to be on.
- `--ipsw=` builds the drive, as dropping the bundle would. It used to fill a field and wait for a
  button that no longer exists.

### For people working on this

`resources/` was reorganised: `drives/` for images that cannot be rebuilt, `derived/` for what a
script regenerates, `vendor/` for upstream checkouts (never renamed, so `git pull` keeps working),
`roms/` for boot ROMs under names that say what they are. The tree itself moved beside the
repositories rather than inside the public one.

## 0.4.0

Four bugs reported by users, all reproduced, all ours. Plus a setup screen that
somebody can actually use.

### It will not eat your disk any more

The cache was keyed on both image paths and **nothing had ever deleted one**. Every pair of files
you tried left an 8 GB working disk and a ~1.6 GB snapshot behind, silently, in a directory the
program never named, on whatever volume it resolved to. Somebody lost 50 GB trying four firmware
versions. One pair is kept now, the rest are deleted on every start, the setup screen states the
total with a button to clear it, and step 3 tells you what starting will cost before it costs it.

### One folder, and it prefers to stay where you put it

Settings went to one directory and gigabytes to another — on Windows, `AppData\Roaming` *and*
`AppData\Local`. There is one directory now, and for an archive you unpacked it is **`data/` beside
the executable**, which is what a program shipped as a zip should do. The platform directory is used
only where beside-the-executable is not writable, which is what a macOS bundle gets. Settings from
the previous version are carried forward, and the old directories are named in the UI so you can
delete them.

### The setup screen is readable, and it is a wizard

It was dark grey text on a black background — nothing called `set_visuals`, so egui followed the
operating system while the device is drawn on black regardless. It survived because **an author
cannot reach that screen**: run the binary inside the repository and the default paths resolve, so
it boots straight past. Only somebody without the files ever sees it.

It now asks one question at a time, verified before the next, with back and forward, real margins,
and the device drawn at the top. The detail — byte counts, model numbers, where to find a dump — is
folded away where a stuck user will look and nobody else has to read it.

**Getting the boot ROM off your own iPod with Rockbox is the first recommendation**, with links,
because it is the route that involves nobody else's copy of anything.

### The mouse wheel turns the wheel

It is the obvious input for this device and every mouse has one. One notch is one detent; a trackpad
glides.

### The app is called `ipod-emulator`

It answered to four names, and the one users' settings actually lived under was `ipod-gui`. Cmd-Tab
said `ipod-gui` because macOS shows the executable's name, not `CFBundleName`. The emulator is named
for the line, not one model, so nothing claims to be an "iPod 5G" any more.

### Also

- **The prototype NOR and its recipe are gone.** It boots a firmware partition the retail ROM
  correctly rejects, and shipping it sent a user hours down a path that cannot work.
- **The archive has one program in it**, with the six developer tools in `tools/`.
- The README says it is alpha, says exactly which NOR and IPSW were tested, and explains that the
  retail dump is archived under *iPod Classic* in a directory named `A1238` — which is why searching
  for "iPod Video" finds nothing.

## 0.3.0

### The app is called what it is

macOS shows the **executable's** name in the application switcher and the process list — not the
crate's, and not `CFBundleName` — so Cmd-Tab said `ipod-gui`. The binary is **`ipod-emulator`**
now, and the copy inside `iPod 5G.app` is named `iPod 5G`, which is what the bundle claimed all
along while the switcher read the file instead.

### A real icon

The drawn iPod with Brick on its screen, at **81 % of its canvas** — which is what Apple's own
icons measure, rather than what looked right. Notes and Reminders both occupy 104x104 of a 128px
icon; this one now does too, to the pixel. It is built from the full-resolution window with
nothing upscaled, and the window's own icon went from 64px to 512px because Cmd-Tab draws at 256
physical pixels and a 64px source upscaled four times looks exactly like what it is.

### The window says what it is doing, and says it once

The cold-boot bar was a 6-point hairline with its label clipped inside it — a progress indicator
that reported nothing during the one minute anybody wants to be told something. The bar keeps its
place and loses its text; the text is in the footer's left, at a size you can read. Debug mode no
longer draws a second bar for the same boot.

The keyboard list moved out of a tooltip and into the empty column beside the device. Nothing in
this window covers anything.

The **black/white** switch moved out of the debug panel, where it sat between two instruments. Which
of the two colours the 5G shipped in is a fact about your iPod, not about the machine — it belongs
in user mode, and it is remembered now, which it was not.

### Also

- `RELEASING.md`, and the check that would have caught the next one of these.
- The bundle's version was a literal that still said `0.1.0` while the workspace had moved on. It
  reads `Cargo.toml`.
- Four `Cargo.lock` files that predate the workspace, deleted. A workspace has one, at its root.

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
