# What the hardware must supply

"112 registers only RetailOS touches" ([research/18](18-differential-register-map.md)) is a list of
*coverage*, not of error. Most of those 112 are ordinary read/write cells the firmware programs and
reads back; a plain memory region models them perfectly. Chasing all 112 would be work without a
criterion.

The sharper question is: **which addresses does RetailOS read that nothing ever wrote?** Those are
pure hardware *inputs* — values the firmware expects silicon to produce. We have no silicon, so we
answer with whatever the region holds, which is zero. Every one is a place this emulator is
inventing an answer, and the list is short.

`--input-regs=BASE:SIZE` reports them.

> **Re-measured 2026-08-13 against the fixed instrument. The table below is superseded — the
> conclusion under it survives.** `--input-regs` was one of the two instruments silenced by the
> `count()` hoist bug (research/20 Addendum 8 §8), and the worse-affected of the two: `input_probe`
> was missing from the hoist in **`read32` as well as `write32`**, so it saw only *byte* accesses
> into mapped regions. Controlled by rebuilding the pre-fix binary and running it on the identical
> machine with identical flags: the old instrument reported **57** read-before-write rows across the
> two blocks, the fixed one **72**. Three addresses were invisible to it outright — `0x60004000`
> (167 264 reads), `0x60004100` (167 264) and `0x60004020` (12) — and `PROCESSOR_ID` was undercounted
> by 2.9× (44 510 against 128 150). The corrected table follows; the counts in it are also on a
> different machine, since the second DMA controller has been modelled since (Addendum 9) and the
> boot now settles at @123.4 M rather than running to a 400 M budget.

> **Re-measured again 2026-08-14 against the corrected stop window, and the count moves by one.**
> The table below was taken with `--stop-when-idle=40000000`, which research/20 Addendum 11 has since
> shown ends every retail run at ~@123 M while the machine is still executing at full rate. Same
> build, same machine, same flags, the *only* variable the stop condition:
>
> | | `--stop-when-idle=40000000` | `--stop-when-idle=400000000` |
> |---|---|---|
> | read-before-write rows | 72 of 204 touched | **73 of 205** |
> | never written at all | 28 | **29** |
>
> The counts in every row are also 5–12× higher, because the run is 5× longer. Both are updated
> below. **The one new address is `0x60006038`** — see the row, and research/20 Addendum 14 §7.

Current retail run — `retail-boot.sh --clock=5 --stop-when-idle=400000000`, budget 600 M, ending on
`BudgetExhausted` at @599 999 952 — **73 addresses** are read before the firmware ever writes them, of
which **29** are never written by the firmware at all. Those 29, grouped:

