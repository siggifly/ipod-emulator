# Build log — the discovery record

The chronological account of how the eApp format, the framework ABI, and the rendering path were
worked out, including the wrong turns. Split out of `README.md` on 2026-08-11, when that file had
grown to 1 215 lines and could no longer answer "where are we and what is next".

**This file is the evidence and the reasoning.** For current status and the roadmap, see
[`../README.md`](../README.md).

Kept in full deliberately — several entries record claims that were later retracted, and the
retractions are the most useful part. The recurring failure mode across this project was
generalising from two confirming samples; the recurring fix was checking the whole population, or
reading the caller instead of the registers.

---


## What this is

The 54 clickwheel-era iPod games were delisted from iTunes in October 2011 and cannot be bought
again. They run only on original hardware, bound by DRM to one iPod's serial number. This project
asks whether they can be made to run **on macOS**, and secondarily inside the modern iPod build.

The goal is the games. It is *not* to reproduce RetailOS, and it is not a general iPod emulator.

---

## The one-paragraph summary

Emulating Apple's RetailOS is the obvious path and probably the wrong one: it is where the only
serious prior attempt stalled, and it buys almost nothing beyond the games themselves. The games are
separate ARM binaries that RetailOS *loads*, and — **confirmed 2026-08-11** — they load through a
**self-describing import table**: magic `eapp`, then a linked list of named framework blocks
(`Audio`, `AsyncFileIO`, `InputEvents`, `Settings`, `miscTBD`, `OpenGLES`) whose thunks the loader
patches at load time. So a bounded ABI genuinely exists, and the shim path is real. But it is
**~98 entry points plus an OpenGL ES 1.x-era surface**, not a handful — and 34 of the 54 titles are
**AES-128-CBC encrypted** with a key only a real iPod can unwrap. The cheapest route is therefore not
to *reimplement* the frameworks but to **host RetailOS's own implementations** under an ARM
interpreter, stubbing only the driver layer beneath them — and to treat hardware as a **one-time key
extraction step**, not an ongoing dependency.

---

## Locked decisions

