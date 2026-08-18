# Known bugs

Things that are **wrong**, as opposed to things that are **absent**. Three lists, deliberately kept
apart, because merging them is how a project starts describing its gaps as choices:

| | |
|---|---|
| **This file** | defects. It behaves differently from the hardware and that difference is not intended |
| `README.md` § *What does not* | features that were never built. No audio, no USB — absent, not broken |
| `research/04-bypass-ledger.md` | deliberate fakes, each with a written condition for retiring it. Known, chosen, and load-bearing |

A bug that turns out to be a bypass belongs in the ledger. A bypass nobody can justify any more
belongs here.

---

## The cold boot takes longer in simulated time than hardware does — MOSTLY EXPLAINED 2026-08-18

**It was the clock, and the clock was ours.** `--clock` sets how many interpreter instructions make
up one simulated microsecond, and the real PP5021C is 75. It defaulted to **5** — adopted in
research/03 as a deliberate accelerant, because the bootloader polls with timeouts and a low clock
skips its delay loops, so a fixed instruction budget reached further. It was never turned back.

At 5, a 1.6 G boot is 320 simulated seconds. At 75 it is 21, against hardware's five or ten. The
factor of thirty this entry could not account for was fifteen of clock and two of everything else.

The residue is real but ordinary, and one part of it is now identified: **every ATA command
completes in a flat 50 µs**, where a 1.8-inch 4200 rpm drive needs roughly 14 ms of seek and
rotational latency before it transfers anything. That makes the emulated machine *faster* than
hardware at the disk, not slower, which is its own fidelity bug — it is why the Apple logo does not
dwell — but it means the remaining simulated-time gap is not the drive waiting.

**A last observation, because it cost a day elsewhere:** the low clock was not merely a measurement
artefact. It made the machine's own sense of time run fifteen times fast, so *anything that waits*
behaved wrongly — the operator found it by playing Brick, where the ball moved so fast the game was
unplayable, not by reading a counter.

**What you see:** the Apple logo, then a white screen for most of a minute. The white screen is
faithful — the hardware shows one too — but not for that long. The footer now carries a progress
bar during the boot, so at least it is visibly working rather than apparently hung.

**How you would know it is fixed:** a cold boot that reaches the language picker in a simulated time
of the same order as the hardware's, without the instruction count falling.

## ~~The hold switch does not reach RetailOS after boot~~ — FIXED 2026-08-18

The diagnosis in this entry was right and the conclusion was reachable from it: RetailOS reads the
line four times in a whole boot, which is initialisation and not a poll, so it learns about every
later movement from a **GPIO interrupt** — and this emulator modelled no GPIO interrupt at all.

RetailOS asks for it in as many words. It writes `GPIOA_INT_EN = 0xe0` once, from `0x00281c20` —
bits 5, 6 and 7, where bit 5 is this switch — and `GPIOA_INT_LEV = 0` from the very next store. Then
it waits. The block is now modelled from `ipodloader2` and Rockbox, which agree address for address.

**Engaging** hold fires the interrupt and Apple's own padlock appears in the title bar. **Releasing**
does not fire — the released level does not match `INT_LEV` — and RetailOS notices anyway, because
once locked it polls the pin rather than waiting. So the release half needed no code, and the four
reads that made this look broken were never the whole story.

## The brightness slider does nothing, and the dimmer is reading the wrong signal — 2026-08-18

Moving brightness in RetailOS's settings does not change the panel, and the level the window shows
is not a brightness at all.

`Backlight` counts pulses on **`GPIOB_OUTPUT_VAL` bit 0x10** and decides direction from how long the
pin stayed low: shorter than `BACKLIGHT_STEP_USEC` steps up, longer steps down. The threshold is
100 µs because **Rockbox's** `backlight_hw_brightness` delays 10 µs to brighten and 200 µs to dim.

Rockbox is not the firmware this emulator runs, and the widths say so. Instrumented over a 2.2 G
boot, the eight pulses measure:

```
0 µs · 1 µs · 28 157 µs · 10 232 702 µs   (and four more at 0)
```

Nothing there is 10 or 200. Six land under the threshold by being **zero** — both edges inside the
same simulated microsecond — and two are separated by tens of milliseconds and ten *seconds*. Those
are not the two ends of a pulse; they are unrelated events being paired by a model that assumes
every falling edge is a dimmer step.

