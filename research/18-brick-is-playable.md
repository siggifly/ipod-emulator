# Brick, played at the faithful clock

**Measured 2026-09-07.** Every number here is a run, and the command that produced it is beside it.

[research/13](13-do-the-games-load.md) established on 2026-08-14 that Apple's own Brick launches,
draws, and is **played** — a serve, six returns, eight bricks, a score of 8. That work stands. What
did not survive is the ability to *re-run* it. Its recipes are written against shell scripts that no
longer exist, at a clock that is no longer the default, with anchors counted in instructions on a
machine whose exchange rate between instructions and time has since changed by about five. Its §9
says so itself, in a warning it dates 2026-08-19 and ends with:

> Recalibrating this descent onto those is the fix, and it has not been done.

This file is that recalibration. It is a separate document rather than another addendum because the
thing it produces is a **recipe**, and a recipe that is nine hundred lines below the claim it
supports is a recipe nobody will find.

---

## 1. What existed, and why none of it could be run

Three artifacts claimed to reach Brick. On 2026-09-07 none of them did.

| artifact | state |
|---|---|
| [research/13 §9](13-do-the-games-load.md), the three published commands | **stale, and says so.** `tools/ipod-boot/*.sh` are gone (the recipes are `ipod-boot <name>` subcommands) and the anchors are instruction counts: `@1500M` was 300 s of simulated time at `--clock=5` and is 20 s at 75 |
| `to_brick()` in `tools/ipod-machine/src/bin/ipod-film.rs` | **anchored in simulated time — and calibrated against a machine that has since moved.** Its comment reads *"`@80s`: the language picker draws at **73.2 s simulated** — measured with `--bcm-film`'s `first_usec` column, on the retail ROM and a drive built from `iPod_20.1.3`"*. That is this file's ROM and this file's IPSW, and §2 below measures the picker at **4.8 s**. The figure was taken on 2026-09-01, before `7e30c1f` stopped a halt from inventing simulated time; every wait in the descent is scaled to a clock the machine no longer keeps |
| `ipod-film asset gameplay` | **refuses to start.** `NEXT.md` §0b suspected this from reading the source and said plainly *"I have not run it"*. Run: `--wheel: step "@2502340000:touch" mixes units: this script is in seconds, and the two clocks diverge whenever the machine idles.` / `ipod-film: the run failed; no film written`. `to_brick()` was re-based on the clock in 2026-09-01 and `do_gameplay()`'s own steps were not, and the two are concatenated into one `--wheel=` |

So the position at the start of this work was: a result that had been seen, recorded in prose, and
could not be reproduced by anybody — including by the program's own shipped asset command.

---

## 2. The descent, measured — 2026-09-07

One command. The ROM is the 5G retail dump; the drive is built fresh from an IPSW by
`ipod-boot make-disk`, which is the drive a new user gets and not the operator's hand-mended one.

```sh
ipod-boot make-disk resources/software/ipsw/iPod_20.1.3.ipsw 5g.img

FLASH=resources/roms/<the 5G dump> DISK=<a writable clone of 5g.img> BUDGET=24000000000 \
ipod-boot retail --clock=75 --until=300s --clickwheel --wheel-click-instr=300000 \
  --bcm-film=0xE0000:140:F0:10000000:descentA \
  --wheel='@8s:touch,+500ms:press=select,+2s:release,+72s:touch,+500ms:rotate=+8,+2s:release,+20s:touch,+500ms:rotate=+8,+2s:release,+20s:touch,+500ms:rotate=+8,+2s:release,+20s:touch,+500ms:press=select,+2s:release,+20s:touch,+500ms:rotate=+8,+2s:release,+20s:touch,+500ms:press=select,+2s:release,+20s:touch,+500ms:press=select,+2s:release,+20s:touch,+500ms:press=select,+2s:release'
```

```
script: 60 of 60 steps fired
clickwheel: 63 frames posted (0 dropped unread), 63 word reads of DATA (63 with a frame waiting)
ata commands: 615
irqs 202 336 054 asserted, 10 494 767 taken; usec 300000000
```

