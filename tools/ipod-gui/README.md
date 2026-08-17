# `ipod-gui` — the emulator, in a window you can turn

A drawn iPod 5G whose screen is the live framebuffer and whose click wheel, five buttons and hold
switch actually drive the machine. It is a **front end over the existing model**, not a second
model: the same `eapp-loader` crate the `trace` recipes use, the same peripheral map, the same
devices, built by the same calls in the same order.

```
cargo build --release --manifest-path tools/ipod-gui/Cargo.toml
$CARGO_TARGET_DIR/release/ipod-gui
```

**Launched with nothing configured it opens a setup screen** rather than exiting with an error, and
that is the supported first-run path — see §"The setup screen" below. If `resources/` is present the
defaults are `retail-boot.sh`'s, verbatim: the retail NOR at
`resources/reference/ipod-bootrom-archive/A1238/` and `resources/derived/disk/ipod8g-retail.img`.
Whatever the setup screen last accepted is remembered, so the screen appears once.

**First launch costs about 75 seconds** and shows a progress bar: it is doing the cold boot. At
1.6 G instructions it writes a snapshot, and every later launch restores that in about three
seconds. `--cold` forces the long way round. The snapshot cache is keyed on the emulator's own
source (`lib.rs` and `emu.rs` are hashed into the filename), the two images, the clock and the
snapshot point, so a change to the model mints a new snapshot rather than silently restoring a
hybrid machine — the failure mode `tools/ipod-boot/from-idle.sh` documents at length. The cost of
that discipline is that editing `emu.rs` buys you one more cold boot; the alternative is worse.

**The snapshot and the 8 GB working disk live in a per-user cache directory**, not in `$TMPDIR`.
That distinction is Linux's: `/tmp` is `tmpfs` on most distributions, RAM-backed and typically
capped at half of it, and an 8 GB image there either fails or eats the machine. It is
`~/Library/Caches/ipod-gui` on macOS, `$XDG_CACHE_HOME/ipod-gui` (or `~/.cache/ipod-gui`) on Linux,
and `%LOCALAPPDATA%\ipod-gui` on Windows. Delete it freely; the only cost is one cold boot.

## Two modes, and one number that is in both

**User mode is the default and is what a fresh install opens in: the iPod and nothing else.** No
counters, no addresses, no instrument panel. **Debug mode** is everything this document describes.
`D` toggles, so does the checkbox in the footer, and the choice is remembered in a four-key settings
file (`~/Library/Application Support/ipod-gui/settings.txt` and the equivalents).

The one thing that appears in **both** is the speed ratio: user mode carries a badge reading
`29 % of real-time — emulated`, debug mode the full readout below. Hiding it would teach somebody
something false about how responsive a 5G is, and timing is where this emulator's remaining unknowns
live. It is the live figure, not a constant — `executed_here / wall_secs / 72e6`.

## The setup screen

With no usable images, the window opens on two slots and validates whatever is put in them, before
anything is built:

- **NOR flash dump** — 1 MiB exactly, word 0 an ARM branch, and a `flsh` image directory at
  `0xffe00` whose entries all load at `0x10000000`. The bootloader's own build string is reported
  and never judged on: the prototype dump says `Nov 28 2006` and the retail one `Mar 10 2008`, and
  both are 5G NORs.
- **A drive** — either an `.ipsw`, from which `Build a drive image from it` writes a fresh 8 GiB
  sparse image (Apple's firmware partition byte for byte, plus an empty FAT32 volume RetailOS
  populates itself), or a drive image you already have: MBR, partition 0 of type `0x00`, an `!ATA`
  directory at partition + `0x4200`, and an `osos` entry loading at `0x10000000`.

Each slot has a native file dialog, a text field, and drag-and-drop, and the dialog is the
platform's own tool — `osascript`, PowerShell's `OpenFileDialog`, `zenity`/`kdialog` — so that a
dialog crate's async runtime and D-Bus stack stay out of a program whose whole dependency argument
is "eframe and nothing else". If the dialog is unavailable the other two still work, which is what
makes that acceptable.

`ipod-gui --check-images [--flash=… --disk=…]` runs the same two checks with no window and exits
non-zero if either fails. The validation is structure, never signatures: a table of known-good
hashes would reject a legitimately different dump for being unfamiliar.

