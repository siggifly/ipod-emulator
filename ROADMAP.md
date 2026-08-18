# Roadmap — what this becomes, and in what order

`KNOWN-BUGS.md` is what is wrong. `CHANGELOG.md` is what happened. `research/04-bypass-ledger.md`
is what is faked. **This is what is intended, why in this order, and what would settle each one.**

Restructured 2026-08-18, when a second operating system booted here for the first time.

## What this project is

Not "an emulator that runs RetailOS". **A reconstruction of the iPod 5.5G platform, accurate enough
that independent software stacks boot on it** — Apple's, Rockbox's, iPodLinux's, and eventually our
own — *and* a controlled environment for writing new ones.

That framing is not decoration; it is the measurement strategy. One stack can only ever tell you
that your emulator satisfies *that stack*. Two disagree, and the disagreement names the hardware.

**This paid for itself the day it was adopted.** Rockbox found, inside two days, **three** device
models that Apple's firmware can never exercise:

| what was wrong | why RetailOS could not find it |
|---|---|
| A USB clock-ready bit that never arrived | Apple's firmware reads `0x70000028` **zero** times in a 600 M boot |
| The ADC completed after two *read transfers* rather than after *time* | Apple's driver polls, so its own poll loop supplied the transfers. Rockbox reads once and never polls — so it never completed a conversion, read 0 mV, and powered the machine off |
| The click wheel delivered frames only after opcode `0x052a` | That opcode is *RetailOS's* way of asking. Rockbox arms the receiver its own way and got silence — 0 frames in a whole boot |

All three are the same shape, and it is the shape to watch for: a model that is not so much wrong
as **shaped around one driver**. No amount of care with a single stack finds those — the stack that
shaped the model is the stack that cannot fail against it.

See [`docs/ideas/run-any-os.md`](docs/ideas/run-any-os.md) for the architecture this implies —
three slots (boot ROM · storage · what is in the firmware partition) and **one entry point**, the
reset vector. Multiple OSes through one entry is a controlled experiment; multiple entries is a
pile of anecdotes.

## What 1.0 means

**1.0 is the 5.5G, complete.** Not an empty ledger for its own sake — a device that behaves like
the device. Checkable, so it cannot be declared by mood:

| | condition |
|---|---|
| **Apple** | RetailOS boots, all menus work, **music plays**, video plays, the built-in games run, settings persist |
| **Rockbox** | boots to its menu, **plays audio**, its plugins run |
| **iPodLinux** | boots to a usable shell or podzilla |
| **Acquisition** | a user with **no NOR dump** can run it |
| **Time** | a cold boot takes the same order of wall time as the hardware did |
| **Fidelity** | the bypass ledger has **no active entries** |
| **Titles** | a decrypted game runs directly, with no Apple OS in the loop |

Anything about other iPod models is **after** 1.0. Breadth before depth is how emulators end up
with six devices that all half-work.

## Where we are

| | state |
|---|---|
| RetailOS | **boots** cold from the reset vector, menus, formats its own volume, Brick plays. No sound. ~300 s of simulated time to the menu |
| Rockbox 4.0 | **boots to its main menu and takes wheel input** (2026-08-18). Beyond the menu: unverified. No sound |
| iPodLinux | **a verified 5.5G-correct kernel has been located** (ZeroSlackr `vmlinux`, 1 531 200 bytes, raw-ARM magic confirmed) and not yet fetched. `ipodloader2` is vendored as source and not yet built |
| Installing an OS | **done** — `install-os` for the firmware partition, `put-files` for the FAT32 volume; the whole chain cold-boots |
| Our own bootloader / OS | not started, and deliberately not designed for yet |
| Bypass ledger | 5 active entries: #4, #6, #7, #11, #17 — and #7 now has a second consumer to be tested against |
| Audio | **nothing modelled.** The Wolfson codec answers no I²C; Rockbox is now talking to it and getting silence back |
| USB | nothing modelled beyond a clock-ready bit — and `disk` (target disk mode) faults after 128 K instructions |
| Titles | purchased/decrypted games do not launch — the identity Apple's DRM binds to is understood, the keystore is not |

## The machine's other modes

The NOR carries four bootable images besides the OS, and Apple's updater is a fifth mode on the
drive. They are what a real iPod does when you hold a chord at power-on, and they belong here
rather than in a footnote. Measured 2026-08-18 with `ipod-boot flsh` (`IMG=diag|disk|scan|logo`)
and `ipod-boot flash-update`:

| mode | what it is | state |
|---|---|---|
| **`aupd`** — flash updater | Apple's firmware updater, run from the drive on first boot | **works.** It runs, it takes, and it retired bypass #12 by itself ([research/07](research/07-the-flash-images.md)) |
| **`diag`** — diagnostics | the hold-`SELECT`+`REW` service menu | **executes** a full 200 M budget without faulting. [research/07](research/07-the-flash-images.md) records it booting and then waiting to be talked to |
| **`scan`** | disk scan | executes to budget, no fault |
| **`logo`** | the boot logo image | executes to budget, no fault |
| **`disk`** — target disk mode | the "do not disconnect" USB mass-storage mode | **broken.** `Lost(0xe19b0000) after 127 952 instructions` — it runs off into data at `0x18` with registers full of instruction words |

**None of the four draws anything at `0x000e0000`**, and that is reported as a measurement rather
than a verdict: it is the surface RetailOS and Rockbox use, and these images may not use it. An
absence at one address is not an absence.

**`disk` is the one that matters and the one that is broken.** Target disk mode is what a person
actually does with an iPod — it is how the drive gets mounted on a computer — so it is the natural
companion to **M8 (USB)** and is scheduled with it. The other three are cheap to re-check whenever
the co-processor or the boot path moves.

## The two modes
## The two modes

The emulator has one mode today and should have two.

| | needs from you | what it is |
|---|---|---|
| **Boot the OS** | a boot ROM + Apple's firmware | the whole device: menus, settings, the built-in games |
| **Run a title directly** | one decrypted game | the game, immediately, with no Apple OS in the loop |

The second is the older goal and the reason this project started. It needs the framework the games
call — around 25 functions carry all rendering across twenty titles — implemented natively instead
of executed out of RetailOS. The **runtime for that belongs in this repository**, because it is our
code and it is a mode of this program. The research behind it does not; that split is by what can
be published, not by subject.

## On replacing firmware with our own code

Two separate ambitions get called the same thing, and keeping them apart matters:

- **Replacing the bootloader** (M5) is an *acquisition* win. It deletes the hardest step in using
  this program — finding a NOR dump — and costs no fidelity, because the bootloader's job is
  finished before RetailOS gets control.
- **Replacing the OS under a game** (M9) is a *different program*: the title runs with no Apple
  code in the loop at all.

**Neither is a shortcut around understanding.** You can only replace what you have measured — a
synthesised boot ROM has to leave behind exactly the machine state the real one leaves, and that
state is only knowable by having watched the real one do it. Every layer we take over is paid for
first in the ledger. Where that ordering was violated, it produced the removed prototype
bootloader: a thing that worked with a hand-modified drive for reasons nobody could state.

---

# Milestones

Each says what it unlocks, what it depends on, and what would settle it.

## M1 · Rockbox all the way through *(in progress)*

**Done:** it boots, reaches its main menu, and **takes wheel input** — 78 frames delivered, 78 reads
of `CLICKWHEEL_DATA`, and the selection moves. The blocker was the third model in two days shaped
around Apple's driver rather than around the part: autonomous frames were gated on opcode `0x052a`,
which is *RetailOS's* way of asking and which Rockbox never sends. Reporting is on at reset now, and
the gate is the pair (reporting AND an armed receiver) that the interrupt was always gated on.

`select` opens the file browser, and it **lists a real volume**: the listing looked empty until a
one-byte positive control (clear the hidden attribute on `iPod_Control` in a *copy* of the image)
made it appear — every entry at that volume's root is hidden, and `filetree.c:352` skips hidden
entries. Rockbox mounts our FAT32 through the emulated ATA and filters correctly.

**Left:** the same thing driven from the window rather than by script, and plugins — which need
`/.rockbox/rocks/` on the volume and therefore wait on M3. The idle poweroff is honest rather than a
defect (this emulator fast-forwards idle time: 1 332 s of skipped sleep inside 11 s of execution),
but a person at the window would not see it, so it wants confirming interactively.

**Depends on:** nothing. **Settled by:** driving Rockbox's menus from the window and reaching its
file browser on a real volume.

## M2 · The oracle instrument *(built; now a standing job)*

