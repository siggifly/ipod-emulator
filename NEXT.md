# The queue

**Ordered. Each item states what would settle it, so a cold session can pick it up without me.**

The constraint on this project is not ideas, it is *my context*. Sessions end when the window fills,
and everything not written down is lost. This file exists so that loss costs nothing: it is the
handoff, maintained continuously, not written at the end.

Every number below was measured on **2026-08-14** by running the command that produces it. When you
change something, re-measure and edit in place. This file drifted badly once already — for two days
its top three items described a machine that no longer existed — and every one of those errors was
an update written from belief instead of from a run.

---

## The rules

**R1 — Work the top item until it is settled**, not until something interesting appears. A finding
is not a stopping point; a *decided question* is.

**R2 — Write the finding into `research/` as it lands**, and fix superseded claims in place. Three
wrong conclusions in this project survived because a correction was deferred to "later".

**R3 — Grep [`research/04-bypass-ledger.md`](research/04-bypass-ledger.md) before opening any
investigation.** A 🔴 bypass in the subsystem you are about to investigate is not context, it is the
likely cause; investigating around a known-false model produces findings about the model. This rule
was promoted from a queue item after it cost most of a session: "which task never gets kicked?" was
worked as an open mystery for two sessions while its answer sat filed as bypass #10, whose
retirement condition read *"finding how this handler really acknowledges."* Same question, asked
years earlier.

**R4 — Every new *or fixed* instrument's first job is to re-run the conclusions the old one
produced — and so does every change to the *machine*.** Two deliberate passes have been run: the
`--watch-range` one (Addendum 8b — 19 claims, 6 wrong, 1 partial) and the `--stop-when-idle` one
(Addendum 14 — 23 claims, **9 wrong**, 2 partial). Fifteen published conclusions were wrong and none
would have been found by re-reading. Four of Addendum 14's nine were not truncation at all: they
were conclusions nobody re-ran after the second DMA controller was modelled. **A model that makes
the boot go further invalidates every "never" measured before it, exactly as a fixed instrument
does.** Run the pass deliberately, not opportunistically — both passes found claims standing for
weeks in files nobody had reason to reopen. A fixed instrument also needs the *old binary kept*:
rebuild the pre-fix build and run it against the current machine with identical flags, or the
machine has moved too and every difference is confounded.

**R5 — A control only proves what it exercises. Match it to the measurement in access *width*,
region type and code path** — not merely in "is this address ever touched". This has failed four
times: `--watch-range`'s positive control counted *byte* writes to `0x60007000`, which is precisely
the path that worked; research/03 §1's `--findptr` control proved literal pointers exist while the
thing sought was a field RTXC never stores; Addendum 12's framebuffer control proved the *readout
geometry* and was read as proving *authorship*; ledger #7's mailbox control was served by `read8`
where the address under test is served by `read32`.

**R6 — A control that saturates the instrument is worse than no control.** It produces a clean
number from a log that stopped recording. Prefer the **quietest** control that is provably live from
the first instruction to the last over the loudest one to hand. The case that named this rule:
`0x60005010` was read 9 588 012 times in a 600 M boot, consuming all 2 000 000 `read_log` slots
shortly after @123 M and silencing four fifths of the run it was supposed to validate.