## The update check

`ipod-gui --check-update`, or the button in debug mode. One HTTPS GET of the GitHub releases API and
a version comparison; if there is something newer, a line with a link, which you follow yourself.

**Off on launch unless you tick the box.** Nothing is downloaded, nothing is installed, nothing is
executed, and it fails silently and completely when offline — an emulator that shows a network error
on launch is worse than one that never checks. The GET is `curl` (PowerShell's `Invoke-WebRequest`
where Windows has no `curl.exe`), because a TLS stack is a large dependency with a build-time story
on at least one of the three platforms and this fetches forty bytes of JSON, once, when asked.

## What it shows

The panel is the co-processor's surface at internal `0x000e0000`, 320x240 RGB565, converted with the
same bit-replication `--bcm-dump` uses — so a screenshot from this window and a PPM from `trace` of
the same instant are the same bytes. The back buffer at `0x00106000` is one checkbox away.

At the 1.6 G idle that surface holds **75 267 non-black pixels of 76 800**: RetailOS's first-run
**Language** list — a real list widget with a highlighted row, a scrollbar and a green battery in its
title bar. One press of Select takes it to the **main menu** (75 791 px: `iPod`, Music / Photos /
Videos / Extras / Settings / Shuffle Songs, chevrons), and **the menu stays there** — measured out to
800 M instructions past the press, about 32 minutes of simulated time, with the panel digest
unchanged at every sample.

*(This section used to say the idle surface was Apple's charging screen at 76 607 px, and that
scrolling woke the machine into the Language list only for it to "time back out to the charging
screen" ten seconds later. Both were true of the machine this front end was written on, and both
stopped being true one commit later: research/10 Addendum 30 found that `GPIOL` bit 3 was being held
low by our own region default, so every boot believed it was **plugged into a wall charger**. It is
seeded correctly now — a bare iPod — and there is no charging screen to time out to.)*

**`--charger` puts the machine back on a wall socket, deliberately**, and then the timeout is real
and reproducible: from the main menu, with no input at all, RetailOS returns to the Charged screen
after **163.4 s of simulated inactivity and before 166.1 s** (research/10 Addendum 31 §3, measured by
bracketing the panel digest at 1 M-instruction resolution, with a matched no-input control that never
leaves the Charged screen). That is authentic behaviour — a 5G on a charger goes back to its charge
display when left alone — and it is worth knowing about before mistaking it for a fault.

## Controls

| | |
|---|---|
| drag around the ring | scroll — angle maps to one of 96 positions, and a finger is on the wheel while the button is down |
| click a label | Menu · next · previous · play/pause — each owns the 16 clicks centred on its quadrant |
| click the centre | Select |
| click the switch on the top edge | Hold, and it slides |
| arrow keys | scroll (Right/Down clockwise, Left/Up anticlockwise) |
| Enter / Space · M · P · , · . | Select · Menu · Play · Prev · Next |
| H | Hold |
| S | screenshot — writes a `.png` and a `.ppm` into `_out/`. Debug mode only: it is an instrument |
| D | user mode / debug mode |
| **power off** · **power cycle** | cut power, or cut it and boot again from the reset vector |
| **hold MENU+SELECT** · **hold PLAY** | the hardware's reset and power-off chords, delivered as buttons |

### Power, and why two kinds of control sit next to each other

**Power off drops the machine.** Not a pause: the `Machine` is dropped outright — CPU, all 64 MB of
SDRAM, the co-processor's surface — the panel goes dark, and the instrument panel's numbers go to
zero rather than freezing, because there is nothing left for them to describe. Powering on builds a
new machine and enters at address 0 through the same `call_with(0, …)` `retail-boot.sh` makes. It is
a real cold boot and costs the same ~75 s; it is never restore-then-pretend.

The **drive survives**, because that is what survives a real power cycle: a second boot finds the
volume RetailOS built on the first. Only the first session of a process may restore a snapshot or
write one — a session reached by powering back on has been asked for a cold boot explicitly, and a
snapshot written from it would have been taken against an already-written drive, quietly changing
what every later restore restores.

