# What the window must make possible

**Written 2026-09-07, before a fourth design.** Not a design. A list of what has to be reachable,
so that the next shape can be judged against something other than taste.

## Why this exists

There have been four window designs — the bench, the noun drawer, §21's verbs, §22's intents — and
**every one started from a shape and then discovered what it could not hold.** The bench had no room
for a boot indicator. The verb drawer grew to twelve rows because state and capability were disabled
by the same rule. §22.3 put a chooser in front of the device and contradicted §21.2 in the same
document.

That is four times the same mistake, and it is not a shortage of care — it is designing a container
before listing its contents. So: the contents.

---

## 1. Who is at the window

**The first-timer** has nothing. No ROM, no firmware, no drive, no idea that a click wheel's centre
is a button. They want to see an iPod work, or play a game they own.

**The returning person** has one working iPod and comes back to use it. Their session should start
where the last one ended and cost them nothing.

**The advanced person** has a real ROM dump off their own device, a drive image, opinions about
bootloaders, and reasons to want the instruments. **Nothing they need may be unreachable** — but
none of it may stand in front of the first-timer either.

These are not three programs. They are one program that reveals itself.

---

## 2. What must be possible

### Running something

- **Apple's software**, on a 5G or a 5.5G, from a real ROM dump or a synthesised one.
- **Rockbox**, installed onto a drive and booted.
- **Apple's diagnostics** — *real dump only*: Apple shipped Diagnostics inside the part, and no
  fetch supplies it.
- **A raw ARM image** entered at `0x10000000` — `ipodloader2`, iPodLinux's kernel, anything of that
  shape.
- **A title** — an `.ipg` — which needs **no ROM, no firmware, no drive and no boot**, because
  `play` never calls `map_hardware`.
- **Stopping**, and having stopping mean something: park with a restore point, or drop it.
- **Resuming**, at the cost of a restore rather than a boot.

### Getting the parts

- **Fetch Apple's firmware.** 66 of the 71 catalogued URLs answer today, each with a recorded size
  and SHA-256; five are marked `served: false`.
- **Fetch Rockbox**, its bootloader, Freedoom, `ipodloader2`, ZeroSlackr's kernel.
- **Synthesise a ROM**, which is what makes a first run possible without anybody owning a dump.
- **Supply your own** — a dump, a drive image, a folder of titles — and have the program prefer it.
- **Build a drive** from an IPSW, and install onto one afterwards.
- **Look inside a drive** without booting it.

### Driving it

- **The wheel**, by every route a person has: keyboard, pointer drag, scroll gesture, and the
  trackpad as an absolute wheel.
- **The five buttons and the hold switch** — and the hold switch is not optional decoration: it is
  the only way into Doom's menu, because `IPOD_4G_PAD` has no ESC.
- **Feedback that the wheel is being touched**, because in trackpad mode the finger is invisible.
- **The click**, driven by the guest's own piezo rather than by the window, as haptics where the
  hardware allows and as sound everywhere.

### Seeing it

- **The panel at a whole multiple**, never a fractional scale.
- **Fullscreen.**
- **The panel in its own window**, so the screen can be on a television while the wheel stays on the
  laptop.
- **A screenshot.**

### Knowing what is happening

- **What it is doing now**, and how far through — a fetch, a build, a boot, with an estimate derived
  from this device's own history rather than a constant.
- **Why something cannot be done**, when and only when nothing the program could do would fix it.
- **What this iPod is made of** — its ROM, its drive, what is installed, what it can boot.
- **Every keyboard binding**, from inside the window.

### The instruments — reachable, never in the way

The Readout · the parts inventory · the work rail · the seven boot recipes · the control socket ·
the ~90 trace instruments, `dis`, `tcb`, `ghidra`, `ipod-film`. `developer = false` already draws
this line and its own comment states the rule: *"those are instruments, and this program's first job
is to be an iPod. **Nothing is UNREACHABLE with it off.**"*

---

## 3. What the window is not for

- **Being an iPod's operating system.** RetailOS and Rockbox are programs this iPod runs, not modes
  of this program.
- **Inventing controls the hardware lacks.** Measured, not assumed: the SELECT|MENU mask appears
  **zero times in 641 479 disassembled instructions** of RetailOS, and on real hardware that chord
  is caught below the firmware.
- **Deciding when to click.** The guest decides; the window renders.
- **Teaching the compatibility matrix to somebody who came to play Brick.** §14.1's *disable with a
  reason* earns its place for capability refusals and was wrong applied to transient state — that
  mistake produced four rows saying *"is not running"* in four phrasings.

---

## 4. Constraints any design must satisfy

These are measured. A design that violates one is wrong, not bold.

| | |
|---|---|
| a game needs no device | `play` never calls `map_hardware` |
| diagnostics needs a real dump | Apple shipped it inside the part |
| the wheel is the only way into a booted OS | every menu, every game, every setting |
| the panel is 320 × 240 at integer `K` | fractional scaling is blurry or unevenly scaled |
| the clock calibrates at the boot's end | a one-second sample reads 16.0–2.3 M steps/s; any short window is a coin toss |
| one core has no working wheel | 52 of 62 frames dropped, 0 of 60 detents |
| a refusal must name a fact nothing can change | otherwise it is a design failure wearing a sentence |
| the model stays out of the window | `settings.rs`, `compose.rs`, `identity.rs` know no toolkit — that is what makes the window replaceable |

---

## 5. The measurement that should shape the next design

**Two thirds of `ipod-gui` does not know what a toolkit is** — 43 937 of 67 723 lines, with ten real
toolkit uses outside `main.rs` in four files. The state machine, the refusals, the wheel arithmetic,
the haptics, the navigation and the model are all portable.

So a new window is `main.rs` and the markup: **~34 000 lines, not 78 000** — and the toolkit is the
*last* decision to make, not the first, because the boundary that makes it swappable already exists
and has held under six agents in one day.

---

## 6. The open question this list does not answer

**Whether the window should draw an iPod at all.**

For: the wheel needs a physical surface to be usable — an angle, four domes, a centre — and the
device is the subject. §14.2 records the drawn device as a deliberate departure from accuracy
emulators that declined it.

Against, and it is not an aesthetic argument: **the drawn device has been dictating every layout
constraint this program has fought.** Its proportions set `MIN_WIDTH` 460, which set `DRAWER_W`,
which gave every row a 372 px column, which is why sentences kept being amputated — and three agents
in a row shortened prose rather than question the number.

That is a real running cost, and it should be paid deliberately or not at all.