> **The mechanical half of R6 landed 2026-08-14.** Every capped log in the emulator now carries an
> **uncapped counter**, prints the counter as the headline, and says `SAMPLE, NOT A CENSUS` in the
> same line when the rows below it are truncated. The mechanism is one type, `Capped<T>` in
> `lib.rs`, which deliberately has **no `len()`** — `seen()` is the census and `sample()` is the
> rows, so a report cannot print the cap by accident. Where a histogram was a tally *of* a capped
> log (I²C by device, `--watch-range` by word, `--enterlog`'s `callers:`, the ADC by channel, the
> IDE registers, `--readlog` by reader, `--retwatch` by producing PC, `--indirect`'s edge set) it is
> now counted at capture time and cannot saturate at all. Four tests assert the contract, each with
> an unsaturated control so "always warns" is distinguishable from "warns correctly".
>
> **The judgement half of R6 is unchanged and is still the point.** A quiet control is still better
> than a loud one, because a log that is 99 % one address is a bad *sample* even when its header is
> honest about the total.

*And a live demonstration of R4 sitting inside R6.* Re-run today, `--readlog=0x60005010,0x60004020`
on the baseline reports `--- reads of watched addresses: 363369 ---` — **the cap is nowhere near
hit, because the machine moved**. The address that saturated the log was hot only in the idle loop
the boot no longer sits in. The hazard is unchanged; the specific address that illustrated it is no
longer the loud one. Never inherit a control's loudness from a document.

**R7 — The measurement window is part of the measurement, and "never" is a claim about the
window.** Before writing that a call never returns or a bit is never set, **vary the stop condition
and re-run** — it costs one command. `--stop-when-idle=40000000` produced four published
conclusions; all four were that one number (see the tombstone at item 3). Measured today, the same
budget with the two windows:

| `--stop-when-idle` | stop | buckets | ata commands |
|---|---|---|---|
| `40000000` | Idle @308 908 411, **0 CPU sleeps** in the window — the tool now says `<- BUSY, not blocked` | 29 283 | 464 |
| `400000000` | Idle @1 562 789 429 | **38 220** | **770** |

> **Re-measured 2026-08-14 after `55854a4`.** This table previously read `@308 909 460 / 29 279` and
> `@1 610 256 821 / 38 262` — the **pre-fix** numbers, which the baseline section directly below
> already identifies as superseded. The rule the table illustrates is unaffected; the numbers in it
> were not re-run when the machine moved, which is the exact failure R4 exists to catch, sitting
> inside the rule that says to vary the window.
>
> **Re-measured again the same day** after the PMU ADC fix and the `GPIOL` seed (research/10
> Addendum 30): `400000000` is now `@1 562 789 429 / 38 220 / 770`. The `40000000` row was not
> re-run; treat it as illustrating the rule, not as a current number.

**R8 — And a resting state is a *sample*, which is R7 one level in.** `state = 0x40` at 2 G and
`state = 0x40` at 6 G is one observation taken twice, not two observations of a deadlock. `0xd1`
looked like a deadlock for two days and was a retry loop with a 19.6 M-instruction period; the
discriminator was to count the pend at two budgets (272 against 325) and cost one command.

**R9 — Do not infer a call's purpose from its shape.** `0x000cd7c8` is a three-call sequence that
reads exactly like lock / issue-request / wait-for-reply, was published as "it is waiting for the
co-processor", and is create-semaphore / spawn-thread / join. That single misreading is why item 3
below spent a week gated on a co-processor that was never in the way.

**R10 — A prediction that measures out to nothing gets written down too.** Four are recorded; all
four were good hypotheses, and the record is what stops them being re-had.

**R11 — Branch per topic.** Direct commits to `main` are refused by a hook.

**R12 — A model defect looks exactly like missing hardware.** Both present as a device that answers
zero, and nothing downstream can tell them apart. `Bcm::read8` popped the co-processor's data FIFO
**twice per `ldrh`**, so a 16-byte read drained 32 bytes and each halfword arrived spliced from two
different words: the memory held `1` at `0x1f8`, the CPU was handed `0x2f01fc78`. That zero was
attributed to the device, published as *"only a running `vmcs.bin` can populate the block"*, and
aimed two sessions at building a co-processor. **Before concluding "the hardware is not there", read
the same address through a second path and check that the two agree** — `--bcm-peek` beside
`--stop-at` on the instruction *after* the load, comparing the device's memory against the register
file. That pairing is now written into the instrument table's `--bcm-peek` row. See research/10
Addendum 29 §1.

**R13 — A tool unverified on your data type is not a measurement.** `grep -c` over binary input
answers with **no count at all** (measured today: it prints nothing and exits 1, so a `$(…)` capture
becomes an empty string that reads as zero); `grep -ac` answers with a count of matching *lines*, and
a NUL-riddled file is one line; `strings … | grep -c` answers with a third number. Any of the three
can be published as "zero hits", and one was. **Run the tool against a positive control in the same
file** — a string known to be present — before believing an absence it reports. One command. It is
the same discipline as R5, applied to the tool instead of to the machine.

---

## The baseline

> **On `--clock=5` in the numbers below.** It is the research accelerant, not a default: 75 is the
> real PP5021C, and 5 makes simulated time run fifteen times fast so the bootloader's delay loops
> collapse. Every clock literal in the code is now [`eapp_loader::CLOCK`] and it is 75. The
> recorded commands here keep `--clock=5` because that is what produced the numbers next to them —
> changing the command without re-running it would make the record wrong. Re-measuring the
> baselines at the real clock is its own exercise, and the instruction counts will not survive it.

`ipod-boot retail --clock=5 --stop-when-idle=400000000`, re-measured today. Check any run you inherit
against this before trusting it; if it does not match, find out why before measuring anything else.

**`--bcm-registry` is OFF in everything below.** It is the control arm and every number in
`research/` is measured on it; with it on, 5 MB of DMA and 64 ATA commands move (see item 2).

**`BUDGET=600000000`** — the fingerprint run.

> **The block below is stale. Re-measured 2026-08-18 against a PRISTINE disk: `ata commands: 576`,
> `ata dma: 467 transfers, 21 088 768 bytes`** — and see item 0b for why the working image gives a
> different, meaningless answer. It reads 554 / 445 / 20 367 872 because it was written on
> 2026-08-14 and four device fixes have landed since. **This was not the halt-clock change** — that
> was A/B'd against a binary built from the stashed tree and is byte-identical here, which is the
> only reason the drift is attributable to anything else. Numbers below the line are the machine of
> 2026-08-14; treat every one of them as owing a re-run.

```
-> BudgetExhausted after 599999952 instructions      29 289 code buckets
ata commands: 554  (log below shows the first 256 — SAMPLE, NOT A CENSUS)
ata dma: 445 transfers, 20367872 bytes to memory
pp dma: 4 transfers, 201216 bytes                    bcm: 4 commands kicked, 2 frame updates
irqs: 221033 asserted, 108017 taken; usec 119999990
ide irq: raised 1176, delivered 479, acked 496; enabled=1 pending=0
unmapped: 4 reads, 0 writes across 1 pages           (0xea000078, first pc 0x000a0bd0)
i2c: 3749 transfers                    cargo test --release: 66 in eapp-loader, 49 in ipod-gui
```

**No `cpu sleep:` line at all**, and `usec` is exactly `budget / clock`. At 600 M the machine never
halts — it is doing real filesystem work for the whole window, so any older number that assumed
idling before 600 M is wrong.

**`BUDGET=4000000000`** — the full boot.

```
-> Idle after 1562789429 instructions                38 220 code buckets
cpu sleep: 2535581 halts, 2531061 ms of simulated time skipped
ata commands: 770   ata dma: 660 transfers, 33637888 bytes
pp dma: 4 transfers, 201216 bytes                    bcm: 4 commands kicked, 2 frame updates
ide irq: raised 1607, delivered 693, acked 713; enabled=0 pending=0
unmapped: 4 reads, 0 writes across 1 pages
bcm: 230572 halfwords written, 28 read, 177508 internal words held
cargo test --release: 66 passed in eapp-loader, 49 in ipod-gui. **Build the whole workspace** —
the GUI was broken on `main` for an hour because a merge verified only one crate.
```

> *Four of eapp-loader's are the command interface's (item 0), and they assert that
> `LCD_UPDATERECT` places the rectangle its header describes, that a header this model will not
> honour changes nothing and says so, that `LCD_UPDATE` reads the buffer with no header, and that a
> non-image command moves no pixels. Both totals re-counted on this tree — the whole workspace, not
> one crate.*

> **Superseded twice on 2026-08-14, and the second time is worth reading.** The block above used to
> read `1 610 232 373 / 38 265 / delivered 685 / 21 passed`. Two changes moved it, both in
> research/10 Addendum 30: the PMU's ADC stopped reporting every completed conversion as **zero**,
> and `GPIOL` stopped claiming a charger was plugged in. **The invariants did not move** — 770 ATA
> commands, 4 unmapped reads, and a framebuffer byte-identical to the `--stop-at=0x10000000:1`
> handoff dump — and the Rockbox oracle's whole run log is byte-identical across the change. Those
> are the things to check a run against; the instruction count is what to check the model against.

## 0 — ~~The Apple logo is missing from the boot~~ · **DONE 2026-08-14 — it was never missing, it was never placed**

The 2 922-pixel frame this project has carried since research/03 as "the handoff frame" **is the
Apple logo**, and it was sitting in the co-processor's command-parameter buffer at 62-halfword pitch
because the model stored the pixels and never executed the operation that places them. One
autocorrelation over the non-black mask in *address* order says 62 and means *somebody else was
supposed to place these rows*.

`LCD_UPDATERECT` (command 5) is now implemented, derived from the eight-word header Apple's
bootloader stages at `BCMA_CMDPARAM` and cross-checked against the byte-length word in the same
header. A retail boot issues two of them: `320x240 -> (0,0)-(319,239)` and
`62x78 -> (129,81)-(190,158)`, which is dead centre. **The logo is on the panel from 8 M to 52 M
instructions, and the screen it is on is black, not white.** Everything in
[research/14](research/14-the-apple-logo.md), including the four predictions that measured out to
nothing.

Controls: the baseline is unmoved to the instruction (`Idle @1 562 789 429`, 38 220 buckets,
770 ata, 4 unmapped, 230 572 halfwords, 177 508 words), and the **whole `--bcm-registry` arm is
byte-identical across the change** — same idle, same buckets, same 706 ata, same 521 gencmd, and
`cmp` silent on the two framebuffer dumps. It could hardly be otherwise: nothing in this boot ever
reads `BCMA_CMDPARAM` back.

**What it leaves open**, and it is a real one: `0xE0000` is the command-parameter buffer, so
`--bcm-registry` handing that address out as a *surface* is now a known-wrong choice rather than an
unexamined one — item 1 below, and research/12 §8 item 4 carries it in those terms.

## 0b — ~~Brick launches and draws. Is it playable?~~ · **DONE 2026-08-14 — it is played**

**The centre button serves the ball**; play/pause does nothing and a wheel gesture only moves the
paddle. After a life is lost the next ball serves itself. The ball moves (±8, ±10) px per tick and
every collision flips exactly one sign — **where it lands on the paddle does not steer it**, which is
the only reason an open-loop wheel script can play it. The paddle is 57 px wide, travels [4, 262] in
**24 px quanta**, and its speed is set by how fast the steps *arrive*: `rotate=+2` every 400 k moves
it 29 px per million instructions, `rotate=+8` every 200 k moves it **150**.

The shipped film is **six returns, eight bricks and a score of 8** —
[`ipod-film asset gameplay`](ipod-film asset), write-up in
[research/13 §10](research/13-do-the-games-load.md). Four predictions measured out to nothing on the
way and are in §10.4a; the most useful is that **a wheel script is not inert with respect to the
game's timing** — traffic costs instructions, so a rally script has to be re-read off its own run.

Two things this needed and now exist: **`--bcm-film-from=N`** (a 100 k cadence is unaffordable
across 2.4 G of boot, and the ball is illegible at anything coarser), and **`--rate=5000000`** for
playback, which is one second of video per second of the machine's *simulated* time — the boot
films' 72 M/s is the CPU's rate and makes a rally unwatchable, because at `--clock=5` the game runs
14.4x fast against its own timer (§10.5).

**Still open, and small:** `Parachute`, `Music Quiz` and `Solitaire` have never been launched.

## 0y — ~~Nothing draws on a synthetic NOR~~ · **FIXED 2026-08-20 — one pin, three cells**

`GPO32_VAL` bit 14 is a general-purpose output Apple's bootloader drives when it powers the BCM. A
warm entry skips that bootloader, so the bit read back zero — and Rockbox's `lcd_init_device` keys
on it directly, with `lcd_update_rect` returning immediately while `display_on` is false. Every warm
boot this project has run took the ROLO recovery branch, and got away with it only because that
branch re-uploads the co-processor firmware from `flash_get_section('vmcs')` — which a real dump
carries and a synthesised NOR does not.

| | before | after |
|---|---|---|
| Rockbox warm, retail 5G | 3 858 px | **3 858 px** |
| Rockbox warm, synthetic 5G | 0 px | **3 858 px** |
| Rockbox warm, synthetic 5.5G | 0 px | **3 858 px** |
| iPodLinux, synthetic 5G | `Lost(0x40020000)` | **boots, same dmesg as retail** |

Retail unmoved. It sits beside `--sysinfo` because it is the same kind of thing: not something the
NOR carries, but machine state a bootloader leaves behind.

**`--norlog` nearly sent this the wrong way.** It counts through the `Nor` model and the warm recipes
install none, so it reported `0 flash reads` for *both* arms — "Rockbox never reads the NOR" was
believed until a control on the cold recipe returned 107 622. It prints `NOT MEASURED` now. **Run the
control before believing a zero**; that is the eighth instrument in this project to report an
absence it could not have observed.

## 0w — iPodLinux's userland stalls at ZeroLauncher's last step

The kernel boot is finished and clean. What is not is what happens next: ZeroLauncher — podzilla's
launcher, `/opt/Base/ZeroLauncher/ZeroLauncher` — draws its startup progress, reaches
**"Finishing Up…"**, and stops there.

**It is stuck, not waiting.** Two measurements, both with their control:

- **Not short of budget.** 21.5 G and 25 G produce the same frame, and the I/O between them is flat:
  6 398 reads / 75 writes against 6 395 / 77. Three and a half billion instructions bought two
  writes.
- **Not waiting for input.** A wheel script at 22 G — touch, SELECT, two rotations, MENU — fired
  **9 of 9 steps**, and the guest *read* them: `14 word reads of DATA (14 with a frame waiting)`.
  The frames arrive, the kernel consumes them, the screen does not move. **The control matters
  here**: this project has published a wheel finding before on a script that turned out to have
  fired 0 of 20 steps, so the fired-count is quoted rather than assumed.

**Its own log is on the drive, and it is empty — which is a fact, not a dead end.** The wrapper
script does `exec >> /opt/Base/ZeroLauncher/Misc/Launch.log 2>&1`, and after a writable run
`ipod-boot fat` finds the entry with **`size=0`, `clus=0`**. So the file was *created* — `/etc/rc`
reached `Launch.sh` and the shell opened its redirect — and nothing was ever flushed to it. The
launcher is running and has printed nothing to stderr, which rules out its failing loudly.

**Profiled, and it is a two-instruction spin.** `--profile-window=22000000000:24000000000` samples
only the stalled phase, so the 20 G of boot before it cannot drown it out:

```
profile: 31 250 000 samples over 160 buckets
  0x00a042d0   63.3%   19 772 049
  0x00a042c0   31.6%    9 862 775
```

**94.9 % in two adjacent addresses, and the whole phase touches 160 buckets.** Nothing else is
running. `0x00a04xxx` is in the flat binary's load region, so this is ZeroLauncher itself spinning in
userspace — not the kernel, and not a driver.

**The hypothesis to test next, and it is testable.** The wheel *is* delivered and the kernel *does*
read it — `9 of 9 steps fired`, `14 word reads of DATA (14 with a frame waiting)`. What is not
established is whether the kernel's input layer then produces something **userland** can read.
iPodLinux exposes the wheel through a device node; if frames reach the driver but no event reaches
`/dev`, a launcher waiting for input spins exactly like this and nothing in the kernel log complains.

Settle it by finding what those two instructions are. The flat binary is on the drive
(`/ZeroSlackr/opt/Base/ZeroLauncher/ZeroLauncher`, readable with `ipod-boot fat`), and the kernel
prints its load base at `BINFMT_FLAT: Loading file:` — subtract, disassemble, read the loop's
condition.

Worth doing before or alongside 0x above: a launcher spinning is exactly the kind of thing that also
shows up as "the guest executes several times the instructions hardware would".

## 0x — Why iPodLinux takes 4.8 *simulated* minutes to boot, when a real 5G takes about one

**Two different numbers, and only one of them is the emulator being slow.**

| | |
|---|---|
| wall clock to ZeroLauncher | ~17 min |
| **simulated iPod time** | 21.5 G instructions ÷ 75 instr/µs = **287 s = 4.8 min** |
| our speed | ~21 M instr/s against 80 M — **~26 % of real time**, which is the known figure |

So the 17 minutes is fully explained by 4.8 simulated minutes at a quarter of real speed, and there
is nothing to find there. **The 4.8 minutes is the question.** ZeroSlackr on real hardware is
reported to boot in about a minute, so this machine appears to be executing several times the
instructions a real one would.

**A profile of the first 4 G says where they go, and it is not spread out:**

```
profile: 62 500 000 samples over 14 508 buckets
  0x40002230   12.5%      0x40002220   12.5%      0x40003b00    9.1%
  0x40002470    7.6%      0x40003b10    6.1%      0x000188b0    5.0%
```

**~48 % of the kernel's boot is four addresses in IRAM**, in pairs a few instructions apart — the
shape of a spin loop, not of work. That lines up exactly with what
[research/16](research/16-the-third-bootloader.md) already recorded and never followed up: the
kernel polls `0x64004000` — the interrupt controller — **8 385 336 times**, from two program
counters a few instructions apart inside the interrupt path.

**What would settle it:** symbolise those four IRAM addresses against `vmlinux` and find what the
loop is waiting for. If our interrupt controller reports "nothing pending" where a real one would
have delivered, the kernel spins for real time that hardware never spends — and that is an accuracy
defect wearing a performance costume, which is the same shape as the masked-completion bug fixed
today.

**Do not read this as "the emulator is slow."** ~26 % of real time is the emulator's speed and it is
uniform. This is the *guest* doing several times the work, and that is ours to explain.

## 0z — ~~iPodLinux does not boot~~ · **IT BOOTS, 2026-08-20 — and the last fix mended Rockbox too**

```
Partition check: /dev/hda:  p1  p2
VFS: Mounted root (vfat filesystem).
Mounted devfs on /dev
Freeing init memory
BINFMT_FLAT: Loading file: …                          ← /bin/init running
kjournald starting.  Commit interval 5 seconds
EXT3 FS 2.4-0.9.19 … internal journal
EXT3-fs: mounted filesystem with ordered data mode.   ← ZeroSlackr's userland.ext3, loop-mounted
```

The whole chain: kernel → both partitions → FAT32 root → `init` → `/etc/pre-rc` → loop-mount →
`pivot_root`. Five emulator defects and one missing installation stood between us and this.

**The last one is the interesting one, and it was never about Linux.** ATA's INTRQ is a level the
drive holds until the host reads Status or writes Command; masking the interrupt *controller* does
not discard it. We asserted into a masked line and let the driver's own housekeeping sweep it away —
a write of IDE0_CFG's clear bits, issued while arming the next wait, drops the line whether or not a
handler ran. Holding the completion until the mask lifts is simpler and truer.

**Attribution, measured one build at a time on warm Rockbox at 1.2 G:**

| build | non-black pixels | what is on screen |
|---|---|---|
| pre-session | 580 | **"Installation incomplete"** |
| + 16-bit data register, `0x91` | 580 | same |
| + absent device 1, per-block completion, RECALIBRATE | 580 | same |
| **+ the masked-completion hold** | **3858** | **Rockbox's main menu** |

So the change that let Linux read its disk is the same one that takes **Rockbox** from a failed boot
to its menu — on a firmware whose source we have, which is the strongest evidence available here
that the model moved toward the hardware rather than toward one guest. RetailOS is byte-identical
across all of it on a pinned A/B: `4 · 611 · 4 · 3 · 11`.

**Run Rockbox in every A/B, not just RetailOS.** RetailOS is inert to most of the ATA path — it moves
bulk data by DMA and polls with interrupts masked. Rockbox exercises PIO, interrupts and the
filesystem, and it is the only guest here whose source can be read when a number moves. Four changes
this session were checked against RetailOS alone and looked like no-ops; on Rockbox one of them was
the difference between a boot and a failure.

## 0a — ~~iPodLinux cannot read its partition table~~ · **SETTLED 2026-08-20 — our IDE data register was 32 bits wide and should be 16**

**The whole iPodLinux blocker was one line of ours.** Every register in the PP502x IDE block is four
bytes apart, so a 32-bit access to `IDE_BASE+0x1e0` touches four byte lanes — but the register under
it is **sixteen bits**, and lanes 2 and 3 are its empty upper half. We served them as more sector
data, so a 32-bit read consumed two ATA words instead of one.

Only iPodLinux ever noticed. Its identify path reads the port with 32-bit loads and keeps the low
halfword — correct for a 16-bit register — so it kept our words 0, 2, 4, 6 … and dropped every other
one. `struct hd_driveid` then read `cyls` out of our word 2 and `heads` out of our word 6, and a
drive reporting 16 heads was diagnosed as having 63. Rockbox declares the port `unsigned short` and
Apple's firmware moves bulk data by DMA; **measured, both read lanes 0x1e0/0x1e1 only and never touch
0x1e2/0x1e3**, which is why the fix cannot move them — the branch is unreachable for them.

Before → after, `ipod-boot loader`:

| | before | after |
|---|---|---|
| `INVALID GEOMETRY: 63 PHYSICAL HEADS?` | yes | **gone** |
| `end_request: I/O error` on sectors 0/2/4/6 | 8 | **0** |
| `unable to read partition table` | yes | **gone** |
| reported drive size | — | **8590 MB**, correct |
| `0x91` INITIALIZE DEVICE PARAMETERS | never sent | **sent, and now modelled** |
| furthest line | `Kernel panic` | **`Partition check: /dev/hda`** |

**The arithmetic was in every log for weeks.** iPodLinux does 256 32-bit reads per IDENTIFY: one
word each is 512 bytes, exactly one sector; two words each is 1024, twice what it asked for. And the
lane census printed the asymmetry in one glance — `+0x1e0` and `+0x1e1` at 820 736, `+0x1e2` and
`+0x1e3` at 512. Reading it required nothing but noticing that two of four byte lanes were 1600×
colder than the others.

**R4 applies to this in full**: the machine now gets further with this firmware than it ever has, so
every "iPodLinux never…" in [research/16](research/16-the-third-bootloader.md) and
[research/17](research/17-the-boot-matrix.md) is a claim measured on a machine that no longer exists.
Two are already corrected there; the rest have not been re-run.

### What it uncovered next, and where it stands

The width fix let iPodLinux reach code no firmware here had ever exercised, and it found three more
inaccuracies immediately. All three are fixed, and **retail is unmoved across all of them**:

- **We were presenting a phantom slave drive.** A 5G iPod has one ATA device; we answered every
  taskfile register whatever the DEV bit said, so the kernel attached *two* disks of the same size
  and interleaved their commands through one state machine. An absent device drives nothing onto the
  bus and reads back zero, which is the signature `ide_probe` uses. Two `ATA DISK drive` lines → one.
- **A multi-sector PIO read interrupts once per block, not once per command.** We armed the
  interrupt controller only on a *write* to the ATA window, so the second and later sectors of a
  transfer — loaded while the guest is *reading* the data port — completed silently.
- **RECALIBRATE (`0x10`) is a legal command**, and we were aborting it. Linux issues it while
  recovering from a timeout, so we answered an error-recovery attempt with a fresh error.

**Where it stops now — `hda: lost interrupt` during the partition scan.** The kernel's own commands,
read off the uncapped log:

```
[3207] cmd 0xec IDENTIFY
[3208] cmd 0x91  nsector 0x3f      INITIALIZE DEVICE PARAMETERS — 16 heads, 63 sectors
[3209] cmd 0x10                    RECALIBRATE
[3210] cmd 0x20  nsector 0x08  lba 0
[3211] cmd 0x20  nsector 0x08  lba 131072
[3212] cmd 0x20  nsector 0x08  lba 65536
```

Three 8-sector PIO reads, of which **15 of the 24 sectors are drained** — so it is not that the
transfer never starts, it is that it stops partway. The suspect is interrupt *timing*, not delivery:
the counters say `raised 6450, DELIVERED to a handler 16, acked by status read 6426`, and a
completion that a status read clears before the CPU takes it is indistinguishable, to the kernel,
from one that never came.

**What would settle it:** an instrument that records, for each IDE completion in the kernel phase,
the instruction count at which it was raised, delivered, or acked — the three counters above are
totals and the loader's polling dominates them. Do not reason about this one further without it;
four wrong answers in this file were produced by exactly that.

**The per-block completion is load-bearing, and this was nearly thrown away on a broken control.**
An A/B appeared to show it moving RetailOS from 611 READ DMA / 4 WRITE DMA to 416 / 86, so it was
reverted. That comparison was confounded: the two arms resolved different settings files and cloned
different source disks, so it compared two machines as well as two builds. With `FLASH=`, `DISK=`,
`BUDGET=` and `WORKDISK=` pinned in both arms, RetailOS is **identical** with it and without it —
`4 · 611 · 4 · 3 · 11`. And for Linux it is the difference between working and not:

| | per-block completion | kernel data drained | IDE completions delivered |
|---|---|---|---|
| with | on | **16 sectors** | **16** |
| without | off | 6 sectors | **0** |

Zero. Without it the kernel's multi-sector reads raise no interrupt after the first block, which is
`lost interrupt` exactly as reported. **A control with two variables in it is not a control** — and
every "retail unmoved" measured before the pinning was after-vs-after and proved nothing.

### The other half was never an emulator problem: we had installed one file out of 1 805

`resources/vendor/ipodloader2/docs/IPOD_LINUX_INSTALL.md` — vendored here throughout — says to copy
**five** directories from the ZeroSlackr archive to the iPod's FAT32 root: `/etc/`, `/ZeroSlackr/`,
`/bin/`, `/boot/`, `/dev/`. The drive had **`/boot/vmlinux` and nothing else**. The archive had been
downloaded once, one file taken out of it, and the rest discarded.

So even a perfect emulator would have booted that kernel, mounted `/dev/hda2`, and found nothing to
execute. **Check that an OS is installed — against its own install document — before treating "it
does not boot" as an emulator question.** That check is one partition-table dump and one directory
listing.

The archive is now kept: `resources/vendor/zeroslackr/` holds the `.7z`, its extracted `tree/`, and a
`PROVENANCE.txt`. `tree/boot/vmlinux` matches the sha256 ROADMAP.md recorded, byte for byte. A drive
built from it (`bin/`, `boot/`, `dev/`, `etc/` via `ipod-boot put-files`) carries `busybox`, `init`,
`sh`, `inittab`, `pre-rc` and the 8 MB `boot/userland.ext3` that `pre-rc` loop-mounts before
`pivot_root`. It reaches the same `lost interrupt`, which is how we know the two problems were
genuinely separate.

## 0b — ~~The retail fingerprint moved to 102 and nobody knows why~~ · **SETTLED 2026-08-18 — the disk had been written to**

**Not a regression, and not the code.** On a genuinely pristine disk the fingerprint is:

```
ipod-boot retail --clock=5, BUDGET=600000000, DISK=…PRISTINE.img
  ata commands: 576      ata dma: 467 transfers, 21 088 768 bytes
  unmapped: 4 reads, 0 writes across 1 pages
```

which is in family with the 554 this file recorded on 2026-08-14, the difference being the device
fixes since. **The 102 was the working image having accumulated state**: `drives/ipod8g-retail.img`
was written at 14:41 that day, so RetailOS found its volume already formatted and did a fraction of
the work. Every number taken against that file since depended on how many times it had been booted.

**Two real defects came out of it, and the second is the one that mattered:**

1. The recipe clones per run, so a run cannot dirty its source — that part was working. But the
   **source itself is mutable**, and nothing announced that it had changed.
2. **The recipe could not run against `PRISTINE` at all.** The clone inherits the source's mode,
   `*.PRISTINE.img` is `chmod 444` on purpose, and the machine died with `Permission denied` before
   executing an instruction — on precisely the image most worth measuring against. That is *why* the
   fingerprint came to be taken on a mutable working copy. Fixed: `clone_file` now makes the clone
   writable, and `ipod-boot retail` against `PRISTINE` reproduces 576.

**Measure against `PRISTINE`.** A fingerprint taken on the working image is a statement about that
file's history, not about the machine — and it will not reproduce on anyone else's copy.

## 0c — Cold-booted Rockbox reads 0 mV and powers itself off · **top of the live queue, 2026-08-18**

*(An orphan duplicate of item 1b's heading stood here with no body until 2026-08-18. Its real
entry, retired, is further down. A file that carries a heading twice will eventually have someone
answer the empty one.)*

**This file is older than the work below it.** Everything under item 0c is RetailOS, measured
2026-08-14; since then a **second operating system boots here** and is now the sharper instrument
(ROADMAP.md M1/M2). Numbers below are not wrong, they are un-re-run, and R4 applies to every one of
them that a Rockbox-driven model change has touched.

`adc_read(ADC_BATTERY)` returns `0x2c0` warm and **`0` cold**, the filter walks 4 160 → 3 300 mV in
about twenty-seven power-thread iterations, and `query_force_shutdown` fires — 315 `sys_poweroff`
calls in a run. The disk is not the variable; the boot path is. Nine explanations are eliminated by
measurement in [research/06](research/06-rockbox-as-oracle.md), and one of the nine was retracted
there for being read off the wrong instrument (see the ADC row in the table below — the correction
**inverted** the finding: Rockbox issued no conversion at all).

**What is left is one contradiction, and it names its own measurement.** The store at `0x000836ac`
ran twice, and `adc-ipod-pcf.c` shows nothing between the `ADCC1` write and that store that could
skip it — so two stores mean two conversions were *started*. The uncapped tally records **one**, and
it is Apple's. A write the CPU executed did not become a conversion in the device.

The cheapest hypothesis covering both symptoms is that the controller's data registers at
`i2c_base + 0x0c + 4i` are unbacked on the cold path: `pp_i2c_send_bytes` stages the register number
and value there before raising `I2C_SEND`, so the model would write PMU register `0x00` instead of
`0x2f` (no conversion) and the following read would copy its answer into the same dead registers
(`data[0..1]` zero, `value` zero). One broken mapping, both symptoms. Its competitor is a non-zero
`adc->conversion` at **`0x40008ea0`** — never assigned by `adc_init`, only ever zeroed by the IRAM
init copy, on the one path where IRAM was seen carrying instruction words — but that can only
explain the zero, never the missing conversion.

**Settled when** the cold path completes a Rockbox-issued channel-2 conversion and the boot survives
its own battery reading. Three commands get there: `--watch=0x40008ea0`; a count of read replies
delivered into those registers and bytes that found no region; and the same count for the **write**
direction, which is the half that decides whether a conversion starts and which no instrument
reports today.

## 1 — Where does a surface actually live on the co-processor? · **top of the RetailOS queue, 2026-08-14**

**This is the successor to "make RetailOS draw", and it is what that question could not check.**
RetailOS draws: with `--bcm-registry` it presents 41 times and lands **76 607 non-black pixels** at
`0xE0000` — its own "Charged" screen — against the control arm's 2 922, with the back buffer at
`0x106000` byte-identical to the front. But the address the model hands back for a surface is
**chosen, not derived**: `0xE0000` upward, on Rockbox's authority (it calls that address
`BCMA_CMDPARAM`) and on the fact that Apple's bootloader fills exactly `0xe0000..0x10581e`. The reply
format the reader accepts says the co-processor returns *an* address, not *which*.

