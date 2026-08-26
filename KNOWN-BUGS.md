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


## ~~Every drive this program builds stops at its own boot logo~~ — FIXED 2026-08-25

Reported by the operator from a clean first run: *"it never reaches RetailOS — it just gets stuck on
the synthesised bootloader logo and then stops."*

Isolated by swapping one variable at a time through the window's own start path. Same synthesised
ROM, same code, only the drive changed:

```
                                        ata    lit pixels   co-proc frames
a real iPod's drive (PRISTINE)          698      75 267           —
the drive the first run built            22       2 612           0
  ... and unchanged from 356 M instructions to 4.4 G
the same built drive, `rsrc` corrected   70      71 695           4
```

Those 2 612 pixels are the synthesised bootloader's own logo, which is exactly what the operator
described, and the machine never did anything after drawing it.

> ## RETRACTED 2026-08-26 — the last row of that table is not a `rsrc` measurement
>
> **`rsrc` is not what moved it.** Re-run with one variable and the arms proved distinct, two
> drives differing in exactly the two `rsrc` words and nothing else (`cmp -l`: `0x405c`-style
> single-field diffs at drive offsets 49215 and 49217, plus the one `aupd` byte at 49240):
>
> ```
> ROM                 rsrc                       ata   lit      buckets
> synthesised 5.5G    forced 0x10000000 / 0      290   2 612    20 196
> synthesised 5.5G    Apple's 0x00000000 / 0x600 290   2 612    20 196
> Apple's retail 5G   forced 0x10000000 / 0       70   71 695    1 812
> Apple's retail 5G   Apple's 0x00000000 / 0x600  70   71 695    1 812
> ```
>
> `70 / 71 695` is what **Apple's own ROM** gives on that drive, at either `rsrc` value. `2 612` is
> what a **synthesised** ROM gives, at either `rsrc` value. The original comparison moved the ROM
> as well as the drive and read the result as the drive's doing — §6's fifth shape, *"a comparison
> that let each arm resolve its own paths, so it compared two machines as well as two builds."*
>
> **The code change stands and the reasoning below it stands**; only the boot-effect claim is
> withdrawn. A built drive whose `rsrc` matches a real drive's measured state is still the right
> drive to build — it is now known to be a *correctness* fix with no measured effect on any boot,
> which is a different and much weaker claim than the one this entry made.
>
> What actually stops that boot is not on the drive at all. research/17 has the five-arm matrix:
> **every** drive built from `iPod_25.1.3` stops, on every ROM, and RetailOS 25.1.3 is not at
> fault — under Apple's real bootloader it loads, runs and reaches Apple's own "restore from
> iTunes" screen, which is the correct response to a 5G ROM holding a 5.5G OS.

**The cause is `mark_aupd_applied` doing half of what Apple's updater does.** An IPSW's firmware
partition is written to the drive verbatim, and the updater is then marked already-applied so the
drive boots its OS on the first power-up rather than the second. But the updater's last act is not
one byte: it also rewrites `rsrc`'s load address and entry point from the values a *shipped bundle*
carries to the values a *restored drive* carries.

```
                  a real iPod, post-restore   an IPSW as shipped
    rsrc  addr          0x10000000                 0x00000000
    rsrc  entry         0x0                        0x600
```

The function's own doc asserted the opposite in as many words — *"Nothing else in the partition is
touched: `osos` and `rsrc` are Apple's bytes, unaltered"* — and that sentence is why nobody looked.

**Not fixed, and now the thing in front:** the corrected drive stops at **70 ATA and 71 695 lit
pixels**, which is where `ipod-boot retail` also stops. That is the older `ipod-boot`-vs-`ipod-gui`
divergence recorded below, and it was hidden behind this one.

**Not established:** whether every IPSW family's post-update `rsrc` takes `LOAD_ADDR_5G`. The
reference drive measured is a 5G's; the drive that exposed this was built from a 5.5G bundle. One
family measured, the other inferred from `osos` sharing the constant.

**Also unfixed, and it is an instrument:** `--check-images` calls the broken drive **OK**. It reads
the directory, reports `osos` present and `aupd` not armed, and has nothing to say about an `rsrc`
that cannot be loaded where it claims.

## ~~The window's high-level boot put the OS beside SDRAM instead of in it~~ — FIXED 2026-08-25

Found by pressing `Start` on a first run. Every synthesised iPod died in about a third of a second:

```
stopped: lost 33554432 at 8388485 instructions
```

`33554432` is `0x02000000` and it is a **program counter** — `Stop::Lost(pc)`, the CPU having left
every mapped region. The arithmetic names the fault before anything is opened:
`8 388 485 × 4 = 33 553 940` and `0x02000000 − 33 553 940 = 0x1ec`, so the CPU walked from `0x1ec`
to the top of a 32 MB window without taking one branch. `0x00000000` decodes as `andeq r0, r0, r0`;
it was sliding through zeros.

**`emu::build` registered the OS as a region at `0x10000000` and a live mirror at 0, and started the
CPU at 0.** Region lookup is first-match and `map_hardware` had already put 64 MB of `sdram` at
`0x10000000`, so the `osos` region was read by nothing — SDRAM was zeros with a copy of the OS filed
behind it. About `0x220` bytes into its own entry RetailOS programs the PP's remap windows at
`0xf000f000`, one of which is `0x00000000..0x01ffffff -> 0x10000000`. `Memory::translate` runs ahead
of the region list, so from that instruction on every low address resolved into the zeros and the
code vanished from under the program counter.

**Apple's own bootloader is the oracle and it does neither thing.** `ipod-boot retail` on a drive
that boots: 58 `READ DMA` put the firmware partition **into** SDRAM at `0x10000000..0x10736000`, and
the console then says `Running 'osos' 0 from 0x10000000`. The high-level boot now does the same —
the bytes are written into SDRAM and the CPU starts at `load_at + entry` — so there is one storage
and the remap points at it.

| `ipod-emulator --headless=200000000`, same ROM and drive | before | after |
|---|---|---|
| ending | **`Lost(33554432)` @ 8 388 485** | `BudgetExhausted` @ 200 149 075 |
| ata commands | **0** | **290** |
| code buckets | 2 097 122, every one of them the slide | 20 196 |

Covered by `emu.rs`'s `a_high_level_boot_survives_the_os_remapping_low_memory_onto_sdram`, which is
the same failure in a twenty-one-instruction operating system and needs nothing out of `resources/`,
and by `a_synthesised_rom_boots_the_os_and_this_needs_resources`, which is the whole boot.