`--power-cycle-at=N` proves it with no window and no hand: restore at 1.6 G, cut power at 1.65 G, and
the second session's fingerprint is a **boot** — the instruction count restarts from zero and climbs
to a fresh `Idle after 1755959163`, which a resume could not do. It is also visibly not the *first*
boot: **573 ATA commands and 36 348 code buckets against 706 and 38 479**, because the drive it woke
up to already had the volume RetailOS built before the power was cut. New machine, same disk.

**MENU+SELECT and PLAY are the real chords, and they are the other kind of control.**
`ClickWheel::buttons` is a five-bit mask, so two buttons in one frame has always been expressible and
these deliver exactly that — the wheel reports the pair held, and RetailOS decodes it. **Nothing acts
on it.** Held for 400 M instructions at the main menu, with 8 arrivals at Apple's ISR decoder and 6
button events to prove it was received, the machine does not reset (research/10 Addendum 31 §6). On
real hardware the chord is caught below the firmware, by the wheel's PSoC or the PMU, and this
project models neither: `ClickWheel` is a transceiver that posts frames, with no path from `buttons`
to anything that could restart a machine. So the chord is labelled as delivering buttons and the
power controls are labelled as emulator controls. A button captioned MENU+SELECT that secretly
rebuilt the `Machine` would be the window claiming a hardware behaviour we have measured to be
absent.

A press that turns into a drag becomes a scroll and the button is released: that is one continuous
gesture on the hardware, and treating it as a click followed by a drag sent Menu at the start of
every scroll begun at twelve o'clock.

## The two speed ratios, because they are different numbers

The instrument panel shows both, and it has to:

- **~21.5 M instructions/second** headless, ~19 M with the window drawing, against a PP5021C's
  ~72 MIPS. So the machine does about **27–30 % of the real hardware's work rate**. That is the
  number that says a game would run slowly.
- **The emulator's own microsecond clock is a separate quantity**, and it does not follow from the
  first. It is `executed / instr_per_usec + slept_usec`: `--clock=5` pushes it forward 15x faster
  per instruction than real silicon, while the idle task's halts push it forward again by whole
  timer intervals at no instruction cost. **Measured on a restored idle machine it comes out near
  1.2x wall clock** — which is neither of the two numbers anyone would predict from the constants,
  and exactly why both are shown rather than one derived from the other.

Timers, timeouts and RTXC sleeps all live on the second clock; the first is what a person watching
an animation would feel.

## The measurement: input demonstrably reaches RetailOS, and moves the screen

A window that looks right proves nothing. `--selftest` runs with no window, pushes a gesture through
**the GUI's own input path** — the same `Link::push` a mouse drag calls, the same gap-spaced drain,
the same appended `WheelStep`s — and prints what arrived inside Apple's firmware. `--selftest-control`
is the matched control: identical run, identical sampling instants, no input at all.

One touch, 36 clicks clockwise, one release, one Select press and release:

```
$ ipod-gui --selftest                       $ ipod-gui --selftest-control
[driving the wheel]                         [CONTROL — no input]
reporting enabled at 1603499952             reporting enabled at 1603499952
position 36, 40 frames posted, 0 dropped    position 0, 0 frames posted, 0 dropped
DATA reads 40 (40 with a frame waiting)     DATA reads 0 (0 with a frame waiting)
IRQ 40 asserted 40 times                    IRQ 40 asserted 0 times
  0x00281350  decoder            40           0x00281350  decoder             0
  0x000c953c  edge               40           0x000c953c  edge                0
  0x000dd018  scroll             38           0x000dd018  scroll              0
  0x000ada4c  button event        2           0x000ada4c  button event        0
  0x000cd6a0  wheel event        10           0x000cd6a0  wheel event         0
the panel at 0x000e0000:                    the panel at 0x000e0000:
  before  @1603499952  76607  6304ad7c        before  @1603499952  76607  6304ad7c
  +8 M    @1611499952  75267  01149fcb  CHANGED   +8 M @1611499952  76607  6304ad7c
  +60 M   @1663499952  76607  6304ad7c        +60 M   @1663499952  76607  6304ad7c
```