| # | Decision | Date | Rationale |
|---|---|---|---|
| L1 | **The iPod UI is recreated from design references, never extracted from RetailOS** | 2026-08-10 | The UI is the most-photographed and least-coupled part of the device; RetailOS's is inseparable from its hardware assumptions. A recreation can also grow a Bluetooth screen — RetailOS structurally cannot. ⚠️ Reference the 5G's **Podium Sans**, don't ship it — it's an Apple typeface |
| L2 | **Emulation is scoped to games only.** It is a subsystem with a budget, not a competing project | 2026-08-10 | Of the four things firmware emulation was hoped to deliver — Bluetooth, modifiability, games, iTunes sync — it delivers **only the games**, and costs the other three. See [What emulation actually buys](#what-emulation-actually-buys) |
| L3 | **Bluetooth, WiFi and streaming live outside the emulation boundary** | 2026-08-10 | RetailOS has no network stack, no TCP/IP and no pairing UI, and adding them means writing subsystems into a closed ARM7 binary. Host-side BT audio (capture emulated I²S → BlueZ) works and needs zero firmware changes — but it's a host feature, with no on-wheel UI |
| L4 | ⚠️ **NEEDS AMENDMENT — see below.** Personal use, own hardware, own purchases. Nothing that lifts keys gets published | 2026-08-10 | The DRM binds to a device serial, so any working path touches FairPlay. Personal interoperability on hardware you own is the defensible posture; distributing circumvention is a different act with a different legal character. See [`safety-and-working-model.md`](safety-and-working-model.md) |
| L5 | **This is a sibling of 02, not a part of it** | 2026-08-10 | Different deliverable, different tech, different risk. It has standalone value on a Mac with no hardware built. Superseded an earlier call to file it as `02/research/08` |

### ⚠️ L4 is incompatible with the stated goal, as written

The 54 games were **delisted in October 2011 and cannot be bought.** "Own purchases" is unachievable
for anyone who did not buy them fifteen years ago, so L4 and *"all 54 playable"* cannot both hold.

The half of L4 doing the real legal work is **"nothing that lifts keys gets published"** — that is
the line between personal preservation and distributing circumvention, and it is unaffected. The
"own purchases" half is the part that needs rewording (toward personal preservation of commercially
unavailable software, nothing published).

**This is an operator decision and is deliberately left unresolved here.** It is open question #8.

---

## What emulation actually buys

The original framing was "if we can emulate the firmware, we get a fully modern iPod." Itemised, that
does not hold:

| Hoped-for | Verdict | Why |
|---|---|---|
| **Original games** | ✅ **Only via emulation** | The DAP is a Cortex-A53 and cannot run ARMv4T bare-metal firmware natively. There is no other route |
| Bluetooth / WiFi in the UI | ❌ | No network stack in RetailOS; adding one means authoring subsystems inside a stripped binary. Host-side audio routing works, UI integration does not |
| "Make changes to it" | ❌ Barely | Binary patching a closed ARM7 image. This is *why* Rockbox exists instead of a modded RetailOS |
| iTunes / Music sync | ➖ Already solved, more cheaply | `02`'s **Experiment A** (~$25, one afternoon) answers this with a USB gadget and no emulator |

So emulation earns its place for exactly one reason, and that reason is enough — but it sets the
scope tightly.

---

## Prior art

| Project | What it is | State |
|---|---|---|
| [**clicky**](https://github.com/daniel5151/clicky) | Rust clickwheel-iPod emulator, QEMU-style device model | ⚠️ **Weaker than it looks — corrected 2026-08-11.** Targets **iPod 4G grayscale, a model that cannot run clickwheel games at all** (5G+ only). **Corrected again 2026-08-12 by the operator, who follows its development: clicky does not boot RetailOS at all.** It boots **Rockbox**; iPodLinux does not yet boot; its author's order is Rockbox → iPodLinux → diagnostics → RetailOS. Its *code and documentation* are useful; its *status claims* are not. daniel5151's last commit was **2022-11**; all 2024–26 work is one contributor (`jeanthom`) doing PP5020 peripherals. **No issue mentions games.** Useful as reference, not as substrate |
| [**Xlinka/iPodReverseEngineering**](https://github.com/Xlinka/iPodReverseEngineering) | 2020 Ghidra writeup on the game DRM | **The single most important source for the crypto.** AES-128-CBC; key+IV recovered from RAM at launch; DRM check NOP'd at `00136B84` in 1.3 5.5-Enhanced. 5G/5.5G only — Classic and nano were never patched |
| [iPodLinux wiki `IPodGames`](https://web.archive.org/web/20220814041437/http://www.ipodlinux.org/IPodGames/) | 2006–07 `.ipg` format documentation | Frozen April 2007, domain dead — **Wayback only.** Documents the full archive layout. States the API goal explicitly and never achieved it: *"This is the API we hope to figure out"* |
| [**Clickwheel Games Preservation Project**](https://github.com/Olsro/ipodclickwheelgamespreservationproject) | Olsro's 54-game archive + a Windows 10 LTSC VM acting as an offline authorisation centre | Mature and working — **on real hardware only.** Explicitly does not emulate. The authoritative source on the DRM mechanics |
| [**freemyipod**](https://freemyipod.org/wiki/Main_Page) / [wInd3x](https://github.com/freemyipod/wInd3x) | Bootrom exploits, patched RetailOS | ⚠️ **Targets Nano 3G–7G and Classic 6G/7G — Samsung S5L silicon.** The 5G Video is PortalPlayer PP5021C. Much less transferable than it first looks |
| Rockbox / iPodLinux | 2007-era PP502x reverse engineering | The substrate everything else is built on. Still the best hardware documentation that exists |

---

## The DRM, precisely

From Olsro's project — these are the constraints any emulated path must satisfy:

- Games ship as **`.ipg` files**, which are **zip archives**, not monolithic encrypted blobs.
- After authorisation, binaries live in `iPod_Control/Games_RO`; saves in `GameData_RW`; stats in
  `GameStats_WO`.
- **`iPod_Control/iTunes/IC-Info.sidb`** holds the authorisation keys. It is "unique to your iPod and
  its current installation."
- **Binding is to the iPod's serial number.** Copying `iPod_Control` to another device does not work.
- iTunes allows 5 authorised accounts per device; the archive works around this by syncing one game
  per account to force key injection.
- iTunes 10.7+ tries to delete `IC-Info.sidb` on sync — the file must be locked.

### The encryption, in three layers (added 2026-08-11)

An earlier draft of this file left open whether the binary is *encrypted* or merely *checked*. It is
encrypted. Per [xLinka's Ghidra writeup](https://github.com/Xlinka/iPodReverseEngineering):

| Layer | What it is | Consequence |
|---|---|---|
| **Per-copy** | The `.bin` is **AES-128-CBC** encrypted. Every copy of a given game ships the *same* ciphertext; the unique part is the key in `.bin.sinf` (1032 bytes, FairPlay blocks `sinf/frma/schm/user/key/tran/name/priv/sign`) | The archive's ciphertext is fine — you need the key, not a better copy |
| **Per-account** | iTunes injects account keys into `IC-Info.sidb`. Olsro's exploit: iTunes ships *all* of an account's keys when *any one* of its games syncs | 5 accounts × their keys = a device that can decrypt the whole archive |
| **Per-device** | `IC-Info.sidb` is bound to the iPod's serial (and "sometimes a currently unknown second value"). A full disk clone **plus a ROM serial swap** has been shown to move signed games between devices | Keys cannot be lifted by copying files |

**Nobody has publicly derived the iPod's internal key.** Every source says so. But that does not
block us: the 2007 crackers never derived it either — they ran the game on a modified `OSOS` and
**read the unwrapped key out of RAM**. Recovery, not derivation, is the path. See
[Extraction](#extraction-how-the-34-get-unlocked).

### What this project does about the DRM

**Nothing.** Purchased titles do not launch here, and this emulator neither ships, bundles, nor
depends on a decrypted copy of any title. The authorisation mechanics above are recorded because
they explain what the firmware is doing — `research/12` still owes a row for every point where this
emulator diverges from the hardware, and the DRM is one of them.

The games that do run are the ones **built into RetailOS itself** — Brick and its siblings, which
are part of Apple's firmware and arrive with the `osos` image rather than as separate purchases.

Olsro's [Clickwheel Games Preservation Project](https://github.com/Olsro/ipodclickwheelgamespreservationproject)
is the authority on the titles and their authorisation, and it works on real hardware. If you want
to play a title you bought, that is the project to use.

---

## The eApp ABI

**Answered 2026-08-11.** The games do *not* need RetailOS booted. They load through a bounded,
self-describing import table.

> ⚠️ **Evidence status: OBSERVED, NOT YET REPRODUCED.** The layout below came from analysing the
> publicly archived plaintext Pac-Man binary. It is not in any cited source — the iPodLinux wiki
> wanted this and never got it; freemyipod names the segment (`dram.frameworks`, *"'Framework' system
> of some kind, interfaces used by eApps"*) but never documents its contents. **Reproducing this
> independently is step 1**, per the evidence discipline in
> [`safety-and-working-model.md`](safety-and-working-model.md).

Header at offset 0 of the `.bin`:

```
+0x00  "eapp"          magic
+0x04  0x10001000      version
+0x08  0x00000005      count of framework import blocks
+0x0c  0x00000028      entry offset  (0x28 holds `eafffffe` = b .)
+0x10  0x1800002c  ->  "OpenGLES"    (load base 0x18000000)
```

Then a linked list of framework-import blocks — each: magic `29 06 19 68`, NUL-terminated name, a
16-byte MD5-shaped interface hash, a function count, a next pointer, then that many `ldr pc, [pc, #N]`
PLT-style thunks the loader patches at load time. For Pac-Man:

| Framework | Functions |
|---|---|
| `Audio` | 61 |
| `AsyncFileIO` | 17 |
| `miscTBD` | 15 |
| `Settings` | 3 |
| `InputEvents` | 2 |
| `OpenGLES` | header-referenced |

**≈98 imports plus a GL ES 1.x-era surface.** The list terminator is a joke block named
`$$$$ a^n + b^n = c^n | n>2 $$$$` with guid `d41d8cd98f00b204e9800998ecf8427e` (MD5 of the empty
string). `miscTBD` is *literally* the name freemyipod recorded — that segment **is** this table.

### The eApp loader, found

**2026-08-11.** Scanning OSOS for the `eapp` magic returned exactly one hit — at file offset
`0x126B08`, runtime address **`0x10122708`**. It is not an image header. It is a **literal pool**,
sitting in the middle of ARM code, holding the constants the loader compares against:

```
0x126B08:  65 61 70 70   "eapp"
0x126B0C:  00 10 00 10   0x10001000   ← the version field, confirmed
0x126B10:  68 19 06 29   0x29061968   ← framework-block magic
0x126B14:  8c ec 81 10   0x1081ec8c
0x126B18:  73 19 06 13   0x13061973   ← a sibling magic, previously unknown
```

**This is better than what we went looking for.** The built-in games turned out not to be separate
eApps — but RetailOS's *own eApp loader* is the authoritative definition of the format.

✅ **Disassembled 2026-08-11 — `research/02-eapp-loader.md`*(moved to the `ipod-games` repository)*.**
Validation function at `0x101224C4`. Headlines:

- The header layout is **confirmed from Apple's code**, with two corrections: the version field is a
  **ceiling** (`≤ 0x10001000`), not an equality — which is how one firmware runs titles built across
  three years — and the pointer array starts at **`+0x14`**, not `+0x10`. Count is capped at **5**,
  which is why Pac-Man declares 5: it is at the maximum.
- The block magic comparison confirms **`0x29061968`** at block `+0x00`, settling the byte order
  against Apple's own `cmp`.
- **Frameworks bind by a 16-byte interface hash, not by name.** The loader `memcmp`s 16 bytes from
  descriptor `+0x20` against a system table of 20-byte entries and takes the pointer at `entry+0x10`.
  The ABI is therefore **explicitly versioned by content hash** — and `eapp-loader` currently resolves
  positionally, which is a real behavioural gap to close.
- Error codes recovered: `-1` for a bad header, `-1001` for a system-table magic mismatch.

#### Correction: the block magic byte order was wrong

The only prior report of this constant had the bytes as `29 06 19 68`. RetailOS holds **`68 19 06 29`**
— u32 `0x29061968`. Read as dates, `0x29061968` and its sibling `0x13061973` are 29/06/1968 and
13/06/1973; that reading only works in this order, which is strong corroboration on top of the
firmware being ground truth.

`eapp-inspect` and `eapp-loader` are corrected, and **both scan for either order** — only a real game
binary settles which appears in the wild, and quietly searching for the wrong bytes is
indistinguishable from "this file has no frameworks."

---

## Per-model coverage

Operator question, 2026-08-11: *some games were only released for specific iPods — what does that
mean for us?* It is the right question, and it may be the one that decides whether "all 54" is one
project or two.

Per [Olsro's project](https://github.com/Olsro/ipodclickwheelgamespreservationproject), the archive
splits into an **"All iPods"** set (~40 titles, nano 3G/4G/5G and Classic 6G+) and a **"5G and 5.5G
only"** set of **14 iPod Video exclusives** — Vortex (+ demo), PAC-MAN, Ms. PAC-MAN (+ demo), Tetris,
Zuma, Sudoku, Mahjong, Mini Golf, Cubis 2, LOST, musika, Royal Solitaire, Texas Hold'em.

**If the 5G is genuinely the superset, our single-target plan is right and nothing changes.** If it
is not — if some later titles (Asphalt 4, Phase, Spore Origins, Crystal Defenders) shipped only for
nano 3G+ / Classic — then "all 54" needs a **second emulator**, because those are Samsung S5L parts
with a real GPU, not PortalPlayer, and their framework ABI may differ in version as well as
implementation. That is a different project wearing the same name.

**Two things point at the 5G being the right target regardless:**

- Olsro records that *"the iPod Videos always uses liter games binaries to accomodate to its less
  horse-power"*, and that Asphalt 4's radar is "generic" on Video versus "generated dynamically"
  elsewhere. So 5G builds are the **simplest** builds — less to emulate, software rendering rather
  than GPU.
- The 5G is the only generation with a documented DRM patch and documented PP502x hardware.

### ✅ Settled 2026-08-11 — the 5G is the superset

Inventory of the `Platforms` array in all **56** `.ipg` archives:

| Set | Titles | PlatformIDs present |
|---|---|---|
| "All iPods" | 49 | `1,2,3` — nine of them also `4` |
| "iPod Videos (5G and 5.5G only)" | 7 | **`1` only** |

**Every title, without exception, ships a `PlatformID 1` build** — including the ones that looked
like the risk cases (Asphalt 4, Phase, Spore Origins, Crystal Defenders, Sonic, Song Summoner).

The mapping is confirmed by a clean natural experiment rather than assumed: the 7 titles Olsro
independently labels *"5G and 5.5G only"* are exactly the 7 whose only platform is `1`. Filename
fields corroborate — `molly_1_1_3028618.bin` (musika) parses as `PlatformID 1, HardwareID 1`.

**Consequence: one emulator, one target, all 54.** No second core for S5L, no GPU work, no ABI
versioning across generations. And per Olsro the 5G builds are the "liter" ones — so the universal
target is also the cheapest to emulate. This was the largest strategic risk in the project and it
resolved the good way.

*(Nano/Classic firmware is therefore no longer needed for coverage. It retains only minor value as
an independent check on ABI stability.)*

### The consequence: host the frameworks *first*

98 functions with guessed semantics, a 61-call audio API and a GL ES implementation is a large,
open-ended write. But RetailOS already contains all of it, as ARM code, and **RetailOS is not
per-device** — every 5G runs the same image.

So the leading architecture is: **load RetailOS's `dram.frameworks` into an ARM interpreter, resolve
the game's import thunks against it, and stub only the driver layer beneath.** That collapses the
problem twice — you never run RetailOS's init (so you never hit the undocumented-PP5020 wall that
stopped clicky — see the correction in the prior-art table), and you never guess a single function's semantics. The surface you actually
implement drops to LCD, audio DMA, wheel and storage — the one layer Rockbox documented thoroughly
for PP502x.

**Hosting is the starting point, not the end state** — reimplementation is what unlocks everything
downstream, and the hosted originals are the oracle you validate it against. See
[Host the frameworks first](#host-the-frameworks-first--then-reimplement-using-the-host-as-the-oracle).

### Performance

Unchanged and still secondary: an ~80 MHz ARM7TDMI interpreted on a Pi Zero 2 W is roughly parity at
best, so the DAP target likely wants a dynarec. The Mac target does not, and nothing here blocks on
it.

---

## Target platforms

Operator ambition (2026-08-11): an app with an **on-screen click wheel** plus the emulator —
macOS first, ideally sideloadable to iPhone, ideally on the App Store with users supplying their own
games.

| Target | Verdict | Why |
|---|---|---|
| **macOS** | ✅ **Do this first** | No gatekeeper, no review, no entitlement limits. Every architectural question can be answered here |
| **iOS, sideloaded** | ✅ Plausible | AltStore or a personal dev cert (7-day re-sign). No review to pass |
| **iOS, EU alt-marketplace** | ⚠️ **Probably not available to you** | The DMA is an **EU** instrument and Apple's alternative-marketplace rollout was scoped to the 27 member states. **Iceland is EEA/EFTA, not EU.** Verify before counting on it |
| **App Store** | ⚠️ **Genuinely uncertain — don't design around it, don't foreclose it** | See the reassessment below |

### App Store — reassessed 2026-08-11 (an earlier "realistically no" here was overstated)

Operator's argument: *if Apple permits retro game console emulators, why not one for Apple's own
retro game console?* On the rules, that argument mostly holds:

| Objection | Holds? |
|---|---|
| "An iPod isn't a game console" | ❌ **Weak.** First-party platform, 54 commercially-sold titles, dedicated store section |
| "It needs Apple's copyrighted firmware" | ❌ **Wrong.** BYO-BIOS is the *standard accepted model* — PS1 emulators requiring a user-supplied SCPH BIOS have shipped. Not disqualifying |
| "The games are Apple-sold FairPlay media" | ⚠️ **Partly.** But ROM sourcing is the user's problem for every other emulator too |

What actually remains is **not a categorical barrier but discretionary risk with a conflicted
decision-maker**: Apple is the rights holder, the emulated hardware vendor, the seller that delisted
the games, *and* the reviewer. That is a business risk, not a rules problem.

**Posture: take on no architecture debt for the App Store, foreclose nothing.** Sideloading works
regardless and needs no claims made to anyone. If there is ever something to submit, submit it
honestly — a clean rejection costs a resubmission and leaves every other path intact, whereas
misrepresentation at review risks developer-account termination and would take the *sideload* path
down with it.

### The one decision this forces *now*

**iOS forbids JIT for non-browser apps** (no `W^X` exemption without the browser-engine entitlement).
So if iPhone is ever a target, the core must be a **solid interpreter, not a dynarec.**

That is affordable: an ~80 MHz ARM7TDMI on an A-series core has enormous headroom — the constraint
that made a dynarec look necessary was the **Pi Zero 2 W** in `02`, not the phone. So the honest
ordering is: interpreter everywhere, and a dynarec *only* if the DAP target ever demands it.
This supersedes the framing in Q6.

### The wheel is already solved — reuse the model, not the protocol

[`02/research/03-clickwheel-protocol.md`](../02-modern-ipod-dap/research/03-clickwheel-protocol.md)
has what a touchscreen wheel needs, and it is **not** the electrical layer:

- **96 absolute positions per revolution**, plus a finger-down bit
- Rockbox's acceleration model: velocity in deg/s, **1/16 EMA**, reset on direction reversal,
  **250 ms** repeat timeout, **150 ms** finger-lift grace so brief contact loss doesn't kill inertia
- `02`'s **L13** haptic finding transfers directly: **hearing resolves ~51 clicks/s, touch does not.**
  Cap the haptic channel around 20/s. On iPhone that maps onto Core Haptics cleanly

An on-screen wheel that implements *that* will feel right. One that just reports finger angle will not.

---

## Build plan

Operator-confirmed sequencing (2026-08-11): **built-in games → the 20 plaintext → all 54.** Each
stage produces something that runs, and each stage's corpus is strictly less encumbered than the next.

| Stage | Deliverable | Needs | Gate |
|---|---|---|---|
| **B0** | ✅ **ARMv4T interpreter core** — [`tools/arm7tdmi/`](tools/arm7tdmi/) | Nothing | **Done 2026-08-11 — 59 tests green, incl. differential fuzz** |
| **B1** | ✅ **eApp loader** — [`tools/eapp-loader/`](tools/eapp-loader/) | **Done 2026-08-11 — real Pac-Man executes and traces** | ✅ ordered call trace, with arguments |
| **B2** | ✅ **First frame** — [geometry rasterised from the game's own vertex data](#-b2-complete--geometry-on-screen) | B1 | ✅ **a picture** |
| **B3** | **Playable** — input (wheel model) + audio | B2 | Brick is playable |
| **B4** | **The 20** | B3 + plaintext corpus | Titles run unmodified |
| **B5** | **The 34** | An authorised 5G | 54/54 |

### B0 — shipped

`tools/arm7tdmi/` — Rust, zero dependencies, `#![forbid(unsafe_code)]`, 54 tests, clippy clean.
Full ARM-state and Thumb-state decode and execute, banked registers for all seven modes, exception
entry and return.

The tests are the point — they encode the details that are silently wrong in a naive core:

- **`R15` pipeline displacement** — reads as `addr+8` in ARM, `addr+4` in Thumb, and `addr+12` for a
  register-specified shift operand. Getting the third case wrong corrupts results with no crash
- **Barrel-shifter zero encodings** — `LSR #0` means `#32`, `ASR #0` means `#32`, `ROR #0` means `RRX`
- **Register shifts of exactly 32 and beyond**, which differ from each other in the carry flag
- **Misaligned `LDR` rotates rather than faulting** — architectural on this part, and real code uses it
- **C means "no borrow"** on subtract; C and V are independent and each has a test that sets one
  without the other
- **`LDM`/`STM` always map the lowest register to the lowest address**, whichever direction the
  transfer runs

One real bug was caught by these during bring-up: leaving an IRQ/SVC/ABT/UND excursion wrote `r13`/`r14`
into the User bank, clobbering the User/System stack pointer. Only shared `r8`–`r12` belong there.

**Differential fuzz against an independent oracle** (`tests/differential.rs`, 5 tests × 20 000
iterations). Hand-written tests share the blind spots of whoever wrote the core, so every expected
value here is computed in **wider integer types** instead: carry-out is "did the `u64` sum exceed 32
bits", signed overflow is "did the `i64` sum leave `i32` range", and shifts are evaluated in `u64`/`i64`
where shifting by exactly 32 is well-defined — precisely the case a 32-bit implementation gets wrong.
A separate test asserts ARM and Thumb agree on operations both can express, which catches drift
between the two decoders without needing to know which one is wrong.

Unicorn was the intended oracle and was rejected on evidence: its vendored QEMU does not compile
under clang 21 (`int128.h` typedef collision, implicit `mprotect` declaration). Widening arithmetic
turned out to be the better oracle anyway — no dependency, and correct by construction rather than
by a second implementation also happening to be right.

**Deliberately not implemented** (none blocks B1, all are recorded rather than forgotten): cycle
counting and timing · CP15 (takes the undefined-instruction vector) · prefetch/data aborts, since
`Bus` cannot fault yet. `LDRSH` from an odd address follows ARM7 silicon (sign-extends the byte)
rather than the architecture's "unpredictable".

### B1 — built, awaiting a real binary

`tools/eapp-loader/` — 9 tests, clippy clean. Parses the eApp header, derives the load base,
discovers framework blocks and their thunks, maps the image alongside a RAM region, and runs the
entry point on the B0 core.

**Import resolution is faithful, not simulated.** Each import is a `ldr pc, [pc, #imm]` thunk whose
literal RetailOS patches at load time; the loader patches *the same slot*, with an address in a
region containing no code. When the game calls the import, the thunk loads our address into `PC`
exactly as it would load the real one — the CPU special-cases nothing. Traps record framework,
function index, `r0`–`r3` and the return address, then resume the caller.

Two properties the tests pin down, both chosen so failures are loud rather than silent:

- **`PC` leaving mapped memory is reported with its address**, not absorbed. A missing stub should
  name itself.
- **Unmapped data accesses are recorded**, not silently zeroed — at this stage an unmapped read *is*
  the finding, because it means the game expects something we have not modelled.

### ✅ B1 closed — the measured ABI surface

**20 plaintext eApps**, and the complete import surface is now measured rather than estimated:

| Framework | Games using it | Functions | Notes |
|---|---|---|---|
| **`OpenGLES`** | **20** | **179** | The **primary** framework — see below |
| `Metadata` | 17 | 152 | Largest Apple-specific surface; likely not needed to *run* |
| `Audio` | 20 | 61 | |
| `AsyncFileIO` | 20 | 17 | |
| `miscTBD` | 20 | 15 | Allocator lives here |
| `Filesytem` *(Apple's own typo)* | 3 | 4 | |
| `Settings` | 20 | 3 | |
| `InputEvents` | 20 | 2 | |

**Every count is identical across every game that uses it** — min equals max, without exception.
The ABI is **fixed, not versioned**: one implementation serves all 20.

Three import profiles exist, and they are strictly nested:

| Total | Composition |
|---|---|
| **277** | the universal six |
| **429** | + `Metadata` |
| **433** | + `Filesytem` |

**433 is the true ceiling**, not the 98 estimated earlier — because that estimate missed
`OpenGLES` entirely.

### `OpenGLES` is the primary framework, and it is invisible to a scan

It carries **no block magic.** It hangs off header `+0x10` as a bare descriptor, which is exactly
why RetailOS resolves it separately from the block loop — and why a magic-scanning parser cannot
see it. It was missed until an **unpatched thunk at the image start branched to zero**, and the
execution history showed the call was `(0, 0, 0, 1.0f)` — an unmistakable `glClearColor`.

Its descriptor is byte-identical in layout to a block's, minus the 4-byte magic. Same name buffer,
same hash at `+0x20`, same count at `+0x30`, same thunks at `+0x38`. Declared count `0xB3` = **179**,
matching the 179 thunks counted independently at the image start.

**This is the best-shaped 179 functions we could have been handed.** OpenGL ES 1.x is a *standard,
documented API* — it is a mapping exercise onto known semantics, not reverse engineering. And the 5G
has no GPU, so it is software-rendered and the semantics are stock GL ES 1.0/1.1. The genuinely
unknown surface is `433 − 179 = 254` Apple-specific functions, of which 152 are `Metadata` and
probably deferrable.

### Three layout errors the real binaries exposed

Each was silent, and each would have cost days later:

1. **The block name is a fixed 32-byte buffer**, not NUL-terminated-then-aligned. The parser was
   reading hash and count out of the name's zero padding and reporting `count = 0` for all 140
   blocks across 20 games. The corrected offsets — name `+0x04`, hash `+0x24`, count `+0x34`,
   thunks `+0x3C` — match RetailOS's validator exactly, which is what made the fix trustworthy
   rather than merely different.
2. **The entry point is not at `+0x0C`.** That field points at `b .`, a self-branch trap; the loader
   ran two million instructions and made zero calls. The real entry is the **absolute pointer at
   `+0x14`**, and RetailOS corroborates — its validator `memcpy`s `count * 4` bytes from `+0x14`
   and never reads `+0x0C`.
3. **There is no BSS in the file.** Pac-Man's first act is ~58 000 writes above the image, clearing
   its own globals. Mapping only the image made that vanish into unmapped space.

### The trace, and how imports get identified

Driving all five vectors in turn on one machine:

```
vector[0] 0x1801ec30 -> Returned  (+4 calls)      init
vector[1] 0x1801ec2c -> Returned  (+0 calls)      bx lr — a no-op hook
vector[4] 0x1801ec8c -> Lost(0)   (+2 calls)      the real work
```

Identified so far, **by behaviour rather than by name**:

| Import | Evidence | Verdict |
|---|---|---|
| `miscTBD #0` | sizes in, result dereferenced; Ms. Pac-Man calls it 10× with `0x7ff80` | **allocator** — implemented |
| `miscTBD #1` | same pointers back, in **exact reverse order** | **free** (LIFO) |
| `Settings #0` | `r0` points at the image; the strings read **`"Language"`** and **`"TimeFormat12"`** | **`getSetting(name, out)`** — conclusive |
| `Audio #52` | any non-zero return removes a divide-by-zero | **a divisor** — rate or channel count |
| `InputEvents #0` | `r0`, `r1` are adjacent *stack* addresses | **input poll**, two out-params |
| `miscTBD #9` | `(count, buffer)` | takes a count and a buffer |

Ms. Pac-Man's startup is legible from the trace alone: it allocates ten 512 KB blocks, then frees
all ten in reverse — **a RAM-size probe**, done before anything else.

### Semihosting — the games narrate themselves

`Lost(8)` turned out to be `svc #0x123456` with `r0 = 3`: **ARM semihosting**, which these titles use
for debug logging. The loader now services `SYS_WRITEC` / `SYS_WRITE0` / `SYS_WRITE` / `SYS_EXIT`,
and five games immediately reported their own diagnosis:

```
Arithmetic exception: Divide By Zero
```

That is a far better guide than disassembly, and it is free — the instrumentation was already in the
shipped binaries.

### The sweep — identification is now mechanical

`trace` takes `--stub=Framework:index=value` and `--write=Framework:index:arg=value`, so a hypothesis
costs one command. Sweeping every reached import and diffing the call count isolates the responsible
one automatically:

```
CHANGED Audio:52  =1  calls=37->39  Returned
```

That single result fixed two titles — Ms. Pac-Man (37 → 39) and Texas Hold'em (**20 → 61**).

### `miscTBD #9` is a clock — and why the sweep could not find it

The last three holdouts (Lost, SimsBowling, SimsPool) resisted every constant. Deepening the history
ring to 512 entries showed why — 32 entries were entirely consumed by the game's own `putchar`
printing the error message one character at a time, hiding the actual fault.

Past the print loop, the cause is unambiguous:

```asm
180077b4  bl  0x18004e14        ; -> getTime(&out)
180077c0  str r0, [r7, #0x1c]   ; store now
180077c4  ldr r1, [r7, #0x18]   ; previous
180077c8  sub r5, r0, r1        ; delta
...
18007834  ldr r9, =0x000f4240   ; 1_000_000
18007838  mov r1, r5            ; divisor = delta
18007840  bl  0x18002ef4        ; __aeabi_uidiv
```

**A frames-per-second counter**: `1_000_000 / (now − last)`. Any stub returning a *constant* makes
the delta zero, so the sweep — which only tried fixed values — could never have found it. **The
value has to move.**

The thunk resolves to `miscTBD #9` (its thunks span `0xe68…0xea4`; `0xe8c` is index 9), which matches
the earlier Ms. Pac-Man trace where `#9` was handed a stack address. `Stub::Clock` now advances a
microsecond counter and reports through the out-pointer.

**Result: nothing crashes.** All 20 titles either return cleanly or run the full two-million
instruction budget. The three holdouts went from a divide-by-zero abort to sustained execution.

| Outcome | Titles |
|---|---|
| **BudgetExhausted** — running a real loop | Lost, SimsBowling, SimsPool, molly, testprep ×3 |
| **Returned** — init completes, awaiting a frame callback | the other 13 |

**Method note worth keeping:** a differential sweep over constants is powerful but structurally
blind to stateful stubs — clocks, counters, handles that must differ between calls. When a sweep
comes back empty, that absence is itself the clue.

---

## The frame loop — games are running

A title that "Returned" had not stalled: it finished a unit of work and expected to be called
again. RetailOS drives the last vector once per frame, and doing the same turns *init completed*
into **running**.

**Pac-Man sustains 500 frames** — 11 510 framework calls, a steady **23 calls per frame**, never
deviating. At 60 fps that is roughly eight seconds of gameplay.

The per-frame sequence is completely legible, and `OpenGLES #157` brackets it at both ends —
almost certainly the buffer swap:

```
OpenGLES #157          frame begin
miscTBD  #9            clock
InputEvents #0         poll input (two out-params)
OpenGLES #167 #35 #159 #40 #40    draw
Settings #0 ×2         read a setting
miscTBD  #0 ×9         allocate working buffers
Audio    #52 #51       audio tick
miscTBD  #6, #9
OpenGLES #157          frame end
```

### 10 of 20 titles sustain a 60-frame loop

| Sustaining | Calls/frame |
|---|---|
| Mahjong | 46 |
| vortex | 44 |
| Pac-Man | 23 |
| Minigolf | 22 |
| Zuma · Ms. Pac-Man · TWA | 16 |
| Bejeweled | 15 |
| Tetris | 12 |
| Cubis 2 | 10 |

The other ten exhaust the instruction budget inside their first frame — they are doing sustained
work, not faulting, and are the next thing to look at.

### `miscTBD #0` — allocator confirmed, with a pluggable out-of-memory handler

Every one of Pac-Man's nine per-frame calls returns to the **same** address, `0x1801e824`. The call
site settles the semantics:

```asm
1801e81c  mov r0, r4              ; size
1801e820  bl  0x18000b64          ; miscTBD #0
1801e824  cmp r0, #0x0
1801e828  beq 0x1801e804          ; zero -> the failure path
1801e82c  ldmia sp!, {r4, pc}     ; non-zero -> done

1801e804  bl  0x18001650          ; fetch state
1801e808  ldr r0, [r0, #0x34]     ; a per-title OOM handler pointer
1801e80c  add lr, pc, #0x8        ; return address = 0x1801e81c, i.e. RETRY
1801e810  cmp r0, #0x0
1801e814  ldreq r0, =0x18001c38   ; none installed -> the default
1801e818  mov pc, r0
```

So the shape is **`while (!alloc(size)) oom_handler();`** — a retry loop around a pluggable
low-memory callback. The default at `0x18001c38` prints **"Abnormal termination"** and issues
semihosting `SYS_EXIT`.

**The result is never dereferenced at this site — only tested against zero.** That is consistent
with allocation and does not on its own prove it, but the retry-plus-OOM-handler structure does:
nothing else is shaped like that.

**What remains genuinely unresolved** is where the memory goes back. `#1` (free) is not called
during the loop, so we leak ~52 KB per frame and exhaust 64 MB by frame 2 000 — at which point the
game prints *Abnormal termination* and exits, exactly as the disassembly predicts. On a 32 MB 5G a
title that truly leaked this fast would die in seconds, so **our model is missing a release path**,
not observing a real bug. Finding it is the prerequisite for indefinite runtime.

*(A previous note here speculated that `#0` might be a "memory available?" query rather than an
allocator. The call-site disassembly rules that out.)*

This is the loop that carries the rest: a stub that needs a real return value announces itself by
causing a null dereference. `Machine::recent()` keeps a 32-entry ring of executed addresses, so a
`Lost` is reported with the last dozen instructions disassembled — which is exactly how `OpenGLES`
was found. Each missing piece names itself; none has to be guessed in advance.

### All 20 titles execute

| Title | Imports | Calls | Result |
|---|---|---|---|
| vortex | 429 | **142** | Returned |
| TWA (Texas Hold'em?) | 433 | 78 | Returned |
| molly (musika) · testprep ×3 | 429 | 39 | BudgetExhausted |
| mspacman | 277 | 37 | Lost(8) |
| Lost · SimsBowling · SimsPool | 429 | 25–26 | Lost(8) |
| Tetris | 429 | 22 | Returned |
| HoldEm | 433 | 20 | Returned |
| Sudoku · Solitaire | 429 | 14 | Returned |
| Pacman · Mahjong · Zuma | 277–429 | 10 | Returned |
| Bejeweled · Cubis2 · Minigolf | 277–433 | 7–8 | Returned |

**Every one parses, maps, resolves its imports and executes real ARM code.** `BudgetExhausted` on
four titles is the *encouraging* outcome — they run two million instructions without faulting,
which means they are doing sustained work rather than bailing out.

`Lost(8)` on five titles is the next thread to pull: PC lands on `8`, the same returned-zero-used-as
-a-pointer signature, one indirection deeper.

### The interpreter decision (forced by iOS)

Unicorn is the obvious choice and is **wrong for this project**: it is QEMU TCG underneath, i.e. a
JIT, which iOS forbids outside browsers. Using it would mean a second core later anyway.

**Write the ARMv4T core in Rust.** It is genuinely small — one architecture revision, ARM + Thumb,
no MMU, no vector unit — it is the most testable component in the project, and it is the one piece
that must run identically on macOS, iOS and (eventually) the Pi. One core, everywhere, no JIT.

### Host the frameworks first — then reimplement, using the host as the oracle

These pull in opposite directions and the order matters:

- **Hosting** RetailOS's own framework code gets something running fastest and gives *exact*
  semantics for free.
- **Reimplementing** is what unlocks everything downstream — upscaling, Bluetooth, resolution
  changes — because you cannot intercept what you did not write.

So hosting is **not throwaway**: it is the reference implementation you diff your own against.
Host to get running and to *learn* the semantics by observation, then replace framework by framework,
validating each against the hosted original. That is a migration path, not a fork in the road.

### Later, explicitly deferred: AI upscaling

Operator interest (2026-08-11), parked as an option, not a goal.

The naive approach — ML-upscale the framebuffer at runtime — is the *worse* one. 320×240 is tiny, so
it would be fast on Apple Silicon, but it fights temporal artifacts and has to run every frame.

**The better approach is offline: upscale the assets once.** The formats are fully documented (see
research/01*(moved to the `ipod-games` repository)*)
— `.tga` spritesheets, `.ipd`, `.anm`, `.raw.lcd5`. Upscale the spritesheets with an ML model, have
the loader substitute them, pay zero runtime cost. This is the texture-pack model, and it produces
cleaner results than any per-frame filter.

⚠️ **The catch, and why this is recorded now rather than later:** the games blit at fixed coordinates
into a 320×240 framebuffer. Substituting 2× assets breaks that *unless you control the blit* — which
means intercepting `OpenGLES` and the drawing side of `miscTBD`. **You can only do that in the
reimplemented frameworks, never in the hosted ones.** So upscaling is downstream of the
host→reimplement migration above. It is not a bolt-on, and knowing that shapes which frameworks get
reimplemented first.

---

## B2 — the framebuffer pipeline is closed

**2026-08-11.** GL entry points identified from the enum values in their arguments, across all 20
titles — no name table required:

| Import | Evidence | Verdict |
|---|---|---|
| `OpenGLES #12` | `r0 = 0x4000` = `GL_COLOR_BUFFER_BIT` | **`glClear(mask)`** |
| `OpenGLES #13` | `r0 = 0x3f800000` = `1.0f` | **`glClearColor(r,g,b,a)`** |
| `OpenGLES #157` | first and last call of every frame | **buffer swap / present** |
| `OpenGLES #4` / `#19` | `0x0DE1` = `GL_TEXTURE_2D`, `0x84F5` = `GL_TEXTURE_RECTANGLE` | **`glEnable` / `glDisable`** |
| `OpenGLES #35` | `0x0B44` = `GL_CULL_FACE`, `0x0B71` = `GL_DEPTH_TEST`, out-pointer | a `glGet`-family query |
| `OpenGLES #45` | `(1, &out)` | `glGen*` |

Floats arrive as raw IEEE-754 bit patterns in `r0`–`r3`, because the EABI passes softfloat arguments
in the core registers — which is why `0x3f800000` is legible as `1.0f` directly in the trace.

**A 320×240 framebuffer is now wired to those three stubs**, and `--ppm=FILE` writes the result.

### The verification, and why the first version of it was worthless

Pac-Man's first rendered frame came back entirely black — which proves nothing, because the buffer
*starts* black. The test was rewritten to initialise the framebuffer to **magenta**, so that a black
result is evidence the clear actually ran rather than a coincidence of initial state:

| Title | Frames presented | `glClear` reached | Result |
|---|---|---|---|
| Vortex | 31 | **31** | all 76 800 pixels black |
| Mahjong | 31 | **31** | all 76 800 pixels black |
| Pac-Man | 31 | 1 | all 76 800 pixels black |

The magenta is gone, so the path from *game code* → *import thunk* → *stub* → *pixels* is closed
end-to-end. It is a black screen, but it is a **verified** black screen.

### The drawing calls — and the ES version, settled

Four-argument traces across all 20 titles produced two decisive constants:

- **`0x0000140C` = `GL_FIXED`**, passed by `#159` and `#40`. That is the 16.16 fixed-point vertex
  type — **present in ES 1.x, removed in ES 2.0.** It also fits the hardware: the PP5021C's
  ARM7TDMI has **no FPU**, so a fixed-point vertex path is the only sensible one.
  **This closes the ES 2.0 discrepancy flagged earlier** — the ES 2.0 symbols in the firmware
  belong to another component; the games call ES 1.x.
- **`0x43a00000` = `320.0f` and `0x43700000` = `240.0f`**, passed by `#167` alongside an image
  pointer. The screen dimensions, in the arguments, exactly as the panel specifies.

### ✅ `OpenGLES #167` is the orthographic projection setup

Reading the *caller* rather than guessing from the first four registers settles it. Pac-Man's call
site builds every argument explicitly:

```asm
18004394  stmdb sp!, {r1-r3, lr}     ; room for stack arguments 5–7
18004398  mov r3, #0x3f800000        ;  1.0f
1800439c  orr r2, r3, r3, lsl #8     ; -1.0f
180043a0  add r1, r3, #0x3f00000     ; 240.0f
180043a4  stmia sp, {r1-r3}          ; stack args: 240.0, -1.0, 1.0
180043a8  mov r3, #0x0               ; arg3 = 0
180043ac  add r2, r1, #0x300000      ; arg2 = 320.0f
180043b0  mov r1, #0x0               ; arg1 = 0
180043b4  ldr r0, =0x1802c6a8        ; arg0 = the identity matrix
180043b8  bl  0x18000300             ; -> OpenGLES #167
```

**Seven arguments**, not four: `(state, 0, 320.0, 0, 240.0, -1.0, 1.0)` — which is
**`glOrthof(left=0, right=320, bottom=0, top=240, near=-1, far=1)`** with a leading state pointer.
The textbook 2D projection for a 320×240 screen.

Two consequences, both large:

- **The games draw in screen pixel coordinates.** A software rasteriser maps vertices straight to
  pixels — no perspective divide, no viewport transform to reverse-engineer.
- **The four-register assumption was wrong.** Arguments beyond `r0`–`r3` go on the stack, so the
  trace has been showing a truncated view of every call. `#167` looked like it took a pointer and
  two dimensions; it takes seven values.

*(An earlier note here read the `320.0f`/`240.0f` as possible stale register contents. The call site
disproves that — the caller computes both deliberately. The suspicion was worth testing and was
simply wrong.)*

### `OpenGLES #40` is vertex-array setup — and the vertex format is readable

With stack capture in place, Vortex's per-frame draw sequence resolves cleanly:

```
#40   (0, 4, GL_FIXED, 0)      array 0 — 4 components
#40   (1, 2, GL_FIXED, 0)      array 1 — 2 components
#40   (2, 4, GL_FIXED, 0)      array 2 — 4 components
#159  (0x23, 0, GL_FIXED, 0)
#37   (7, 0, 4, 0x180bf600)    the draw
```

`#40(index, size, type, stride)` with indices 0/1/2 and sizes 4/2/4 is the textbook trio:
**position (4) · texture coordinates (2) · colour (4)**, all in **`GL_FIXED`** 16.16 — exactly what a
part with no FPU would use.

So the full picture of how these games draw is now: an **orthographic 0–320 × 0–240 projection**,
**fixed-point vertex/texcoord/colour arrays**, and an indexed draw. That is a complete enough
specification to write a rasteriser against.

### ✅ `OpenGLES #37` is `glDrawArrays` — and the games draw quads

Vortex's call site:

```asm
18010054  mov r2, r8       ; arg2
18010058  mov r1, #0x0     ; arg1
1801005c  mov r0, r9       ; arg0
18010060  bl  0x180000f8   ; -> OpenGLES #37
```

**`r3` is never set.** So `#37` takes *three* arguments — `(7, 0, 4)` and `(5, 0, 4)` — and the
`0x180bf600` that looked like a vertex pointer was **stale register contents**.

That makes it `glDrawArrays(mode, first, count)`, and it resolves the mode puzzle: **`7` is
`GL_QUADS`** and `5` is `GL_TRIANGLE_STRIP`. `GL_QUADS` is not in the ES 1.x standard — Apple kept
the desktop-GL mode — which is why it looked invalid. With `count = 4`, every draw is **one quad**.

**These games are sprite blitters.** Ortho screen-space projection, fixed-point vertex/texcoord/colour
arrays, four vertices per draw. That is the entire drawing model, and it is small enough to
implement directly.

*(Note the symmetry with `#167`: there the stale-register suspicion was wrong, here the same check
shows it was right. Reading the caller is what settles it either way — the register contents alone
never could.)*

| Import | Arguments | Reading |
|---|---|---|
| `#167` | `(state, 0, 320.0, 0, 240.0, -1.0, 1.0)` | **`glOrthof`** — confirmed from the call site |
| `#40` | `(index, size, GL_FIXED, stride)` | **vertex-array setup** — position/texcoord/colour |
| `#159` | `(0x23, 0, GL_FIXED, 0)` | array-related; first argument unexplained |
| `#37` | `(5\|7, 0, 4\|11, ptr)` | the draw call; leading argument is not a GL mode |

Turning these into geometry is what replaces black with a picture.

### 🟡 The export table is located; the OpenGLES array geometry is **not** solved

> **Retraction, same day.** An earlier version of this section claimed all 179 GL implementations
> had been located, and called it "the largest single change in the project's shape so far." That
> was over-claimed on two data points. See [the failure below](#why-the-two-anchor-validation-was-not-enough).

Because binding is by interface hash, every hash a game declares **must** exist in RetailOS.
Searching OSOS for Pac-Man's six finds **all six**, and the surrounding export table validates
perfectly: **eight records, eight counts, every one matching the game side** (`OpenGLES` 179,
`Metadata` 152, `Audio` 61, `AsyncFileIO` 17, `miscTBD` 15, `Filesytem` 4, `Settings` 3,
`InputEvents` 2).

A candidate base of **`0x796b0`** was derived by locating `glClear` (the only implementation
referencing `COLOR|DEPTH|STENCIL` bits) and matching it to the traced `#12` call. Two spot checks
appeared to confirm it from opposite directions — `#12` → `glClear`, `#4` → `GL_TEXTURE_2D`.

#### Why the two-anchor validation was not enough

Checking the *whole* array instead of two entries breaks the model:

| Property a real function table must have | Result |
|---|---|
| All 179 entries within OSOS | ❌ 177 — two run past the end |
| Monotonic across all 179 | ❌ false (true only for the first ~20, which is what misled) |
| Max value inside the image | ❌ `0xa54d1f04` — that is **hash bytes being read as pointers** |
| Entries starting with a plausible ARM prologue | ❌ **34 of 179** |
| The anchor `#4` starting with a prologue | ❌ no — it may be mid-function |

The array **overruns into the hash**, so it is shorter than 179, differently strided, or not a flat
pointer array at all. `#13` resolving to `glClear`'s literal pool (`"Error inside %s"`) was the first
symptom, and should have stopped the claim before it was written down.

**The lesson is the same one that caught the `0x29061968` byte order and the `count = 0` blocks: two
confirming samples are not validation when the population is 179.** The cheap whole-population check
— *do all entries look like function starts?* — takes one command and would have refuted this
immediately.

#### What does survive

- All six framework hashes **are** present in RetailOS at `0x79664…0x79ec8`.
- The export table **is** real: eight records, and **every count matches the game side** (179 / 152 /
  61 / 17 / 15 / 4 / 3 / 2). That validation compared eight independent values and stands.
- `glClear`'s implementation at `0x26ce1c` is genuinely identified, by its constants.

What is **not** established is the index→implementation mapping. Recovering it needs the real record
layout — most likely by parsing a record whose small count makes the geometry unambiguous
(`InputEvents` has 2, `Settings` 3, `Filesytem` 4) rather than starting from the 179-entry case.

See `research/02-eapp-loader.md`*(moved to the `ipod-games` repository)*.

---

## ✅ B2 complete — geometry on screen

**2026-08-11.** Vortex renders **123 quads over 40 frames**, rasterised from its own vertex buffers
at its own screen coordinates:

```
                 @@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@
                 @@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@
                 @@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@
                 ======================================================
                 ======================================================
                 ======================================================
```

Three distinct colours, ~30 000 non-black pixels. Not a fill — actual geometry.

### ⚠️ The first render was right for the wrong reason

`glVertexAttribPointer` was initially wired to **`#40`**, and it produced correct geometry. It should
not have. `#40` takes **one** argument and is `glEnableVertexAttribArray(index)` — called with 0, 1,
2. The six-argument `glVertexAttribPointer` is **`#137`**.

The stub read `sp[0]` and `sp[1]` on `#40` and found the stride and data pointer there — because
`#137` is called immediately before it and its stack arguments had not been overwritten. The output
was correct; the reasoning was not.

Caught by noticing that Pac-Man's `#40` carried a *pointer* where Vortex's carried a *size*, and all
20 titles share one `OpenGLES` interface hash (`041f4da5…`) so a single index cannot have two
signatures. The call sites resolve it: both games' one-argument call targets thunk `0x18000104`
(index 40) and the six-argument call targets `0x18000288` (index 137).

After rewiring to `#137`, the render is **byte-identical** — same 123 quads, same three colours. The
picture was always right; only now is it right *by construction*.

**The trap this fell into:** a stub reading stack slots it was never passed will silently pick up the
previous call's arguments. Any stub that reads beyond `r0`–`r3` needs its arity confirmed at the call
site first.

### `#137` takes six arguments

`glVertexAttribPointer(index, size, type, normalized, stride, pointer)` — **stride and pointer are
passed on the stack**, and the trace had been capturing them all along as what looked like frame
noise. Vortex's setup makes it explicit:

```asm
1800ff94  mov   r2, #0x28        ; stride = 40
1800ff98  stmia sp, {r2, r6}     ; stride and the data pointer go on the stack
1800ffa4  mov   r1, #0x2         ; size = 2
1800ffa8  mov   r0, #0x1         ; index = 1
1800ffac  bl    0x18000288       ; -> OpenGLES #40
```

### The vertex format, read from live memory

A 40-byte stride, and the three attribute pointers land at `base+24`, `base+0`, `base+8` —
accounting for exactly 40 bytes. Decoding the buffer as 16.16 fixed point gives a complete,
sensible vertex:

| Attribute | Offset | Components | First vertex |
|---|---|---|---|
| texcoord | `+0` | 2 | `(0.0, 0.0)` |
| colour | `+8` | 4 | `(1.0, 1.0, 1.0, 1.0)` — white |
| position | `+24` | 4 | `(49.0, 83.0, -8.0, 1.0)` — screen pixels |

Because the projection is `glOrthof(0, 320, 0, 240, -1, 1)`, positions need **no transform at
all** — they are already pixel coordinates. The rasteriser is an even-odd scanline fill with a Y
flip (GL's origin is bottom-left, the framebuffer's is top-left).

### The GL surface actually in use — 20 indices, not 179

Across all 20 titles, only about twenty `OpenGLES` entry points are ever called:

| Index | Calls | Status |
|---|---|---|
| `#40` | 93 | ✅ `glEnableVertexAttribArray(index)` — **1 arg** |
| `#137` | 67 | ✅ `glVertexAttribPointer(index, size, type, norm, stride, ptr)` — **6 args** |
| `#4` | 50 | ✅ `glEnable`/`glDisable` (takes capability enums) |
| `#157` | 49 | ✅ present / swap |
| `#13` | 36 | ✅ `glClearColor` |
| `#12` | 30 | ✅ `glClear` |
| `#167` | 20 | ✅ `glOrthof` — **7 args** |
| `#37` | 25 | ✅ `glDrawArrays(mode, first, count)` — **3 args** |
| `#165` | 30 | 🔧 **1 arg**, a pointer — arity confirmed, meaning unknown |
| `#159` `#99` `#175` `#53` `#158` `#125` `#45` `#36` `#152` `#35` `#164` | 12–26 each | 🔧 unidentified |

**Roughly twenty functions carry the entire rendering surface.** The other ~160 are never touched.
That is the real size of the remaining GL work, and it is an order of magnitude below the count that
made this look daunting.

### Arity detection, automated

Doing call-site analysis by hand for a dozen functions is slow and easy to get wrong, so it is now a
script (`scratchpad/arity.py`): walk backwards from each call site, record which of `r0`–`r3` the
caller writes before the `bl`, and flag `stmia sp, {…}` for stack arguments.

**Validated against every hand-derived answer before being trusted** — `#37`→3, `#40`→1,
`#137`→4+stack, `#167`→4+stack, `#165`→1. All reproduced.

One correction it needed: the first version counted `stmdb sp!, {…}` as stack arguments, but that is
a *function prologue push*. It inflated several arities until fixed.

### ✅ `#19` is `glCompressedTexImage2D` — and the texture format is exact

Four registers plus four stack words:
`(target, level, format, width, height, border, imageSize, data)`.

`format` is **`0x8B96` = `GL_PALETTE8_RGBA8_OES`**: a **1024-byte RGBA palette** followed by
`width × height` single-byte indices. Confirmed arithmetically on every observed upload:

| Dimensions | `1024 + w×h` | Declared `imageSize` |
|---|---|---|
| 150 × 76 | 12 424 | 12 424 ✅ |
| 256 × 256 | 66 560 | 66 560 ✅ |
| 286 × 341 | 98 550 | 98 550 ✅ |

Three for three, exact. `#4` (two arguments) is **`glBindTexture(target, texture)`**.

### ✅ Textured rendering works

The first textured attempt produced 61 quads, all black. Instrumenting instead of guessing answered
it in one run:

```
upload tex#0 150x76 opaque=9578/11400
draw n=4 bound=tex#0 known=true uv=[0.000..149.000 , 0.000..75.000] rgb=(1.00,1.00,1.00)
```

The decode was correct all along — 9 578 opaque texels of 11 400 is a real image with real
transparency. **The bug was that texture coordinates arrive in *texel units*, not normalised 0–1**:
`[0..149, 0..75]` against a 150×76 texture. The sampler multiplied by the dimensions a second time,
so every sample clamped to the last column — uniformly black.

With that fixed, **TWA renders 120 distinct colours with visible artwork**:

```
                        #######################                ############
                             +####+**+    ++++####################*++*++######
                                      -##     :###########################
                                       ##      ###     +##     =##*
                                           +++ ###     +##+    =
                                                ##     +##+
                              #########   **++ ###############################
```

| Title | Quads | Colours | |
|---|---|---|---|
| TWA | 31 | **120** | textured artwork |
| HoldEm | 1 | **22** | textured |
| vortex | 93 | 3 | geometry only — binds textures it never uploads in the traced window |
| molly · testprep ×3 | 2 | 1 | draw, but nothing visible yet |

**Two titles now render real textured content.** Vortex issues 19 binds and 0 uploads in the window
we trace, so its textures are uploaded somewhere not yet observed — the remaining gap is *when*
uploads happen, not whether the decode works.

**Method note:** the previous entry recorded this as "unresolved rather than nearly-working", which
was the right call — the fault was a real bug in the sampler, not a missing piece. Three counters
(uploaded? bound? what UV range?) found it immediately, where more disassembly would not have.

### 🔑 The next unlock: file I/O

Only **4 of 20** titles draw anything, even given 300 frames and an 8-million-instruction budget.
TWA and Vortex scale with frame count (301 and 903 quads), so they are genuinely running; the other
sixteen simply never reach `#37`.

It is not a blocked stub — sweeping every import Pac-Man reaches changes nothing. It runs a stable
23 calls per frame indefinitely and never draws.

**The games load their content from disk, and our file I/O returns nothing.** The two titles that do
render are the ones that need no files: TWA embeds its texture via `glCompressedTexImage2D`, and
Vortex is a flat-shaded tunnel shooter that legitimately uses no textures at all. Every silent title
calls `AsyncFileIO` and gets zeros back.

**And the assets are all present.** Each game directory carries ~48 files alongside the executable:

```
Pac-Man/tex_ig.tga          the in-game spritesheet
Pac-Man/tex_menu1.tga       menu graphics and font glyphs
Pac-Man/PM_Logo.raw.lcd5    splash screen (RGB565, format documented in research/01)
Pac-Man/audio/*.wav         sound effects
```

So the next stage is **implementing `AsyncFileIO` (17 functions) against the game's own directory** —
open, read, seek, close. Every format those files use is already documented in
`research/01`*(moved to the `ipod-games` repository)*.

That single subsystem is what stands between four rendering titles and most of the twenty.

### The method note that keeps earning its keep

Every identification that survived came from **disassembling the caller**; every one that had to be
retracted came from reading `r0`–`r3` and assuming.

Registers beyond a function's true arity hold **stale values from previous calls**, and they are
routinely plausible — `#165` appeared to take `(ptr, 4, 3, ptr)` and takes one argument; `#37`
appeared to take a vertex pointer and takes three integers; `#167` appeared to take three and takes
seven. In each case the residue looked like meaningful data.

**Rule for this project: never assign a signature without checking how many registers the caller
actually sets.** It costs one disassembly and it has been wrong to skip every single time.

### What is not yet drawn

- **No textures.** Quads are flat-filled with the first vertex's colour, so the shapes are right
  but the content is missing. Texture upload and sampling is the next surface.
- **Mahjong and Pac-Man draw 0 quads.** They present and clear normally but never reach `#37`, so
  they use different draw entry points that have not been identified yet.

---

## Extraction — how the 34 get unlocked

Needed once, on one authorised 5G. Two strategies; **A is strongly preferred.**

**A — recover the key, decrypt offline.** Read the *unwrapped* AES key out of RAM at launch, then
decrypt the archived ciphertext on the Mac with stock AES-128-CBC. Byte-exact output, no relocation
artefacts. xLinka found Tetris 1.0's key at `13d05688` with the IV in R10 at `13b486cc` — so the hook
point is known to exist.

> **The IV is already in hand — 2026-08-11.** Parsing the real `.sinf` archives shows a 16-byte
> **`iviv`** atom sitting in plaintext in every one of the 116 executables' side files. So extraction
> only needs to recover **16 bytes of key per binary**, not key *and* IV. Half the on-device problem
> disappears, and the IV half can be verified before the iPod is even bought.

**B — dump the decrypted image from RAM.** Fallback. Yields a post-load snapshot: relocated, thunks
already patched, BSS zeroed. Not a clean file.

**Getting code onto the device.** The 5G is pre-S5L, and xLinka *recomputes checksums* rather than
re-signing — which implies the bootloader validates a **checksum, not a signature**. That is the same
door iPodLinux and Rockbox walk through. So: extract `OSOS` from the firmware partition, patch in a
hook after the key is in the clear, write it back.

**Exfiltrate to disk, not through a crash.** The 2007 crackers dumped RAM after a deliberate crash.
Unnecessary — the data partition is FAT32 and mounts over USB. Have the patch write 32 bytes to a
file there, then mount the iPod on the Mac and read it. Re-runnable 34 times without re-flashing.

⚠️ **Two assumptions to confirm on hardware before trusting any of this:** that 5G `OSOS` really is
checksum-only, and *where* the unwrap happens — the hook must land after the key is plaintext but
before the game takes control.

---

## Open questions

| # | Question | Status |
|---|---|---|
| 1 | **What system interface does a game binary use?** Shim or full RetailOS? | ✅ **Shim. A bounded `eapp` import table exists** — see [The eApp ABI](#the-eapp-abi). Reproduction pending |
| 2 | What is inside a `.ipg`? | ✅ **Closed — `research/01-ipg-format.md`*(moved to the `ipod-games` repository)***. Full container spec, `.sinf` atom list, asset formats, on-device layout, 5G GUID table |
| 3 | What exactly does FairPlay bind? | ✅ **Three layers** — per-copy AES, per-account keys, per-device `IC-Info.sidb`. See [The encryption](#the-encryption-in-three-layers-added-2026-08-11) |
| 4 | ~~Does Siggi own an iPod 5G?~~ | ✅ **Obtainable in a few weeks (operator, 2026-08-11).** No longer blocking — the 20 plaintext titles carry all pre-hardware work |
| 5 | Is clicky the right substrate? | ✅ **No.** Targets 4G grayscale, which cannot run these games at all. Reference only |
| 6 | Performance: interpreter vs dynarec | ⏸ Mac trivial, Pi marginal. Only matters after a first frame |
| 7 | Do 5G builds differ from Nano/Classic? | 🔧 **Yes, and it may help.** Olsro: iPod Video always gets "liter" binaries. The 5G has no GPU, so its GL ES is software — likely a *narrower* surface than the nano 3G+ titles |
| 8 | **Does L4's "own purchases" clause survive the 54-title goal?** | ❓ **Operator decision, now unavoidable** — the operator has already run the preservation-project route on games he did not own (see #10). L4 as written does not describe what is actually happening |
| 9 | ~~Is a 5G RetailOS image obtainable without the iPod?~~ | ✅ **YES — obtained 2026-08-11.** Stock Apple `iPod_20.1.3`, `Firmware-20.6.3`. OSOS extracted: file offset `0x4400`, length `0x735A00`, **loads at `0x10000000`**, plaintext (entropy 6.17). See [The eApp loader, found](#the-eapp-loader-found) |
| 13 | **Are the built-in games separate eApps in the firmware?** | ❌ **No — hypothesis disproved 2026-08-11.** Exactly one `eapp` occurrence in all 7.5 MB of OSOS, and it is a *constant in the loader's literal pool*, not an image header. Brick/Parachute/Solitaire/Music Quiz are linked into RetailOS directly |
| 14 | ~~Does the 5G actually cover all 54 titles?~~ | ✅ **YES — measured 2026-08-11 across all 56 archives. Every single title ships a `PlatformID 1` build.** One emulator, one target. See [Per-model coverage](#per-model-coverage) |
| 10 | ~~Can a fresh iPod still be authorised in 2026?~~ | ✅ **YES — operator first-hand, 2026-08-11.** Done on a 5G a few months ago via the preservation-project instructions, without owning any of the games. **The ceiling is 54, not 20.** (That iPod has since been sold; a replacement is weeks out) |
| 11 | Is 5G `OSOS` checksum-validated, not signature-validated? | 🔧 Implied by xLinka recomputing checksums. Confirm on hardware |
| 12 | **Which target platforms?** macOS / iOS sideload / App Store | 🔧 See [Target platforms](#target-platforms). macOS first is uncontested; **App Store is the one that probably cannot happen** |

---

## Next steps

**Before the iPod arrives** — none of this needs hardware.

0. **Answer #9.** A 5G RetailOS image is now the single biggest unlock: it carries the frameworks to
   host, and probably the built-in games as un-DRM'd eApps to prove the loader on. With B0 done,
   this is the only thing standing between here and a running game.
1. ✅ **eApp parser built — [`tools/eapp-inspect/`](tools/eapp-inspect/)** (2026-08-11, Rust, zero
   deps, 6 tests green). Scans for framework blocks *by magic* rather than walking `next_ptr`, so a
   wrong layout assumption surfaces as a count mismatch instead of a silent early stop; cross-checks
   every declared `func_count` against the `ldr pc,[pc,#imm]` thunks that actually follow; derives
   the load base rather than hardcoding `0x18000000`; and classifies plaintext vs ciphertext by
   Shannon entropy — verified separating real code (6.37 b/B) from random (8.00 b/B), which is how
   the 20 get sorted from the 34 mechanically.

   ```
   eapp-inspect <dir-of-games> --json      # aggregate ABI surface across all titles
   eapp-inspect <file.bin> --hex           # annotated raw dump, for correcting the layout
   ```

   **Still needs the binaries pointed at it.** Running it across all 20 produces the artifact —
   *the complete eApp ABI surface, per game, per framework* — which does not exist publicly, and
   simultaneously **reproduces the unverified observation** the architecture rests on. If the
   aggregate shows a framework whose function count *varies between games*, the ABI is versioned and
   a loader has to resolve against a specific version — that would be the single most important
   finding of this step.
2. **Unicorn harness + call trace.** Run one game with every import wired to a logging stub.
   **Milestone: an ordered trace of every framework call Pac-Man makes from entry to first frame.**
   This is the number that sizes the project — how many of the ~98 a real game *actually* touches.
3. **First frame on the Mac.** Implement only what the trace demanded; point the framebuffer at a
   window.

**When the iPod arrives** — it has exactly one job: [Extraction](#extraction-how-the-34-get-unlocked).
Verify #10 *before* buying it.

`02`'s **Experiment A** remains independently worth doing and is unaffected by any of this.