**And what it was nearly filed as.** The run's log held twenty-five identical high-level boots and
no reason for any of them, so it read as a retry loop. There is no retry loop — `session` parks on
`wait_after_stop` and only a power command moves it, and every one of those is queued from
`on_start_device`, which is reached from a press and from nothing else. Twenty-five boots was
twenty-five presses. What was actually wrong is that **a machine that stopped printed nothing**:
`build` prints two lines and the stop path printed none. It prints `stopped: <reason>` now, on the
same stream as the boot line it ends, and a second identical ending says it is the second.

---

## ~~`ipod-boot <recipe> --flash=FILE` ignored the file~~ — FIXED 2026-08-25

Every recipe appends the caller's flags after its own, and `trace` reads a single-valued flag with
`args.iter().find_map(|a| a.strip_prefix("--flash="))` — the **first** match. So

```sh
ipod-boot warm --flash=my-synthetic.bin
```

composed `… --flash=<the configured retail dump> … --flash=my-synthetic.bin`, booted the retail
dump, and named it in its own output (`sysinfo at 0x40015898, sdram_size 0x4000000, from MA146`).
The same held for `--disk=` and for every recipe. Every `warm` measurement anyone had taken with
`--flash=` was taken on the wrong ROM, and nothing said so.

Fixed twice over: `resolve` reads `--flash=`/`--disk=` as **inputs**, ahead of `FLASH=`/`DISK=`, so
`--print` reports `command line`; and `passthrough_wins` drops the recipe's own copy of any
single-valued `--key=` the caller spelled — only keys the caller wrote, and only from the recipe's
half of the argv. `--osos-at=` is exempt by name, because `trace` collects it with `filter_map` and
`ipod-boot warm` writes one deliberately, so taking it away would be a fix that broke a mirror.
`every_repeatable_flag_a_recipe_writes_is_exempt` reads `trace.rs` and holds that exemption list to
exactly the flags that need to be on it.

---

## ~~The window declares the cold boot over 2 250 000 instructions in, before the drive has answered~~ — FIXED 2026-08-25

Found by booting Apple's software from the bench for the first time. The window's start path **does**
boot RetailOS — 768 ATA commands, the co-processor drawing, and **75 267 non-black pixels**, which is
`research/10` Addendum 10 §8's own fingerprint for this exact machine, to the pixel. What was wrong is
what the window *said about* that boot.

```
                                 before            after
the window leaves `Booting`     2 250 000     823 624 842 instr
  with                        0 ata, 0 lit    765 ata, 75 267 lit
  and publishes                 2 250 000     823 593 896 instr — 94.4 % of the boot, not 0.26 %
the first pixel lights         42 999 970 instr     0 ata
the drive first answers        57 499 970 instr
the machine stops working     872 043 218 instr   768 ata   75 267 lit   2 co-proc frames
```

At the instant the bench stopped saying *booting* and started saying *running*, the drive had
answered **nothing** and the panel was **black**. The first lit pixel was nineteen times further on;
the first ATA command twenty-five times.

**The cause was the signal, and it was chosen from a measurement taken on a different machine.**
`emu::boot_end`'s observed arm ended the boot phase on the first `0x8001052a` — RetailOS asking the
click wheel for autonomous frames — reasoning that *"a machine asking for input is a machine that has
finished starting"*. Cold-booting from Apple's own NOR, that command arrives from the **boot ROM**
long before RetailOS is loaded: `--storeaddr=0x7000c120` puts it at `@2 211 983` from
`pc = 0x4000e654`, 55 M instructions before the drive answers at all. `eapp-loader`'s snapshot code
had said so in as many words — *"the firmware turns it on once with opcode `0x052a` **early in the
boot**"*.

**And watching for a later one would not have helped**, which is the half a control caught.
RetailOS does send its own; the window's boot sends **five** `0x052a` commands in total, and
RetailOS's earliest is at `@111 545 868` — still only 12.8 % of the boot — while its other three
arrive *after* the machine has already stopped working. No arrival of this command is the end of a
cold boot.

**Fixed by measuring what a booted machine does that a booting one does not: it halts.**
`emu::Quiet` ends the boot phase on an 8 M-step trailing window that is 95 % halted with at least
one ATA command issued. Over the whole 872 M cold boot the halted fraction never exceeds **61.7 %**;
from 823.6 M it holds **99.7 %** for ever. It needs no detection of which operating system is on the
drive — Rockbox's 400 M-step budget executes 77 264 434 instructions, 80.7 % halted, on a boot of
about 100 M. `research/10` Addendum 32 is the measurement, the candidate window widths, and the
control that made an `--enterlog` zero worth reading.

**The denominators already learned are dropped, once.** `main::Learned::boot` wrote `Out::booted_at`
to the device library, so every settings file written before today carries
`device.N.boot_instructions = 2250000` and nothing would ever have cleared it —
`Settings::set_boot_shape` runs only when a recipe is committed, and a device nobody edits is never
committed. The key is now `device.N.cold_boot_instructions`: a key's name is a promise about what its
value measures, the value's meaning changed, and `settings.txt`'s own header states the mechanism —
*"keys this version does not know are ignored"*. The stale line is read by nobody and is gone at the
next save. Those devices draw no fraction until their next real boot, which is what a device that has
never booted draws.

**How to see it**, in one command — it needs `resources/`, so it is `#[ignore]`d:

```sh
cargo test --release -p ipod-gui --bin ipod-emulator \
    the_bench_boots_apples_software -- --ignored --nocapture
```

It prints the whole boot as a timeline of halted fractions, every `0x052a` the boot sent with the
instruction count of each, and what the window said about it — and it now **asserts** the three
numbers this entry is about instead of printing them under a `KNOWN BUG` heading.

## ~~`ipod-boot retail` and `ipod-gui` do not boot the same machine~~ — ROOT-CAUSED + FIXED 2026-08-26

Same NOR dump, same `PRISTINE` drive, both pinned on the command line:

| | ATA commands | co-proc | what is on the panel | instructions |
|---|---|---|---|---|
| `ipod-boot retail` | **70** | 4 kicked / 2 updates | the Apple logo | 1.2 G, still going |
| …`--clickwheel` | 70 | 4 / 2 | the same | 1.2 G |
| …`--clickwheel --bcm-registry` | 70 | 4 / 2 | the same | 1.2 G |
| `ipod-gui`'s start path | **768** | 4 / 2 | the language picker, 75 267 lit | 872 M, then quiet |

The trace front end stops at Apple's bootloader's own screen and stays there; the window boots
through to RetailOS's first interactive screen. The two build the machine in different places —
`trace.rs`'s flag parsing and `emu::build` — and the click wheel is not the difference and the
co-processor's GENCMD registry is not the difference; both were tried.