Read the `script:` line before believing anything about input: a machine that spends its budget
halted fires 0 of N and reads exactly like a firmware that has stopped listening (AGENTS.md §6).
`0 dropped unread` is the other half — every frame the wheel posted, the firmware collected. A
script can fire in full into a firmware that is being flooded, which is what §5 below is about.

> **Two ways to get this wrong at the shell, neither of which announces itself.** The `--wheel=`
> string is long and the temptation is to put the whole flag list in a variable — but **zsh does not
> word-split an unquoted `$VAR`**, so five flags become one argument, the machine silently ignores
> all of them, and the run still prints `script: N of N steps fired` because the script it parsed
> was fine. `ipod-boot --print` composes the argv and shows it without running anything, and that is
> the check. And `ipod-boot <recipe> <budget>` is **inert**: the recipe emits its own budget first
> and `trace` takes the first positional, so a budget goes in `BUDGET=` and nowhere else.

Nine gestures, and every one landed where it was aimed. The film's own manifest, `first_usec` and
`nonblack` columns:

| @ simulated | non-black | screen |
|---|---|---|
| 1.73 s | 76 800 | blank white |
| **4.80 s** | **75 267** | the Language picker |
| 60.67 s | 75 396 | the Select at 8 s taken |
| **66.80 s** | **75 791** | **the main menu**, Music selected |
| 83.73 s | 75 817 | Photos |
| 106.13 s | 75 809 | Videos |
| 128.67 s | 75 789 | Extras |
| 154.00 s | 75 767 | the Select taken |
| **159.33 s** | **75 565** | **Extras opened**, Clock selected |
| 173.73 s | 75 594 | Games |
| 198.13 s | 75 573 | the Select taken |
| 201.47 s | 76 027 | **the Games list** |
| 221.33 s | 76 131 | the Select taken |
| **233.87 s** | **76 763** | **the playfield** |
| 242.00 → 252.53 s | 76 763 ×39 | **the ball, served** |

**Four of those pixel counts are research/13's own** — 75 267 for the picker, 75 791 for the main
menu, 75 565 for Extras, 76 763 for Brick's playfield. They are also the four `do_boot()` in
`ipod-film.rs` asserts its published stills against. Reproduced here on a different clock, from a
different recipe, on a drive built by a different command, three weeks and a great many machine
changes later. Nobody designed that as a check and it is the strongest single piece of evidence in
this file that the two runs are looking at the same firmware doing the same thing.

### 2.1 The latencies, which are the whole calibration

Nothing above needs to be guessed at again. Subtracting each gesture's own instant from the frame
it produced:

| step | costs |
|---|---|
| the boot, to the Language picker | **4.8 s** |
| **the first Select, to the main menu** | **58.3 s** |
| a one-row gesture | 0.6 – 1.0 s |
| a Select that opens a submenu | 6 – 9 s |
| the Select that launches Brick | **15.7 s** |
| the centre button, to the ball moving | 1.4 s |

**The 58.3 s is the whole cost of this row**, and it is first-boot work rather than a hang —
research/12 §RESOLVED names the same wait at clock 5, where it is 1 213 s. It is unavoidable here
because a harness that builds a fresh drive for every run gets a first boot every time, which is
the point of building fresh drives.

Everything after it is cheap. A descent with margins on the measured numbers rather than on fear
puts the playfield at **152.9 s** instead of 233.9 s — §4's script — and that is the version worth
running.

### 2.2 Brick is row 0 on a drive built from an IPSW

research/13 §3 puts Brick at row 5 of a 56-entry Games list, between `Brain Challenge` and
`Bubble Bash`. That was the operator's own drive, whose `iPod_Control/Games_RO` holds 56 purchased
titles interleaved alphabetically with Apple's four built-ins.

A drive built from an IPSW carries none of them. The list is `Brick · Music Quiz · Parachute ·
Solitaire`, Brick sorts first, and the first row of a RetailOS list is selected on entry — so the
descent opens the list and presses Select, with no scroll inside it at all. **A descent that copied
research/13's five one-row gestures would walk off the end of a four-entry list.** Which drive you
are on changes the script, and neither script is wrong.

---

