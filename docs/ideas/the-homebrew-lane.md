# The homebrew lane — what opens up, and what is worth building in it

**Status: an ideas note, 2026-09-06.** Nothing here is planned work. It exists because the lane is
about to be real and it is worth having thought about the destination before starting to walk.

---

## The lane exists, and it does not depend on the DRM

This is the part that is easy to miss. Running a **homebrew** eApp needs three things, and the
encrypted retail titles are on a different track entirely:

| | |
|---|---|
| the ABI | ✅ **known.** Container format, framework descriptor layout, and the 16-byte interface hashes — which are not secrets, they are constants sitting in every shipping game binary, and RetailOS matches them by value |
| firmware that will run unsigned code | ✅ **exists**, for 5G and 5.5G, and we hold both. It is a firmware partition image, so this emulator can boot it as-is |
| a build toolchain | ❌ **does not exist.** This is the whole gap |

And a patched firmware is mintable rather than merely obtainable: the image's only integrity check
is a **plain byte sum** (`research/07` — *"All five checksums verify as a plain byte sum"*). There is
no signature on the firmware image itself. Patch, re-sum, boot.

**So Snake on RetailOS does not wait on FairPlay.** The 116 owned encrypted titles are a separate
problem with a separate blocker, and nothing in this note touches them.

The honest caveat, from `clickwheel/research/eapp/03-sdk-feasibility.md`: *"No unsigned eApp has been
built or loaded. It is a reasoned expectation, not a demonstrated result, and the DRM check may not
be the only gate."* That is a one-afternoon experiment in this emulator, and until it is run this
whole note is conditional.

## The envelope, measured rather than guessed

| constraint | what it means |
|---|---|
| ARM7TDMI at ~80 MHz, **no FPU** | fixed point only — confirmed by the retail games' own use of `GL_FIXED` |
| 320 × 240 | fine for 2D, tight for anything else |
| `GL_PALETTE8_RGBA8_OES` | 256 colours per texture, 1024-byte palette |
| drawing is quads in screen space | it is a sprite blitter, not a 3D pipeline |
| **max 5 framework imports** per eApp | a hard structural limit in the loader |
| 32–64 MB RAM | generous for the era |
| ~25 of 433 functions | carry all rendering across all 20 titles examined |

Roughly **GBA-class compute with a better screen and more memory**. Vortex is the existence proof at
the top end — a flat-shaded tunnel shooter running real time on this hardware.

## Two things about this device that are worth designing *for*

Most ports treat the iPod's limits as damage to route around. Two of them are not limits at all any
more.

### 1. The click wheel is a paddle controller

This is the one that changes what is worth building. The wheel reports **absolute angular position**
— it is a one-turn potentiometer with detents, which is precisely what Atari's paddle was. Every
paddle game of 1976–1980 does not merely *port* to this device, it comes **home**: Breakout, Pong,
Super Breakout, Circus Atari, Kaboom!, Warlords, Night Driver, Avalanche, Arkanoid.

A touchscreen cannot do this. A gamepad cannot do this. A mouse approximates it badly, because a
mouse gives you a delta and a paddle gives you a position.

**And it is already proven here**: Brick — Apple's own descendant of the hidden Breakout easter egg
in the 2001 iPod — is playable in this emulator, driven by the wheel, verified against a
byte-identical inverse-gesture control. The genre works. Nobody has written a *new* one.

Beyond paddles, the wheel is the right input for anything one-dimensional and continuous: scrubbing
a long timeline, dialling a duration, tuning, picking from a very long ordered list. That last one is
what Apple built it for, and it is still the best list-navigation input anyone has shipped.

### 2. No radio, 60 GB, and fourteen hours of battery

In 2006 this was a limitation. In 2026 it is a product category — people pay real money for
distraction-free devices with exactly these properties. A 5.5G has no WiFi, no cellular, no
notifications, and enough storage for more text than a person can read in a lifetime.

That reframes the strongest software for it: **things that make offline-with-lots-of-storage a
virtue.**

## What would actually be worth building

Three that use what this device uniquely is, rather than apologising for what it is not.

**A real reader.** 320 × 240 is small but entirely usable for text, the wheel is the best page-turn
and chapter-scrub input ever fitted to a portable, and 30–80 GB holds essentially every book out of
copyright with room to spare. No iPod ever had a good one. This is the most obviously missing
application on the platform.

**Paddle games, written new.** See above. A wheel-native Breakout with modern level design would be
better on this hardware than on any device made since, and it is trivially inside the envelope.

**Spaced repetition with audio.** The iPod is *already* an audio device with a large library and a
database. Flashcards with a spoken side, graded on the wheel, is a genuinely strong language-learning
tool that nothing else quite is — and every expensive part of it (storage, audio playback, battery)
is the thing this device was built to do.

Others that fit without strain: an offline reference corpus (text-only Wikipedia compresses to
roughly 20 GB and would fit on the smallest 5G with room over); a focus or interval timer, where
dialling the duration is the whole interaction; a metronome; a sleep-sound generator; a habit log,
where dial-entry beats typing.

Worth naming what does **not** fit, so nobody designs into a wall: there is **no microphone** (5G
audio input is accessory-only), **no accelerometer**, **no GPS**, and **no network of any kind**.
Anything that records, senses motion, knows where it is, or syncs by itself is out. Maps can be
viewed; they cannot locate you.

For ports, the realistic band is 2D of any kind, puzzle and board games, sprite-based action, and
Doom-era 2.5D with effort. Unrealistic: anything leaning on floating point or textured 3D.

## What the toolchain actually is

Smaller than it sounds, and none of it is research:

1. **`arm-none-eabi-gcc`** — already in use in this project for `ipodloader2`, a bottled Homebrew
   formula.
2. **A linker script** placing code where the loader enters it, with the vector table at `+0x14`.
3. **A header** declaring the ~25 framework functions that carry rendering, with their 16-byte
   interface hashes — extracted from shipping binaries and reusable verbatim.
4. **A packer** producing the container: magic, version ceiling `0x10001000`, at most five framework
   descriptors (name(32) · hash(16) at `+0x20` · count at `+0x30` · thunks at `+0x38`).
5. **This emulator**, which is already most of the development environment — it loads eApps, resolves
   imports, traps every framework call with its arguments, disassembles, single-steps, keeps an
   execution-history ring, and renders to a framebuffer you can dump.

## The thing worth remembering

The iPodLinux wiki set out this exact goal and never reached it:

> *"It is most likely pure ARM asm, interfacing with an API provided by the RetailOS. **This is the
> API we hope to figure out, and code against for our own games in the future.** There is a huge
> homebrew community anxiously awaiting to do this."*

That page was last edited in **April 2007**. The API was never published.

## The first step, and it is small

Build the smallest possible unsigned eApp — one that fills the screen with a colour — patch a
firmware, and load it **in the emulator**. That settles the open question above with no hardware and
no risk to anybody's iPod. If it loads, hand a signed-by-nobody `.ipg` to somebody with a real 5.5G
and find out whether metal agrees.

Everything in this note is downstream of that one experiment.