**Built 2026-08-18** — [research/15](research/15-the-register-agreement-table.md). Same instrument
on both stacks, diffed: **93 addresses, 18 both, 56 RetailOS-only, 19 Rockbox-only**, each named
from Rockbox's own `pp5020.h`.

It found something on its first run that no amount of RetailOS work could have: **`MBX_MSG_STAT`
(`0x60001000`) is read 52 868 892 times by Rockbox and never by RetailOS**, first from
`switch_thread` — Rockbox's scheduler leaning on a CPU↔COP mailbox this emulator does not model at
all, at the rate of a spin loop. It sits directly under every timing claim that could be made about
Rockbox here.

It also gave ledger **#7** its second consumer: `COP_CTL` is read by Rockbox and not by RetailOS, so
`--cop-awake` finally has something other than Apple's firmware to be tested against.

**Left:** regenerate it whenever a stack is added or a device model changes — that is the R4 rule
(*a new instrument's first job is to re-run the conclusions the old one produced*) applied to a
table instead of a flag. And the standing caveat: it reports reads-before-writes, so it is a floor
on where we invent, never a ceiling.

## M3 · Install an OS, don't warm-enter one *(done)*

`trace --osos=FILE --boot-osos` skips the bootloader. Making that a normal path would mean every
observation afterwards is missing what the bootloader established — a seventh bypass, and the end
of the controlled comparison M2 depends on.

**Done 2026-08-18.** `ipod-boot install-os SRC.img OS.ipod OUT.img` writes the OS into a **new**
drive image's firmware partition exactly as `ipodpatcher` does — append after `osos`, move the
directory's entry point to it, fix the checksum, shift the later images — and the machine's own
cold path runs it. The Rockbox bootloader boots this way and reports our hardware back to us:
`IPOD version: 0x000B0005`, `Emulated iPod Disk`, `Partition 1: 0x0C 16744448 sectors`. It never
touches the source image; Apple's `osos` cannot be re-downloaded.

The installer **reproduces the checksums already in the directory before writing new ones** and
refuses if it cannot — added because the first attempt did not, and produced an image the
bootloader rejected after 71 ATA commands with *"Use iTunes to restore"*.

The data partition is the other half, and `ipod-boot put-files DISK.img SRC_DIR` does it — a
FAT32 writer with long-name support, because the two paths it was written for are `.rockbox`
(leading dot) and `rockbox.ipod` (four-character extension) and neither fits 8.3. 381 files in
1.7 s, verified against an independent reader.

**So the whole chain runs**: Apple's boot ROM → Apple's bootloader → the Rockbox bootloader in the
firmware partition → `rockbox.ipod` off the FAT32 volume → Rockbox's splash. Nothing warm-entered,
no step skipped.

**Left:** the cold-booted Rockbox powers off after its splash, from
`handle_auto_poweroff+0x64` — the `query_force_shutdown` branch — where the warm-entered one goes
to `+0xb8`, the idle branch. Four explanations are already eliminated by measurement
([research/06](research/06-rockbox-as-oracle.md)). Also left: doing all of this from the window as
a content-routed drop, and a machine list that shows what each drive holds.

## M4 · iPodLinux

A third stack, and the one most likely to disagree with the other two.

The 5.5G's notorious ATA problem — the 80 GB drive's 1024-byte physical sectors — **does not apply
here**, because in an emulator we supply the drive. Two concrete local blockers do: our drive
images use MBR partition type `0x0C` where `ipodloader2` handles only `0x0B` (real iPods use
`0x0B`), and no kernel image exists locally.

A milestone is reachable with local material alone: build `loader.bin` from the vendored source,
install it into a copy of `OSOS.bin`, cold boot, and get the loader's own menu. That proves the
whole chain except the kernel.

**Depends on:** M3. **Settled by:** the ipodloader2 menu rendering; then a kernel booting.

## M5 · A synthesised boot ROM · *the biggest usability win available*

Finding a NOR dump is the hardest step in using this program, and the **first outside report**
(issue #2) is exactly someone stuck on it.

The bootloader's job is knowable and largely known: bring up SDRAM, talk to the PMU, upload `vmcs`
to the co-processor, read the partition table, DMA `osos`, checksum it, jump — leaving a `sysinfo_t`
block whose layout is documented. We can synthesise that state with our own freely distributable
code. A user would then need only Apple's firmware bundle.

**Depends on:** the ledger being honest about what the real bootloader leaves behind — so M2 helps
directly. **Settled by:** RetailOS reaching the menu with no NOR dump supplied.

## M6 · Audio

Nothing is modelled. Rockbox is already writing to the Wolfson codec at I²C `0x1a` and getting
silence, which makes this the largest single gap between "boots" and "is the device".

Three pieces: the **WM8758 codec** over I²C (registers only — Rockbox's `wm8985.c` is a readable
specification), the **I²S transport**, and the **DMA** feeding it. The hard part is timing, not
samples, which is why M7 sits next to it.

**Depends on:** M7 in practice. **Settled by:** Brick's sounds, at the right pitch, in time — and
Rockbox playing a file.

## M7 · The boot-time gap and real-time speed

~300 seconds of simulated time to reach the menu against five or ten on hardware. The interpreter's
~30 % of real speed explains a factor of three, not thirty: something waits far longer than it
should, and that is a bug rather than slowness.

**Settled by:** a cold boot reaching the language picker in a simulated time of the same order as
the hardware, and the window reporting ≥ 100 % of real time.

## M8 · USB, and the ledger to zero

USB unlocks disk mode and restore — the two things a user does to a real iPod. The remaining ledger
entries (#4, #6, #7, #11, #17) close here, #6 (the video co-processor's `vmcs` firmware) being the
largest and the one M2 is most likely to reframe.

**Settled by:** `research/04-bypass-ledger.md` with an empty active section.

## M9 · Titles without Apple's OS

The older goal, and **now bounded** rather than open-ended. RetailOS publishes a self-describing,
content-hash-versioned framework surface at `0x000793fc`–`0x00079ce0` — **8 frameworks, 433
functions** (`OpenGLES` 179, `Metadata` 152, `Audio` 61, `AsyncFileIO` 17, `miscTBD` 15,
`Filesytem` 4, `Settings` 3, `InputEvents` 2). A title's dependency on it is enumerable *before any
code is written*: Pac-Man declares 98 of the 433, and its import hashes are byte-identical to the
ones in RetailOS's own table — the ABI confirmed from both sides at once.

So this is implementing a published interface against a known size, with a hash that says when we
have got it wrong. Around 25 functions carry all rendering across twenty titles, so the practical
first cut is far smaller than 433.

Separately, the DRM question: the identity Apple binds to is understood, the keystore is not.

**The built-in games are a different thing and not a shortcut into this.** Settled 2026-08-18: they
are *not* eApp containers — one `"eapp"` occurrence in the whole of `osos` and it is the loader's
own literal pool. They are plain compiled-in code with no container, no import table, no resources
of their own, sharing RetailOS's widget toolkit, display path, string pool, settings store, event
system and allocator. "Extracting" one means reimplementing the host. See
[research/13](research/13-do-the-games-load.md).

**Settled by:** a decrypted title running from the window with no `osos` loaded.

## M10 · 1.0

Everything in *What 1.0 means* satisfied, on the 5.5G, with the documentation and the release
process to match.

---

# After 1.0

**More devices.** The 5G, the nano, the Classic — each is a different SoC generation and a
different amount of work, and each benefits from every instrument built for the 5.5G. The device
table (`MODELS`) already separates "models we know about" from "models that boot", so the shape is
there.

**Our own bootloader, and our own OS.** Kept as intent rather than architecture: designing for it
now means designing around something that does not exist. Note that "multi-boot" is not a layer we
have to write — `ipodloader2` already is one; it is an OS to install (M3).

**Upstream.** Where we fix something that is genuinely upstream's — a Rockbox ROM dumper that
leaves a 0-byte file, an iPodLinux 5.5G gap — the fix should go there rather than stay here.

## Not on this list

- Other people's emulators, or comparisons to them
- A plugin API, a scripting layer, or anything else nobody has asked for
- Windows/Linux-specific UI work beyond what the one window already does
- Netplay, savestates-as-a-feature, achievements, shaders

## Parked ideas

Written down, deliberately not scheduled. Each says what it would take and why it is not next.

- [**Running an OS that is not RetailOS**](docs/ideas/run-any-os.md) — now M3, kept here for the
  design discussion behind it.
- [**Running from a real iPod in disk mode**](docs/ideas/run-from-a-real-ipod.md) — importing a
  device's firmware partition is 14 MB and easy; running *off* the device is the dangerous version,
  because RetailOS would format it. The safe form needs a copy-on-write layer in `Ata`, which is a
  change to the machine every number in `research/` was measured through.