**Why it matters more than it looks.** `ipod-boot` is what `research/` was measured with, so any
figure in that directory quoted from a run past ~70 ATA commands describes the machine that stops at
the logo. It also cost this file a wrong sentence: `--enterlog` reporting zero arrivals at all five
of RetailOS's `0x052a` senders over 1.2 G was written up as *RetailOS never sends the command*, when
what it measures is a machine that never reaches the code. Found while fixing the entry above.

**One datum for whoever picks this up, measured 2026-08-25 while fixing the high-level boot.**
*Apple's bootloader is not where the trace front end stops.* `BUDGET=1500000000
DISK=resources/drives/ipod8g-retail.PRISTINE.img ipod-boot retail` prints

```
Running 'osos' 0 from 0x10000000
```

and the last instructions of that run are at `0x0029d944` / `0x00289e98` — **low** addresses, which
on the cold map are RetailOS itself seen through the remap window it programmed. 140 function names
were recovered from loaded SDRAM. So the bootloader hands over and RetailOS executes for the rest of
the budget; what it does not do is ask for the drive. Counted the same day, one line printed per
`rebuild_mmap_aliases`: `ipod-boot retail --clock=5` at 600 M programs those windows **56** times,
and the window's high-level boot on the same drive programs them **408** times in 400 M and reaches
**484** ATA commands. Whatever divides the two front ends, it is downstream of the handover and it
is not the address map.

### It was two defaults, and neither was the address map

Both front ends build the machine in their own code — `trace.rs`'s flag parsing and `emu::build` —
and they disagreed about exactly two knobs. Each is worth an order of magnitude on its own.

**1. The second core.** `trace` ran it unless told not to; the window runs it only under
`--second-core`. Same NOR dump, same `PRISTINE` drive, `BUDGET=900000000`:

```
ipod-boot retail --no-second-core       766 ata
ipod-boot retail (two cores)             70 ata
ipod-emulator --headless                769 ata   75 267 lit   the language picker
ipod-emulator --headless --second-core   70 ata    2 916 lit   Apple's logo
```

The same factor of eleven in **both** front ends, so it is the coprocessor and not a harness. It
was defaulted on in `research/04` ledger row 7 on the evidence that *"every recipe is identical
with one core and two — retail 599 ATA commands and 2 916 non-black pixels"*, which was true then.
The one-core boot has since gone to 769 and 75 267 — Apple's logo replaced by RetailOS's first
interactive screen — and the two-core boot did not come with it. **The premise expired**, so the
default is one core again and `--second-core` is where the defect now lives. Row 7 is reopened,
because one core puts its `COP_STATUS` override back on the retail path.

**2. The co-processor's GENCMD registry.** The window sets `bcm.registry` unconditionally; `trace`
required `--bcm-registry`. This is the whole of the *panel*, and it was tried in the table above
and ruled out — at 70 ATA, on a machine that never reached the code that reads it. Retried once the
first fix let it get there, one core, `BUDGET=1500000000`:

```
without the registry   769 ata    2 916 lit   Apple's logo
with the registry      705 ata   75 267 lit   the language picker
```

**3. The clickwheel.** The window builds one unconditionally; `trace` required `--clickwheel`.
A real iPod always has one. This was the whole of the ATA residual left by the first two:

```
without the wheel   705 ata
with the wheel      769 ata      <- the window's own number
```

**Both front ends, no flags on either side:**

| | ATA | instructions | panel |
|---|---|---|---|
| `ipod-boot retail` | **769** | 872 646 182 | **75 267** |
| `ipod-emulator --headless` | **769** | 872 236 211 | **75 267** |

The same ATA count, the same screen to the pixel, instruction counts 0.05 % apart. Closed.

One asymmetry survives and is **not** closed: the window reaches 75 267 with `bcm.registry`
ablated and `trace` does not, which `emu::build`'s own comment records from 2026-08-24. The two
still need different things to draw, and neither knows what.

**What this costs.** Every figure in `research/` taken through `ipod-boot` past ~70 ATA commands
describes the machine that stalled at the logo, and that is now a re-baseline rather than a mystery.
`--ppm` was no help in finding it: on the `--boot-osos` path it sat below an early return and wrote
nothing at all, and once that was fixed it wrote 76 800 magenta pixels, because it dumps the *game*
framebuffer and not the panel. `--bcm-dump=e0000:140:f0:FILE` is the one that reads what the window
counts.

## ~~The window opens at its minimum height, not its preferred one~~ — FIXED 2026-08-21

**The window never did that, and the report this replaces was written off a trace that printed two
moments on adjacent lines.** Measured from outside the process with the accessibility API — the real
`NSWindow` frame, not the program's opinion — the window is **1180 x 878 outer** at 0.5 s, 1 s, 2 s,
3 s and 5 s after launch, in every run. 878 outer is 846 client plus a 32 px title bar: the preferred
size, held. **It reads 1180 x 878 with `MIN_HEIGHT` set to 846 as well** — measured, because that was
the change everyone expected to be the fix. All raising the minimum does is stop the third block being
*printed*: the dump is gated on the fit changing, and the only term a resize moves is the too-short
boolean.

**What was really wrong is smaller and worse.** Slint clamps a not-yet-existing window's size *up* to
the declared minimum — `update_window_properties` runs before `set_visible`, the adapter's size is
still 0 x 0, and the clamp writes `min-width x min-height` into the pending window attributes. So the
window is **created** at 880 x 400 and resized to 1180 x 846 **before it is ever mapped**. Both sizes
then arrive as two queued `Resized` events in creation order, and this program computed `k` and the
too-short boolean from the first one — a size no window was ever on screen at. `too-short` went true
on every launch. Nothing read that boolean at the time (GUI.md §20 item 15), so it had no drawn
effect; it was wrong state published by the program. **§9.5's pane reads it now**, which is what
would have turned it into a first run that replaced the bench with a message about a display that
was tall enough — so this fix is what that pane is standing on.

The trace made it look like a resize because `dump_layout` printed `window` — Slint's **cache**,
which the winit event filter runs one event ahead of — beside a fit computed from a different source.
The `880 x 400` line was the creation size printed against a cache that had already caught up.

**The fix**, in `tools/ipod-gui/src/main.rs`: `own_height_logical` asks
`winit::Window::inner_size()`, the platform, now — not the `Resized` payload (a size the window
*had*) and not `slint::Window::size()` (a cache that is stale in both directions inside the filter).
`live_scale` takes the scale factor from the same place, so both halves of a logical height come from
one moment. `MIN_HEIGHT` is untouched, and raising it was never the fix: at 846 the third block
merely stops being *printed*, because the dump is gated on the fit changing.

```
window      2360 x 1692 physical — Slint's cached size, which inside the event filter is one event old
platform    2360 x 1692 physical, 1180.0 x 846.0 logical at scale 2 — winit::Window::inner_size(), asked now
measured    846.0 logical — the height the fit below was computed from
```

`IPOD_LAYOUT=1` prints all three sizes now, and `measured` is compared against `platform` rather than
against the cache — so a difference on that line means a defect instead of an expected lag.

**Why no test caught it, and this is the part worth keeping.** The suite was green — 482 tests — and
`the_column_terms_sum_to_the_declared_chrome`, `the_too_short_state_is_an_input_with_nothing_reading_
it` (since retired with the gap it asserted) and the whole `fit` module all passed. Every one of them checks arithmetic about a window. None
launched one. The guard is `the_fit_is_computed_from_the_size_the_platform_reports`
(`tools/ipod-gui/tests/startup_fit.rs`), which launches the real binary, reads `IPOD_LAYOUT=1`, and
**resizes the window from outside the process** by an amount chosen after it is running. That last
part is not decoration: an earlier version compared the program only with itself, and a
`own_height_logical` replaced by a constant — no platform call at all — passed it while the binary
believed 846 with the window 700 px tall. Five breaks were each proved to make it red, including that
one.

**Related but not the same, and now closed**: GUI.md §20 item 15 recorded the too-short state
having no §9.5 pane. It is reachable — on a 1280 x 800 display, which gives 735 usable logical px
against the 809.8 the iPod at 1:1 needs, at every scale factor. That was a real open defect, it was
never this one, and `ShortPane` in `ui/bench.slint` is the answer to it.

## ~~The window offered no way to make an iPod once you had opened it and closed it~~ — FIXED 2026-08-21

**The state is reached by opening the program, looking at it, and closing it.** `Settings::welcomed`
is written when the bench is wired, before any press, so the second launch on an empty library was
already "not your first minute" — and that bench had no route to an iPod at all. The plan was not
filed, the drawer stayed shut, the cradle was drawn `fg-dim` and unpressable, and pressing it
answered *there are no devices in the library yet, so there is nothing to start*, while the shelf
row above went on saying *the centre button makes one*. The one promise the README is built on was
gone permanently, with no error and no way past.

GUI.md §9.1 and §10.3 both describe that bench correctly — *the ghost iPod, `No devices yet`, cradle
label `press ● to make an iPod`* and *both routes offered equally*. The flag was meant to stop the
**welcome copy** returning, which is the bug §10.3 exists about; it disarmed the button instead.

**The fix** is that `Offer` has a fourth state, `Again`, and three of its four carry the plan;
`empty_device`'s `startable` reads `caps.download` alone rather than `first && caps.download`; and
the press is routed **per press, by the row that was pressed**. That last part fixed a second defect
of the same family in the other direction: `has_plan` was one boolean computed at startup, so with a
half-made first-run device sitting beside one somebody had composed by hand, pressing the composed
device **resumed the first run** instead of starting it.

Two tests: `the_wizard_does_not_come_back` (extended — it asserted the plan was *not* filed on the
second launch, which is how the gap was locked in) and
`a_bench_that_is_empty_a_second_time_still_makes_an_ipod`, which drives the registered handler.

## ~~A finished drive could be built and never recorded~~ — FIXED 2026-08-21

`Queue::busy()` reads `JoinHandle::is_finished`, so it goes false the instant the worker thread
exits — and `pump_once` stops the 100 ms timer as soon as it does. A report sent in the window
between the drain and that check was therefore stranded in the channel for ever, because nothing
would ever drain again. For the install's `Done` that is an 8 GiB-apparent drive sitting on disk
with its real name, which the library never learns about: `settings.disks` empty, the Rail stuck on
`Working`, and the next press building `my-5.5g (2).img` beside the orphan.

The close path lost the same report a second way: `Queue::stop` drained the channel looking for one
`Cancelled` and threw everything else away, so a window closed between the install's rename and the
next tick wrote a settings file that did not mention the drive.

**Fixed** by draining once more after observing `!busy()`, and by having `stop` apply what it drained
through the same path a tick uses. The interleaving itself is a few microseconds and no test can
force it; `a_run_that_finished_before_the_first_pump_is_still_recorded` and
`stopping_on_the_way_out_records_a_drive_that_was_finished` cover the consequence, offline, and both
were proved red.

## ~~A resumed first run never ticked the steps it skipped, and never said it had finished~~ — FIXED 2026-08-21

The §10.3 case: a first run fails at the build, and is relaunched. The bundle is in the cache and
verifies, so the run resumes at the build — and `Queue::press`'s resume branch updated sub-lines and
nothing else. `fetch Apple's firmware` sat `Planned` for the rest of the run, and `self.done` kept a
hole at index 1, so `first_unticked()` answered 1 for ever. §12.2's handoff note is gated on it, so
the drive finished, the timer stopped, and the window said nothing at all.