Both arms also write each sample to `_out/selftest-{driven,control}-{before,plus8M,plus60M}.png`,
so the two `+8 M` files are the whole experiment in two pictures.

> ⚠️ **That transcript was taken on the charger-present machine of before research/10 Addendum 30,
> and `--selftest` cannot reproduce it on the current default.** Two reasons, both measured in
> Addendum 31 §5. (1) A snapshot does not carry the click wheel, so a restored machine starts with
> the `0x052a` reporting gate shut, and on a *bare* iPod nothing ever re-opens it — **0 `0x052a`
> commands in 500 M instructions**, because RetailOS sends that enable on power-state transitions
> and a machine sitting on a list has none. The test now abandons after 400 M instructions and says
> so, rather than waiting forever and looking slow. (2) Its two arms were never anchored to the same
> moment: stage 0 waits for the gate *and then acts*, and the gate opens during the boot in a cold
> machine and after the idle point in a restored one — so the cold and restored arms delivered their
> gesture hundreds of millions of instructions apart. `--probe` was written to replace it for that
> reason: it acts at a fixed instruction anchor in every arm.

## `--probe`: the same measurement with an anchor both arms share

```
ipod-gui --probe=menu|menu-control|combo|combo-control [--probe-at=N] [--samples=A,B,C]
         [--charger] [--cold] [--clock-v3] [--ablate=pmu]
```

No window. Wait for instruction `--probe-at` (default 1 500 000 000 — research/10 Addendum 30's own
`--wheel=@1500M:…` anchor, so a cold arm here and that recipe press Select at the same point of the
same boot), press Select, then sample **both** surfaces at fixed instruction counts afterwards,
writing a PNG per sample per surface into `_out/` and printing the non-black count, the digest and
the simulated clock at each. `menu-control` is the matched no-input arm; the `combo` pair holds
MENU+SELECT (against SELECT alone) for 400 M instructions.

The flags exist because each one is a variable somebody needed held: `--charger` is the mains-charger
GPIO of Addendum 30 §1, `--clock-v3` reproduces the pre-2026-08-14 snapshot format on a machine
restored from a correct one, and `--ablate=pmu` hands a running machine a factory-fresh PCF50605 —
the state a *restored* machine runs with, since a snapshot omits the chip entirely.

Three things are settled by that pair, and the control is what settles them:

1. **The gesture reaches Apple's ISR decoder at `0x00281350`** — 40 arrivals for 40 posted frames,
   every one of the 40 `DATA` reads finding a frame waiting, 40 IRQ 40 assertions, nothing dropped
   and nothing suppressed. This is the same evidence a `--wheel` script produces (research/10
   Addendum 21 §6 measured decoder 36 / edge 36 / scroll 32 for a 36-step script), made instead by
   the window's own plumbing.
2. **Twelve UI events reach RetailOS's event system** — 10 wheel events at `0x000cd6a0` and 2 button
   events at `0x000ada4c` — from a hand on a wheel rather than from an instruction-anchored script.
3. **The screen changes because of the input, and it changes into RetailOS's UI.** Eight million
   instructions after the gesture the surface at `0x000e0000` is a different picture — 75 267
   non-black, different digest — and `_out/selftest-driven-plus8M.png` shows what it is: the
   first-run **Language** list, English highlighted, scrollbar drawn, battery glyph in the title
   bar. By sixty million it is back to the charging screen. The control holds every one of those
   samples at the original digest and writes the charging screen three times. Same restore, same
   instants, one variable.

`--headless=N` is the other self-check. It boots and prints the fingerprint, so that "the GUI runs
the same machine as the recipe" is a comparison and not a claim:

```
$ ipod-gui --cold --headless=2000000000       $ BUDGET=4000000000 tools/ipod-boot/retail-boot.sh \
headless: Idle after 1812313976 instructions      --clock=5 --stop-when-idle=400000000 \
  ata commands: 706                               --clickwheel --bcm-registry \
  unmapped: 0 reads, 0 writes across 0 pages      --bcm-dump=0xE0000:140:F0:out.ppm
  38479 code buckets executed                 -> Idle after 1812313976 instructions
  bcm: 4 kicked, 2 frame updates              38479 code buckets executed
  framebuffer: 75267 non-black of 76800       bcm: 4 commands kicked, 2 frame updates
  wheel: reporting ON, 5 `0x052a` commands    ata commands: 706
  137.9 s wall, 13.14 M instructions/s        bcm dump -> out.ppm (75267 non-zero of 76800)
```

