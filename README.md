# ipod-emulator

**Apple's retail iPod 5.5G firmware boots here, from the reset vector, on an emulator written from
scratch. It formats its own filesystem, reads the click wheel, draws its own menus, and runs a game.**

![cold boot through to a game](docs/media/ipod-12-device-boot.gif)

> ### This is alpha software
>
> It boots, it draws, it plays Brick — and it is four days old with one pair of images behind it.
> Expect rough edges, expect to read a paragraph to get started, and expect things that work here
> to fail on files we have never seen. **[Please open an issue](https://github.com/siggifly/ipod-emulator/issues)**
> if something breaks or could be better; the reports so far have found real bugs and every one of
> them has been fixed.

The iPod Video 5.5G shipped on 12 September 2006. Twenty years next month, its firmware still boots
— with no iPod anywhere near it. The PortalPlayer it addresses is arithmetic, the drive it formats
is a file, the wheel it reads is a mouse, and none of that is something the firmware can tell. It is
also the model I owned — my first Apple product, at twelve. That this is the one that ended up
emulated was not deliberate, and I liked it more than I expected to.

Not a reimplementation of the interface. Apple's own code the whole way: the bootloader brings up
SDRAM, talks to the PCF50605 power chip over I²C, uploads firmware to the video co-processor, reads
the partition table, DMAs 7.5 MB of RetailOS into memory, checksums it and jumps. RetailOS then
remaps memory, starts its RTXC kernel and 61 tasks, mounts a FAT12 volume out of the firmware
partition — its own boot sector claims `FAT16` and is wrong, which truncates every file to its
first cluster if you believe it — formats and populates its own FAT32 volume, spins the drive
down, and draws.

| | |
|---|---|
| ![](docs/media/ipod-07-apple-logo.png) | ![](docs/media/ipod-03-main-menu.png) |
| ![](docs/media/ipod-05-games-list.png) | ![](docs/media/ipod-06-brick.png) |

## What you have to supply

Two things, from an iPod you own. Apple wrote both, this project ships neither, and an iPod on your
desk has both on it.

| | What to look for | |
|---|---|---|
| **The boot ROM** | a 1 MB NOR dump, conventionally `internal_rom_000000-0FFFFF.bin` | Any name works — the size and the reset vector are what get checked |
| **Something to make a drive from** | Apple's `.ipsw` (~14 MB), **or** a drive image you already have | An `.ipsw` is built into a drive as it lands. `ipod-boot make-disk your.ipsw disk.img` does it without the window |

**The two must be for the same iPod**, and the emulator checks before it boots — a mismatched pair
otherwise fails quietly, reaching about 70 ATA commands and a request to restore from iTunes where a
matching pair reaches the language picker with 618. Drop both in and it says which files it has,
what is inside them, and whether they go together.

Without them it starts, says what is missing, and does nothing else.

### What has actually been tested

Everything in `research/` was measured on exactly one pair of files. That is part of what "alpha"
means here, and it is worth stating rather than implying any pair works:

| | |
|---|---|
| **NOR** | the retail iPod Video dump — 1 048 576 bytes, `HwVr 0x000b0005`, `Mod# MA146`, non-blank `HwId` |
| **IPSW** | `iPod_20.1.3.ipsw` — `Firmware-20.6.3` inside it is 13 895 680 bytes, exactly 27 140 sectors, exactly the size of the firmware partition |

**The best way to get the NOR dump is to read it off your own iPod.** [Rockbox](https://www.rockbox.org/wiki/RockboxUtility)
can do it in about five minutes and can be uninstalled straight afterwards: install it with Rockbox
Utility (only *bootloader* and *rockbox* need ticking), then on the iPod go to **System → Debug (Keep
Out!) → Dump ROM contents** and copy the `internal_rom_…` file off when you plug it in. The
[flash guide](https://www.rockbox.org/wiki/IpodFlash.html) has the detail. This is the route that
involves nobody else's copy of anything, and the only one guaranteed to match the iPod you have.

**If the dump comes out 0 bytes**, which has been reported: the file is written and closed at the
end, so an iPod reset before it finishes leaves a correctly named empty file. Let it finish rather
than hard-resetting — the read itself is seconds, not minutes, so a wheel still frozen after a
minute has failed rather than gone slowly — and shut down through Rockbox so the volume is flushed
before you unplug. `ipod-emulator --check-images --flash=… --disk=…` will tell you which of the
size, the reset vector and the image directory is wrong with any dump you end up with.

**Failing that, it is archived — under the wrong product.** BootROM collections file the iPod
Video's dump as *iPod Classic*, in a directory named `A1238`, which is the Classic 6G's model
number. The Video is `A1136`. Searching for "iPod Video", "5.5G" or "A1136" finds nothing;
searching for the Classic finds it. This cost someone hours, and we had the same file mislabelled
in our own tree.

A **prototype** dump also circulates (`HwVr 0x000b0011`, `Mod# M8976`, blank `HwId`). It will **not**
boot a pristine firmware partition. It was this project's first dump, and the recipe that paired it
with a hand-modified drive has been removed rather than explained.

**Apple no longer serves these IPSWs**, so there is no official source to try.

## Running it

**Open it and drop your files on it.** From a
[release](https://github.com/siggifly/ipod-emulator/releases), unpack it and double-click
**`ipod-emulator.app`** on macOS, or run `ipod-emulator` anywhere. With nothing configured it opens
on one screen asking for your two files — **drop them anywhere on the window, in any order**. Each
one is identified by what it contains rather than by which box you put it in, so there is nothing to
get the wrong way round, and an `.ipsw` builds the drive for you as it lands. **Choose…** opens a
file dialog that takes both at once.

Each file gets a verdict saying what it *actually is*, which is how a 2 MiB dump gets told it is
somebody else's iPod instead of failing ninety seconds into a boot.

It remembers both, so you do this once.

```sh
cargo build --release          # or use a release build
./target/release/ipod-emulator   # a window; D shows the readout
```

Or straight from the repository, with no clone — the packages have to be named, because the
workspace root is a virtual manifest and `cargo install` will not guess:

```sh
cargo install --git https://github.com/siggifly/ipod-emulator ipod-gui eapp-loader eapp-inspect
```

`ipod-gui` is the crate; the binary it installs is `ipod-emulator`.

### From a terminal

The recipes use whatever the window was last pointed at, so once you have done the above they
need no arguments:

```sh
./target/release/ipod-boot retail            # the recipe every number in research/ is measured on
./target/release/ipod-boot retail --print    # compose the argv, run nothing
```

`--print` also says where each path came from — environment, the window, or repository default —
because a recipe with an input you cannot see in its command line is one you cannot check.
`FLASH=` and `DISK=` override, and `ipod-boot make-disk your.ipsw disk.img` builds a drive without
the window. `ipod-emulator --check-images --flash=… --disk=…` reports on a pair with no window at all.

`tools/ipod-boot/README.md` covers the command-line recipes, and `tools/ipod-film/` records the
panel to a PNG sequence or an mp4.

### Nothing here is signed with a certificate

Deliberately. Buying one to make a reverse-engineering tool look official is the wrong trade for
this project, and the source is right there to build.

The consequence is that the operating system refuses the first launch of anything you download.
**On macOS 15 and later the old right-click → Open shortcut no longer works**: open it, let it be
blocked, then go to **System Settings → Privacy & Security**, where a button offers to open it
anyway. Once. `xattr -dr com.apple.quarantine "ipod-emulator.app"` does the same from a terminal. On
Windows, SmartScreen shows **More info → Run anyway**.

Anything you build yourself is not quarantined and none of this happens.

## The window

A drawn iPod whose screen is the live framebuffer and whose wheel, buttons and hold switch drive the
machine. Vector geometry rather than a photograph, because the wheel needs angular hit testing across
96 detents and that wants real geometry. The panel is blitted at integer scale with nearest-neighbour
sampling, so what you see is what the co-processor holds and not an interpolation of it.

| user mode | with the readout |
|---|---|
| ![](docs/media/ipod-11-gui-user.png) | ![](docs/media/ipod-10-gui-debug.png) |

The iPod, the controls that belong to it, and one footer line. **`D` puts the readout over the
device** — instruction counts, both clocks, the wheel's state and the surface addresses — as a
corner overlay rather than a panel that changes the window's shape.

| | |
|---|---|
| arrows | scroll the wheel |
| Enter / Space | select |
| `M` `P` `,` `.` | menu · play · previous · next |
| `H` | hold switch |
| `S` | write a PNG and a PPM into `_out/` |
| `D` | show / hide the readout |
| `Esc` | leave the settings, or the help page |

**Power off** and **restart** are real, in every mode: the machine is dropped and re-entered at the
reset vector, not restored and pretended. `hold MENU+SELECT` and `hold PLAY` latch the two-thumb
gestures a single pointer cannot make.

Conditions that make a working emulator look broken get a line of their own, in every mode, because
the person who needs them is the one who does not know what a counter is: a machine that has halted
says so, a hold switch that is on says so, and a picture being drawn to the surface nobody is
looking at says that.

### Settings

**`settings…`** in the footer opens them, and **the iPod keeps running behind them**. Case colour,
the readout and the update check apply as you change them. Only the two files and where the iPod
writes need a restart, and when one of those changes the screen names it and offers the restart —
`Done` leaves it for the next launch instead. This used to end the machine on the way in, because
the settings screen and the first-run screen were the same screen.

Dropping a file on a running iPod opens the settings on the row it landed in, rather than changing
what boots next time without saying so.

### Where the iPod writes

**By default it writes to the drive image you gave it**, exactly as a real one writes to its own
disk — so your settings, your language and your music stay on it. Closing the window **parks the
machine**: RAM and a stamp naming the drive go down together, and the next launch resumes in about
three seconds instead of cold booting for seventy-five. If anything touched the drive in between —
iTunes, `make-disk`, a second window — the stamp no longer matches and it cold boots and says so.

**Work on a copy** in the settings is the other way: your image is never written to, at the cost of
a second copy of it — up to 8 GB where the filesystem cannot share blocks, which is most of Linux
and all of NTFS — and the iPod forgets what it wrote between launches. `--copy` and `--no-copy`
choose for one run.

## What works

- The boot chain, cold from address 0, including Apple's flash updater
- RTXC, 61 tasks, all 24 startup modules and all five startup phases
- The disk: ATA with bus-master DMA, both PP502x DMA controllers, RetailOS formatting its own volume
- The click wheel, 96 detents of absolute position, and the hold switch
- The display, through a co-processor transport derived from RetailOS's own parser
- The games built into RetailOS. Brick plays

### It is not only Apple's software

**Rockbox 4.0 boots here, to its main menu** — its own logo through the same co-processor
transport, then `Scanning disk…`, then the menu, over 2 393 ATA commands of it reading the volume.

<img src="docs/media/ipod-14-rockbox-menu.png" width="320" alt="Rockbox 4.0's main menu running on this emulator">

That is a volume **this project's own installer wrote** — `ipod-boot put-files`, 381 files — so the
theme, the icons and the 15 px font are all being read back off the emulated disk. It used to be a
picture of the same menu in Rockbox's built-in 8 px fallback, because the recipe's default drive is
a stock Apple one with no `.rockbox` on it at all; [research/06](research/06-rockbox-as-oracle.md)
has the measurement and the reproduction.

A second, source-available operating system on this hardware model is the reason
[research/06](research/06-rockbox-as-oracle.md) exists — RetailOS is stripped C++ with no symbols,
Rockbox ships an ELF with 5 808 of them — and it has already earned it. **Three device models here
turned out to be shaped around Apple's drivers rather than around the parts**: a USB clock-ready
bit Apple's firmware never reads, an ADC that completed after a number of *transfers* rather than
after *time*, and a click wheel that only delivers input to firmware speaking Apple's own opcode.
None of the three is findable with one operating system.

**And the wheel drives it** — the third bug above is fixed, so the menu selection moves:

<img src="docs/media/ipod-15-rockbox-wheel.gif" width="320" alt="Rockbox's menu selection moving under wheel input">

Not finished: nothing past the menu is verified, there is no sound, and it is not yet something the
window can start for you — that is [on the roadmap](ROADMAP.md).

## What does not

- **No audio.** The Wolfson codec is unmodelled
- **~30 % of real time headless, ~19 % with the window.** About 21 M instructions/sec against an
  80 MHz ARM7TDMI, and around 14 M once a frame is being drawn. The window reports the figure it is
  actually achieving, whether or not the readout is up
- **No USB inside the emulator**
- **Purchased titles do not launch.** Apple's DRM refuses them; the identity it binds to is understood, the keystore is not
- **Four values in the co-processor transport are chosen rather than measured**, and there is no timing model at all, so a bug that only appears when a reply is late is invisible
- **The boot takes ~300 seconds of simulated time** where hardware takes five or ten. Something waits far longer than it should

Three lists, kept apart on purpose. **`KNOWN-BUGS.md`** is what is *wrong*. The section above is
what is *absent*. **`research/04-bypass-ledger.md`** is what is *faked*, with a written condition
for retiring each one — and nothing is faked without a row in it. Merging them is how a project
starts describing its gaps as choices.

**`CHANGELOG.md`** is what changed between releases, and **`RELEASING.md`** is how one is cut —
including the one edit that sets the version, and the check that proves nothing else holds a copy.

## Roadmap

1. **The simulated-time gap in the boot** — ~300 seconds of simulated time where hardware takes
   five or ten. That is the long white screen, and it is a bug rather than slowness: the
   interpreter's ~30 % of real-time accounts for a factor of three, not thirty
2. Audio
3. A JIT. The interpreter decodes every instruction every time; a JIT would be worth 10–50× here
4. The GPIO interrupt, so hold reaches the OS
5. Retiring the last four assumptions in the co-processor transport
6. **A Homebrew tap**, so `brew install` works. A formula rather than a cask, deliberately: it
   builds on your machine, and a binary built locally is never quarantined — so the Gatekeeper
   dance above stops applying to anyone who installs it that way
7. **Every non-iOS iPod.** This models the 5.5G (PortalPlayer PP5021C). The end goal is the whole
   clickwheel line, including the Classic — Samsung S5L8702, encrypted firmware, a different chip
   family and closer to a second project than a port

## How it was built

Four days, day by day, in `docs/HOW-IT-WAS-BUILT.md` — taken from the commit log rather than memory,
because memory was wrong about several of them.

## The research

`research/` is the larger half of this project: 20-odd documents, and the record of what was believed
and why it was wrong is deliberately preserved rather than tidied away. Retractions are made in
place. `research/04` is the bypass ledger, `research/11` documents the co-processor's runtime, and
`research/12` describes how RetailOS draws.

## Credit

None of this would exist without other people's work, and some of it would have taken months longer.
In rough order of how much this project owes them:

- **[Rockbox](https://git.rockbox.org/)** — the largest debt by a distance. `pp5020.h` and the iPod
  target code are where most of the register semantics came from: the PP502x memory map, the click
  wheel frame format, the co-processor's addresses, the PCF50605 register map. If you want to
  understand this hardware, read Rockbox first. It was also the oracle — a known-good OS to boot
  when something broke and you needed to know whether it was you.
- **[iPodLinux](http://www.ipodlinux.org/)** — the older layer beneath Rockbox, and still the only
  source for things like the MMAP window encoding and the `sysinfo_t` layout.
- **`dreamlayers`**, on the Rockbox forums in **2009**, who identified `vmcs.bin` and the `.vll`
  files as ELF DLLs loaded into the Broadcom chip, with the extraction recipe. This project worked
  that out independently in 2026 and then found the post. Sixteen years early.
- **[Olsro's Clickwheel Games Preservation Project](https://github.com/Olsro/ipodclickwheelgamespreservationproject)**
  — the reason this project exists at all, and the authority on the games and their authorisation.
- **[daniel5151/clicky](https://github.com/daniel5151/clicky)** — a 4G/PP5020 emulator that
  independently needed the same two undocumented register bits, found by a different method on a
  different SoC revision. That agreement arrived at a point where I was not sure of myself.
- **[freemyipod](https://freemyipod.org/)** and **q3k's [wInd3x](https://github.com/freemyipod/wInd3x)
  writeup** — different silicon, but the *oracle test* described there is a method this project
  stole outright and used repeatedly.
- **[devos50/qemu-ios](https://github.com/devos50/qemu-ios)**,
  **[giek2000/ipod-classic-firmware-research](https://github.com/giek2000/ipod-classic-firmware-research)**,
  **[Xlinka/iPodReverseEngineering](https://github.com/Xlinka/iPodReverseEngineering)**,
  **[dstaley/ipod-sysinfo](https://github.com/dstaley/ipod-sysinfo)**.
- **[raspberrypi/userland](https://github.com/raspberrypi/userland)** — DispmanX, two chip
  generations later, which is what made the display tractable.
- **Broadcom**, for leaving the BCM2722 product brief public, and **Alphamosaic**, whose patents
  disclose the VideoCore architecture.
- **EE Times**, whose report on the Wedbush Morgan teardown is the only published bill of materials
  for this board — it is where the part numbers came from.
- **[theapplewiki](https://theapplewiki.com/)**, for the model-to-hardware tables: which model
  number is which generation, and what silicon is inside it. Sorting the 5.5G from the Classic in
  the first place started there.
- **[Ghidra](https://ghidra-sre.org/)**, and **[GhidraMCP](https://github.com/bethington/ghidra-mcp)**
  for putting it a query away instead of a window away.

### What got me started

Two projects that had nothing to do with iPods and everything to do with attempting this:

- **The Raspberry Pi classic Mac emulators** — the small builds where a Pi hides inside a case and
  boots System 7 like it never left. The idea that you can keep a dead machine usable by rebuilding
  the parts that wore out, rather than hunting for the originals.
- **[Tahoe 26.5's kernel running natively on a Galaxy A55](https://www.reddit.com/r/hackintosh/comments/1virmsv/tahoe_265_kernel_running_on_a_galaxy_a55_natively/)**
  — someone taking a modern macOS kernel and getting it to run on a phone it was never meant to
  touch. It was posted three days before I started this, and it is most of the reason I did: the
  thing between you and a project like that is mostly whether you decide to begin.

### If you want to give something back

**Give it to them, not to me.** Rockbox in particular has been maintained for over twenty years by
people who documented this hardware so that anyone could use it, and every one of them did it before
there was an LLM to do the typing. Olsro's preservation project is the reason the games can be played
at all.

If you still want to throw something at this one: **[ko-fi.com/siggifly](https://ko-fi.com/siggifly)**.
It goes on parts for **oPod**, the open player this is meant to run on when the hardware runs out,
on coffee, and on the tokens that write the code. There is no obligation and the work continues
either way.

## Part of a wider effort

The iPod Preservation Project. Other arms live in their own repositories and are not public yet:
running games without RetailOS at all, presenting a virtual iPod to iTunes, and **oPod**, an open
player to run this on when the hardware runs out.

## Who wrote this

**I did not write a single line of code in this project.** It was written with Claude Opus 5 under
direction over four days. What I did was steer: decide what was worth chasing, push back when an
answer sounded too convenient, find the prior art that unstuck it, and say "that can't be right, look
again". That isn't nothing, and it also isn't writing an emulator. I would rather say so than let
anyone assume otherwise.

## Licence

GPL-3.0-or-later for the code, CC BY-SA 4.0 for `research/`. See `docs/LICENSING.md`. Apple's
firmware, NOR dumps, IPSW files and disk images are not covered, not distributed, and never will be.