Only reachable across processes: the same-process retry was green because `pump` had already ticked
those steps. `a_resumed_run_ticks_what_it_skipped_and_reports_the_handoff` drives it offline.

## ~~An interrupted download left a partial file nobody could see or delete~~ — FIXED 2026-08-21

`fetch_watched` removes its `.part` when the **watcher** stopped the transfer and on no other path,
so curl 7 / 18 / 28 / 56 — a refused connection, an interrupted transfer, the common ones — left a
partial in the firmware cache. Nothing showed it: `Rail::fail` clears `cancellable`, so no `Cancel`
was drawn, and neither `Retry` nor `Provide` routes to the delete, so the path sitting in
`Entry::temp` was unreachable. Nothing here resumes a download, so those bytes were worth nothing.

`a_transfer_that_ends_early_leaves_no_partial_file` drives it against a local one-shot listener that
promises the release's length, sends 64 KiB and hangs up — no packet leaves the machine.

## ~~`build_volume`'s pre-write refusal was 32x under the real FAT32 floor~~ — FIXED 2026-08-21

`refuse` checked `sectors <= DATA_LBA + 65_536`, which is one **cluster count** rather than one
cluster count times 32 sectors each. So a 65 537-sector drive passed the check that exists to catch
exactly that, the file was created and `set_len` to size, and `fat32` then refused with *"2046
clusters, which is FAT16 territory"*. A pre-write refusal that lets the write start is not one.
`MIN_FAT32_SECTORS` is derived from `FAT32_MIN_CLUSTERS`, `FAT32_SPC` and the FAT itself now, and
the refusal says which of those it applied.