Identical in every number, re-measured 2026-08-14. Note which recipe that is: `--clickwheel
--bcm-registry` is the configuration the window runs, and it is **not** the
`Idle @1 562 789 429 / 38 220 / 770 ata / 4 unmapped` baseline — that one is the same boot with the
wheel and the registry off, and it is unchanged by this work (verified `diff`-identical before and
after the snapshot-format change, down to everything but the per-run temp disk name).

*(This table read `Idle after 1609725109`, 38 521 buckets and a 76 607-pixel framebuffer until
2026-08-14. Those numbers were measured before research/10 Addendum 30 corrected the charger GPIO,
and — as the next section records — could not have been re-measured on the merged tree, because this
crate did not compile there.)*

## Things worth knowing before trusting what you see

**The simulated clock round-trips a snapshot now, and the window checks it every time.** It did not
until 2026-08-14: `Machine::snapshot` saved `Memory::usec`, which is *derived* — the run loop
recomputes it every instruction as `executed / instr_per_usec + slept_usec` — and did not save the
accumulator behind it. So the restored clock survived exactly zero instructions and fell 2.62 billion
µs, 44 minutes of simulated time, backwards. The format is `IPODSNP4`, it carries `slept_usec`, and a
v3 image is refused rather than read with a zero in the new field. Printed on every restore:

```
restore: the simulated clock round-trips — 3036268993 µs, and `executed / 5 + slept_usec` agrees,
         so the next instruction will not move it.
```

**The check is the identity, not a threshold**, and that is the part worth copying. A first version
compared the clock across the first slice and complained at any step over 1 000 µs — and fired on a
perfectly healthy restore, because 250 000 instructions at the idle point legitimately advance the
clock 1 725 688 µs through the idle task's sleeps. Worse, the old defect is not recognisable by sign
either: in the u32 arithmetic firmware actually does, a 2.7-billion-µs step *backwards* reads as
**+1 580 473 981 µs forward**. Only `usec == executed / instr_per_usec + slept_usec` tells them
apart. `--clock-v3` reproduces the old behaviour deliberately, for A/B work.

**What that fixed, and what it did not.** It did not fix — and was measured not to cause — the
charging-screen revert (`--clock-v3` versus not, over one shared snapshot file: identical panel
digests at all seven instants). See research/10 Addendum 31 §4.

**A restored machine can be one page flip out of phase, and then the window shows a stale surface.**
This is the current form of "a restored machine and a cold one answer the same input differently",
and it is much less alarming than it sounded: fed the identical Select press at the identical
anchor, a restored machine produces **the identical picture, digest for digest** — it just lands in
the *other* buffer, `0x00106000` instead of `0x000e0000`. Nothing in this model represents which
surface the panel scans out, so every instrument the project owns (`--bcm-dump`, `--selftest`, this
window) reads the front one by convention. The instrument panel therefore counts **both** surfaces
and says, in as many words, when the one on screen is static while the other is moving — the `back
buffer` checkbox is the remedy. Addendum 31 §5.

**The wheel is refused after a restore until RetailOS re-arms it — and on a bare iPod it never
does.** A snapshot does not carry the click wheel, so a restored machine starts with the `0x052a`
reporting gate closed, and events pushed before RetailOS re-sends `0x8001052a` are **suppressed and
counted**, never silently eaten: the panel shows `reporting: OFF — gated` and a running
`frames suppressed`. On the charger-present machine the gate reopens 1.75 M instructions after the
restore point. On the current default — a bare iPod on the Language list — it is **0 `0x052a`
commands in 500 M instructions**, because RetailOS sends that enable on power-state transitions and
an idle bare machine has none. A restored window is deaf until something changes its power state.
Forcing the flag on at restore would be one line, and would hide the whole of that paragraph.

