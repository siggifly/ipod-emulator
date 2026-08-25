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

**You do not need anybody's iPod to start one any more.** Pick a model, press a button, and the
emulator builds the boot ROM and fetches Apple's firmware itself. Your iPod remembers, the settings
no longer reboot it, and it is not only Apple's iPod that runs here: a second operating system
boots too, from a disk this program wrote, started by Apple's own bootloader.

### Start with nothing: pick an iPod and press a button

Until now this program needed **a dump of a real iPod's boot ROM** — a 1 MiB file you could only get
off hardware you owned, with a soldering iron or a bootrom exploit. That was the wall in front of
everyone who did not already have one.

**There is a list of iPods now, and you choose from it.** 198 models, from the 2001 original to the
last iPod classic, transcribed mechanically from libgpod's table rather than typed. Choose one and
the emulator **synthesises its boot ROM**: the identity block a real iPod carries — serial number,
model number, hardware id and version, region — built to the same layout, with a serial that looks
like the real thing because it is assembled the way Apple assembled them (factory code, year and
week, a production code from the range that model actually shipped in).

**And the case colour is not a switch.** It is read out of the model number, the way the iPod itself
reads it, so a `MA146` is white and a `MA446` is black and a U2 special edition is black with a red
wheel — because that is what those part numbers *are*. Nothing to set, and nothing to get wrong.

If you *do* have a dump, nothing changes: it is still the more faithful machine, and pointing at one
still wins. If you have a real iPod's **drive**, the identity found on it is used in preference to
generating a new one — your iPod stays your iPod.

### Apple's firmware, fetched from Apple

The other file you needed was an `.ipsw`. The emulator now offers to **download it from Apple**, off
Apple's own servers, and the ones it offers are the 66 releases Apple still serves — every one of
them downloaded once and recorded by SHA-256, so a download that comes back wrong is refused rather
than booted.

Providing your own still works, and it is checked **by content, not by filename**: a renamed bundle
is recognised, and a *modified* one is called out and then allowed, because running a patched
firmware is a legitimate thing to want and being told what you have is not the same as being stopped.

Downloads are cached, deduplicated, and yours to remove — the cache states what is in it and how
large it is, and clearing it is one button.

### It boots without a dump, on all three revisions

A synthesised ROM is not a copy of Apple's bootloader — nobody can distribute that — so it does not
run one. It reproduces the **effects** of the boot, measured off a real one rather than guessed:
Apple's bootloader reads the operating system off the drive into memory at `0x10000000` and jumps to
the top of it, leaving a handoff block with this iPod's identity in it, and that is what the
synthesised boot leaves too.

All three firmware revisions boot this way — 5G Initial, 5G Rev A and the 5.5G — each with the
firmware bundle that belongs to it, and each reaching the same place the real ROM does.

What a synthesised ROM **cannot** do is stated rather than left to be discovered: diagnostics, disk
mode and the boot logo are self-contained programs Apple ships inside the flash, and a generated ROM
has none of them. Those need a real dump, and the program says so.

### A boot screen, and it can be yours

The synthesised boot draws a screen in the colours that model actually booted in — white iPods dark
on light, black and U2 iPods light on dark — with a click wheel outline where Apple's logo goes,
drawn filled, shaded and anti-aliased rather than stamped.

**You can supply your own image.** PNG or PPM, any size: it is resized to exactly the size Apple's
own boot image occupies, aspect preserved, averaged rather than sampled so a large picture does not
come back speckled. Its brightness is taken as coverage and the case supplies the colour, so **one
image is correct on both a white and a black iPod** without anybody inverting anything.

### A window a black iPod can be black in

The window was black, so a black iPod had to be drawn grey to be visible at all. It is charcoal now,
and the black iPod is black. Every piece of text on it is checked against its own background for
contrast at build time — which found one caption that had been below the readable threshold since
before this change.

### Apple's diagnostics runs

The service diagnostic — the one a real iPod shows when you hold **Select+Rewind** at power-on —
boots, draws and can be driven. `SRV Diag Boot`, then its manual-test menu, then down through
Memory, IO, Wheel and Display into the individual tests. It is Apple's own program out of the boot
ROM, on Apple's own video co-processor protocol.