**Wall A — the display. DOWN 2026-08-14 with pixels, not a counter — research/10 Addendum 29, and
the screen is the MAIN MENU since Addendum 30.**
RetailOS's output stage is `FUN_00164f44` (upload the dirty scanlines to a co-processor surface,
then `FUN_00286b6c` to show them). It had **0 arrivals** while its caller `FUN_001650f8` was asked
to flush **42 times**. It now runs **41 times**, and
`--bcm-dump=0xE0000:140:F0` differs from the handoff dump for the first time in the project.

**The first screen was Apple's "Charged" screen, and that was our fault, not RetailOS's.**
`map_hardware` left `GPIOL_INPUT_VAL` (`0x6000d13c`) at zero; bit `0x08` is the main/FireWire charger
line and it is **active low**, so we were telling RetailOS it was plugged into a wall charger — 130
times a boot, through pin `0x63` of Apple's own GPIO accessor `FUN_00282b70`. Telling the truth
needed a PMU fix (our ADC reported every completed conversion as **zero**, which is what made
Apple's bootloader refuse to boot with no charger). With both done:

```
ipod-boot retail --clock=5 --stop-when-idle=400000000 --bcm-registry            -> the LANGUAGE list
  ... --clickwheel --wheel=@1500M:touch,+2M:press=select,+2M:release          -> the MAIN MENU
      BUDGET=3000000000 --bcm-dump=0xE0000:140:F0:menu.ppm
```

`iPod` / Music / Photos / Videos / Extras / Settings / Shuffle Songs, chevrons, battery in the
corner. **The click wheel drives it** — the whole path from `0x7000c140` through the ISR, the event
queue, the widget and the compositor, validated by one picture. Everything below about Wall B being
"severed at the last hop" is superseded by that: the events reach the widget now that the widget is
on screen. See research/10 Addendum 30 for the A/B, the ADC bisect (`ADCIN1_SUBTR >= 0x080`), and
the retraction of research/09's "acceptance condition not identified".

Two ways to settle it, cheapest first:

1. **Ablation.** Move the allocator's base to some other free region and re-run. If the drawn frame
   follows it, the address is arbitrary and the pixels are RetailOS's wherever they land — the claim
   survives and gets weaker in a stated way. If the panel goes blank or the frame lands somewhere
   nothing reads, the address is load-bearing and must be sourced rather than chosen.
2. **From the co-processor's own image.** `research/11` §1 gives the `rsrc` `vmcs.bin`'s export table
   at `0x2160C` — 183 `(code_addr, name_ptr)` records — so `dispman_object_create` and the
   `vc_image_*` allocators have real addresses inside a file we hold. What they do with an address
   is readable without executing anything.

**Settled when** the surface address is either derived from `vmcs.bin` or shown by ablation to be
arbitrary, and `research/12` §8 item 4 can be struck.

**The other three assumptions and the missing timing model are in the same section**, and item 2
below carries the order to attack them in.

### What is already settled here, and must not be re-derived

The ARM-side pipeline is **described end to end in [research/12](research/12-how-retailos-draws.md)**
— paint, show, damage/flush, present, transport, composite — with each stage's arrival count on both
arms. Read that rather than reconstructing it from research/10's Addenda 20–29, which are the
chronology and contain three retracted framings (research/12 §9 names them).

The two facts most likely to be re-derived by accident:

- **Painting and showing never depended on the co-processor.** `0x0021acac` is entered **566** times
  from 5 sites, and the visibility slots `0x00219284` / `0x0021ada8` **2 278** / **68** times — *the
  same, to the arrival, with and without `--bcm-registry`*. An explanation of a blank screen that
  reaches for widget state is explaining the wrong stage.
- **Mailbox `0x16` is not the output stage.** It is the display server's async blit-request channel;
  `t_graphicsManager` sits in `KS_receive` on it because no image view ever has an image, because no
  photo is ever opened. `[obj+0xa0]` is **a photo**, not a draw target.

### Still open under this heading, and genuinely open

**The stuck visibility state.** Two widget chains cross an object whose flags are `0x5a00` —
`0x5a00 & 0x1800 == 0x1800`, both bits set at once — and `FUN_00219284` is inert in both directions
on it, so the show walk never descends past it. `FUN_0021a0fc` clears both bits before setting one
and so cannot produce that state; some other writer OR'd `0x1000` in. **This is real and it is not
why the screen was blank** (566 paints happen regardless); it is why *that subtree* is not shown.
Finding the writer is research/10 Addendum 24 §4's open question, unchanged.

**Who consumes the 19 wheel events.** `0x000ada4c` x4 (buttons) and `0x000cd6a0` x15 (`'Weel'`
messages) reach the event system in a run where the list widget at `0x001ae214` receives none of
them. One `--enterlog` on `0x00151a40`'s queue consumer answers it. Unchanged by any of the display
work.

> **A second agent is working RetailOS from the charging screen to the main menu as this is written,
> and will touch `research/10` and possibly the PMU model. That question is open and nothing here
> predicts its result.**


## 2 — Retire bypass #6 · **narrowed again: it is now four assumptions, not a missing device**

**What #6 was — "the co-processor never publishes its service directory, so RetailOS disables its
own display path" — is done.** `--bcm-registry` publishes the header at internal `0x1f0`, the
eight-slot channel directory behind it, one 0x50-byte record tagged 2, and a responder for the ring;
every field is derived from RetailOS's own reader, none of it tuned to make a symptom go away.
Measured, both arms of the standard recipe today:

| | control | `--bcm-registry` |
|---|---|---|
| present — `0x00164f44` | **0** | **41** |
| RPC — `0x0028861c` | **0** | **165**, matching `bcm gencmd: 165 answered, 0 dropped` |
| `pp dma` | 4 / 201 216 B | 104 / 5 225 216 B |
| `0xE0000` | 2 922 non-black px | **76 607** |
| Idle / buckets | 1 610 279 157 / 38 266 | 1 609 736 757 / 38 518 |
| ata commands | 770 | 706 |

**The flag stays OFF and stays out of every recipe**, because it moves 5 MB of DMA and 64 ATA
commands: a run carrying it is not comparable to anything measured before it.

**What is left of #6**, in the order to attack it —
[research/04 §#6 today](research/04-bypass-ledger.md) and
[research/12](research/12-how-retailos-draws.md) §8 carry the same list:

1. **Where a surface lives.** Item 1 above. Largest of the four, and the one that would invalidate a
   published pixel count.
2. **Give the responder a delay.** There is **no timing model at all** — the reply is placed
   synchronously inside the doorbell write. Any bug that only appears when a reply is late cannot
   appear here, and this emulator has already been caught answering too early twice (`IDE_COMPLETION_USEC`,
   `OPTO_REPLY_USEC`). Cheap: add a delay, re-run, see whether the frame survives.
3. **Publish tags 1 and 7.** Tag 1 is GENCMD (a printf-formatted command string in, a text stream
   back); tag 7 is unidentified and its consumer allocates two event groups and four buffers
   (0x520, 0x520, 0x34, 0x34). Publishing them forces both to identify themselves.
4. **Retire the `--bcm` synthesis underneath.** `BCMA_COMMAND` acknowledged and `CONTROL` answering
   a fixed `0x52` — the *original* #6, and the only part still live in all six recipes.

Handles being a counter and non-`8` opcodes replying with a handle in payload word 0 are the
remaining two assumptions; neither is constrained by any branch RetailOS takes on this path, so
neither is testable until something downstream reads them.

**Settled when** a frame reaches the panel with none of those in place. Until then a drawn frame is
evidence that **RetailOS's own pipeline works end to end** — which is all Addendum 29 claimed — and
not evidence that we have a co-processor.


## 1a — ~~What is RetailOS waiting for at the 1.61 G idle?~~ · **ANSWERED 2026-08-14 — the census, then the display**

> The census landed: 61 tasks, 1 runnable, 47 `KS_pend`, 8 `KS_waitm`, 5 `KS_receive`, each with
> the semaphore or mailbox it named. Most are device tasks waiting on hardware that is not there —
> normal for an iPod on a bench. Two are not, and they are **independent**: the display (Wall A)
> and the wheel (Wall B). The framing this item was written in — *one* gate explaining both — is
> disproved by the A/B in Addendum 17 §8. Kept for the measured shape of the idle below.

*(That work landed. The census is research/10 Addendum 17; the display half became item 2 and then
research/12, and the wheel half is item 3. What survives is the measured shape of the idle below,
which is still the cheapest orientation for a cold session — and the residual question, which is
still open: **what is the last unnamed wait?** It is no longer a blocker on anything, because the
display draws through it and the boot idles at the same instruction with and without a
co-processor.)*

The boot runs to `Idle after 1 610 279 157` with 2 686 062 halts, `ide irq … enabled=0`, and no new
code for the last 400 M instructions. *(This paragraph said `1 610 256 821` / 2 685 679 — two
emulator fixes ago. Re-measured 2026-08-14.)* Most of those halts fall inside the trailing window, so
R7's busy-tell does *not* fire: this is a machine actually asking to be switched off, not one
spinning over code it has already run. *(The "2 199 826 of those halts" this paragraph used to give
was measured two emulator fixes ago and has **not** been re-run; the qualitative claim is what the
`cpu sleep` line supports, and that line is in the baseline block above.)* What it is waiting *on* is
unnamed.

What is already known and should not be re-derived — `--enterlog` on all four, with `0x0016b044` as
a reached control, on today's 4 G baseline:

```
0x0016b044  @52249160        MP3ExampleTask's body — the control
0x0019db14  @870718070       the recursive view-tree builder returns
0x0016b094  @1061153043      MP3ExampleTask posts 0xea
0x00284538  @1069131064      APPLEBOOT's terminus; lr=0xeeeeee09, not a code address
```

All four were recorded as *"still unreached at 2 371 809 167 instructions"* as recently as
Addendum 11. So this is a **later** wall than every wall this file has ever carried, and by R4 it is
a new question rather than a survivor of the old ones — do not assume any prior diagnosis applies.

**Still settled when** the object being waited on is named the way `0xd1` was named in Addendum 15 —
the heap object, the RTXC id, and the producer that would post it — and the pend is counted at two
budgets so R8 is satisfied before anyone writes "deadlock". Nothing depends on it today, which is why
it is a tombstone rather than a queue item; promote it back if it starts blocking something.

## 1b — ~~The UI phase: two walls, and only one of them is ours~~ · **RETIRED 2026-08-14 — the framing was wrong twice and the display now draws**

> Kept for its answers, per this file's convention. Three things this item asserted are dead:
>
> 1. **"Two independent walls."** They were not independent in the way stated. The A/B that
>    established it showed the `CPU_INT_STAT` bit-30 fix moving no display counter — which is true,
>    and is a much narrower claim than "the display and the wheel are separate problems". The
>    display's blocker turned out to be one function's early return four calls above the bus, and
>    the wheel's turned out not to exist.
> 2. **"Wall A is the output stage."** Right, and one level too low: the output stage was waiting on
>    a channel index, the channel index was waiting on a directory, and the directory was being
>    destroyed in transit by our own `Bcm::read8` (R12). The stuck `0x1800` visibility state this
>    item spent most of its length on is real and blocks *that subtree*, not the screen.
> 3. **"The display does not draw."** It does, as of 2026-08-14 — 41 presents, 76 607 non-black
>    pixels, its own "Charged" screen.
>
> **Its surviving answers, all still true:** the boot reaches a genuine idle with 61 tasks, 24 of 24
> startup modules and 5 of 5 phases; `APPLEBOOT` terminates; RetailOS formats and populates its own
> FAT32 volume, and **exactly 41 sectors differ** from the pristine image — anything outside those
> 41 is the image's, not RetailOS's, and reading a directory listing instead of a diff is what makes
> that easy to get wrong; the widget at `0x001ae214` has received **no key or scroll event of any
> kind**, and its six type-13 events were every one of them addressed to another widget
> (codes `0xa9` / `0x4296` / `0x5082` against ids `0x509c` / `0x50a1`); `0x001ad70c` is the mode-2
> arm of that widget's `setVisible` and nothing calls it. The event question moves to item 1 above;
> the display question is answered.
>
> Two corrections it owed the file, kept because they are still true: `0x00180ce8` has **five**
> callers, not one, and all five measure zero; and `0x00180e54` *clears* `[view+0xa0]` on the way
> out, so that field reading zero in an idle dump is not evidence of anything.


## 3 — ~~Answer the click wheel's `0x8001052a`~~ · **SETTLED 2026-08-14 — it is a write, not a question**

`--clickwheel` / `--wheel=SCRIPT` / `--wheel-no-irq` model the four registers (`0x7000c100` control,
`0x7000c104` status, `0x7000c120` transmit, `0x7000c140` receive) and IRQ 40, second-sourced from
Rockbox and re-attested against Apple's own binary.

**`0x8001052a` is opcode `0x052a` — *set reporting* — with a payload byte at bits 23..16, and the
hardware sends no reply.** `0x00283e10` is the whole API (`orr r0, =0x8000052a, r0 lsl #16` then a
tail `b` to the sender); its callers are the one-liners `0x000bbdb0` (`mov r0,#1`) and `0x000b4638`
(`mov r0,#0`). Five senders across the two Apple stages, none of which reads `0x7000c140` afterwards
— the boot ROM's copy at `0x000c9714` writes TX, starts the transmit, spins 10 000 iterations and
returns. `--wordref=0x0000052a` over 7.5 MB is **0**, so nothing could parse such a reply; a reply
would take the decoder's bad-frame arm and drive `SerialOptoTask`'s receiver reset seventy times a
boot. See [research/10](research/10-the-resource-image.md) **Addendum 20**.

Landed: the model recognises the opcode, records the payload, replies with nothing, and gates
autonomous frames on it. A/B, both ways, one variable:

| arm | frames posted | suppressed |
|---|---|---|
| `--clickwheel`, no script | 3 (the ROM's own query replies) | 0 |
| 36 steps `@1650 M` | 39, 0 dropped, 39 reads all ready | 0 |
| 36 steps `@100 k…160 k` (**before** the ROM's enable at `@238 346`) | **0** | **36** |
| `@200 000:touch, +50 000:rotate=+4` — straddles it | 4 | **1** (the touch) |

Inert where it should be: the baseline fingerprint reproduces to the instruction, and the wheel run
at `@1650 M` is byte-identical to the same recipe on the pre-change binary but for the
unknown-command line it retires. `cargo test --release` is **21 passed** — the new test asserts the
silence, the classification and the gate, each with a matched control.

**Two corrections it owes this file** (both fixed in place above): the "30 re-sends" were a periodic
caller, not a retry — `SerialOptoTask` transmits nothing; and Addendum 19's "no event ever posted
upward" is wrong. Watching the posters rather than the display chain gives `0x000ada4c` **x4** and
`0x000cd6a0` **x15** in the same run, the latter putting a `'Weel'` (`0x5765656c`) message into the
event queue at `[0x1081e0e0+0x18]`. **Nineteen UI events, and the widget at `0x001ae214` receives
none of them.**

**What this opens** (still open, and it now sits under item 1's "still open" heading): *who
consumes those 19 events?* One `--enterlog` on `0x00151a40`'s queue consumer answers it, and it is
the only remaining live link between a working wheel and a blank screen.

## 4 — The prototype ROM's power-off after a restored `aupd` · **bypass #12's open half**

Retired on the retail ROM: `ipod-boot flash-update` reproduces the real thing end to end — boot 1 prints
`Running 'aupd'` → two `iPod CFI Flash Firmware update` passes → `END MARKER - VALID`; boot 2 prints
`Running 'osos'`, with no file edited between them. The bypass turned out to have been *the
updater's own last write*: it ends with `WRITE SECTORS` to LBA 96, setting the directory entry's
`+0x08` from 0 to 1 so the ROM skips it next time. `--disk-writable` is therefore as load-bearing as
the flash model.

**Still open on the prototype ROM**, which `cold-boot.sh` runs by default. It reads its firmware
partition at **4× the MBR LBA** (252/284 against the retail ROM's 63/96, 2 KiB blocks against 512 B).
With `aupd` restored where that ROM actually looks, it reads all 2 104 sectors and then runs an
orderly power-off — `0x40006138` → `0x40003984` → `0x4000159c`, `b .` — without printing a line.
Undiagnosed.

Two things to carry forward regardless: **`aupd` is encrypted**, not "plainly executable ARM" as
research/07 used to claim — 7.9998 bits of entropy per byte, not one printable string including the
ones it demonstrably prints, and its checksum is over the plaintext. And the chosen NOR part
(SST `0xbf`/`0x273f`, uniform 4 KiB sectors) is **a choice among the eight the ROM accepts, not a
measurement** — neither dump records what the hardware carried.

## 4b — Do the hardware chords do anything? · **one negative, and it is not enough to conclude from**

**The input side is real.** The window's keys map to a genuine press and release, so holding `M`
and `space` holds `MENU`+`SELECT` for as long as your fingers do, and `--wheel`'s `down=`/`up=`
pairs do the same on the command line. Nothing had to be added for that; what was missing is that
nobody could know the chords existed, and the key legend now lists them.

**What the firmware does with them is measured once, and the answer was no.** `PLAY` held for
300 M instructions — 4 s of simulated time — on a retail boot:

```
ipod-boot retail --bcm-registry --clickwheel \
  --wheel="@1800M:touch,+10M:down=play,+300M:up=play,+10M:release"
  -> 2 244 489 794 instructions, 933 ATA commands, the Language picker still on screen
```

RetailOS is alive and did not sleep. **That is one negative on the first screen the firmware ever
draws**, which is the screen least likely to handle a power chord, so it does not establish that the
chord is unmodelled — only that it does nothing *there*. The same test from the main menu has not
been run.

*(The first attempt at this measured nothing at all: without `--bcm-registry` RetailOS never gets an
answer to its service lookup and never draws — ledger #6 — so the panel showed Apple's boot logo and
the run said nothing about sleep. A recipe missing the flag that makes the machine draw is a recipe
that cannot answer a question about the screen.)*

Until it is measured, the window's `power off` and `restart` stay what they are and **say** what
they are: dropping the machine is the equivalent of pulling the battery, not the sleep chord, and
the hover text now says so and points at the key that is.

## 4c — ~~`--wheel` scripts do not fire under `ipod-film`~~ · **SETTLED 2026-08-19 — it was the unit**

The film was not the cause and neither was Rockbox. **`--wheel` anchors in *executed instructions*,
and a machine sitting at a menu spends most of its budget halted**, so the two diverge silently:

```
IMG=rb-main.raw BUDGET=200000000 ... --wheel="@50M:touch,+10M:down=select,+30M:up=select,+10M:release"
  script: 2 of 4 steps fired          <- with and WITHOUT the film, identically
```

Under a 200 M budget it executed under 90 M. So a script anchored at `@1600M` over a 2 G budget
fired **0 of 20 steps**, and that read as "Rockbox ignores the wheel". It does not — the press never
happened.

A step may now be anchored in **simulated time** instead: `@12s`, `+250ms`. Same script, same drive,
same everything else:

```
--wheel="@12s:touch,+1s:down=select,+1s:up=select,+2s:release"
  script: 4 of 4 steps fired
  bcm: 34 commands kicked, 34 frame updates      (23 with no input)
```

and Rockbox opens its file browser.

**Instructions stay the default and stay right for measurement** — reproducible, unmoved by how much
the machine slept, which is what every calibrated recipe here depends on. Seconds are for driving a
user interface, which is what the firmware's own timers measure. A script must use one or the other
throughout; mixing is refused rather than resolved by a rule nobody would remember.

## 5 — Standing, unblocked, small

- **Is `0x70000030` bit 27 ever *not* ready?** Nothing in the ROM image ever finds it clear — its own
  waits are the only readers and are satisfied on the first read — so "always ready" and "ready
  after a real busy window" are indistinguishable from this firmware. Settling it needs the PP5022
  datasheet or a logic capture of a real part mid-write. Same caveat, smaller, for `XMB_RAM_CFG`
  bit 31.
- **Which NOR part the hardware actually carried.** We drive SST `0xbf`/`0x273f` — row 3,
  `SST39WF800A`, which is what the run report prints (`nor model: JEDEC 0x00bf/0x273f`) and what
  `lib.rs`'s own test asserts. The ROM accepts eight, and neither dump records the answer. A
  photograph of a board would settle it. *(This bullet said `0x2781` until 2026-08-14 — the row-4
  `SST39VF800A` we drove before the switch recorded in research/04 §"The flash part". The switch
  landed in the model and not in the three documents describing it.)*
- **`--boot-osos`'s `COP_STATUS`/`PLL_STATUS` guard.** `trace.rs` installs those two built-in
  overrides *only when no `--rdval` is given*, so adding or removing any `--rdval` silently toggles
  two unrelated models (ledger #7 and #8). It has already produced one false A/B. Decouple it.
- ~~**Answer the click wheel's `0x8001052a`**~~ — **settled 2026-08-14, item 3 above.** It is a
  write (`0x052a`, payload byte at bits 23..16), the hardware sends no reply, and no Apple code
  reads one. The two sequels this bullet used to carry were both corrected earlier and are gone
  with it.
- **WM8758 audio codec at I²C `0x1a`** — ~~432~~ ~~at least 52~~ **exactly 52** transfers on the
  baseline recipe, entirely unmodelled. The registers it touches are recorded, so a model gets a
  verdict on a run we already have: `reg 0x54 ×5 · reg 0x6f ×4 · reg 0x06 ×3 · reg 0x6b ×3`.
  *(432 was a different run at a different address encoding — corrected 2026-08-13, research/10
  Addendum 13 §2.)*
  **The "at least" is retired 2026-08-14, and the caution that produced it was right to be issued.**
  `i2c_log` capped at 4 096 and the 4 G baseline printed exactly that, so every per-device figure
  under it was a floor and this bullet said so. The tally is now kept on the bus: the census is
  **4 933** transfers, the PMU's two addresses go 2 034 / 2 010 → **2 506 / 2 375** — and the
  WM8758's **52 does not move**, because all of its traffic happens before the log fills. A floor
  that turns out to be tight is still not a measurement until it is checked, which is the whole
  content of R6.
- **`0x0000133c` / `IDE_BASE+0x410`** — a 0.6 s periodic poke from low-vector code that lands one
  byte outside the modelled DMA window and is never read. Unexplained.

---

## Retired

Kept, not deleted. The record of what was believed and why it was wrong is the most valuable thing
in this repo, and every entry below is an item that was worked as *the* blocker.

### ~~0 — Retire the ledger before investigating around it~~ · **DONE 2026-08-14 — it is a rule now (R3)**

**Nine bypasses are retired** (#1, #2, #3, #5, #10, #11, #12-retail, #14, #15), and `research/04`
carries a per-row **`Live in`** column — added after an audit found `ipod-boot flsh` and `ipod-boot flash-update`
still passing three bypasses the file had marked RETIRED, which meant the run that *proved* #12's
retirement was obtained with three retired bypasses switched on. A ledger that records a retirement
but not where the flag still lives cannot catch that.

**`ipod-boot retail` carries exactly four live bypasses: #6, #7, #8, #9 — and only #6 is a flag.**
#7 (`COP_STATUS` sticky) and #8 (`PLL_STATUS` locked) are pushed unconditionally by `trace.rs`;
#9 (`IDE0_CFG` bit 3) is ORed in by `Ata::read`. A reader who greps the recipe for "which bypasses
am I running" finds one of the four. #4 `--sysinfo` is **not** on the retail path at all — verified
today, `cold-boot.sh` and `ipod-boot retail` pass none. **#17 is gone** — both its flags were
deleted 2026-08-19 once `0x60009000` was modelled and its ring was shown to drain unaided.

### ~~0b — Model the transfer engine at `0x60009000`~~ · **DONE 2026-08-13 — a second DMA controller**

Register-identical to the one Rockbox names at `0x6000a000`, two channels instead of four,
completion on IRQ 27. Modelled, default-on, no flag. Three things had to be modelled, not one: the
engine, the **read-to-clear** `STATUS` latch, and the **forced-interrupt** registers
`0x60004014/18/1c` that carry RetailOS's deferred completion. `vmcs.bin` uploads in full and the
in-use ring drains to `[0,0,0,0]` with the emulator never touching it, because the real driver's ISR
owns it. Addendum 9.

### ~~1 — Which task never gets kicked?~~ · **ANSWERED — `APPLEBOOT`, and it now finishes**

`APPLEBOOT` (TCB 9, priority 15, entry `0x002844e0`) blocked at @51 764 626 in `KS_pend` on
semaphore `0xe0` — the `sync()` after the first 64 KB chunk of the `vmcs.bin` upload. `0xe0` is
signalled once the engine above is real; `APPLEBOOT` runs on, re-blocks on `0xea`, and — after
item 3's two fixes — **reaches `0x00284538` and terminates**.

Three premises in the original framing were wrong and are corrected at their sites: `0x002a9c84` is
not a hook table (one non-zero word, walked once, `KS_execute(1)`); "no TCB at all" was wrong for
seven of the ten tasks named in Addendum 7 §7 (only `DiskReaderTask` and `ImagePresentationEngine`
really are absent); and `0x60009000` **was** being programmed all along — 208 byte-writes 1 100
instructions before the pend, invisible because `--watch-range` could not see word stores.

### ~~1a — Model CPU sleep~~ · **DONE 2026-08-13**

`CPU_CTRL` (`0x60007000`) bit 31 is modelled; the run report prints `cpu sleep: N halts, M ms of
simulated time skipped`. Instruction count means progress again. This item was still sitting in the
queue as open work a day after the commit landed — the exact drift this file exists to prevent.

### ~~2 — Why does RetailOS never read `rsrc`?~~ · **SUPERSEDED — on the retail path it does**

The premise was prototype-only. `ipod-boot retail` reads LBA 14864 (the FAT boot sector), 14870 (the
FAT), `RenderServer.bin` at 22429 and `vmcs.bin` at 22645+, and lands the latter in SDRAM at
`0x13eaf188`. The prototype bootloader's 157 self-resets are `BX` to address zero through a null
`this` (Addendum 5) — that, not a filesystem decision, is what the question was really about.

### ~~3 — Execute `vmcs.bin` and retire bypass #6~~ · **the gate was never real**

Carried for a week as *the* blocker on the theory that `APPLEBOOT` was waiting for the co-processor
to answer. It was not. `0x000cd7c8` is create-semaphore / spawn-`MP3ExampleTask` / wait-for-thread
(R9), and the actual wall was `KS_pend` on semaphore `0xd1`: heap object `0x13ee2130`, the disk
driver's single in-flight **ATA taskfile request**. Of 272 arrivals in a 600 M run, 22 blocked — and
the same run built exactly 22 `WRITE DMA`s. Every blocking pend was a disk *write*; no read ever
blocked. Twenty-one ended in a 19.6 M-instruction timeout, each followed by five `SET FEATURES` and
another attempt. **Not a deadlock — a retry loop with a 3.9 s period**, sampled at the point it
spends 99.9 % of each cycle (R8).

**Two defects, each on its own sufficient, both one line, neither an ablation — both landed today:**

- **recipe:** `ipod-boot retail` passed no `--disk-writable`, so every write was refused. It now
  `cp -c`-clones the image per run (APFS copy-on-write, ~3 ms for 8 GB) and runs writable on the
  clone, never on the pristine file. `WORKDISK=` keeps the disk when you want accumulated state.
- **model:** `Ata::command`'s write-abort branch set `ERR`/`ABRT` but never `irq_pending`, so the
  driver was not told the write failed — it was told nothing. A real drive interrupts when it
  aborts. Fixed on the write-abort, read-error and unknown-command branches.

Also killed by it: the **font-registry lead** (the first blocking pend is 19.6 M instructions
*before* the first Podium Sans lookup, and the two fixes touch nothing that knows what a font is),
and the claim that **LBA 22169 is never read** — it is, at command #342, and "all 256 commands" was
the log's cap. Full four-arm A/B in Addendum 15.

### ~~4 — Retire bypass #12~~ · **done on the retail ROM 2026-08-13**

See item 4 above for what survives — the prototype half is still open, which is why it kept a live
item rather than only a tombstone.

---

## Not blockers, deliberately parked

- **The games.** B3/B4's HLE path runs shipping titles and renders. A booted RetailOS retires that
  whole category by binding all 433 functions itself, so the games get *easier* by waiting.
- **`research/07` privacy pass** — the file carries third-party FireWire GUIDs, Apple serial
  numbers, initials and a Discord handle. Flagged and deliberately **not redacted**: deciding what
  in someone else's identifiers is safe to erase, and what erasing would destroy as evidence, is
  **the operator's call, not an agent's**. Nothing has been redacted unilaterally and nothing
  should be. There are zero redaction markers in the file today.
- **117 commits unpushed on `main`** — `git rev-list --count origin/main..main`, measured today
  against both remotes. Also the operator's call. (This file said 138; it was never re-counted.)

---

## Instruments, and what each is for — and how each one lies

Built from a specific failure, in the order the failures happened. Reach for the one that matches
the question rather than adding a flag to the one in hand.

**As of 2026-08-14 every cap in this table announces itself in its own report.** A capped log prints
its uncapped census as the headline and appends `(log below shows the first N — SAMPLE, NOT A
CENSUS)` when the rows are truncated; the histograms under those logs are counted at capture time
and cannot saturate at all. The caps are still listed here, because knowing *which* rows you are
looking at is still part of reading the output — but a saturated number can no longer be mistaken
for a measurement by someone reading in a hurry, which is what R6 was written about.

| flag | question | how it lies |
|---|---|---|
| `--storelog=PC` | every store *this instruction* makes — enumerates every object a constructor built | `store_pc_log` caps at **2 000 000**, and the header now prints the uncapped census with `SAMPLE, NOT A CENSUS` when it bites. Strides are computed over the kept rows only, deliberately — a gap straddling the cap boundary is an artefact of the log, not of the heap |
| `--storeaddr=ADDR\|FILE` | every store that *lands here*, whatever made it — hundreds of disjoint words at once | — |
| `--readlog=ADDR\|FILE` | who *consumed* this — the only way to trace a value that arrived by DMA | caps at **2 000 000**, and the ordered log still does. **The header is now the uncapped census and the per-reader table is counted on the read**, so the failure this row was written about — a control read 9 588 012 times returning a clean zero for four fifths of a run — can no longer be read as a measurement. Watching a hot address alongside a quiet one still fills the *sample* with the hot one, so R6's judgement half still applies |
| `--enterlog=PC` | `r0`–`r3` and `lr` on **arrival**, so tail calls and virtual dispatch are not missed | the log caps at **65 536** entries and the detail print at **400 rows**. The `callers:` histogram at the bottom is **uncapped, and as of 2026-08-14 that is actually true** — it used to be tallied *from* the capped log, so this row's advice was only sound below 65 536 arrivals. It is now counted on arrival. **Read the histogram, not the rows**, and note that the arrivals header carries the census while the rows say when they are a sample |
| `--watch-range=B:N` | writes to a span, distinguishing "wrote 0" from "never wrote" | `watch_range_log` caps at **4 096**, and the report prints only the **first** PC per word — so on a busy span it is an attribution instrument that cannot attribute. **2026-08-14: this cap, already documented in this row, produced "RetailOS never touches the VideoCore" (research/10 Addendum 25).** The bootloader's own firmware upload fills all 4 096 slots before RetailOS runs an instruction; `writes into the watched range: 4096` is a saturation flag, not a count. Retracted in Addendum 26 by arrival counters on the three functions that carry the `0x3000xxxx` literals. **Was blind to word-sized writes into a mapped region until 2026-08-13** — it only ever saw byte writes, because `read32`/`write32` hoist the `count()` call behind a list of consumers `watch_range` was not on. That produced "the engine at `0x60009000` is never programmed". Fixed; everything that concluded *absence* from it was re-run (Addendum 8b). **Both remaining defects fixed 2026-08-14**: the per-word table is counted on the store rather than tallied from the log, and it names **every** writing PC instead of the first. On the same command that produced the retracted claim it now reports **423 450 byte-writes across 5 words** (was `4096` across 4), with RetailOS's `0x00287ca8` / `0x00287c28` / `0x002879a4` beside the bootloader's `0x4000exxx` — so the instrument refutes its own claim |
| `--input-regs=B:N` | which addresses the firmware reads that nothing ever wrote — hardware *inputs* | same 2026-08-13 bug and worse: `input_probe` was missing from the `read32` hoist too, so it undercounted reads as well as missing writes. It produced [research/09](research/09-what-the-hardware-must-supply.md)'s register table, which is superseded; the conclusion under that table survived re-measurement |
| `i2c: N transfers` in the run report | which chips the firmware drives and which of their registers | **was a capped log length**: `i2c_log` stops at 4 096 and the 4 G baseline printed exactly that, so every histogram under it — by device, by register, by CTRL — was a picture of the first 4 096 transfers. `NEXT.md` §5 was about to fit a WM8758 model to a number out of it. **Fixed 2026-08-14**: the census is **4 933**, the tallies are kept on the bus, and the ordered log is labelled as the sample it is. At 600 M the log never fills (3 749), which is why the defect survived so long |
| `pcf50605 ADC conversions by channel` in the run report | which ADC channels were converted, how often, and in what order | **two instruments printed as one, and only one of them is a census.** The by-channel table is `adc_by_channel` and is uncapped — trust it. The line under it, `order (first 12 of N kept)`, is the head of `adc_log`, which caps at **4 096**, so it shows the first twelve conversions of the **whole run** and never a later window. On a cold boot Apple's bootloader converts 9 237 times before Rockbox executes an instruction, so *nothing Rockbox does can appear in that ordering at all* — and research/06 read a `(2,704)` out of it as "Rockbox's own, right channel, right value" when it was the bootloader's. The correction inverted the finding: Rockbox issued **no** conversion on that boot. **The ordering answers "how did this run open", never "what did the second stack do"** |
| `--writelog=…` | stores by region, with a DROPPED tag | `write_log_entries` caps at **8 192**; the per-region totals — including **DROPPED**, which is the whole question — are counted on the store and cannot. The "last 4" rows are the last 4 *kept*, which on a truncated log is not the last 4 that happened, and the report says so |
| `--stop-when-idle=N` | ends a run once N instructions pass with no NEW code | a **novelty** test, not a halt test. ~~Use 40 000 000~~ — **40 M truncates the boot**: at 40 M this recipe stops at @308 909 460 with 29 279 buckets and 464 ATA commands, against @1 610 256 821 / 38 262 / 770 at 400 M. RetailOS's startup contains a bounded 226 M-instruction scan loop over already-seen code (`0x000ff2ec`) and 40 M stops inside it while the machine runs at full rate — that is how "`bl 0x001ebe9c` never returns" got published. **Use `400000000`.** Read the second line of the stop report: `0 CPU sleeps` in the trailing window means busy, and the tool now says `<- BUSY, not blocked` outright |
| `--callers=ADDR` | every branch *in memory* that targets ADDR — static, so it finds paths a run did not take | reports `region.base + offset`, so an address is only right if the region is mapped where its base says — **IRAM code scatter-loaded out of NOR is not**. Counts both `bl` and plain `b` (a tail call is a caller: `0x4000b534` is reached only by `b`, and a BL-only scan calls it uncalled). Prints 24, then `… and N more` |
| `--callgraph` | runtime edges, including plain `B` | — |
| `--disasm=`, `dis` | reads the machine, not a model of it. Ghidra conflates by name and invents bodies | **`dis --fn=` does not stop at a tail `B`, and this firmware is full of them.** `--fn=0x002102a4` prints **229 instructions**; the real body is the first **six**, ending `b 0x000ff2ec`. A `--fn=` listing whose last instruction is `b` rather than a return is a thunk — follow the branch and re-run. Bisecting the wrong listing once measured zero arrivals at all 22 of its call sites and read like a block |
| `--symbols` / the profile's labels | names recovered from the image | **the six boot tasks are mislabelled, one record late.** `extract_symbols`'s pattern A assumes *name then pointer*; that pool is *pointer then name*. In `OSOS_correct.bin` at file offset 867616: `t_power\0`, `0x002844e0`, `APPLEBOOT\0`, `0x00284ea0`, `t_graphicsManager\0` — so the profile calls `0x002844e0` "t_power" when it is **APPLEBOOT**, and `0x00284ea0` "APPLEBOOT" when it is **t_graphicsManager**. Pattern A cannot simply be reversed: in the device registry at `0x0025d63c` the word before each name is the *previous* entry's pointer, so a blind reversal renames `OptoTask` to `SerialOptoTask`. Read the creation code at `0x000d3b60` instead |
| `ata commands: N` in the run report | how many ATA commands the boot issued | **was a capped log length** — `commands` stops recording at 256, and "256 ATA commands" served as this project's baseline fingerprint while being the cap; the true figure is 770. The count is now uncapped and the line prints `(log below shows the first 256 — SAMPLE, NOT A CENSUS)`. **That wording is now the shared one**: every capped instrument in the report speaks it. Any pre-2026-08-14 document saying 256 is saying "at least 256". One absence claim died with it: LBA 22169 *is* read, at command #342 |
| `--wheel=SCRIPT` | inject click-wheel input — `@N:touch,+2M:rotate=+12,+2M:release,@100M:press=menu` | anchored in **instructions**, because simulated µs is dominated by idle sleeps and is not comparable across runs. The parser expands `rotate`/`press` and the run prints the expanded schedule, so a log reproduces itself. `--clickwheel` models the device with nothing injected; `--wheel-no-irq` ablates IRQ 40 — the control that separates "the firmware read a frame" from "the firmware was interrupted into reading one". **Snapshots do not carry the wheel**: `--restore` plus `--wheel` fires every step at once. **And `press=` is too short for firmware that polls**: it expands to a down/up pair `--wheel-click-instr` apart, 20 000 by default, which at the real clock is **0.27 ms**. Apple's `diag` reads its button byte once per **150 ms** — 11.25 M instructions — so every `press=` fell between two polls. The tell is that it does not look like a missed press: the interrupt handler records the button at `0x1001aa9c` and the next poll reads a *later* value, so `--storeaddr` shows the press arriving and the firmware shows no reaction. Use explicit `down=`/`up=` pairs held across the poll interval |
| ~~`--force-vc-upload`, `--force-vc-retire`~~ | **DELETED 2026-08-19.** They faked the VideoCore transfer's two completion signals; `0x60009000` now moves all 201 216 bytes and the in-use ring drains without help, so both reproduced a run the machine can do unaided | `--force-sem=ID` survives as the general form — make any RTXC pend return. Still an ablation, still in no recipe |
| `--nor` | a real AMD/JEDEC NOR — unlock, autoselect, CFI query, sector/chip erase, program | — |
| `--no-cfg-ack` | ablates the IDE0_CFG acknowledgement, reproducing the historic interrupt storm on demand | — |
| `--bcm-ppm=FILE` | render the co-processor's framebuffer to a PPM | proves *readout geometry*, never *authorship*. Addendum 12 was retracted for reading it as the latter. Pair it with a dump at `--stop-at=0x10000000:1` — if the two files are identical, RetailOS drew nothing |
| `--bcm-peek=ADDR[:N]` | print N 32-bit words of the co-processor's internal memory at the end of the run | the answer to "what does the host actually see there" is **not** this — this is what the *memory* holds. The two differed for two sessions: `0x1f8` held `1` while the CPU was handed `0x2f01fc78`. Pair it with `--stop-at` on the instruction after the read and compare against the register file |
| `--bcm-registry` | publish the service directory RetailOS reads at internal `0x1f0`, and answer the ring RPC behind it | **ledger #6, off by default, in no recipe.** Derived from RetailOS's reader (research/10 Addendum 29 §2–§3), but four things in it are chosen — chiefly that surfaces are allocated from `0xE0000`. With it on the machine draws 41 frames and every other number moves: ATA 770 → 706, DMA 4 → 104 transfers |
| `co-processor timeline:` in the run report | **what the co-processor was ASKED to do, in order** — every contiguous run of host data writes with its base and length, every command, and every image operation a command became. No flag; capped at 4 096 ops with the usual census header | it is the *traffic*, not the state, and the two answer different questions. The state report (`internal write runs`) said `0x000e0000 … 76 816 halfwords` for weeks, which is true and says nothing about how they arrived; the timeline says `4 852 halfwords in ONE run` and that is what identified the Apple logo as a staged tile nobody had placed. **A picture written at one pitch where the panel wants another shows up here as one long run**; 78 runs of 62 at a 320-halfword stride would have been a host placing its own rows |
| `--bcm-film-from=N` | start the film's sampling at instruction N | **the machine is unchanged** — the run is still issued in `--every`-sized chunks from instruction 0, so the film's no-perturbation control still holds and only the surface scan is skipped. It exists because reading Brick's ball needs a 100 k cadence and there are 2.4 G instructions of boot in front of the game |
| `--novelty`, `--profile` | where new code stops appearing, and where the time goes | the profile's symbol labels are the mislabelled ones — see `--symbols` above |
| snapshot / restore | 17 s to re-reach a 400 M state instead of 70 s | — |
| `tcb SDRAM.bin` | **the whole scheduler out of a `--save-region=sdram` file, in 40 ms and no run.** Every TCB: name, priority, state, entry, tick, which RTXC primitive it is blocked in and on which semaphore/mailbox/resource — the frame walk of research/10 Addendum 7 §1 as a *census* rather than the one-task sample it had been three times. `--walk` adds the BL-preceded stack walk (and the `mov lr,pc; bx` form, which is how the thread trampoline at `0x000e1b10` calls a body handed to it at runtime, so pooled tasks get named). `--free` shows terminated slots — that is how "`APPLEBOOT` finished" became a field read. `--irq=OBJ` prints the interrupt controller's handler tables; `--findobj=OFF:LO:HI`, repeated, locates an object by several fields at once. Its control is that it reproduces Addendum 7 §2's five mailbox/semaphore numbers without being told them | — |
| `dis --iscan=W[:MASK][:FOLLOW]` | every word-aligned instruction matching `W` under `MASK`, disassembled — register-wildcard search (`--iscan=0xe58000a0:0xfff00fff` = `str rN,[rM,#0xa0]`). **It exists because `grep -abo $'\xa0\x00\x84\xe5'` cannot work**: command substitution strips the NUL, the pattern shrinks to three bytes, and it silently reports zero for an instruction that occurs 114 times. Caught only by re-running it for an offset whose answer was already on screen | — |


*(This section carried a **second, near-duplicate** copy of the table until 2026-08-14 — same
flags, no `how it lies` column, and two rows that existed only in the copy (`tcb`, `dis --iscan`).
Both have been folded in above and the duplicate deleted. A file that documents an instrument
twice will eventually update one of them.)*
Three recipes, not one: `cold-boot.sh` (ROM out of NOR), `ipod-boot warm` (RetailOS entered directly —
this existed only as a pasted command line until 2026-08-13, which is why #5 sat unvalidated for
weeks), and `ipod-boot flash-update` (two boots, `aupd` then `osos`, no file edited between them).
`--storelog-dump=FILE` writes TSV so one run's addresses feed the next run's `--storeaddr`. That
chaining is how research/10 got from 791 objects to a named font file in six runs.

### The recipes

`ipod-boot retail` — **the configuration every current number in `research/` is measured on.** Apple's
shipping 5G bootloader, the disk image it accepts, and since today a writable per-run APFS clone.
It `exec`s `cold-boot.sh`, so it inherits every flag there.

`cold-boot.sh` — a *prototype's* NOR (archive.org dump, blank HwId, unpublished HwVr `0x000b0011`).
That was never a decision, it was the first dump we had. It self-resets 157 times and never mounts
`rsrc`. Numbers measured on it do not transfer, and a retail run does not retract them — it
supersedes them.

`ipod-boot warm` — RetailOS entered directly. `ipod-boot flash-update` — two boots, `aupd` then `osos`, no
file edited between them. `ipod-boot flsh`, `ipod-boot rockbox` — the remaining two.

### Two method notes that cost real runs

**A shared working tree is not a stable measurement substrate.** A control run that had been
byte-identical to its baseline came back twenty minutes later with 35 577 code buckets instead of
27 985, from source `git diff` said contained only my own additions — a concurrent agent's in-flight
edit had been compiled in and then reverted. Measure from a **private snapshot of `tools/`** built
into a private `CARGO_TARGET_DIR`. `cargo build` finishing in four seconds is not evidence that the
tree it read was the tree you wrote.

**When firmware arms a wait and only *then* acknowledges, a device that finishes inside the store is
racing code written assuming it could not.** This emulator has made that mistake twice — the drive's
`IDE_COMPLETION_USEC` and the click wheel's `OPTO_REPLY_USEC`. Model the round trip.