**Rotation is delivered one click per 20 000 instructions** (`--wheel-click-instr=`, the same figure
and default the `--wheel` scripts use; at `--clock=5` that is 4 ms per click). Delivering a whole
drag in one tick is not faster, it is `frames_dropped` — Addendum 21's arm D posted 39 frames and had
35 overwritten unread. A drag that outruns the drain queues up to 96 clicks and then **drops**, and
the drop is on screen as `queued / dropped`.

**The finger indicator on the ring is drawn from the emulator's `position`, not from the pointer.**
If a click did not reach the machine, the dot does not move. That is deliberate: a UI that drew its
own idea of the wheel would look correct while delivering nothing.

**The panel is drawn at an integer physical-pixel scale with nearest-neighbour sampling**, on a rect
snapped to the physical pixel grid — and the scale in use is printed under the device. Bilinear
filtering or a fractional scale would blur an emulator artefact into a rendering artefact, and this
project has retired nine published conclusions to instruments that lied.

**The device is vector geometry, not a photograph.** Proportions are the real 5G's — 61.8 x 103.5 mm
case, 50.8 x 38.1 mm active area, ~28 mm wheel with a ~13 mm select button — but every pixel of it is
drawn, including the three transport glyphs, which were text until the pause bars came out as a tofu
box on the bundled font. Ninety-six hit regions want real angles rather than tuned offsets, white
and black are a fill swap rather than two assets, and no Apple product image enters the repository.

**Where position 0 sits on the bezel is chosen, not derived.** 96 clicks per rotation and
"clockwise increases" are Rockbox's, and the firmware only ever consumes *differences* between
frames — nothing in RetailOS, the boot ROM or Rockbox pins the wheel's zero to an angle. This GUI
puts 0 at twelve o'clock because it is legible. `wheel::position_at_angle` is the only place that
choice exists.

## What is not here

- **No audio.** The Wolfson codec is not modelled anywhere in this project.
- **No games.** The window boots RetailOS; the eApp loader path (`trace --native`, `play`) is a
  different entry point and is untouched.
- **No mouse wheel or trackpad scroll.** Scroll events would be a second, less honest path to the
  same place — a two-finger flick has no touch-down and no angle, so it would have to invent both.
  The ring drag and the arrow keys are the whole input surface.
- **No fast-forward, no pause, no rewind.** The machine runs flat out and the panel reports the
  rate; there is no way to make it faster and no reason to make it slower. **Power off is not a
  pause** — it drops the machine, and powering on rebuilds it from the reset vector.
- **No way to see the co-processor's own state.** `--bcm-peek` in `trace` still exists for that.
- **No model of which surface the panel scans out.** Both are counted and either can be shown; which
  one the hardware would be displaying is not something this emulator knows.
- **No working MENU+SELECT or PLAY chord**, in the sense of the machine acting on one. They are
  delivered as buttons and RetailOS decodes them; nothing resets. See the Controls section.
- **No lock icon when you throw the hold switch**, because RetailOS does not draw one — see below.

## The hold switch moves, and RetailOS does not notice it move

The switch pulls **GPIOA bit 5 low at `0x6000d030`**, which is the line, bit and polarity Rockbox's
`button_hold()` reads, exactly (`firmware/target/arm/ipod/button-clickwheel.c:348`), and it clears
bit 31 of the click-wheel frame. Neither produces any visible effect after boot. Measured
2026-08-14, all on `retail-boot.sh --clock=5 --bcm-registry --clickwheel`:

| | |
|---|---|
| RetailOS reads `0x6000d030` | **4 times in a whole boot**, all from PC `0x002218c8`, the last at instruction **49 689 151** |
| positive control in the same run | `0x6000d13c` (GPIOL) read **3 549 times** — the read log works |
| `+150M:hold` after the main menu | **no further read** in the 400 M instructions to idle; the panel is byte-identical to the arm without it |
| `@10M:hold`, *before* that last read | the machine behaves differently — a Select press at 1.5 G leaves the panel on the Language list (75 267 px) where the matched control reaches the main menu (75 791 px) |
| `@10M:hold,@1000M:unhold` | still the Language list, and the press did arrive: 2 arrivals at `0x000ada4c`, 5 at `0x00281350` |