It had never run. Two things were in the way, and neither looked like a fault:

**The wrong program was being loaded.** The images the emulator ran came from a directory extracted
once, off a *prototype* iPod's ROM, and were handed to every run whatever ROM was configured. The
prototype's diagnostics is a 200 KB factory build; the retail one is a different 98 KB program.
Images now come out of the ROM under test, every run.

**And the co-processor is at two addresses.** Everything else in this machine drives it at
`0x30000000`; Apple's diagnostics drives the same chip at `0xb0000000`, and that window was not
mapped — so the diagnostics uploaded its firmware into nothing and waited forever for a chip that
could not answer. It is mapped at both now. Nothing else moved: a retail boot never touches the
second window, and its numbers are unchanged.

`ipod-boot flsh` also refuses to *enter* an image that is not a program. `logo` is a bitmap and
`vmcs` is the co-processor's firmware; running them looked like it worked, because an interpreter
pointed at data does not fail — it decodes what is there and runs out of budget.

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

A third followed: **the click wheel only delivered input to firmware that asked Apple's way**, using
an opcode Rockbox never sends. With that fixed Rockbox reaches **its main menu, takes wheel input,
and opens its file browser onto a real volume** — one this program's own installer wrote, so the
theme, the icons and the font are all read back off the emulated disk.

It renders in its own font now, too. It used to come up in the tiny 8 px fallback it carries
built-in, which was not a bug in the emulator at all: the recipe pointed at a stock Apple drive with
no Rockbox on it, and a themeless install is an ordinary condition rather than an error, so nothing
said so.

**Cold-booted from disk it does not yet reach the menu** — it draws its splash and stops. RetailOS
is unchanged to the digit across every one of these fixes.

### Install an operating system, and cold boot it

**`ipod-boot install-os` puts somebody else's operating system where Apple's bootloader will find
it** — appended after `osos`, the directory's entry point moved to it, checksums fixed, the later
images shifted out of the way, exactly as `ipodpatcher` does on hardware. **`ipod-boot put-files`
writes the other half**: a directory tree into the FAT32 volume, long names and all, 381 files in
1.7 s.

So the whole chain runs with nothing warm-entered and no step skipped: Apple's boot ROM → Apple's
bootloader → the bootloader you installed → its operating system off the volume you wrote.

Neither ever touches the image you point it at. `install-os` writes a **new** file, and it
**reproduces the checksums already in the directory before writing new ones**, refusing if it
cannot — added because the first version did not, and produced an image the bootloader rejected 71
ATA commands into a boot with *"Use iTunes to restore."*

### Three bootloaders, and a third operating system

Apple's retail bootloader, Rockbox's, and **`ipodloader2`** — and through the third of them,
**iPodLinux boots**. Not "executes instructions": it finds both partitions, mounts the FAT32 volume
as its root, runs `/bin/init`, and loop-mounts ZeroSlackr's 8 MB ext3 userland out of a file on that
volume, with no ATA error anywhere in the kernel's log.

```
Partition check: /dev/hda:  p1  p2
VFS: Mounted root (vfat filesystem).
Mounted devfs on /dev
BINFMT_FLAT: Loading file: …
EXT3-fs: mounted filesystem with ordered data mode.
```

**Getting there meant fixing six things in our ATA model, and every one of them was a place the
hardware behaves one way and this emulator behaved another.** RetailOS is byte-identical across all
six, so none of them is a change made to suit one guest:

- **The IDE data register is sixteen bits wide, in a four-byte slot.** We served the upper two byte
  lanes as more sector data. iPodLinux reads that port with 32-bit loads and keeps the low halfword
  — correct for a 16-bit register — so it received every *second* word of its IDENTIFY and read
  `heads` out of the wrong field. Rockbox and Apple's firmware never noticed: both read it 16 bits
  at a time and never touch those lanes.
