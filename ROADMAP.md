# Roadmap — what this becomes, and in what order

`KNOWN-BUGS.md` is what is wrong. `CHANGELOG.md` is what happened. This is what is intended, why in
this order, and what would settle each one. Written 2026-08-17, after 0.4.0.

**1.0 is not an empty ledger.** It is: *any supported device boots from files a user can legitimately
obtain, plays its games with sound, at real time, without reading a paragraph first.* That is a
statement about the experience, not about the bypass count — several bypasses can outlive it.

---

## The two modes

The emulator has one mode today and should have two.

| | needs from you | what it is |
|---|---|---|
| **Boot the OS** | a boot ROM + Apple's firmware | the whole device: menus, settings, the built-in games |
| **Run a title directly** | one decrypted game | the game, immediately, with no Apple OS in the loop |

The second is the older goal and the reason this project started. It needs the framework the games
call — around 25 functions carry all rendering across twenty titles — implemented natively instead
of executed out of RetailOS. The **runtime for that belongs in this repository**, because it is our
code and it is a mode of this program. The research behind it does not; that split is by what can be
published, not by subject.

## Ⅰ. The boot-time gap · *next*

~300 seconds of simulated time to reach the menu, against five or ten on hardware. The interpreter's
~30 % of real speed explains a factor of three, not thirty. Something waits far longer than it
should.

It is first because it blocks two other things. **Audio needs real timing** — samples produced at
the wrong rate stutter, so a clock that is thirty times slow is not a foundation to build on. And
the first thing anyone meets is a minute of white screen.

**Settled by:** a cold boot reaching the language picker in a simulated time of the same order as
hardware, without the instruction count falling.

## Ⅱ. Audio · *and it is more tractable than it looks*

The display was solved by not emulating the co-processor: RetailOS contains the parser for the
stream it sends, and the surface is memory we can read. **Audio has the same shape.**

The I²S DMA reads PCM out of a buffer in SDRAM. So the Wolfson codec does not have to be modelled at
all — service the DMA, collect the samples, hand them to the host. Sample rate and format come from
the I²C writes RetailOS makes to configure the codec, which are readable. *Do not emulate the chip;
capture what it would have been sent.*

The hard part is not the samples. It is timing, which is why Ⅰ comes first.

**Settled by:** Brick's sounds, at the right pitch, in time.

## Ⅲ. A synthesised boot ROM · *the biggest usability win available*

Finding a NOR dump is the hardest step in using this program. It is archived under the wrong product
name, and the alternative is owning an iPod and running Rockbox on it.

The bootloader's job is knowable and largely known: bring up SDRAM, talk to the PMU, upload `vmcs`
to the co-processor, read the partition table, DMA `osos`, checksum it, jump — leaving a `sysinfo_t`
block whose layout is documented. **We can synthesise that state and enter `osos` directly**, with
our own code, freely distributable. A user would then need only Apple's firmware bundle.

Two honest costs. It is a **large bypass**: every number in `research/` is measured through a real
boot and a synthesised one is not the same machine, so it ships as a labelled mode, never the
default for measurement, with a row in the ledger. And it is the mechanism that would let a machine
claim *any* identity, so it synthesises a neutral one — presenting somebody else's is not a feature
this builds.

**Settled by:** RetailOS reaching the menu with no NOR dump supplied, and the ledger row saying
exactly what was assumed.

## Ⅳ. The rest of the line

Everything here assumes one device. `FB_W`/`FB_H` were constants until 0.4.0 and are a `Device`
struct now — case, screen, wheel and framebuffer in millimetres — which is the first step and the
only one taken.

What remains: physical dimensions for each model, a picker, and **identifying the device from the
dump you hand it**. That last one is not decoration. The first bug report this project received was
somebody pairing a prototype ROM with a pristine firmware partition and getting
`Bootloader could not execute target image!` an hour later. A setup screen that recognises what it
has been given says so in a sentence.

Clickwheel models, all PortalPlayer or Samsung: 1G–4G, mini, Video (5/5.5G), Classic (6/6.5/7G),
nano 1–5G. The nano 6G dropped the wheel and the 7G has a touchscreen, so both need a different
input model before they mean anything. Shuffle has no screen.

**A device drawn in the picker is a promise.** Each one appears when it boots, not before.

## Ⅴ. Speed

Measured: ~21 M instructions/sec, about 30 % of an 80 MHz ARM7TDMI. Everything else is estimated
from that one point:

| | interpreted | with the framework hosted | with a JIT |
|---|---|---|---|
| Apple silicon laptop | ~30 % | plausibly real-time | far past it |
| x86 laptop | ~20–25 % | near real-time | past it |
| Raspberry Pi 5 | ~10 % | ~30 % | real-time |

Hosting the framework removes RetailOS's own instructions and nothing else, so **it reaches real
time only if RetailOS is ~70 % or more of the instruction stream during gameplay.** That is one
`--profile` run away from being known rather than guessed, because a title loads at `0x18000000`
and RetailOS at `0x10000000`. **Measure it before building either.**

A JIT is the larger win and the larger project. It is after audio because a silent emulator at full
speed is worth less than a slow one that plays.

## Ⅵ. Native ports

Static recompilation — machine code lifted to IR and compiled, no human reading it. The titles are
0.15–0.81 MB and the import boundary is already explicit, so this is far more tractable than the
decompilation projects people compare it to. It needs the framework from mode two, which is why it
is last.

Ship the recompiler, never its output.

## Not on this list

**An SDK for new games is buildable the moment the framework is** — but homebrew will not run on
unmodified firmware, because RetailOS validates a signature chaining to Apple's certificate
authority. On a real 5G that needs a patched `OSOS`, which is possible because its bootloader checks
a checksum rather than a signature. Under mode two it needs nothing, because there is no Apple OS to
satisfy. So *Doom on an emulated iPod* and *Doom on your iPod* are separated by one firmware patch,
and only the second is anyone else's decision.