| address | name (Rockbox `pp5020.h`) | reads before write | modelled? |
|---|---|---|---|
| `0x60005010` | `USEC_TIMER` | 9 588 012 | ✅ yes |
| `0x60000000` | `PROCESSOR_ID` | 1 982 526 | ✅ yes |
| `0x60004000` | `CPU_INT_STAT` | 1 898 632 | ✅ yes — the emulator writes it from `service_interrupts`, which is `internal` and so excluded from this count. Firmware only ever reads it, correctly |
| `0x60004100` | `CPU_HI_INT_STAT` | 1 898 632 | ✅ yes, same |
| `0x60005004` | `TIMER1_VAL` | 947 904 | ✅ yes |
| `0x7000c01c` | I²C `STATUS` | 11 094 | ✅ yes |
| **`0x6000d13c`** | **`GPIOL_INPUT_VAL`** | **3 560** | 🟡 **seeded `0x08` 2026-08-14** — no charger, no USB charger; was ❌ "we answer 0", which on an active-low line asserted *charger present*. RetailOS reads bit 3 of this word 130 times a boot and drew the "Charged" screen because of it ([research/20](20-the-resource-image.md) Addendum 30) |
| `0x70000000` | `PP_VER1` | 260 | ❌ **we answer 0** |
| `0x7000c140` | **the click wheel** — `0x7000C100`/`0x7000C140` per the reverse-engineered PP502x map ([research/15](15-the-chip-inventory.md) §2) | 60 | ✅ **modelled 2026-08-14** (`--clickwheel`) — was ❌ "we answer 0". All 60 byte-reads are 15 word-reads by Apple's bootloader at `0x4000e59c`: three calls to its query routine, each failing all five retries against a zero. With the device modelled it is 3 word-reads, one per call, each answering `0x8000023a` — and Apple's button-decode block at `0x4000e5c0` runs for the first time. See [research/20 Addendum 16](20-the-resource-image.md#addendum-16-the-click-wheel-modelled-and-the-only-thing-reading-it-is-apples-bootloader) |
| `0x60009004` | second DMA controller, ch0 `STATUS` | 48 | ✅ yes, since Addendum 9 |
| `0x6000603c` | `PLL_STATUS` | 24 | 🟡 bypass #8, "always locked" |
| `0x60004020` | `CPU_INT_EN_STAT` | 12 | ✅ yes |
| **`0x60006038`** | **unnamed — not in `pp5020.h` at all**, between `PLL_CONTROL` (`+0x34`) and `PLL_STATUS` (`+0x3c`) | **8** | ❌ **we answer 0**, and the shape of the reader says that is wrong. `0x0000055c` is a five-instruction leaf with three call sites: `ldr r1,[r0,#0x38]; ldr r0,[r0,#0x38]; eor r0,r1,r0,lsl #16; bx lr` — **read the same register twice and mix the halves**, which is how firmware samples a free-running counter or an entropy source. Answered 0 both times, the mix is identically 0 on all four calls. **Invisible at the old stop window** |
| `0x70000004` | `PP_VER2` | 8 | ❌ **we answer 0** |
| **`0x6000d030`** | **`GPIOA_INPUT_VAL`** | 7 | 🟡 seeded `0x20` by `map_hardware` — hold off, no headphones |
| `0x6000500c` | `TIMER2_VAL` | 4 | ✅ yes |
| `0x60009024`, `0x60009044`, `0x60009064` | second controller, per-channel `STATUS` | 4 each | ✅ yes |
| `0x6000b004`…`0x6000b0e4` (8, stride `0x20`) | first controller, per-channel `STATUS` | 4 each | ✅ yes |
| **`0x6000d03c`** | **`GPIOD_INPUT_VAL`** | 2 | ❌ **we answer 0** |
| **`0x6000d034`** | **`GPIOB_INPUT_VAL`** | 1 | 🟡 seeded `0x01` by `map_hardware` — not charging |

## The result

**The conclusion survives both re-measurements.** Every address the original table named is still in
the list, and everything the fixed instrument added is either modelled or one of the two GPIO ports
`map_hardware` now seeds. What we still invent from nothing is ~~**five**~~ ~~**six**~~ ~~**five**~~
**four**
addresses: ~~the GPIO input ports `GPIOL` (`0x6000d13c`, the load-bearing one at 3 560 reads) and~~
**`GPIOL` leaves the list on 2026-08-14, seeded `0x08` per Rockbox's per-bit map like `GPIOA` and
`GPIOB` before it** — leaving
`GPIOD` (`0x6000d03c`), the two SoC **version registers** `PP_VER1`/`PP_VER2`, and — added by the
2026-08-14 window correction — the undocumented `0x60006038`, which the firmware reads twice and
XORs. `0x7000c140` — **the click wheel**, which the original pass missed entirely and which was, for
a project named after it, the most interesting addition on the list — **leaves the list on
2026-08-14**: it is a modelled device now, not an invented answer (research/20 Addendum 16). Note
what that entry was actually reporting while it sat here: not "a register nobody answers" but "a
register we answer with a value that fails Apple's own validity test", which is a different and
worse thing — the firmware was taking its **error** path, five retries deep, on every call.
`GPIOA` and `GPIOB` have since been seeded with per-bit values read off Rockbox's source for this
target, which is this list's first repair.

The GPIO input ports are where an iPod's world arrives: hold switch, headphone detect, USB and
accessory presence, charger presence, the click-wheel interrupt line. ~~`GPIOL` and~~ `GPIOD` we
still
report as **zero, permanently**. On an active-low line, zero reads as *asserted* — so for ~~those
two~~ that one
we are not telling RetailOS "nothing is connected", we are telling it **everything is connected at
once**, forever. ~~`GPIOL` is the one that matters: leaving it zero says "charger present", which
Apple's bootloader needs to hear because our PMU cannot yet survive the battery check it does
otherwise (see the bisect below).~~
**`GPIOL` was the one that mattered, and this paragraph's own logic turned out to be the whole
finding**: leaving it zero said "charger present", RetailOS believed it, and drew the charging screen
for as long as the emulator has been able to draw anything. It is seeded `0x08` since 2026-08-14, and
the reason it could not be before was a defect in our PMU rather than a fact about the battery check
— [research/20](20-the-resource-image.md) Addendum 30, and the box in §"GPIOL, and what it exposed"
below.

`PP_VER1`/`PP_VER2` are worse in kind: a part-number register answered as 0 tells the firmware it is
running on a SoC that does not exist.

## Why this is the right list

It is derived rather than guessed, it is bounded, and each entry has a falsifiable next step: supply
a plausible value and measure whether behaviour changes — the same experiment already run on
`0x6000d13c` ([research/11](11-rtxc-and-the-video-coprocessor.md) §48), which is how we learned that
forcing it high hangs the *bootloader*. That result now reads differently: it was not evidence that
the line is irrelevant, it was evidence that **all-ones is the wrong value** for a port whose bits
mean different things.

The honest position is that all four GPIO ports need **per-bit** semantics, not a blanket value, and
that Rockbox's `button-clickwheel.c` and `power-ipod.c` document what several of those bits mean on
this exact hardware. Two of the four have since been given exactly that treatment.

## What this does not claim

Fixing these five is **not** known to fix the boot loop. The blocker is a member call through a
delegate nothing writes, and no measurement yet connects it to a GPIO read. What this list does is
enumerate, exhaustively and by construction, every place the machine currently invents an input —
so that "RetailOS is reacting to something we told it" can be checked rather than assumed.

## Modelling them, one bit at a time

Rockbox's source gives per-bit meanings for this exact target:

| port | bit | meaning | polarity | source |
|---|---|---|---|---|
| `GPIOA` `0x6000d030` | `0x20` | hold switch | **active low** | `button-clickwheel.c:350` |
| | `0x80` | headphones inserted | active high | `button-clickwheel.c:355` |
| `GPIOB` `0x6000d034` | `0x01` | charging | **active low** | `power-ipod.c:90` |
| `GPIOL` `0x6000d13c` | `0x08` | main/FireWire charger | **active low** | `power-ipod.c:53` (IPOD_VIDEO) |
| | `0x10` | USB charger | active high | `power-ipod.c:56` |

A bare iPod — nothing plugged in, hold off, not charging — is therefore `A=0x20`, `B=0x01`,
`L=0x08`. Setting all three **broke the boot**: the bootloader stopped loading `osos` and halted
having drawn a partial screen.

Changing three things at once is how you learn nothing, so: bisect.

| configuration | `osos` loaded | resets |
|---|---|---|
| all three | ✗ | 1 |
| **GPIOA only** | ✓ | 37 |
| **GPIOB only** | ✓ | 37 |
| GPIOL only | ✗ | 1 |
| none (previous baseline) | ✓ | 37 |

**GPIOA and GPIOB are correct and free.** They are now the emulator's defaults: the machine no
longer claims the hold switch is engaged and the battery is mid-charge.

### GPIOL, and what it exposed — **RESOLVED 2026-08-14, and both halves below were our own defects**

> **The section that follows is retained for its shape and retracted in its conclusions.** `GPIOL` is
> now seeded `0x08` — *no charger* — by `map_hardware`, the boot proceeds, and the reason it did not
> before is that **this model's ADC reported every completed conversion as the value zero**: `busy`
> counted down inside `read_reg(0x30)`, which also answered `0` while counting, so Apple's two-byte
> `ADCS1`+`ADCS2` poll had one byte served from the in-flight state and the next from the completed
> one. There was never a threshold this model could not reach; there was a converter that could not
> report a number.
>
> The "not established" subsection below is worse than retracted — **it is not a measurement at all.**
> `--pmu-adc=CH=VALUE` pushed into `m.mem.pmu` *before* `--pmu` constructed the chip, and the
> construction then overwrote it. The flag has been a no-op in every run it has ever appeared in,
> while printing a confirmation line. Fixed the same day; with it working, channels `0x0` and `0x4`
> gate nothing and **channel `0x3` gates at exactly `0x080`** — `0x07f` halts, `0x080` boots.
>
> The bit that this section says "makes Apple's bootloader refuse to boot" is also the bit RetailOS
> reads 130 times a boot to decide whether to draw the charging screen or its menu. Full account,
> with the A/B and the pictures: [research/20](20-the-resource-image.md) Addendum 30.

`GPIOL` bit `0x08` says *no charger*. That single bit makes Apple's bootloader refuse to boot — and
the reason is not the bit:

| GPIOL | battery ADC | boots |
|---|---|---|
| `0x08` no charger | default | ✗ |
| `0x18` USB charger present | default | ✗ |
| **`0x08` no charger** | **forced to full scale** | **✓** |
| `0x00` charger present | default | ✓ |

**With no charger the bootloader checks the battery, and our PMU cannot answer convincingly.**
`powermgmt-ipod-pcf.c` gives the scale — `mV = (adc * 6000) >> 10` — so the model's catch-all of
`0x200` is **3000 mV**, a flat cell. But raising it to `0x2c0` (4125 mV, a healthy Li-ion) is **not
enough**: only full scale, ~5994 mV, satisfies the check. That is not a battery voltage, so the
bootloader is reading some channel or threshold this model does not understand, and picking a number
that happens to pass would be exactly the kind of invented answer this document exists to enumerate.

~~So `GPIOL` is deliberately left reporting *charger present* — a known, documented lie — until the
PMU's ADC channel map is understood. Its retirement condition is now precise: **identify which ADC
channel the bootloader tests with no charger attached, and what it requires.**~~
**Retired 2026-08-14.** The channel is `0x3` and what it requires is `>= 0x080`; the reason no value
worked before is in the box at the top of this section. `map_hardware` seeds `0x6000d13c = 0x08`.

### A regression caught by having two OSes

The first attempt also cut Rockbox from 29 frame updates to 2 and its menu to a fragment. That is
the value of keeping a working reference: the change looked defensible in isolation, and a control
that renders a known-good screen said otherwise immediately. Both are back to baseline —
RetailOS `Running 'osos'`, Rockbox 29 frames and 74 057 pixels.

## Narrowing the charger/battery gate: channel 3, and an unresolved requirement

`--pmu-adc=CH=VALUE` answers a single ADC channel on its own scale, and the model now logs every
conversion. That identifies the gate precisely.

**With a charger present** the bootloader converts channels `0x0` (×2) and `0x4` (×7).
**With `GPIOL` reporting no charger** it converts `0x0` (×2), `0x4` (×2) and — only in this case —
**channel `0x3`, three times.**

Rockbox's `pcf5060x.h` names the mux:

```c
#define PCF5060X_ADC_BATVOLT_RES     0x0
#define PCF5060X_ADC_ADCIN1_RES      0x2   /* Rockbox's ADC_BATTERY */
#define PCF5060X_ADC_ADCIN1_SUBTR    0x3   /* <- the bootloader's no-charger check */
#define PCF5060X_ADC_BATTEMP         0x4
```

Channel 3 is **ADCIN1 in subtractor mode** — the same physical input as Rockbox's battery channel,
but with an offset removed and rescaled, so it does not share channel 2's millivolt scale. Answering
it with the model's catch-all was answering a subtractor reading on a resistive scale.

### ~~What is *not* established~~ — **NOT A MEASUREMENT. `--pmu-adc` was a no-op.**

> Every sweep in this subsection swept a value the device never received (see the box at the top of
> §"GPIOL, and what it exposed"). The `--pmu-force` rows *are* real, and they are real evidence of
> the model defect rather than of the firmware: `force` short-circuits ahead of the `busy` logic, so
> `0x30=0xff` alone froze the countdown and the ready bit never set, while `0x31=0x83` alone still
> handed `ADCS1` the synthetic zero. Only both together dodged both halves of the bug. Measured
> properly on 2026-08-14: channel `0x3` gates at `0x080`, channels `0x0` and `0x4` do not gate at all.

Sweeping channel 3 alone from `0x200` to full scale **never** lets the boot proceed, and neither does
channel 0, channel 4, nor all three together. The only configuration that boots is forcing registers
`0x30 = 0xff` **and** `0x31 = 0x83` — which sets a full-scale value *and* pins conversion-ready
permanently on. Either alone fails:

| | boots |
|---|---|
| high value, normal ready timing (`0x30=0xff`) | ✗ |
| normal value, ready always set (`0x31=0x83`) | ✗ |
| per-channel values at full scale | ✗ |
| **both `0x30=0xff` and `0x31=0x83`** | **✓** |

So the requirement involves **both the result and the conversion-ready handshake**, and this model's
"a conversion takes observable time" behaviour (`busy = 2`) is implicated alongside the value. That
is as far as measurement takes it today.

~~**The honest state:** the channel is identified (`0x3`, `ADCIN1_SUBTR`), the trigger is identified
(`GPIOL` bit `0x08`, no charger), and the acceptance condition is **not** identified. Picking the
pair of numbers that happens to boot would be precisely the invented answer this document exists to
enumerate, so `GPIOL` stays at charger-present until the condition is understood — most likely from
the PCF50605 datasheet's subtractor-mode scaling, which this project does not have.~~

**The honest state, 2026-08-14.** The channel is `0x3` `ADCIN1_SUBTR`, the trigger is `GPIOL` bit
`0x08`, and the acceptance condition is **`>= 0x080`** — 128 of 1023, one eighth of full scale, an
exact power of two, which reads as a bit test rather than a millivolt comparison. Bisected with a
calibrated instrument (120 M-instruction boots; the 30 M budget tried first failed its own positive
control) and with both controls carried: `0x07f` halts, `0x080` boots, and channels `0x0` and `0x4`
swept to `0x000` and `0x3ff` change nothing. No datasheet was needed; a working flag was.

## The tool the reverse engineering actually needed

Static call-graph analysis has dead-ended on this binary **four separate times** — §46 (three dead
ends in a row), §52, and again in [research/18](18-differential-register-map.md). Always the same
cause: RetailOS dispatches virtually, so `BL` targets are only part of the graph and the interesting
edges are invisible to a scan.

That is a tooling gap, not a hard problem. At runtime the question is trivial.

`--callgraph[=ADDR]` records every branch edge actually taken — direct `BL` *and* indirect (`BX`,
`mov pc,rX`, `ldr pc,[…]`) — deduplicated with counts, so the map is bounded by distinct edges rather
than executed instructions. A boot records about **10 300 edges**. With an address it reports the
runtime *callers* of the function containing it:

```
runtime callers of 0x000cd844:  0x000d05cc, 0x001d32cc
runtime callers of 0x000fb8a4:  0x0011e1c8
```

Those are answers the static graph returned "none" for.

### And it found an unmodelled chip

The same run's I²C log shows a third device address:

```
dev 0x10  1837 transfers      (PMU write, 0x08)
dev 0x11  1827 transfers      (PMU read)
dev 0x34   432 transfers      <- address 0x1a
```

I²C `0x1a` is the **WM8758 audio codec** — [research/15](15-the-chip-inventory.md) lists it as
present and not boot-critical. We model nothing there, so every one of those ~~432~~ transfers is
answered by the bus fill rather than a chip. It appears on the retail-NOR path and was invisible
before. Not necessarily related to the blocker; recorded because an unmodelled device that the
firmware is actively driving is exactly the kind of thing that has cost this project weeks.

> **Count corrected 2026-08-13 (research/20 Addendum 13 §2), and it is 52, not 432.** The 432 came
> from a different run at a different address encoding. On the baseline recipe the log reads
> `dev 0x10 1829 · dev 0x11 1823 · dev 0x34 52`, and the useful part is the register numbers rather
> than the total: `reg 0x54 ×5 · reg 0x6f ×4 · reg 0x06 ×3 · reg 0x6b ×3`. That is a real register
> map to check a codec model against, on a boot we already run.

## ~~The delegate, measured properly~~ — WRONG, retracted 2026-08-13

> **The measurement in this section is an instrument artefact and the conclusion drawn from it does
> not stand.** `--watch-range` could not see word-sized writes into a mapped region — SDRAM is one —
> so on a heap record it recorded only `strb`-class stores. A pointer field is written with `str`.
> The instrument was blind to precisely the access class that would have refuted the finding. See
> research/20 Addendum 8 §8 for the mechanism and the fix, and the control below for what it costs.
>
> **The control, run on the identical machine with the identical flags.** Rebuilding the pre-fix
> binary and re-running `--watch-range=0x13e27424:0x74` reproduces this section's shape — a handful
> of offsets, small counts. The fixed binary on the same run reports **3 040** byte-writes against
> the old build's **212**, and every one of the record's 29 words written, `+0x20` among them.
> `--storeaddr` — which hooks `note_store_pc` at the top of `write32`, ahead of the hoist, and was
> never broken — independently records stores landing on `+0x20` from `0x00000124`, `0x0007cc18`,
> `0x0019f7b0`, `0x001ac424`, `0x001ac83c`, `0x001ac3bc`, `0x001ac82c`, `0x0000011c`, `0x00223a64`
> and `0x002234cc`. Two instruments, two code paths, one answer, and it is not "nobody".
>
> This was already established once, independently, and not carried back here: research/20 §2 used
> `--storeaddr` over the `+0x20` word of **all 790** heap objects of this shape and found **95 456
> stores — every single object written**. That measurement and this table cannot both be true, and
> the one with the working instrument wins.
>
> **What survives.** The heap-independence observation below still stands on its own evidence (same
> thunk, same `r0 = 0`, same caller on both NOR images). What does not survive is
> "`FUN_002102bc` fills four fields and steps over the delegate" — the premise for that was the zero
> in the table, and there is no zero. The address `0x13e27424` is in any case no longer this record
> on the current machine: `--storeaddr` there now catches ASCII path fragments (`"\Dev"`, `"ecij/"`,
> `"Phot"`), i.e. the allocator has recycled it. Re-deriving the record's address on the current
> build is the first step of any successor to this section.

`--watch-range=BASE:LEN` records every write into a structure with its PC — the thing `--watch`
could not do, and whose absence produced a false conclusion in [research/16](16-rockbox-as-oracle.md).

First it caught a mistake of my own. The object address `0x13e26f8c` was derived on the **prototype**
NOR; on the retail NOR the heap differs and the handle is at **`0x13e27444`**. The fault is otherwise
identical — same thunk, `r0 = 0`, `r1 = 0xea00007a`, same caller `0x0011e1cc` — so it is
**independent of heap layout**, which is worth knowing on its own.

Watching the correct 0x74-byte record across a whole boot:

| offset | byte-writes | written by |
|---|---|---|
| `+0x10` | 7 | `0x00210650` |
| `+0x14` | 5 | `0x00210664` |
| `+0x18` | 1 | `0x00210678` |
| `+0x28` | 2 | `0x0021068c` |
| `+0x44` | 4 | `0x00243e98` |
| **`+0x20`** | ~~**0**~~ **artefact** | ~~**nobody**~~ **`str`-class stores the instrument could not see** |

The first four come from one tight sequence at stride `0x14` inside **`FUN_002102bc`** — the record's
real initialiser, which fills four fields and steps straight over the delegate. (§52's candidate,
`0x00211a70`, was the prototype run's; this is the retail one.)

`FUN_002102bc` takes **nine parameters**, computes `param_4[3] - param_4[1]`, casts `param_3` to
`short`, iterates a collection through virtual slots `+0x40` and `+0x30`, and allocates. That is
layout or geometry work, not device setup — consistent with everything else about this fault being
above the hardware.

### The loop this closes

Three instruments now compose into one workflow, each built because a *measured* failure demanded
it:

1. `--callgraph=ADDR` — runtime callers, including through virtual dispatch (four static dead ends)
2. `--watch-range=BASE:LEN` — every write to a structure, with PC (two false conclusions)
3. Ghidra on the exact address the first two produce

~~Applied once, that took the question from "an object somewhere has an unwritten field" to
"`FUN_002102bc` initialises four of this record's fields and not the fifth".~~ **Retracted
2026-08-13.** Instrument 2 was blind to word stores into mapped regions, so the "and not the fifth"
half was never measured — see the banner on "The delegate, measured properly" above. What the loop
actually did was take a wrong premise and locate a function very precisely against it. The
composition is still the right workflow; two of its three legs were sound. The lesson is that a
workflow inherits the blind spots of every instrument in it, and reports them as a *converging*
answer rather than a shared one.

### A gap in my own tool, caught by using it

The first `--callgraph=0x002102bc` reported **zero** runtime callers for a function that
demonstrably runs. The graph was recording only `BL` and indirect branches — and **a plain `B` is
how ARM compilers emit a tail call**, so an entire class of edge was missing. Recording every
non-fall-through transfer takes the graph from 10 296 edges to **20 766** and the callers appear:

```
0x0025ebd8 -> 0x002100f4  x36     <- the hot one
0x0020fe88 -> 0x0020fff0  x1
0x00249ef0 -> 0x00210000  x1
```

So `0x002102bc` is *inside* a larger function entered at `0x002100f4`, reached 36 times from
`0x0025ebd8`. The "nearest preceding push-lr" heuristic that produced `starts.json` had split one
function into two.

Worth stating plainly: a tool reporting "zero callers" is indistinguishable from a true negative,
and this one was wrong. It was caught only because the answer contradicted a known fact — the
function writes fields we watched it write. **Instruments need their own controls**, the same
lesson as §36, §48 and [research/16](16-rockbox-as-oracle.md), now applied to something I built
rather than something I measured.

### Read from the running machine instead

Ghidra could not be trusted here (see `tools/ipod-boot/GHIDRA.md`), so the two frames were read with
`--disasm`, which reads the machine rather than a model of it.

**The caller**, `0x0025ebd0` — three instructions, a tail call:

```
ldr r1, [pc, #0x4]   ; 0x14939f60
ldr r0, [pc, #0x4]   ; 0x149ee0fc
b   0x002100f4
```

Two constants into `r0`/`r1`, then a plain `B`. (That `B` is exactly the tail call the call graph
was missing until it was fixed to record every non-fall-through edge.) Both constants are
`0x14xxxxxx` — *past* the top of the 64 MB SDRAM at `0x14000000`, and the README already records
`osos` jumping through a thunk into `0x149xxxxx`, "mapped nowhere". Whether these are relocated at
run time or genuinely point outside RAM is **not yet established** and is worth its own measurement.

**The initialiser**, `0x00210640` — field by field, with `r0` as the record:

```
str  r1, [r0, #0x00]        strb r1, [r0, #0x10..0x13]   ; zeros
str  r2, [r0, #0x08]        mov  r3, #0xff
str  r2, [r0, #0x0c]        strb r3, [r0, #0x14..0x17]   ; 0xff
```

This is the constructor, and it corroborates the range watch independently: the object dump showed
`+0x14 = ffffffff` and `+0x18 = ff`, which is exactly `mov r3,#0xff` written as four bytes.

**It sets `+0x00`, `+0x08`, `+0x0c`, `+0x10..0x17` — and never `+0x20`.**

So `+0x20` is not a field this constructor forgot; it is a field **some other code path is supposed
to fill**, and that path is not running. The question is no longer "which constructor" but "which
setter", and the same three instruments answer it: the runtime callers of this constructor, what
they do next, and a watch on the record while they do it.

### The `0x149xxxxx` constants are fine

Flagged in the previous section as possibly pointing outside RAM. They do not. Both resolve and hold
real data:

```
0x149ee0fc  94 43 67 00                        -> 0x00674394, a pointer into the vtable region
0x14939f60  00000000 149dd638 149dd750 0063ee44
```

`0x149ee0fc` has **bit 26 set**, and RetailOS's SDRAM MMAP window leaves bit 26 uncompared — the
don't-care decoded in [research/11](11-rtxc-and-the-video-coprocessor.md) §33 — so it aliases onto
`0x109ee0fc`, inside the 64 MB. `--verify-memory` reports no fast/slow disagreement on this path, so
the alias is being resolved consistently.

Recorded because the README's older note about `osos` jumping into "`0x149xxxxx`, mapped nowhere"
predates the MMAP decode and reads as an open problem. It is not one: that range is SDRAM, seen
through the window RetailOS itself programmed.

## Function boundaries, measured instead of guessed

Every static step in this investigation has rested on `starts.json` — function starts found by
scanning for the nearest preceding `stmdb sp!, {…, lr}`. That heuristic split
`0x002100f4`/`0x002102bc` into two functions, which is how a caller search returned "zero callers"
for something that demonstrably runs.

The runtime edge graph is better evidence, and it is already sitting there. An address reached by a
`BL` **is** an entry point; no inference required. `--callgraph-dump=FILE` writes all 20 766 edges,
and splitting them by instruction type gives:

| | count |
|---|---|
| **call targets — true entry points** | **2 209** |
| jump targets (tail calls, branches, returns) | 11 404 |
| call targets the push-lr heuristic **missed** | **852** |
| heuristic "starts" only ever *jumped* to — i.e. mid-function | **598** |

**Wrong in both directions, substantially.** It missed 852 real functions and invented 598
boundaries that are not entry points at all. Every previous static call-graph walk was run against
that.

The measured set is written to `entrypoints.json`. It covers only what executed on one boot — 2 209
of the image's ~12 900 candidate functions — so it is a floor, not a census. But within the code
that actually runs, which is the only code this investigation cares about, it is *evidence* rather
than pattern-matching.

This is the last of the substrate work: names imported into Ghidra, boundaries measured, callers
resolvable through virtual dispatch, and every write to a structure observable. The next question —
which setter fills `+0x20` — is the first one in a while that can be asked with all four in hand.

## ~~A sibling with the field set — which turns out to be stale heap~~ — WRONG, retracted 2026-08-13

> **The "stale heap" verdict rests entirely on a `--watch-range` zero, and that zero is an
> instrument artefact.** Same cause as the section above; this one is the cleaner demonstration,
> because the claim reproduces exactly under the broken instrument and inverts under the fixed one,
> on the same machine, same flags, same run length.
>
> | `--watch-range=0x13e273b0:0x74` | total byte-writes | at `+0x20` (`0x13e273d0`) |
> |---|---|---|
> | pre-fix binary (the control) | 22, in six words at the record's tail | **0 — "nobody"** |
> | fixed binary | **670**, in all 29 words | **4, from `pc 0x001ac694`** |
>
> The control reproduces the reported shape down to which offsets appear: writes at the high
> offsets, silence at `+0x20`. That is what a width-sensitive instrument looks like when the field
> you are asking about happens to be written with `str` and the ones you can see happen to be
> written with `strb`. **A non-zero count elsewhere in the same range proved nothing** — the four
> offsets that did show up exercised the byte path, which was the working one.
>
> So `+0x20` here is **not** demonstrated to be uninitialised heap. It is written, by an
> identifiable instruction, on the current build.

If `+0x20` is fillable, some other record should have it filled. Dumping the records either side of
the failing one at stride `0x74`:

```
0x13e273b0  +0x00=0x00677920  +0x20=0x01005410   <- non-zero!
0x13e27424  +0x00=0x00000000  +0x20=0x00000000   <- the failing one
```

`0x13e273b0` has a vtable pointer at `+0x00` and a **non-zero delegate at `+0x20`**. That looks
exactly like the working case, and finding who wrote it would name the setter.

~~**Nobody wrote it.** `--watch-range` over that record's whole `0x74` bytes records writes to
`+0x1c`, `+0x68` and `+0x6c` — and none to `+0x20`. The value was already there when the run started:
it is **uninitialised heap that happens to be non-zero**, not an initialised field.~~ *Retracted; see
the banner above. The record is written in all 29 of its words, `+0x20` included.*

Worth stating because the dump alone is genuinely convincing, and one record having the field
"set" while the failing one does not is precisely the shape of evidence that invites a wrong
conclusion. ~~The range watch is what distinguishes them, which is the whole reason it exists~~ — and
the range watch could not distinguish them, which is the part that matters now. It was built to
resolve the ambiguity "wrote 0" versus "never wrote" that produced a false conclusion in
[research/16](16-rockbox-as-oracle.md), and it introduced a *second* ambiguity nobody was looking
for: "wrote a byte" versus "wrote a word". A new instrument built to close one blind spot opened
another, and the whole of this section is what that cost.

~~It also undermines the stride-`0x74` framing itself: if these were uniform records with a common
initialiser, the neighbours would show the same write pattern, and they do not.~~ *They do.* Under
the fixed instrument both records are written across their whole length, and `0x001ac424` /
`0x001ac84c` appear as first-writer in both — which is weak positive evidence for the uniform-record
framing this paragraph used the artefact to argue against. The `0x74` stride
came from `--retwatch` observing two pointer-advance instructions; that is good evidence for an
array *somewhere*, and weak evidence that this particular address is element *n* of it.

**Open, and stated as open:** whether `+0x20` is a delegate that a missing setter should fill, or a
field of a structure this investigation has mis-framed. Distinguishing them needs the type, and the
type needs the constructor's caller — which is where the next session starts. Note that the
successor to this question must **re-derive the record's address first**: `0x13e273b0` and
`0x13e27424` hold recycled heap on the current build.