So the line is read, once, during startup, and it demonstrably changes what the machine does. What
is missing is any way to tell RetailOS the switch **moved**: on the hardware that is a GPIO
interrupt, `research/08` records that RetailOS programs `GPIOA_INT_LEV` and `GPIOA_INT_CLR` where
Rockbox never does, `HoldSwitchTask` sits pended on semaphore `0xba` rather than polling, and this
emulator models no GPIO interrupt block at all — the whole `0x6000d0xx` range is a plain backing
region. Writing the bit changes a word of RAM and raises nothing.

The switch therefore **works at the device level and is labelled as doing only that**: throwing it
logs "the wheel reports it; RetailOS is not measured to act on it", and the instrument panel says
the same at length while hold is engaged. Drawing the status-bar lock ourselves would be the window
claiming a behaviour nobody has measured, which is the failure this whole project is organised
against. The last row above is one run per arm and is a lead, not a settled mechanism.

## Files

| | |
|---|---|
| `src/emu.rs` | the machine, and the thread that runs it — build, restore, power cycles, drain input, publish frames, the self-test and the probes |
| `src/wheel.rs` | angle → position on the 96-ring, the button quadrants, and their tests |
| `src/png.rs` | a dependency-free PNG writer (stored deflate) and the PPM writer, and their tests |
| `src/inspect.rs` | what a NOR dump, a drive image or an IPSW turns out to be, and the sentence to say about it |
| `src/settings.rs` | the four things the window remembers, and the three per-platform directories |
| `src/update.rs` | one HTTPS GET, one version comparison, and silence on every failure |
| `src/main.rs` | the eframe app: the setup screen, the drawn device, the pixel-exact panel, the instrument panel |

`cargo test --release` in this directory is **49 passed**, all of it arithmetic, file format and
parsing — the angle mapping including the wrap at 95 → 0, the shortest-path delta for every one of
the 9 216 position pairs, the button quadrants, the integer-scale invariant across five device-pixel
ratios, the PNG chunk CRCs and stored-deflate block framing, the settings round trip, the version
comparison including the "an unparseable tag is not an update" case, and the image validator against
**forty bytes copied out of a hex dump of the real images**. That last one is not decoration: the
first version of `inspect.rs` compared the drive's directory magic against `ATA!` instead of `!ATA`
and reported this project's own reference image as having no firmware partition — and every
synthetic test passed throughout, because the helper wrote the same wrong magic.

## Changes outside this directory

**`map_hardware` moved** from `tools/eapp-loader/src/bin/trace.rs` into `lib.rs` as
`eapp_loader::map_hardware`, and `trace.rs` keeps a one-line delegate. Two front ends now stand the
same machine up, and a peripheral map that existed in two copies would become two different machines
the first time either copy was corrected. The move was byte-for-byte and the baseline proved it:
`BUDGET=4000000000 retail-boot.sh --clock=5 --stop-when-idle=400000000` before and after produced
run reports that `diff` reports as identical — at the time, `Idle after 1610279157`, 38 266 code
buckets, 770 ata commands, 4 unmapped reads, and `cargo test --release` in `eapp-loader` 24 passed
either way. *(That baseline has since moved to `Idle after 1562789429` / 38 220 buckets — not from
this work but from research/10 Addendum 30's charger-GPIO fix, which landed on a parallel branch.)*

**`Machine::snapshot` carries `Memory::slept_usec`** (format `IPODSNP4`, older images refused), so a
restored machine's simulated clock is the one the snapshot was taken with. Three tests cover it —
including a negative control that reproduces the old format and asserts the clock falls back by
exactly the amount dropped — and `cargo test --release` in `eapp-loader` goes 30 → 33 (45 once the
parallel `ipod-film` work is merged in, which brings twelve of its own). The full-boot
baseline is `diff`-identical across the change, as it must be: a cold boot never restores.
research/10 Addendum 31 §1–2.

**One compile fix.** `report_headless` here read `Ata::command_count`, a field the `Capped<T>` work
(research/12) had already replaced with `commands: Capped<…>` — so this crate **did not build** on
the merged tree, from the moment it landed until 2026-08-14. It is `commands.seen()` now, which is
the census; `commands.sample().len()` would be the cap wearing a census's clothes, which is that
document's whole subject.