## ~~A cancel that arrived a moment too late was accepted and then vanished~~ — FIXED 2026-08-21

The worker's last cancellation boundary is before the install's rename. `Queue::cancel` answers
`true` for as long as the thread is alive, which is a few microseconds longer — so a request could
be accepted, the run finish correctly, and nothing ever say what had happened to it. It now reports
`Report::TooLate` and the Rail carries *The run finished before the cancel could take effect.
Nothing was undone.*

## ~~A worker that could not be started said nothing at all~~ — FIXED 2026-08-21

`Worker::spawn` ended in `.ok()`, so a failed `thread::spawn` became `handle: None` — which reads as
*already finished*. `press` returned `Press::Running`, the timer started, the first tick found
nothing to do and stopped it, and the window sat on one ticked step and four `Planned` ones with no
failure anywhere, nothing on the Rail and nothing on stderr. Out of file descriptors or out of
thread stacks is the one resource failure that is genuinely plausible here, and it was the one that
said nothing. `spawn` is fallible now and the error becomes a `Class::Permission` refusal; the
compiler is the guard, because a `Result` cannot be dropped silently.

## ~~A test downloaded 6.5 MB from Apple and wrote an 8 GiB file, and asserted nothing about either~~ — FIXED 2026-08-21

`the_registered_centre_button_starts_the_first_run_on_an_empty_library` was not `#[ignore]`d and
drove the real registered handler three times. Each press reached `volume::probe`'s
`set_len(8_589_934_592)` on the developer's own disk and spawned `curl` at Apple; the download
stopped only because the queue was dropped at the end of the test, up to one 100 ms tick later. It
asserts device count and seed identity, neither of which needs a worker. The drives directory is a
**file** in that test now, so the probe refuses and the press stops after the mint.

The same run left one `ipod-gui-data-<pid>/` in the system temp directory per test process, for
ever — 93 of them when somebody counted. `DataDirGuard` takes its directory with it now, and only
the outermost guard does, because the lock is re-entrant and a first cut of this deleted the tree
halfway through the test that had set it up.

## The iPod the first run makes says 30 GB and its drive is 8 GiB — 2026-08-21

`work::plan`'s first step reads *5.5G, 30 GB, white, model A444* — the model table's own figures for
that part number; it read `black, model A446` until the default moved on 2026-08-25, and the
disagreement this entry is about is untouched by that, because `A444` and `A446` are one 30 GB row in
two colours — and `Plan::of` then builds a drive of `ipsw::DEFAULT_SECTORS`, which is 8 GiB.
RetailOS learns its capacity from ATA `IDENTIFY`, so its About screen will read 8 GB on a device the
shelf, the plan and Parts all call 30 GB. Nothing is corrupt and the drive boots; the two numbers
simply disagree, and somebody will notice.

**Measured**, so the cost of each answer is on the table rather than assumed: an 8 GiB drive costs
20 987 904 bytes on APFS and a 30 GB one costs 31 440 896 — 21 MB against 31 MB. Cost is not the
objection.

GUI.md §10.1 says 8 GiB three times, so this is an **operator decision** rather than a defect with
an obvious fix: either the drive follows the model (and §10.1's three figures change), or the model
follows the drive (and the synthesised iPod stops being a 30 GB one). The plan and the worker take
`sectors` from one call, so whichever way it goes they cannot end up disagreeing with each other.

## ~~A resumed machine was dead, and looked like one that ignored input~~ — FIXED 2026-08-20

Reported as *"the booted RetailOS is not taking anything when I click on the hold switch or use the
click wheel — just stuck on the language screen."* It was not an input defect. The saved machine
carried every memory region, both interrupt banks and the whole CPU, and omitted **`mmap_regs`** —
the sixteen words defining the MMAP unit's address windows. RetailOS executes entirely through the
low window, so a restored machine read zeros at its own program counter, executed `andeq r0, r0, r0`
through address space, and was declared `Lost` a few hundred instructions later.

```text
before   pc 0x002079dc reads 00 00 00 00    Lost after 223 instructions
after    pc 0x002079dc reads 04 00 00 1a    3 000 000 and still running
```

**The silence is the lesson.** The co-processor still held the last picture it had been given, so
the window showed an ordinary iPod that ignored every button — a dead machine and an unresponsive
one are indistinguishable from outside when the panel is somebody else's memory.

Ruled out along the way, each with its control: the second core (arms with and without it were
byte-identical), the user's particular snapshot (reproduced with a fresh one, same binary, same
flags), and the click wheel itself (`CTRL 0x600a1f00`, receiver armed, in the saved state).

The format is `IPODSNP7`; older images are refused rather than misparsed.
`tests/snapshot_round_trip.rs` is the guard, and it is verified to fail without the fix — its first
version was **not**, because it restored into a machine that had already configured the windows
itself.

## ~~A truncated snapshot took the whole program down~~ — FIXED 2026-08-20

`Machine::restore`'s doc had always promised *"returns false on a bad or truncated image"*. It
indexed the buffer directly, so a short one **panicked**. A snapshot is written by a background
thread as the window closes, which makes a truncated one exactly what a crash or a full disk leaves
behind — the moment when killing the program is least useful. Every read is bounds-checked now and
the caller cold-boots.

## iPodLinux's userland stalls at ZeroLauncher's last step — 2026-08-20

> **The window does not offer iPodLinux because of this.** The install works, the kernel boots
> cleanly, and then this happens — so offering it would be offering a 101 MB download that ends at a
> stalled screen. `ipod-boot install-linux` still builds the drive, and `Os::IPodLinux` and
> `Loader::IPodLoader2` are still in the compatibility engine with their rules and their tests. The
> way back is deleting a line from `Os::OFFERED`.

Unchanged as a symptom, but no longer a mystery. ZeroLauncher reaches **"Finishing Up…"** and spins;
the profile puts **94.9 %** of the phase in two adjacent buckets. Disassembling the flat binary off
the drive shows a three-instruction poll:

```
    mov  r12, #0xc5000000
    ldr  r2, [r12, #0x184]
    orr  r0, r2, #0x100        ; set bit 8
    str  r0, [r12, #0x184]
    ldr  r3, [r1, #0x184]
    tst  r3, #0x100
    bne  .-16                  ; spin until the hardware clears it
```

`0xc5000000` is `USB_BASE` and `+0x184` is `PORTSC1`, whose bit 8 is **port reset**: software sets
it, hardware clears it when the reset completes. We model that region as plain memory, so the bit
stays set and the poll never ends. One register away, `USB_BASE + 0x140` bit 1 is already filtered
for precisely this reason.

