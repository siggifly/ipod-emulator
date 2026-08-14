# ipod-emulator

**Apple's retail iPod 5.5G firmware boots here, from the reset vector, on an emulator written from
scratch. It formats its own filesystem, reads the click wheel, draws its own menus, and runs a game.**

![cold boot through to a game](docs/media/ipod-01-boot-to-brick.gif)

The iPod Video 5.5G shipped on 12 September 2006. Twenty years next month, its firmware runs on a
machine that does not exist. It is also the model I owned — my first Apple product, at twelve. That
this is the one that ended up emulated was not deliberate, and I liked it more than I expected to.

Not a reimplementation of the interface. Apple's own code the whole way: the bootloader brings up
SDRAM, talks to the PCF50605 power chip over I²C, uploads firmware to the video co-processor, reads
the partition table, DMAs 7.5 MB of RetailOS into memory, checksums it and jumps. RetailOS then
remaps memory, starts its RTXC kernel and 61 tasks, mounts a FAT12 volume out of the firmware
partition, formats and populates its own FAT32 volume, spins the drive down, and draws.

| | |
|---|---|
| ![](docs/media/ipod-07-apple-logo.png) | ![](docs/media/ipod-03-main-menu.png) |
| ![](docs/media/ipod-05-games-list.png) | ![](docs/media/ipod-06-brick.png) |

## What you have to supply

Two files, from an iPod you own. **Neither is distributed here and neither ever will be.**

| | What to look for | |
|---|---|---|
| **NOR flash dump** | `internal_rom_000000-0FFFFF.bin` | Exactly 1 048 576 bytes. The name is the convention, after the offset range it covers — `000000`–`0FFFFF` is the whole megabyte. Any name works; the size and the reset vector are what get checked |
| **IPSW** | `*.ipsw` — Apple's own bundle, about 14 MB | The filename varies and does not tell you which iPod it is for. `ipod-boot make-disk your.ipsw disk.img` builds a fresh drive from it |

**The IPSW's updater family must match the iPod your NOR dump came from.** The family is inside the
bundle, not in its filename — `make-disk` reads it out of the manifest (`Firmware-25.1.2` → family
25) and prints it, because a mismatch does not fail loudly: RetailOS boots, does not recognise the
drive as its own, and asks to be restored from iTunes, after roughly 70 ATA commands instead of 600.

`ipod-gui --check-images --flash=… --disk=…` reports on both files before any boot is attempted.

Without them the emulator starts, tells you what is missing, and does nothing else.

## Running it

From a [release](https://github.com/siggifly/ipod-emulator/releases): unpack it, and on macOS
double-click **`iPod 5G.app`**. The command-line tools sit beside it.

Or from source, which works on macOS, Linux and Windows:

```sh
cargo build --release
./target/release/ipod-boot make-disk your.ipsw disk.img
./target/release/ipod-gui              # a window; D toggles debug mode
```

`tools/ipod-boot/README.md` covers the command-line recipes, and `tools/ipod-film/` records the
panel to a PNG sequence or an mp4.

### Nothing here is signed with a certificate

Deliberately. Buying one to make a reverse-engineering tool look official is the wrong trade for
this project, and the source is right there to build.

The consequence is that the operating system refuses the first launch of anything you download.
**On macOS 15 and later the old right-click → Open shortcut no longer works**: open it, let it be
blocked, then go to **System Settings → Privacy & Security**, where a button offers to open it
anyway. Once. `xattr -dr com.apple.quarantine "iPod 5G.app"` does the same from a terminal. On
Windows, SmartScreen shows **More info → Run anyway**.

Anything you build yourself is not quarantined and none of this happens.

## The window

A drawn iPod whose screen is the live framebuffer and whose wheel, buttons and hold switch drive the
machine. Vector geometry rather than a photograph, because the wheel needs angular hit testing across
96 detents and that wants real geometry. The panel is blitted at integer scale with nearest-neighbour
sampling, so what you see is what the co-processor holds and not an interpolation of it.

| user mode | debug mode |
|---|---|
| ![](docs/media/ipod-11-gui-user.png) | ![](docs/media/ipod-10-gui-debug.png) |

**`D` toggles between them.** User mode is the iPod and nothing else. Debug adds instruction counts,
both clocks, the wheel's state, the surface addresses, and a *does the input reach RetailOS?* panel
carrying arrival counts at the real addresses — so the window proves its own claim rather than
asserting it.

| | |
|---|---|
| arrows | scroll the wheel |
| Enter / Space | select |
| `M` `P` `,` `.` | menu · play · previous · next |
| `H` | hold switch |
| `S` | write a PNG and a PPM into `_out/` |
| `D` | user ⇄ debug |

**Power off** and **power cycle — cold boot** are real: the machine is dropped and re-entered at the
reset vector, not restored and pretended. `hold MENU+SELECT` and `hold PLAY` deliver the buttons, and
the panel says plainly that nothing in RetailOS has been measured to act on either — on a real 5G
that pair is caught by the wheel controller or the PMU, and neither is modelled here.

It restores a snapshot of the booted machine in about 3 seconds, or cold boots in 75.

## What works

- The boot chain, cold from address 0, including Apple's flash updater
- RTXC, 61 tasks, all 24 startup modules and all five startup phases
- The disk: ATA with bus-master DMA, both PP502x DMA controllers, RetailOS formatting its own volume
- The click wheel, 96 detents of absolute position, and the hold switch
- The display, through a co-processor transport derived from RetailOS's own parser
- The games built into RetailOS. Brick plays

## What does not

- **No audio.** The Wolfson codec is unmodelled
- **~30 % of real time headless, ~19 % with the window.** About 21 M instructions/sec against an
  80 MHz ARM7TDMI, and around 14 M once a frame is being drawn. The window reports the figure it is
  actually achieving, in both modes
- **No USB inside the emulator**
- **Purchased titles do not launch.** Apple's DRM refuses them; the identity it binds to is understood, the keystore is not
- **Four values in the co-processor transport are chosen rather than measured**, and there is no timing model at all, so a bug that only appears when a reply is late is invisible
- **Hold does not reach RetailOS after boot.** The line is right and is read four times, all before instruction 49 689 152; what is missing is a GPIO interrupt, which this emulator does not model
- **The boot takes ~300 seconds of simulated time** where hardware takes five or ten. Something waits far longer than it should

`research/12-bypass-ledger.md` is the full list of what is faked, with a written condition for
retiring each one. Nothing is faked without a row in it.

## Roadmap

1. Audio
2. A JIT. The interpreter decodes every instruction every time; a JIT would be worth 10–50× here
3. The GPIO interrupt, so hold reaches the OS
4. The simulated-time gap in the boot
5. Retiring the last four assumptions in the co-processor transport
6. **Every non-iOS iPod.** This models the 5.5G (PortalPlayer PP5021C). The end goal is the whole
   clickwheel line, including the Classic — Samsung S5L8702, encrypted firmware, a different chip
   family and closer to a second project than a port

## How it was built

Four days, day by day, in `docs/HOW-IT-WAS-BUILT.md` — taken from the commit log rather than memory,
because memory was wrong about several of them.

## The research

`research/` is the larger half of this project: 20-odd documents, and the record of what was believed
and why it was wrong is deliberately preserved rather than tidied away. Retractions are made in
place. `research/12` is the bypass ledger, `research/21` documents the co-processor's runtime, and
`research/22` describes how RetailOS draws.

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
- **EE Times** and the Wedbush Morgan teardown, for the only published bill of materials for this
  board. **The Internet Archive**, for the NOR dumps. **theapplewiki**.
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