## 3. The clock: this is the one measurement that cannot use the accelerant

`tools/clean-run-matrix.sh` runs every row at `--clock=5` and says why: at the faithful 75 its
budgets do not finish booting. For a *does it draw* row that is free — research/04 records a 6 G A/B
at 5 and at 75 landing on the same 933 ATA commands and the same 75 267 non-black pixels.

For a **playability** row it is not free, and the evidence is the operator's, in research/04's own
ledger row for the clock change:

> *"when i play brick the balls just shoot immediately super fast, nearly unplayable"*

Brick's animation is driven by the firmware's timer, and at `--clock=5` the firmware's timer runs
15× fast against the code. Measuring Brick at clock 5 measures Brick in the state we already know is
wrong, however green the row comes out. So this row runs at 75 and its ROUTE column says so.

**And the fear that made the accelerant the rule does not survive being tested.** It is the
instruction budgets that fail at 75, not the clock. The picker draws at **4.8 s** here against
research/12's **200.8 s** at clock 5 — fifteen times less iPod-time for the same work, because at
75 the CPU delivers fifteen times as many instructions per simulated microsecond. 208 s of iPod is
15.6 G instructions, which is about 25 minutes on the machine this was written on and roughly 1.4×
a `retailos` row. That is the price, it is affordable, and nothing about it needed the accelerant.

---

## 4. ✅ The proof of play — a frame that comes back, and the arm where it does not, 2026-09-07

Reaching a title screen is not the claim. research/13 §6 set the standard the claim has to meet — an
**inverse gesture returning the panel to a byte-identical earlier frame** — and this section meets
it, and then adds the half §6 did not have: **the arm where the gesture is removed.**

Two runs. Both built from the same lines of the same script, differing in exactly one thing:

- **play** — the four paddle gestures are `touch, rotate=±8, release`, in the order `+8 −8 +8 −8`
- **null** — the four are `touch, release`, of the same duration, releasing on the **same
  microsecond**. The finger lands on the wheel and comes off without turning it.

Both are run **before the ball is served**, which is not a detail: a screen can only return to a
byte-identical earlier frame if nothing else on it is moving, and pre-serve the paddle is the only
thing that can move. (research/13 §6 makes the same point about its own runs, in the note that says its
eleven rows are "a smaller claim than it reads as".)

```sh
# the play arm; BRICK_NULL=1 in tools/clean-run-matrix.sh builds the null one from the same lines
FLASH=<the 5G dump> DISK=<a writable clone of 5g.img> BUDGET=17000000000 \
ipod-boot retail --clock=75 --until=208s --clickwheel --wheel-click-instr=300000 \
  --bcm-film=0xE0000:140:F0:10000000:brick-play \
  --wheel='@8s:touch,+500ms:press=select,+2s:release,@76504ms:touch,+500ms:rotate=+8,+2s:release,@83032ms:touch,+500ms:rotate=+8,+2s:release,@89560ms:touch,+500ms:rotate=+8,+2s:release,@104088ms:touch,+500ms:press=select,+2s:release,@116592ms:touch,+500ms:rotate=+8,+2s:release,@127120ms:touch,+500ms:press=select,+2s:release,@137624ms:touch,+500ms:press=select,+2s:release,@160128ms:touch,+500ms:rotate=+8,+3s:release,@167656ms:touch,+500ms:rotate=-8,+3s:release,@175184ms:touch,+500ms:rotate=+8,+3s:release,@182712ms:touch,+500ms:rotate=-8,+3s:release,@191240ms:touch,+500ms:press=select,+2s:release'
```

Every anchor is absolute and in the firmware's clock, so the two instants the verdict keys on — the
launch at `@138124000 us` and the serve at `@191740000 us` — are the same numbers the script used.

```
play   script: 100 of 100 steps fired   103 frames posted (0 dropped unread)   ata commands: 615
null   script:  68 of  68 steps fired    71 frames posted (0 dropped unread)   ata commands: 615
```

The null arm has 32 fewer steps because it has 32 fewer clicks, and it posts 32 fewer wheel frames
for the same reason. **The ATA census is identical**, which is what "the same machine, doing the
same work, given different input" should look like.