**Not yet claimed as the cause**: the address the profile reports depends on where the kernel loaded
the flat binary, and that base is inferred rather than read. A run logging reads of `0xc5000184` is
what settles it, and until it lands this is a decode that fits, not a measurement.

## `install-linux` cannot fit a bootloader into a full firmware partition — 2026-08-21

`ipod-boot install-linux` refuses, before writing anything, with

```text
no room: moving the later images by 57344 bytes needs 13952512 of a 13895680-byte partition
```

on every drive it has been tried against — one built here by `ipod-boot make-disk`, and one off real
hardware. **Reported, not independently reproduced**; the numbers above are one measurement and the
recipe to re-check it is `ipod-boot install-linux` against a drive of either kind.

The arithmetic in `install::install_linux` is coherent with them, which is what makes the report
worth recording rather than discounting. `osos` is written at `entry_offset` inside its own slot,
anything after it has to shift by `delta`, and the shift has to end inside the partition:

```rust
delta  = end - next.dev_offset + FW_SECTOR;
needed = last.dev_offset + align(last.len) + delta;
if needed > part_sectors * 512 { … }
```

`13895680 = 27140 × 512`, `delta = 57344 = 112 × 512`, and the overshoot is `56832 = 111 × 512` —
one sector less than `delta`. So on these drives the last image already ends **512 bytes** short of
the partition's end: the firmware partition is packed to within one sector and there is no room to
push anything into it at all. That is a property of how the partition is laid out, not of the
bootloader's size — the vendored 2.9.0d is 57 676 B against the release's 56 912 B, so replacing one
with the other moves the shortfall by 512 bytes and does not remove it.

**The consequence to say out loud**: `compose.rs` will happily verdict an iPodLinux recipe `Ok`, and
the command that would build it cannot complete. The `0x0C` refusal in `install.rs` used to end
*"A drive built by `ipod-boot make-disk` from an .ipsw is 0x0B and works"*; the last two words were
removed on 2026-08-21 because nobody can currently demonstrate them. They go back when this is
fixed.

**What would settle it**: run it against a `make-disk` drive with `RUST_LOG` off and read the
report's `firmware partition at …, N image(s)` line, which prints before the refusal. If the
partition really is packed to one sector, the fix is to grow the firmware partition in `make-disk`
rather than to shrink the payload.

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

**What you actually see.** The panel does dim — the level drops during the boot and the window
renders it — so it looks like brightness support and is not. Moving RetailOS's own slider changes
nothing, and the number the window dims by is a tally of screen-wake toggles rather than a
brightness. Partially working in appearance, not in fact, which is the worst of the three states and
the reason this entry is long.

**Parked 2026-08-18** after six eliminations. What follows is the record, so that the next attempt
starts from what is ruled out rather than from the top.

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

**Where the search has got to.** The instrument now exists: `pmu` and `writes` on the control
socket dump the PMU's per-register write census and the `--watch-writes` census live, so a control
can be moved and the answer asked for immediately.

Ruled out, each by measurement rather than by argument:

| | |
|---|---|
| **The PMU** | Slider full → 65 %: not one PCF50605 register *value* changed. Only `0x2e`/`0x2f` moved, and only their counts — those are the ADC control registers being polled |
| **An unmodelled address** | `unmapped` is empty before and after, so the write lands somewhere this emulator already maps |
| **GPIOD bit 0x80** | Rockbox's own method for this model. RetailOS writes `0x6000d02c` **twice in a whole boot**, with `0x20` |

What the census did turn up is a **second GPIO bank at `0x6000d800`–`0x6000d96c`**, which appears
nowhere in this project's notes and nowhere in Rockbox's `pp5020.h`. It mirrors the A–D bank's
layout one bank up, and it is the busiest thing in the device window:

```
0x6000d80c  x5106     0x6000d81c  x5106     0x6000d82c  x1730     0x6000d92c  x1440
```

By the A–D layout that makes `0x6000d82c` an `OUTPUT_VAL` whose enable and direction registers are
being toggled 5 106 times each — a pin being bit-banged hard, by something we do not model and have
never named.

**That bank was tried, and it is the wheel — 2026-08-18.** Pointing the dimmer at `0x6000d82c`
bit `0x80` made the panel dim while somebody *navigated*, which the operator spotted immediately.
The two-arm control:

```
  scroll DOWN on the brightness screen — a real brightness change    0 pulses
  scroll the same amount in a MENU     — no brightness change       +3 pulses
```

Backwards in both arms, so it is not the dimmer. Reverted.

**The mistake is worth more than the result.** The measurement that pointed there — *"slider full to
minimum: `0x6000d024` +0, the `0x6000d800` bank +236/+236/+112"* — looks like a controlled A/B and
is not one. **Moving RetailOS's slider means turning the wheel**, so "what moved while the
brightness changed" and "what moved while the wheel turned" were one question asked once, and every
register that answers to the wheel answered it. `research/04`'s rule R5 says a control only proves
what it exercises; this one exercised the wheel and was read as proving brightness.

The control that separates them is the two-arm one above, and it is cheap. Run it before believing
any candidate.

**Where that leaves it.** Two pins are now eliminated by measurement rather than by argument:
`0x6000d024` (steps on screen-wake activity, never on the slider) and `0x6000d82c` (steps on the
wheel, never on the slider). A real brightness change produced **no pulses on either**, and no PMU
register value, and no unmapped write. So the brightness path is still unfound, and the remaining
candidate this project has not looked at is the **video co-processor** — which would make this a
facet of ledger #6 rather than a bug of its own.

**Localised, 2026-08-18, by a control that finally matched.** Every earlier attempt compared two
different *situations* — brightness screen against menu, slider moving against not — so brightness
and wheel activity were never separated. The control that works keeps everything identical and
varies only whether brightness can change:

```
  arm 1   Brightness at MAX, 31 clicks down   brightness falls      wheel: 31
  arm 2   Brightness at MIN, 31 clicks down   cannot change         wheel: 31
  arm 3   a MENU,            31 clicks down   redraws, no change    wheel: 31
```

| | arm 1 | arm 2 | arm 3 |
|---|---|---|---|
| `0x6000d80c` | +72 | **0** | +1214 |
| `0x6000d81c` | +72 | **0** | +1214 |
| `0x6000d82c` | +36 | **0** | +434 |
| `0x6000d824` | +18 | **0** | +44 |
| `0x6000d92c` | +18 | **0** | +52 |