- **A completion is a level the drive holds, not a pulse.** We asserted it into a masked interrupt
  line and let the driver's own housekeeping sweep it away, which is `hda: lost interrupt` exactly
  as reported. **This one also fixed Rockbox** — see below.
- **There is one drive on this bus.** We answered for a device 1 a 5G does not have, so the kernel
  attached two disks of the same size and interleaved their commands through one state machine.
- **A multi-sector PIO read interrupts once per block**, not once per command.
- **RECALIBRATE and the power-management family are legal commands.** We aborted them, so Linux's
  error recovery got a fresh error and a routine spin-down got `DriveStatusError`.
- **INITIALIZE DEVICE PARAMETERS now takes effect** rather than being acknowledged and ignored.

**What is not finished:** ZeroSlackr's launcher draws its startup screen and stalls at its last step.
That is a real open bug, and the README shows the picture with that said rather than implied.

### Rockbox draws on a synthesised ROM, and that was the same fix

`GPO32_VAL` bit 14 is a pin Apple's bootloader drives when it powers the video co-processor. A warm
entry skips that bootloader, so the bit read back zero — and Rockbox's `lcd_init_device` keys on it
directly, taking a recovery path meant for ROLO and getting away with it only because that path
re-uploads the co-processor's firmware from the ROM, which a synthesised one does not have.

| Rockbox's main menu, non-black pixels | before | after |
|---|---|---|
| real 5G dump | 74 057 | 74 057 |
| synthesised 5G | **0** | **74 057** |
| synthesised 5.5G | **0** | **74 057** |

Rockbox's full themed menu — icons, backdrop, watermark — on a boot ROM that contains none of
Apple's code. The same line fixed iPodLinux on synthesised ROMs.

### Installing iPodLinux, and triple boot

`ipod-boot install-linux` builds the drive: `ipodloader2` into the firmware partition, all five of
the distribution's directories onto the volume, and a boot menu naming **only what is actually
there**. On a drive that already carries Apple's software and Rockbox, that comes out as a
three-entry menu — *ZeroSlackr, Apple OS, Rockbox*. There is a button for it in the window.

ZeroSlackr is fetched and verified like everything else here — URL, size, SHA-256, and nothing
renamed into place until it verifies. **So is the bootloader.** `ipodloader2` v2.8.1 — 56 912 B,
SHA-256 on record — is downloaded and checked the same way, and it is resolved *before* the 101 MB
distribution rather than after it, so a failure arrives before the download instead of at the end of
one. Until now the loader was built from `resources/vendor/ipodloader2`, which is not in the
repository: iPodLinux could be installed only by somebody working inside a checkout of this project.

**This changes which bootloader you get.** The vendored build is `iPL 2.9.0d`, from upstream's
`master` and newer than any release; the fetched one is v2.8.1, the newest thing upstream publishes a
binary for. Every number in `research/17` was measured on 2.9.0d, so until the same run is made
against 2.8.1 those figures describe a loader most people will not be running.
`IPOD_LOADER=/path/to/loader.bin` installs one you built instead — including 2.9.0d — and the report
says which of the two ran, marking a supplied one `not hashed`, because this project holds no hash
for a build somebody made.

**It refuses drives it cannot boot rather than building them.** `ipodloader2` reads FAT32 partition
type `0x0B` and has no case for `0x0C`; every drive image taken off real hardware here is `0x0C`.
That is an upstream limitation, and rewriting the partition type to suit it would make the loader
happy and the disk a lie.

### The clock stopped inventing time

**An iPod left alone used to switch itself off after about thirteen seconds.** Not a bug in the
firmware — the firmware was right. When the processor halts to wait, the emulator used to jump its
clock forward to whenever the next interrupt was due, which made idle time *free*: a machine doing
nothing aged thousands of times faster than one doing something, and ten idle minutes went by while
you were reaching for the wheel.

Now a halt costs what running costs. The whole machine keeps **one honest ratio** to the real part,
busy or idle: at a third of speed everything takes three times as long, waiting included. Nothing
was added to pace it and nothing is throttled — the invented time was simply deleted.