### 4.1 The play arm — the paddle window

```
 15  @152.93 s   held 590 M    76763   0x14854c1ecf65e5d9   the playfield, as Brick drew it   D0
 16  @160.80 s   held  10 M    76763   0x70ec99ebce417695   +8 — the panel mid-update
 17  @160.93 s   held 560 M    76763   0x3d5be72583aab771   settled, one quantum right        D1
 18  @168.40 s   repeat_of 16                                −8 — mid-update again
 19  @168.53 s   repeat_of 15  = 0x14854c1e…                 BACK TO D0, byte for byte
 20  @175.87 s   repeat_of 16
 21  @176.00 s   repeat_of 17  = 0x3d5be725…                 BACK TO D1, byte for byte
 22  @183.47 s   repeat_of 16
 23  @183.60 s   repeat_of 15  = 0x14854c1e…                 BACK TO D0, byte for byte
```

`repeat_of` is the film's own column: it names the earlier frame index whose digest — over all
76 800 halfwords of the surface — this one matched. It is not a similarity score and there is no
threshold in it.

**Six returns.** The paddle settles **0.13 s** after the gesture, which is one film sample, so the
response time is at most that.

**The mid-update frame is the same in both directions**, three times: `0x70ec99eb…` appears during
`+8` and during `−8` alike. The natural reading is that it is the panel with the paddle erased
before it is redrawn, which would not depend on which way it is about to go. Nothing here isolates
that mechanism; what is measured is that the two directions pass through one identical picture.

### 4.2 The null arm — and this is the half that makes the above mean something

```
 15  @152.93 s   held 3 010 M   76763   0x14854c1ecf65e5d9   the playfield
      … 301 consecutive samples, through all four touch/release gestures, without one new picture
 16  @193.07 s   …                                            the serve
```

**Zero returns. Zero new pictures in the paddle window. Not one `repeat_of` row anywhere in the
run**, which has 54 frames and 54 distinct pictures.

And — the part worth reading twice — **the null arm reaches the same playfield and serves the same
ball**: 38 distinct pictures after the centre button, against the play arm's 37. It fails on the
paddle and on nothing else.

### 4.3 What the pair rules out

| the alternative | what kills it |
|---|---|
| the panel changed because the game animates on its own | the null arm holds one frame for 3.01 G instructions across the same window |
| the *touch* moved the paddle, not the rotation | the null arm touches and releases on the same microseconds |
| the returning frame was a near-match called equal | 76 800 halfwords, digested; `repeat_of` names an index, not a score |
| it was luck | three returns, alternating D0/D1/D0, plus three of the mid-update frame |
| the emulator is non-deterministic and this is a sample | the two arms are digest-identical frame for frame **at identical instruction counts** for all sixteen frames before the paddle window — the A/A research/13 §10.4b had to run, obtained here for free because both arms are one script with one branch. §4.3a is the caveat that has to go with that |

What it does **not** claim: nothing here measures a rally, the ball's physics, or the paddle's
24-pixel quantum in this build. research/13 §10 measured all three at clock 5 on the pre-`7e30c1f`
machine and `NEXT.md` §0b marks them SUSPECT; they stay suspect. This file measures that the wheel
moves the paddle and that the centre button starts the ball, at the faithful clock, on the build
that ships.

### 4.3a Why those two arms are identical and two runs of the same row were not

The play and null arms above agree to the instruction for every frame before the paddle window.
Two runs of the **same** command, an hour apart, did not: they diverged by **24 298 instructions**,
which slid one mid-update frame off a 10 M film sample and reported **5 returns where the other
reported 6**.

That is not the emulator being non-deterministic, and it is not a mystery — the machine documents
it. `--rtc`'s comment in `trace.rs` says the RTC and the battery *"are the only two things in this
machine that come from outside it"*, and records the same recipe an hour apart giving 44 511 132
instructions against 44 509 887. Left free, both default to the host: the PMU clock is
`host_local_time()` and the charge is `host_battery_percent()`. So the two arms agreed because they
were started **eight seconds apart** — `11:14:26` and `11:14:34` — and the two rows disagreed
because one ran at `11:14` and the other at `12:36`.