Rockbox is also explicit that this is the wrong pin for the job. `backlight_hw_brightness` pulses
**`GPIOD_OUTPUT_VAL` bit 0x80**; `GPIOB` bit `0x08` and `GPIOD` bit `0x80` together are the
*enable*. Our `0x6000d024` bit `0x10` is neither, and `0x6000d02c` is written exactly twice in a
whole boot, at init, with `0x20`. So GPIOB bit 0x10 is almost certainly an enable line — a handful
of toggles for screen-on and inactivity — and the level derived from it is an artefact of counting
them.

**What you see:** the slider moves and the panel does not. Worse, when the window *does* dim, the
figure it dims by is not the device's brightness.

**Confirmed on a live machine, 2026-08-18.** Driven over the control socket to Settings →
Brightness, RetailOS's own slider moves from full to a quarter — it is on screen, it responds, the
firmware is doing its half. Across that:

```
before:  backlight=32/32 up=46 down=2
after :  backlight=32/32 up=46 down=2      ← the model saw nothing at all
unmapped writes: none, before and after
```

Two facts, and the second is the useful one. The dimmer counts **zero** steps while the slider
travels its whole range, so the pin it watches is not the one brightness goes to. And **nothing is
written to an unmodelled address**, so the real write lands in a device this emulator already has —
it is being received and then not understood, rather than missed. The PCF50605 over I²C is the
prime suspect, since it drives the panel supply.

The level had also already railed: reaching that screen at all took it from 20 to 32 with **46 up
against 2 down**, because scrolling keeps the backlight awake and every one of those toggles is
counted as a brightness step.

**How you would know it is fixed:** moving the slider through its range changes the level
monotonically and in the same direction, and the count rises with the number of steps asked for.
Finding the register comes first, and the instrument for that is an I²C write census — which does
not exist yet.

This is the fifth model defect in this project first attributed to missing hardware and found to be
a misread of a signal we already had.

## `MENU`+`SELECT` and `PLAY` are delivered and ignored

Held for 400 M instructions at the main menu, the machine keeps running (`research/10` Addendum 31
§5). On a real 5G that pair is caught by the wheel controller or the PMU before the OS is involved,
and neither is modelled. The window says so on screen rather than pretending the buttons work.

## Four values in the co-processor transport are chosen, not measured

The display path was derived from RetailOS's own parser rather than from a datasheet, and four
values in it were picked because they worked. There is also **no timing model** in that transport at
all — every reply is instant. A bug that only appears when a reply is late is therefore invisible
here, and would be a difference from hardware nobody would see until real timing arrived.

Listed in `research/04` with retirement conditions. Repeated here because "it works" and "it is
right" are not the same claim.

## Purchased titles do not launch

Apple's DRM refuses them. The identity it binds to is understood — the 8-byte FireWire GUID in
`sysinfo_t` — but supplying the right one gets past identity and into a keystore that returns a null
context, and the wall after that is a control-flow-flattened function.

**Corrected 2026-08-18: it does not "return failure".** The context flag at `0x14937190` is written
thirteen times in a boot, **all zero**, by three PCs — two of which are the allocator and memset
paths that zero the same buffer elsewhere. Nothing ever writes a verdict there. So the check is not
running and deciding no; the **success path never executes**, which is a different search: find what
would write it, and why we never arrive.

Three hypotheses died the same day and are recorded so nobody re-runs them. It is **not the second
core** — `wake_cop` first executes at 1 700 032 431, after the refusal at 1 628 342 943, and
`--cop-awake` is identical to the instruction across the whole path. It is **not the clock** — a 6 G
A/B at `--clock=5` and `--clock=75` lands on the same 933 ATA commands, the same framebuffer, and
the flag zero in both arms. It is **not unmodelled hardware** — of 164 registers touched in the
`0x60000000` window and 42 in `0x70000000`, six have no mention anywhere in the model, all are read
between four and eight times, and none from the DRM's own code.

Arguably not a bug: the emulator is doing what the firmware tells it to. It is here because someone
will reasonably expect a game they bought to run, and it will not.

---

## Not bugs, though they look like ones

- **A white screen during a cold boot.** The hardware does this too. Its *length* is the bug above.
- **The charging screen after a while.** Authentic 5G behaviour with a charger attached.
- **"Restore from iTunes" after `make-disk`.** The bundle's updater family does not match the iPod
  your NOR dump came from. The window now says so before it boots, and `make-disk` prints the family
  on the command line, so this should be hard to reach by accident.
- **`aupd` running the flash updater on first boot.** Correct — it takes two boots.
  `ipod-boot flash-update` is the recipe.