The same change fixed a bug nobody had connected to it. Cold-booted Rockbox read its battery as
0 mV and shut down; the byte carrying the request was being written correctly and read back as
zero a few instructions later. Time jumping in the middle of a function let something land between
the two in a way hardware cannot do. `sys_poweroff` went from 315 calls to none.

`--clock=5`, which made the simulated clock run fifteen times fast so the bootloader's delay loops
would collapse, is retired with it. It turned out to cost nothing: the honest clock reaches the
same place in **half** the instructions.

### The films have their colours back

Every published animation was built with **one 256-colour palette for the whole film**. A single
frame of this machine's screen carries 211 to 238 colours and the boot film is 24 different
screens, so each frame lost 30-45 % of its own colours — which is why the battery's green and
Brick's bricks looked wrong in the animations while the stills beside them, written from the same
frames, looked right. Each frame now gets its own palette and is exact.

Three of the still images were also being taken from the wrong frames — a half-drawn menu published
as `extras`, the Extras menu published as the games list — because the frame numbers had not been
re-measured since the boot got longer.

### An empty ROM dump now says so

Rockbox's **Dump ROM contents** can leave a correctly named file with nothing in it — reported from
a real 5.5G, and the failure looks like success until something tries to read it. `--check-images`
used to answer *"cannot read this file: failed to fill whole buffer"*, because it read the file
before it measured it. It measures first now, and an empty dump gets the sentence written for it:
the file is empty, the dump wrote nothing, and a reset before it finishes leaves exactly this.

### Resuming gave you an iPod that ignored every button

**And it was not ignoring you — it was dead.** Closing the window parks the machine so the next
launch resumes in three seconds instead of cold-booting for seventy-five. What came back could not
execute a single instruction: the saved state carried every region of memory, both interrupt banks
and the whole processor, and left out the sixteen words that describe the **address windows**.
RetailOS runs entirely through one of those windows, so it resumed, read zeros where its own code
should be, and ran off into nothing a few hundred instructions later.

Nothing reported it, and that is why it presented as an input bug. The screen still held the last
picture the video chip had been handed, so the window showed a perfectly ordinary iPod that would
not respond to the wheel or the hold switch.

```text
before   the resumed program counter reads 00 00 00 00    dead after 223 instructions
after    the same address reads 04 00 00 1a               3 000 000 and still going
```

Saved machines from before this release are **refused rather than misread**, so the first launch
after updating cold-boots once and saves a new one. Nothing else is lost.

Three more things came out of the same investigation:

- **A truncated save file crashed the program** instead of being rejected. It is written as the
  window closes, so a truncated one is exactly what a crash or a full disk leaves behind — the worst
  moment to take the program down with it. It cold-boots now, as it always said it would.
- **`from-idle` cached a machine built differently from the one it resumed into**, so a run that
  asked for a click wheel got a machine that had never had one.
- **The click wheel's report named the wrong cause.** Two different refusals — reporting switched
  off, and a receiver that was never armed — shared one counter, so a run could print `reporting ON`
  and `12 frames suppressed while off` on consecutive lines.

### The iPod is full size again, and stays that size

It had been drawing at **half size**. The window's height was reduced to fit a 13-inch laptop, and
because the iPod is drawn at a whole number of screen pixels per emulator pixel — which is what
makes the panel exact rather than blurred — that reduction crossed a step and halved it. Nothing
failed; it just got small.

It also **changed size while you watched**: the strip under it grew when the machine had something
to report, and a warning appearing could push the iPod down a whole step. Both strips are fixed
heights now, so a notice is a notice rather than the emulator appearing to break.

### One settings button, and a library behind it

There were two buttons — `settings…` and `software…` — that opened the same page. There is one now.

And **the page has a library**: every boot ROM, `.ipsw` and drive this program knows about, in one
list. Before, a file that was not the one running had nowhere to live, so choosing a second boot ROM
lost the first. A machine is now **composed of** entries from that library, so the same ROM can back
several machines and editing an entry changes all of them. Files you drop on the window are filed as
well as used, and the list fills itself the first time from whatever you already had.

Settings → About carries the repository link.

### Fixed

