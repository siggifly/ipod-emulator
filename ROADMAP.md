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
| The click wheel delivers frames only after opcode `0x052a` | That opcode is *RetailOS's* way of asking. Rockbox arms the receiver its own way and gets silence |

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
| Rockbox 4.0 | **boots to its main menu** (2026-08-18). Takes no input — the click wheel answers Apple's protocol only. No sound |
| iPodLinux | **a verified 5.5G-correct kernel has been located** (ZeroSlackr `vmlinux`, 1 531 200 bytes, raw-ARM magic confirmed) and not yet fetched. `ipodloader2` is vendored as source and not yet built |
| Our own bootloader / OS | not started, and deliberately not designed for yet |
| Bypass ledger | 5 active entries: #4, #6, #7, #11, #17 |
| Audio | **nothing modelled.** The Wolfson codec answers no I²C; Rockbox is now talking to it and getting silence back |
| USB | nothing modelled beyond a clock-ready bit |
| Titles | purchased/decrypted games do not launch — the identity Apple's DRM binds to is understood, the keystore is not |

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

## M1 · Rockbox all the way through *(next)*

It reaches the menu and then powers off, and **the cause is measured, not guessed**: our click
wheel posts frames only when `reporting` is set, and `reporting` is set by opcode `0x052a` —
*RetailOS's* way of asking for autonomous frames (`lib.rs:4801`). Rockbox arms the receiver its own
way and is handed silence: **0 frames posted, 0 reads of `CLICKWHEEL_DATA`**, and it writes
`0x7000c104` once where RetailOS re-arms continuously. With no input, its 10-minute idle poweroff
arrives on schedule — honestly, because this emulator fast-forwards idle time (1 332 s of skipped
sleep inside 11 s of execution).

So the wheel has to answer the part's protocol rather than Apple's driver. That is the third model
in two days shaped that way, after the USB clock-ready bit and the ADC.

**Depends on:** nothing. **Settled by:** driving Rockbox's menus from the window and reaching its
file browser on a real volume.

## M2 · The oracle instrument

Running several stacks is not the instrument. The instrument is a **register-agreement table**: for
every MMIO address, which implementations touch it, and do they agree.

Half of it exists — `--input-regs` already enumerates addresses read before ever written, which is
literally the list of places this emulator invents. Run it per stack and diff, and the four-way
split falls out mechanically:

- touched by all, same expectation → **hardware behaviour**
- touched by one → **OS-specific**
- read-before-written by all, invented by us → **an emulator assumption, ranked by how many stacks
  depend on it**

**Depends on:** M1 (a second stack that runs long enough to touch things).
**Settled by:** a generated table in `research/`, and at least one ledger row moved because of it.

## M3 · Install an OS, don't warm-enter one

`trace --osos=FILE --boot-osos` skips the bootloader. Making that a normal path would mean every
observation afterwards is missing what the bootloader established — a seventh bypass, and the end
of the controlled comparison M2 depends on.

The route that is not a bypass: write the OS into a **new** drive image's firmware partition and
cold boot it, exactly as `ipodpatcher` does on real hardware. It lands on the window's existing
content-routed drop — a `.ipod` file carries a checkable `ipvd` checksum, so it is identification
rather than a guess.

**Depends on:** M1. **Settled by:** dropping `rockbox.ipod` on the window and cold booting it.

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

The older goal. Around 25 functions carry all rendering across twenty titles; implemented natively,
a decrypted game runs with no Apple code in the loop. Separately, the DRM question: the identity
Apple binds to is understood, the keystore is not.

Open and being investigated: whether RetailOS's **built-in** games are the same eApp container as
the downloadable ones, or are woven into the OS.

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