**Arm 2 is zero on every register.** Same screen, same wheel, same redraw path — the only difference
is that brightness had nowhere left to go. So brightness does write to this block, and the earlier
conclusion that it was "the wheel" was wrong for the same reason everything before it was wrong: the
control did not match.

Arm 3 says the block is **shared with the panel** — menu redraws drive it hard. But the ratios
differ: against `d80c`, register `0x6000d824` is **seven times more active** in the brightness arm
(0.25) than in the redraw arm (0.036), which is what a different transaction on a shared bus looks
like.

**What remains is decoding, not searching.** Counting writes cannot separate two transaction types
on one bus; the bytes can. `watch_range_log` already records `(pc, addr, value)` and is printed only
by the headless report — exposing it over the control socket would let one brightness step be
captured as a byte sequence and read.

**The transaction is captured, 2026-08-18.** `bus` on the control socket drains the value log, so
one brightness step can be isolated. Twenty clicks produced **120 writes = five identical 24-write
units**, one per step, four clicks apart:

```
  d82c/d82d = 80 80   @0x282a38        d80c/d80d = 00 20   @0x2829f0
  d81c/d81d = 80 80   @0x282a04        d81c/d81d = 00 20   @0x282a04
  d80c/d80d = 80 80   @0x282a18        d80c/d80d = 00 80   @0x2829f0
  d824/d825 = 08 08   @0x282d80        …  d92c/d92d = 80 80
```

Two pins bit-banged in one word — bit 7 of each byte — a clock and data pair.

**Stepping UP and stepping DOWN are byte-for-byte identical**, across all 120 writes, and the unit
repeats identically 5/5 times. So neither the direction nor the level is in any byte. The writer PCs
are identical to the panel's too, so filtering by routine cannot separate them either.

That leaves the **intervals**, which the log does not record — same writes, different delays, which
is Rockbox's model of this dimmer after all. The width figure this file used to cite is worthless
for the same reason: `Backlight` pairs a falling edge with the *next* rising edge, and on a clocked
bus that spans an entire transaction, giving 820 306 µs for a "pulse".

**The next step is small and specific:** add `usec` to `watch_range_log`'s tuple. With a timestamp
per write, the intervals inside one unit are readable, and up-versus-down should fall out
immediately — after which the level is a count of units in a direction, not a guess.

**And the interval measurement retires that whole line of attack — 2026-08-18.** With `usec` in the
log, one step down and one step up were captured and compared:

```
  DOWN   0 0 0 0 0  1 0  2 0 0 0  1 0 0 0  30  0 0 0 0  1 0     (us between writes)
  UP     0 0 0 0 0  0 0  3 0 0 0  1 0 0 0  30  0 0 0 0  1 0
```

Identical structure, the same 30 µs gap in the same place, 1–3 µs of jitter and nothing else. So the
direction is **not** in the bytes, **not** in the writer PCs, and **not** in the timing.

**Which means the "found it" above was over-claimed, and the control was confounded again.** The
argument was: arm 2 changed nothing because brightness had nowhere to go. The truer reading is that
with the slider already at the floor **nothing redrew** — and arm 3 shows this window answers to
redraws hard (`d80c` +1214, `d82c` +434 for a menu scroll that changes no brightness at all). Arm 1
changed brightness *and* redrew the slider bar, so its traffic is explained by the redraw alone.
Varying two things together is the same mistake as before, better disguised.

**Honest state: brightness produces no observable MMIO write in any window yet watched** — GPIO A–D,
the `0x6000d800` bank, the PMU's registers, the `0x70000000` window, unmapped space. The one path
that is structurally invisible to `--watch-writes` is the video co-processor, which RetailOS feeds
by DMA, and that is where this now points. It makes brightness a facet of ledger **#6** rather than
a defect of its own.

**How you would know it is fixed:** a control that varies brightness while holding *redraw* constant
— not merely wheel movement — shows traffic, and the level tracks the slider monotonically while
menu scrolling changes nothing.

This is the fifth model defect in this project first attributed to missing hardware and found to be
a misread of a signal we already had.

## ~~The boot progress bar is an estimate presented as a measurement~~ — FIXED 2026-08-18

Reported by the operator: *"the boot indicator on the bottom and the actual boot state are not
properly connected... the language screen was long there before it finished."* Correct, and the code
says so plainly.

The bar is `executed / snap_at`, and the phase flips on `executed >= cfg.snap_at`. `snap_at` is
**1 600 000 000 instructions** — a point chosen because it is a good place to *resume from*, not
because it is where the boot ends. RetailOS reaches the language picker before it, so the bar keeps
filling after the machine is up and interactive, and the "about N s left" beside it is counting
toward a number the user cannot observe.

**What is wrong is the claim, not the number.** An instruction budget is a fine trigger for taking a
snapshot. It is not a statement about whether the OS has finished starting, and presenting it as one
teaches somebody to distrust the one progress indicator this program has.

**There is a real signal.** The click wheel's `reporting` flag is RetailOS enabling wheel reports —
it means the UI is accepting input — and `Stats::reporting` already carries it. A phase derived from
that is an observation; a phase derived from a constant is a guess.

**Fixed.** The phase now ends on `Stats::reporting` — RetailOS writing `0x8001052a` to ask for wheel
frames, which is a machine that has finished starting — with `snap_at` kept only as a fallback so a
boot that dies before the UI does not claim to be booting for ever. The bar itself still counts
instructions, because nothing better is available *during* a boot, but it says **"roughly N %"** and
no longer offers a seconds-remaining figure it cannot honour.

**And the replacement signal was wrong too, measured 2026-08-24, fixed 2026-08-25** — see *The
window declares the cold boot over 2 250 000 instructions in*, at the top of this file.
*"RetailOS writing `0x8001052a`"* is the sentence that turned out not to be true of a cold boot from
Apple's own NOR: the boot **ROM** writes it first, at `@2 211 983` from `pc = 0x4000e654`, 821
million instructions before RetailOS reaches its menu. The operator's original complaint — the bar
filling after the language screen was up — was still fixed; the bar then finished long **before** it,
which is the same defect with its sign flipped.

**Three signals, and only the third is an observation of a boot.** `snap_at` was a constant. The
first `0x8001052a` was a real event belonging to a different machine's startup. What ends the boot
phase now is *the machine going quiet with its drive answered* — `emu::Quiet` — because a booted
machine halts and a booting one does not, whatever is on the drive. The paragraph above that names
`Stats::reporting` as *"a real signal"* is the reasoning that produced the second wrong answer, and
it is left standing: it is a good example of a plausible sentence about a register that nobody had
watched fire.

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