| run | PMU clock | first frame after the Games list |
|---|---|---|
| play arm | `2026-09-07 11:14:26` | `first_instr 11236110626` |
| null arm | `2026-09-07 11:14:34` | `first_instr 11236110626` |
| the row, unpinned | `2026-09-07 12:36:47` | `first_instr 11236134924` |

**A control that happens to be reproducible because it was run twice in the same minute is not a
control.** The gate now pins both — `--rtc=2026-01-01T12:00:00 --battery=100` — and §4.4 carries an
A/A of the pinned row against itself. The date is arbitrary and fixed; nothing depends on which day
it is, only on it being the same day every time.

**The rows above `brick` in the matrix do not pin it**, and that is a smaller risk rather than none:
their evidence is picture counts and ATA censuses, which `trace.rs` records as *not* having moved
across the same comparison. A row whose evidence is a count of byte-identical returns has no such
margin.

### 4.4 The same thing, as a gate

All of the above is a `brick` row in [`tools/clean-run-matrix.sh`](../tools/clean-run-matrix.sh),
which is the 0.5 acceptance test. It is the **only row in that harness that runs at `--clock=75`**
and the only one that measures play; its ROUTE column carries the clock so a reader cannot mistake
it for one of its neighbours.

The row builds its script from the same absolute anchors printed above and computes three numbers
from the film's own manifest — the playfield's pixel count, the returns, and the pictures after the
serve. `BRICK_NULL=1` builds the null arm from the same lines. It is not a debug switch; it is the
control, and a gate nobody has made fail is a gate nobody has tested.

Five runs of that row, unmodified, on 2026-09-07:

```
TARGET GEN   NOR        ROUTE                VERDICT  EVIDENCE
brick  5G    real       Apple ROM, clock 75  PASS     script: 100 of 100, playfield 76763 non-black, 6 returns, 38 pictures after the serve
brick  5G    realB      Apple ROM, clock 75  PASS     script: 100 of 100, playfield 76763 non-black, 6 returns, 38 pictures after the serve
brick  5G    synthetic  from the drive, 75   PASS     script: 100 of 100, playfield 76763 non-black, 4 returns, 37 pictures after the serve
brick  5.5G  synthetic  from the drive, 75   PASS     script: 100 of 100, playfield 76763 non-black, 6 returns, 40 pictures after the serve
brick  5G    null       Apple ROM, clock 75  FAIL     arrived but did not play — the panel never came
                                                      back to an earlier frame under the inverse
                                                      gesture: script: 68 of 68, playfield 76763
                                                      non-black, 0 returns, 38 pictures after the serve
```

**`real` and `realB` are the same run twice**, and their manifests are identical — `diff` is silent
across all 63 rows, every instruction count and every digest. That is the A/A §4.3a's pin was for,
and it is what makes the other three rows comparable to anything.

**The null arm fails, and fails only where it should.** Same 100-of-100 descent shape, same 76 763
playfield, same 38 pictures once the ball is served. It differs in one number, and that number is
the claim.

**Three different machines reach the same three pictures.** The real dump runs Apple's bootloader
out of the ROM. The synthetic ones have no bootloader at all and are entered through the drive's own
firmware partition. The 5.5G is a different generation on a different updater family — `iPod_25.1.3`
rather than `iPod_20.1.3`, a separate download. Different route, different code in front of
RetailOS, different firmware bundle — and the paddle window is

```
0x14854c1ecf65e5d9  ->  0x70ec99ebce417695  ->  0x3d5be72583aab771
```

on **all three**, the same digests over 76 800 halfwords, and so is every menu digest above them.
That is consistent with research/13's finding that the 1.3 `osos` is byte-identical between the 5G
and 5.5G bundles, and it is the first time this project has checked it by running both to a game.

The 5G synthetic scores 4 returns rather than 6 only because the film catches the mid-update picture
twice instead of three times; both settled returns, which are the load-bearing ones, are there. **The
threshold is 2 for that reason** — how often a 10 M sample happens to land inside a redraw is a fact
about the sampling rate, not about the emulator.