- **Press Start on an iPod you had just made, and it died before the drive answered.** The
  operating system was read off the drive and then filed *beside* memory instead of into it, so the
  moment Apple's software remapped its own address space — about a fifth of a millisecond in — the
  code went out from under it and the machine walked off the end of memory. `stopped: lost 33554432
  at 8388485 instructions`, every time, with the drive untouched. It is put where Apple's own
  bootloader is measured putting it now, and the same iPod reaches **484 disk commands** and a lit
  panel in the same run that used to reach none.
- **A machine that stopped said nothing to the log.** Starting one prints two lines; stopping one
  printed nothing at all, so a session where the same iPod died five times read as five identical
  boots with no endings — which is indistinguishable from a program restarting itself. It says
  `stopped:` and why, now, and the second one in a row says it is the second.
- **`ipod-boot warm --flash=…` ignored the file you named** and ran the configured ROM instead,
  printing the configured ROM's model in its own output. The same held for `--disk=` and for every
  recipe. Your flag wins now, and `--print` says so.
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

**The recipes are one program now.** There used to be six `.sh` files and six arms of `ipod-boot`
composing the same command line, kept equal by a test that read the scripts off disk and compared
flag lists — and that test was the tell. The scripts are gone; `ipod-boot retail | warm | flsh |
rockbox | flash-update | from-idle` is what they were, and `--print` still shows the command line it
composes without running it.

**`ipod-film` films more than the boot.** `RECIPE=` picks which machine is filmed, `--realtime`
paces the film at the machine's own clock instead of one second per sample, `--cap=` stops a final
frame that holds until the budget from dominating the film, and `ipod-film asset diag` is the whole
diagnostics tour as one command. `trace --bcm-png=` writes the co-processor's framebuffer as a PNG
beside the existing `--bcm-ppm`, so a screenshot no longer needs a converter.

**One calibration worth knowing before you script input.** `--wheel`'s `press=` expands to a
down/up pair 20 000 instructions apart — 0.27 ms at the real clock. Firmware that polls its buttons
on a timer will not see it: Apple's diagnostics reads them once per 150 ms, so every press fell
between two polls while the interrupt handler recorded each one perfectly. Hold buttons with
explicit `down=`/`up=` pairs when the reader is a poll rather than an interrupt.

**Two new subcommands, and no Python left in the project.** `ipod-boot fat` reads the FAT32 volume
out of a drive image — `tree`, `find`, `cat`, and `lba`, which turns the absolute sector numbers in
`trace`'s DMA log back into paths. `ipod-boot rsrc` does the same for the `rsrc` volume in the
firmware partition. Both open the image read-only and neither mounts anything, because a
partitioning command aimed at the wrong device is the one mistake here with no undo.

**Anchor a wheel script in simulated time, not instructions — especially against a snapshot.** A
machine resumed at an idle snapshot is already at a menu, so it is halted from its first step: a
3 G-instruction budget executed **495 M**, and a script anchored at `@2200M` fired **0 of 12** steps.
Anchors that did land were worse than ones that did not — they all fired at once during a disk scan,
and the run reported `1 word read of DATA, 11 frames dropped unread`, which reads exactly like a
firmware that has stopped listening. The same script anchored at `@24s` on the same snapshot:
**16 posted, 0 dropped, 16 read, 16 interrupts.** Nothing about the machine changed. `trace
--restore=` prints the clock it resumed at; anchor past it.

**`--restore` checks whether anything is mapped at the resumed program counter** and says so, rather
than leaving a machine to execute zeros until it is declared lost. **`IPOD_LAYOUT=1`** makes the
window print the measurement its size constants are derived from, so those can be re-measured rather
than trusted.

**`tests/snapshot_round_trip.rs`** is new, and its first version was worthless: it restored into a
machine that had already configured the address windows itself, so it passed with the fix reverted.
It restores into a bare machine now, the way the real path does, and is verified to fail without the
fix. `every_settings_pane_draws_and_fits` closes a similar gap — laying out the settings screen
draws the rail and exactly one pane, so five of six were never rendered by any test.

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