## `ipod-boot rockbox` runs Rockbox against a volume Rockbox is not installed on — 2026-08-18

`DISK` defaults to `resources/drives/ipod8g.img`, a stock Apple volume with no `.rockbox` directory
anywhere on it. Rockbox boots, mounts it, finds no theme and no fonts, and falls back silently to
the 8 px sysfont it carries — which is what `docs/media/ipod-14-rockbox-menu.png` was a picture of
until today. Nothing in the run says so: a themeless install is an ordinary condition for Rockbox,
not an error.

**It is not a FAT32 or ATA defect**, and the measurement that says so is in
[research/06](research/06-rockbox-as-oracle.md): on a volume written by our own `put-files`, the
231 928-byte font is present across 57 clusters, reads back SHA-256-identical to the zip's copy
through the independent reader, and Rockbox loads it — **15 px rows, five consecutive gaps of
exactly 15**, against **8 px, seven of seven** on the default disk.

A second flag is wanted with it and is equally invisible: Rockbox **writes** to a volume that has
`.rockbox` on it, so a run against a real install needs `--disk-writable` or it panics at
`dc_writeback_callback()` about 20 M instructions in. On the stock volume it changes nothing, which
is why its absence was never noticed.

**How you would know it is fixed:** `ipod-boot rockbox` with no arguments either boots a volume Rockbox is
installed on, or says out loud that the one it was handed has no `.rockbox`. Today it does neither.

## The gif encode invents pixels the panel never showed — 2026-08-18

Not an emulator defect; an asset-pipeline one, recorded here because it was read as an emulator
defect and cost a measurement to clear.

`docs/media/ipod-15-rockbox-wheel.gif` carries yellow ticks at a 24 px period that **do not exist in
any frame the machine produced**. The gif stores a keyframe plus three difference bands, each with a
transparent index and disposal 1 — ffmpeg's `-gifflags +transdiff`, on by default. On row 23 the
stored difference frame writes only `#000000` and leaves 26 pixels transparent, and the keyframe
under every one of them is the selection bar's yellow. So the ticks are the keyframe showing through
holes the encoder punched. The raw frames read one run of 320 px on that row at every cadence tried,
including frames caught mid-redraw, and the gif's own keyframe is byte-identical to ours — 0 of
76 800 pixels differ — so the encode's input was clean.

**How you would know it is fixed:** every frame in a shipped gif reports `transparent no`.
`-gifflags -transdiff` does that, measured. `ipod-film asset`'s `publish()` does not
set it.

---

## ~~An idle iPod ages thousands of times too fast, so it powers itself off in seconds~~ — FIXED 2026-08-18

**Nothing anywhere bounds simulated time against real time**, and the sleep path does not merely
run fast, it runs *instantly*.

`lib.rs`'s run loop, on `CPU_CTRL`'s sleep bit: the core asks to be switched off, and the clock
**jumps straight to whichever interrupt is due first** — `slept_usec += delta` — with no real time
passing and no instructions executed. Rockbox's tick is 100 Hz, so an idle machine loops *run a
handful of instructions, skip 10 ms, repeat*. One 4 G boot accumulates `2 535 581 halts,
2 531 061 ms of simulated time skipped`: forty-two minutes of iPod inside a few seconds of ours.

**That is correct for a headless run and wrong at the window**, and the window is where a person is.
`ipod-gui`'s `emu.rs` uses `Instant` only to *measure* (`wall_secs`) — there is no throttle, no
frame budget, and no cap on how far the machine may run ahead of the wall clock. Measured: with no
wheel input, warm Rockbox prints `Shutting down…` at **190 M instructions**, which at the window's
~14 M instructions/sec is about **thirteen seconds after you stop touching it**. On the hardware the
same firmware gives you the ten real minutes it was configured for, and any button resets the timer.

**This is not the firmware misbehaving and not a wrong device model.** Rockbox and RetailOS both do
exactly what an iPod does after N idle minutes; they simply get there absurdly fast. Which is why it
reads as "the emulator powers off for no reason" and was filed for weeks as an oddity rather than a
defect — [ROADMAP](ROADMAP.md) M1 carried it as *"honest rather than a defect … but a person at the
window would not see it, so it wants confirming interactively."* It has now been confirmed from the
mechanism rather than by watching.

**FIXED the same day, by deleting the teleport rather than by pacing anything.** A halted core now
costs one loop iteration per cycle, exactly as a running one does, so the clock advances at the same
rate whether the machine is busy or idle and the whole thing keeps **one honest ratio** to the real
part. At a third of speed everything takes three times as long — including waiting.

| | halts | simulated time halted | outcome |
|---|---|---|---|
| before | 2 535 581 | 2 531 061 ms | `Shutting down…` at 190 M |
| after | 2 122 | 19 026 ms | menu still up at 1.5 G |

1.43 G idle cycles ÷ 75 per µs = 19.0 s, which is the 19 026 ms reported — the accounting closes.

**The control that matters:** the 600 M fingerprint is **byte-identical** to the pre-change binary
(`ata commands: 488`, `ata dma: 466 transfers, 21 087 744 bytes`), built by stashing the change and
compiling the old tree into its own target directory, per R4. The boot path never halts, so this
could not have moved it — and now it is shown not to have.

Two details worth keeping. **Nothing armed still wakes at once and costs nothing**: with no deadline
we hold, a real core is waiting on an external event we have no model of, and charging even one
cycle for it would invent time. And `idle_frac`/`idle_steps` are **not** in the snapshot — a
sub-microsecond remainder carried across a restore makes the first step after one credit a
microsecond it did not earn, which is exactly how the version-3 control caught this, falling back
by 998 where it owed 999.

**How you would know it is fixed:** leave the window alone for a minute and the iPod is still on.
The fix belongs in the window rather than in the machine — clamp the sleep jump so simulated time
cannot outrun the wall clock while a person is watching, and leave the headless path skipping,
because a research run that waited out 42 minutes of idle in real time would be useless. That makes
it the same question as **M7**, approached from the other end: M7 is the machine being too slow while
executing, this is the machine being infinitely fast while halted.

## Not bugs, though they look like ones

- **A white screen during a cold boot.** The hardware does this too. Its *length* is the bug above.
- **The charging screen after a while.** Authentic 5G behaviour with a charger attached.
- **"Restore from iTunes" after `make-disk`.** The bundle's updater family does not match the iPod
  your NOR dump came from. The window now says so before it boots, and `make-disk` prints the family
  on the command line, so this should be hard to reach by accident.
- **`aupd` running the flash updater on first boot.** Correct — it takes two boots.
  `ipod-boot flash-update` is the recipe.