---

## 5. The click gap was calibrated against a clock that had moved — fixed

Found on the way here, and it is the same shape as everything else in this file.

`--wheel-click-instr` sets the spacing between the frames of a `rotate`. Its own comment promised
*"default 20000, which at `--clock=5` is 4 ms per click — a brisk but human scroll"*, and
`parse_wheel_script` records what happens when that promise breaks: at RetailOS's language menu, a
scroll no thumb could produce, **31 frames posted and 18 dropped unread**.

The clock's default moved from 5 to 75 on 2026-08-17. The constant did not. Measured on `dev`,
by asking the machine to expand its own schedule with the flag omitted:

| | clicks land |
|---|---|
| `--clock=5` | 4 000 µs apart — as documented |
| `--clock=75`, **the default** | **266 µs apart** |

research/04's ledger row for the clock change says the number moved with it, `20 000 -> 300 000`.
`ipod-gui` moved — `emu.rs` sends `click_gap: 300_000` — and `trace` did not, so the two front ends
disagreed in silence for three weeks. The default is now `4000 * clock`: **byte-identical at
`--clock=5`**, so no recipe written at the accelerant changes, and 300 000 at 75, which is what the
window already sent. `wheel_click_gap` is a function with two tests, and the test was checked
against the old constant before being trusted — it fails with
`20000 instructions at 75/us is not 4 ms`.

Every run in this file states the flag anyway. A recipe that names its own calibration cannot be
re-calibrated by a change to a default somewhere else, which is exactly what happened here.

---

## 6. Predictions that measured out to nothing

- **"At the faithful clock the budgets will not reach the menu."** The premise of every other row
  in the matrix, and it is false as stated. What does not survive at 75 is a budget *written in
  instructions* for clock 5. The boot itself is **fifteen times cheaper in iPod-time** at 75, so a
  descent stated in the firmware's clock is affordable at the faithful rate and always was.
- **"The first press costs 1 200 s, so this is unaffordable."** research/12 measured 1 213 s at
  clock 5 and that number is right. It is 58.3 s at 75. The wait is work, not waiting, so it scales
  with the clock like everything else.
- **"Brick is row 5 of the Games list."** True on the operator's drive, false on every drive this
  harness builds. The row index is a property of what is installed, and a script that hard-codes it
  is a script calibrated against one person's iPod.
- **"A one-row gesture will need re-deriving at the new clock."** It did not. `touch, rotate=+8,
  release` is still exactly one row, on every list in the descent, at 75 as at 5 — which makes
  sense of research/13 §2.2's finding that the variable is *whether the finger came off*, not the
  rate.
- **"The paddle proof will need the ball parked, so it will be fragile."** It is the opposite: the
  parked ball is what makes the proof possible at all, and it is stable enough that the null arm
  holds a single frame for three billion instructions.

---

## 7. What this leaves open

- **`ipod-film asset gameplay` is still unrunnable.** §1 measured the refusal; the fix is to re-base
  `do_gameplay()`'s instruction anchors on the clock, the same fix `to_brick()` already had. Doing
  it properly means re-reading a rally off its own run at HEAD — research/13 §10.4a's own rule —
  and that is a separate piece of work from this one.
- **`to_brick()`'s `@80s` and its 73.2 s comment are stale** and now demonstrably so. It is not
  broken in the way research/13 §9 was — it is anchored in the right unit — but its spacing is calibrated
  against a machine that answered on a different schedule. Nothing in this file changes it.
- **research/13 §10's rally numbers stay SUSPECT.** The ball's `(±8, ±10)` step, the 45° reflection,
  the 24-pixel quantum and the rate-sensitive accelerator were all measured at clock 5 before
  `7e30c1f`, in units of executed instructions. This file does not re-measure them.
- **`Parachute`, `Music Quiz` and `Solitaire` are one gesture away.** They are rows 1, 2 and 3 of
  the same four-entry list on this drive. Nobody has launched Parachute or Music Quiz.
- **No frame in this file is committed.** They are Apple's UI. `_out/` is gitignored and stays that
  way; every picture quoted here is reproducible from the command beside it.
