# The window

The design of the program's interface, written **before** the interface. When the window and this
document disagree, this document is what gets argued with first.

> **Status: draft for review.** Nothing here is built. §14 lists the questions that need an answer
> before it can be.

---

## 1. Why the old one is being thrown away

The current window began as a proof of concept: one machine, one boot ROM, one drive, and a settings
page. Everything since has been layered onto that — a library, a wizard, a compatibility engine, a
resource manager — without the foundation ever being redrawn. `main.rs` is **8,039 lines** and its
structure is archaeology.

The symptoms were all the same disease:

- Opening settings **rebooted the iPod**, because "no machine" was how the window knew to draw the
  settings screen.
- The wizard **re-opened itself** on an empty list, because an empty list was how it knew to offer
  itself, and a cancelled build empties the list.
- Screens **jumped** as you changed your mind, because space was drawn only when it was filled.
- The identity fields did not agree with the model, because identity was designed before the model
  was a choice.

None of those are bugs in a screen. They are a window that has no model of itself.

**What is being kept.** The toolkit only ever touched one file. `settings.rs`, `compose.rs`,
`identity.rs` and `nor.rs` (the device model, the compatibility rules, the serial/GUID validation,
the ROM recipes) know nothing about any toolkit and survive intact — as do `emu.rs`, `wheel.rs`,
`control.rs` and `png.rs`. **The nouns are right. The surfaces are wrong.** This is a redesign of the
window, not of the program.

---

## 2. Principles

Eight, and every one of them is the scar of something that actually went wrong.

1. **The library is home.** The program opens on what you have, not on a machine. Nothing is running
   while you are there. *(Earned: settings could only be reached by destroying the machine.)*

2. **Nothing moves that you did not move.** Every field, row and control reserves its space whether
   or not it currently has content. A layout that reflows as you choose is a layout you cannot aim
   at. *(Earned: "things should take their space even when empty so things are not jumping around".)*

3. **Disable with a reason. Do not hide.** An option that cannot be used stays visible, greyed, and
   says why when you point at it. Hiding it hides the machine's rules; showing it teaches them. See
   §13 — this is a deliberate reversal of a rule we follow elsewhere. *(Earned: the compatibility
   matrix. A bootloader that silently vanishes when you pick a ROM teaches nothing.)*

4. **Nothing floats.** No modals, no dialogs, no popovers, no toasts, no tooltips carrying
   information you need. Contextual content **pushes** the layout aside; it never covers it. The one
   thing that may sit above content is a focus ring.

5. **One name for one thing.** A device is a device on every surface, in every message, in the
   settings file and in the changelog. *(Earned: "running" meant "selected", and a device that had
   never started said it was running.)*

6. **Say what will happen, then say what happened.** Every action that touches the network, the
   disk, or several minutes of the operator's time announces its plan first and reports its result
   after — including which file it wrote and how big it was.

7. **Every surface can be left.** There is always a way back, it is always in the same place, and it
   always goes somewhere useful. *(Earned: it was possible to get stuck inside settings.)*

8. **Fidelity is not a style choice.** The iPod's screen is 320×240. It is presented at an **integer
   scale, nearest-neighbour, never smoothed, never stretched, never letterboxed to a wrong aspect**.
   Everything else in the window is ours to design; that rectangle is not.

---

## 3. The nouns

Already correct, already implemented, unchanged by this redesign. The window's job is to show these
and nothing else.

| noun | what it is |
|---|---|
| **Resource** | something a device is made *from*. Four kinds: **boot ROM** (a retail dump, or a recipe for a synthetic one), **firmware** (an Apple `.ipsw`), **software** (Rockbox), **installer** |
| **Disk** | a drive image. Not a resource — it is the thing resources are combined *into*. Knows what built it and what is installed on it |
| **Device** | a *name for one selection*: a boot ROM, a disk, a chassis colour, and whether to work on a copy. Cheap to make, cheap to keep. Several devices may share one ROM |
| **Title** | a decrypted game. Not built yet — see §12 |
| **The machine** | one device, running. There is exactly one, and only while you are on Running |

The single most important consequence: **a device is a selection, not a copy.** Making a second
device does not duplicate a 60 GB image, and deleting a device does not delete anything it pointed at.

---

## 4. The shape

Three places. That is the whole navigation model.

```
┌─────────────────────────────────────────────────────────────────────┐
│  ipod-emulator                          Devices  Resources    ? ⓘ   │   ← the only chrome
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│                        L I B R A R Y                                │
│              what you have — nothing is running                     │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
         │  Start                                    ▲  Esc
         ▼                                           │
┌─────────────────────────────────────────────────────────────────────┐
│   ← My 5.5G                                              ⏻  ⤓  ⌗    │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│                        R U N N I N G                                │
│                     one device, one machine                         │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘

          REFERENCE  ← ? or ⓘ from anywhere, returns to wherever asked
          help · about · where the files are · licences · credit
```

**Library** is home and the launch surface. **Running** is one device. **Reference** is reachable
from either and returns to whichever asked — which is already how `back_to` works and is the one
piece of the current navigation worth keeping.

Everything else — creating a device, editing one, building a disk, inspecting a ROM — happens in a
**Sheet** that pushes in from the right *without leaving the surface underneath*. You can always see
where you came from, so there is never a question of how to get back.

### Parking

Leaving Running (`Esc`, or the back arrow) **parks** the machine: it is snapshotted, the window
returns to the Library, and pressing Start on that device again resumes in about three seconds
rather than cold-booting for seventy-five. The device tile says `parked` and offers **Resume** and
**Cold boot** as separate, both-visible actions.

Parking is not saving and does not touch the drive image beyond what the machine already wrote.

---

## 5. The vocabulary

Seven primitives, closed. Anything the window shows is one of these. A new one requires changing
this document first.

| | what it is | ARIA | used for |
|---|---|---|---|
| **Pane** | a full surface | `region` | Library, Running, Reference |
| **Sheet** | edge-anchored push for one focused task; the surface behind stays visible and does not dim | `dialog`, `aria-modal="false"` | new device, edit device, build a disk, inspect a file |
| **Tile** | one thing, shown as an object — a drawn iPod, a disk, a cover | `button` / `listitem` | the device grid, later the title grid |
| **Row** | one thing, shown as a line, with its actions at the trailing edge | `listitem` | resources, disks, firmware |
| **Expand** | in-place detail for a Row; pushes the rows below it down | `button` + `aria-expanded` + `aria-controls` | what is inside a ROM or an `.ipsw` |
| **Rail** | a stream of progress and results along an edge; pushes, never covers | `log` | fetching, building, installing, the debug readout |
| **Field** | a labelled input, which may be **locked** and states why | native | identity, names, paths |

And one thing that is not a primitive because it obeys different laws:

| **Screen** | the iPod's framebuffer, 320×240, integer scale, nearest neighbour | `img` | Running, and each device Tile's thumbnail |

---

## 6. The visual system

### Type

One family. The system UI font, because it is the only one that is right on three platforms and this
program should not ship a webfont to draw a settings page. One monospace, for paths, hashes,
instruction counts and anything a reader might need to compare character by character.

| role | size / line | weight | used for |
|---|---|---|---|
| `display` | 28 / 34 | 600 | the Library's own title, empty states |
| `title` | 20 / 26 | 600 | Sheet headings, device names on Tiles |
| `body` | 14 / 20 | 400 | everything |
| `strong` | 14 / 20 | 600 | the one word in a sentence that matters |
| `label` | 12 / 16 | 500 | field labels, tile subtitles, table headers. Sentence case, never uppercase |
| `mono` | 13 / 18 | 400 | paths, hashes, serials, GUIDs, instruction counts |

Six roles. If a seventh is needed, one of these is wrong.

### Space

A 4px base, and a closed set of steps: **4, 8, 12, 16, 24, 32, 48**. Nothing between them, nothing
above 48 except deliberate page margins.

- Inside a control: 8 vertical, 12 horizontal.
- Between related things (a label and its field): 4.
- Between fields in a group: 12.
- Between groups: 24.
- Page margin: 32.

### Colour

Follows the system: both light and dark are specified and neither is an afterthought. Named roles
only — no raw hex anywhere but the one table that defines them.

| role | light | dark | used for |
|---|---|---|---|
| `bg` | near-white | near-black | the page |
| `bg-raised` | white | one step up | Tiles, Sheets, Rows |
| `bg-sunken` | one step down | one step down | the well the iPod sits in |
| `fg` | near-black | near-white | body text |
| `fg-dim` | 60% | 60% | labels, subtitles, secondary detail |
| `fg-disabled` | 38% | 38% | a control that cannot be used |
| `line` | subtle | subtle | the only borders that exist |
| `accent` | *§14 Q3* | *§14 Q3* | focus rings, Start, progress |
| `warn` | amber | amber | "this will write to your image" |
| `danger` | red | red | destructive confirmation only |

**The accent is used for three things and no others**: the focus ring, the primary action on the
current surface, and progress. A window where four things are blue has no primary action.

### The iPod

The drawn device is this program's best asset and the current window wastes it in a list. It appears
at three sizes, all of them the same drawing:

- **thumbnail** (~48 px tall) — on a device Tile, in the right chassis colour, screen dark.
- **hero** (fills the Library's empty state) — at rest, the click-wheel outline showing.
- **full** (Running) — the live Screen at integer scale, the wheel and buttons live.

The chassis colour comes from the ROM unless the operator overrides it, which is already how
`Settings::chassis` works and is right.

---

## 7. Motion

**Springs only.** No `ease-in-out`, no fixed-duration curves, with one exception noted below.

| transition | motion |
|---|---|
| Sheet in / out | slide from the trailing edge, `gentle` |
| tab change | 4 px rise + crossfade, `tight` |
| Expand open / close | height, `gentle`; content fades in after the height settles |
| Rail appears | push from the edge, `gentle` |
| press | `scale(0.98)`, `tight`, restored on release |
| focus ring | appears instantly. A focus ring that animates is a focus ring you lose |
| **Library → Running** | **the showpiece.** The device Tile's iPod grows into the Running iPod — one continuous object, `lively`. Going back reverses it |
| progress | **linear, not sprung.** It is data. A progress bar that overshoots is lying |

`prefers-reduced-motion` collapses every one of these to an instant state change. The structural
change still happens; only its animation does not.

---

## 8. Four states, everywhere

Every surface that can show a list, run a task, or take input specifies all four. A surface that
only specifies the happy one is not designed.

**Empty** — never a bare "nothing here". It says what this is for, and offers the one action that
fills it. The Library's empty state is the hero iPod at rest, `No devices yet`, and the button.

**Working** — an inline Rail entry naming what is happening and against what: `Fetching
iPod_25.1.3.ipsw from Apple — 6.5 MB`. Never a spinner alone. Long tasks are cancellable and say so.

**Failed** — stays on screen until dismissed. Says what was being attempted, what happened in the
program's own words, and **what to do next**. A failure that leaves the surface unchanged and shows
nothing is the bug that sent the wizard back to step one with no explanation.

**Disabled** — visible, `fg-disabled`, non-interactive, and **carries its reason on focus and on
hover**. Note for implementation: in the current toolkit `.clicked()` is never true on a disabled
widget and a normal hover-text call is silently dropped on one; whatever replaces it must be checked
the same way.

---

## 9. Library — Devices

The default tab, and the program's front door.

```
┌───────────────────────────────────────────────────────────────────────┐
│  ipod-emulator                        [Devices] Resources       ? ⓘ   │
├───────────────────────────────────────────────────────────────────────┤
│   Devices                                              [ + New… ]     │
│                                                                       │
│   ┌───────────────┐  ┌───────────────┐  ┌───────────────┐             │
│   │      ▟▙       │  │      ▟▙       │  │      ▟▙       │             │
│   │     ▐  ▌      │  │     ▐  ▌      │  │     ▐  ▌      │             │
│   │     ▐()▌      │  │     ▐()▌      │  │     ▐()▌      │             │
│   │      ▜▛       │  │      ▜▛       │  │      ▜▛       │             │
│   │               │  │               │  │               │            │
│   │  My 5.5G      │  │  Rockbox test │  │  Retail dump  │             │
│   │  80 GB · 25.1.3│ │  60 GB · RB4.0│  │  30 GB · 20.1.3│            │
│   │  parked        │ │               │  │               │            │
│   │ [Resume][Cold] │  │   [ Start ]   │  │   [ Start ]   │            │
│   └───────────────┘  └───────────────┘  └───────────────┘             │
└───────────────────────────────────────────────────────────────────────┘
```

A **grid of Tiles**, not a list. Three reasons: the drawn iPod is worth showing; a grid tells you at
a glance which iPod is which by colour; and it is the same shape as the title grid in §12, so the
two rhyme instead of being unrelated screens.

**Each Tile carries** the iPod in its chassis colour, the name, one summary line (capacity ·
what it boots), a state word if it has one (`parked`, `never started`), and its action — always
visible, never revealed on hover.

**Clicking the Tile body** opens the device Sheet. **The action button** starts it. Two targets,
two outcomes, no ambiguity.

**Right-click / long-press / the ⋯ affordance** morphs the Tile's action area into an inline action
strip — Edit, Duplicate, Reveal disk, Remove — in place, on the Tile. Not a context menu; nothing
floats.

**Remove asks**, names what will and will not be deleted (`Removes the device. The disk and the ROM
stay in Resources.`), and is the only place `danger` appears.

---

## 10. Library — Resources

Everything a device is made from, including disks. Rows here, not Tiles: these are files, and files
compare by name, size and date, which is what a row is for.

```
┌───────────────────────────────────────────────────────────────────────┐
│  ipod-emulator                        Devices [Resources]       ? ⓘ   │
├───────────────────────────────────────────────────────────────────────┤
│   Boot ROMs                                        [ Add… ][ New… ]   │
│   ▸ retail-5g.bin           1.0 MB   5G · A1136 · from a real iPod    │
│   ▸ synthetic 5.5G          recipe   5.5G · 80 GB · seed 4f2a…        │
│                                                                       │
│   Apple firmware                             [ Fetch… ][ Provide… ]   │
│   ▸ iPod_25.1.3.ipsw        6.5 MB   5.5G · verified                  │
│   ▸ iPod_20.1.3.ipsw        6.5 MB   5G   · verified                  │
│                                                                       │
│   Software                                            [ Fetch… ]      │
│   ▸ Rockbox 4.0             8.1 MB   5G/5.5G · verified               │
│                                                                       │
│   Disks                                    [ Build… ][ Provide… ]     │
│   ▸ my-5.5g.img            74.5 GB   from iPod_25.1.3 · Rockbox 4.0   │
│   ▸ rockbox-test.img       55.9 GB   from iPod_20.1.3 · Rockbox 4.0   │
└───────────────────────────────────────────────────────────────────────┘
```

Four groups, fixed order, **always all four present even when empty** — an empty group shows its
name, its actions, and one dim line saying what belongs there. A page whose sections come and go is
a page you have to re-learn every visit.

**`▸` expands in place** to show what is *inside* that file — the ROM's image directory and the
identity it declares; the `.ipsw` firmware versions and their checksums; the disk's partitions and
what is installed. This replaces the current separate Details and Firmware pages, both of which
exist only because there was nowhere to put this.

**Every row states its provenance**, because in this program that is the interesting fact: fetched
and verified against a recorded hash, or provided by you. A disk says what built it.

**Removing** a resource that a device depends on says which devices, by name, and offers to remove
it anyway or cancel. It never silently breaks a device.

---

## 11. The device Sheet

One Sheet, two modes: **new** (steps) and **edit** (all of it at once). Same layout, same fields,
same order — so what you learn making one you keep when changing one.

```
                            ┌────────────────────────────────────────┐
   Library stays visible    │  New device                    ✕ Close │
   and does not dim         ├────────────────────────────────────────┤
                            │  ① The boot ROM                        │
                            │     ◉ Synthesise one                   │
                            │     ○ Use a dump…                      │
                            │                                        │
                            │     Model     [ 5.5G  ▾][ Black ▾]     │
                            │               [ 80 GB ▾]               │
                            │     Serial    [ 7B4••••••X3N        ]  │
                            │     GUID      [ 000A27••••••••      ]  │
                            │       ⓘ Generated from the seed, so    │
                            │         the same iPod comes back.      │
                            ├────────────────────────────────────────┤
                            │  ② What it runs                        │
                            │     ◉ Build a disk                     │
                            │       from [ iPod_25.1.3.ipsw     ▾]   │
                            │       plus [✓] Rockbox 4.0             │
                            │            [ ] iPodLinux — experimental│
                            │     ○ A disk I already have            │
                            │       [ my-5.5g.img              ▾]    │
                            ├────────────────────────────────────────┤
                            │  ③ Name it                             │
                            │     [ My 5.5G                       ]  │
                            │     [✓] Work on a copy                 │
                            ├────────────────────────────────────────┤
                            │              [ Cancel ] [ Create ]     │
                            └────────────────────────────────────────┘
```

**The ROM comes first and decides everything after it.** That ordering is settled and is not a
layout preference: a retail dump *states* its model, capacity, serial and GUID, so those fields are
filled in and **locked**; a synthetic ROM makes them a choice, and the choice constrains which
firmware and which software can follow.

**Retail and synthetic look identical.** Same controls, same positions, same heights — the retail
case simply has them locked with a reason attached. This is the only way the surface does not jump
when you switch between them, and it is why a locked dropdown is a dropdown and not a line of text.

**Changing the model regenerates the serial and the GUID**, and both are validated against the model
that is actually selected — a 5G serial is not a 5.5G serial, and the program knows the difference.
A typed serial is validated the same way and says specifically what is wrong.

**Impossible combinations are disabled with their reason attached**, and the best available option is
selected by default. Nothing disappears.

**In `edit` mode** the three numbered groups become three plain groups, all open, no step counter,
`Save` and `Cancel`. Changing the ROM of an existing device warns before it invalidates anything.

**Build failures land in this Sheet**, in the Rail, with the surface intact and the inputs still
filled. They do not close it, and they do not return you to step one.

---

## 12. Running — and the console

### Running a device

```
┌───────────────────────────────────────────────────────────────────────┐
│  ←  My 5.5G                     booting · 62%              ⏻  ⤓  ⌗    │
├───────────────────────────────────────────────────────────────────────┤
│                              ▟▙▙▙▙▙▛                                  │
│                            ▐          ▌                               │
│                            ▐  ┌────┐  ▌     ← Screen: 320×240,        │
│                            ▐  │    │  ▌       integer scale,          │
│                            ▐  └────┘  ▌       nearest neighbour       │
│                            ▐   ( ● )  ▌                               │
│                            ▜▙▙▙▙▙▙▙▙▙▛                                │
└───────────────────────────────────────────────────────────────────────┘
```

The iPod is the subject; the chrome is one bar. `⏻` power, `⤓` screenshot, `⌗` the readout. The
readout is a **Rail that pushes the iPod aside**, not an overlay — the current `D` overlay covers
the thing you are debugging.

**Progress is honest**: the denominator is this device's own last completed cold boot, which is why
it works for Rockbox (~100 M instructions) and iPodLinux (~21.5 G) without knowing which is on the
drive. Before a device has ever booted it says `booting` with no fraction rather than inventing one.

### Titles — the console (0.6, designed now)

The older goal, and the reason the project exists: a decrypted game runs directly, **with no Apple
OS in the loop**. It is designed into this document now so that it is not bolted onto a finished
window later.

It is a **third Library tab**, and it is deliberately the same grid as §9 with a different object on
the Tile:

```
│   Titles                                            [ + Add… ]        │
│   ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐                 │
│   │  cover   │ │  cover   │ │  cover   │ │  cover   │                 │
│   │   art    │ │   art    │ │   art    │ │   art    │                 │
│   ├──────────┤ ├──────────┤ ├──────────┤ ├──────────┤                 │
│   │ Brick    │ │ Vortex   │ │ Mahjong  │ │ Solitaire│                 │
│   │ [ Play ] │ │ [ Play ] │ │ [ Play ] │ │ [ Play ] │                 │
│   └──────────┘ └──────────┘ └──────────┘ └──────────┘                 │
```

Play uses the **same** Library → Running transition, the same Screen at the same integer scale, and
the same chrome bar. A title is a thing you run, exactly like a device; making it feel like a
different program would be a mistake.

**Two things it needs that a device does not**, both specified now: a **cover** (supplied or drawn
from the title's own name if there is none — never a blank rectangle), and **input that is not a
click wheel**, since these are played with the wheel *as a control* rather than as furniture. Gamepad
support belongs here and is toolkit-independent.

**What it does not need, and must not grow**: a shader pipeline. The screen is 76,800 pixels. Any
presentation effect worth having costs nothing on the CPU, and rendering the game at higher than
320×240 would change the machine rather than the presentation — which principle 8 forbids.

---

## 13. Where we deliberately differ

The operator's design work elsewhere locks a rule: **hide, do not disable** — an option that cannot
be used is removed rather than greyed.

**This program does the opposite, on purpose**, and principle 3 is the statement of it.

The reason is that this program's subject *is* the compatibility matrix. Which bootloader can carry
which operating system on which ROM is not incidental complexity to be smoothed away — it is the
thing the user came to find out, and much of `research/` exists to establish it. An option that
silently vanishes teaches nothing. An option that is visible, greyed, and says *"ipodloader2 reads
FAT32 type 0x0B and this drive is 0x0C"* has taught the reader something true about the hardware.

Recorded here so that it reads as a decision rather than an oversight.

---

## 14. Open questions

Answers needed before building.

**Q1 — Grid or list for devices?** §9 chooses a grid, for the reasons given. A list is denser and
scales better past a dozen devices. How many devices do you expect to have?

**Q2 — Where does `Reference` live?** §4 makes it a third place reached by `?`. The alternative is a
Sheet like everything else. A Sheet is more consistent; a place is easier to read long prose in.

**Q3 — The accent colour.** The program has no brand. Options: a restrained blue-grey (neutral,
invisible, safe); iPod-era chrome silver (thematic, weak as an accent); or take the accent from the
device's own chassis colour (charming, and inconsistent between surfaces, which is why I have not
just chosen it).

**Q4 — Light or dark by default?** §6 follows the system. A black iPod on a near-white page looks
better than either of them alone; a dark window looks more like an instrument.

**Q5 — Does the title grid ship in 0.5?** §12 designs it and marks it 0.6. If it is 0.5, the tab
exists and is empty until there is a decrypted title to put in it, which is a worse first impression
than not having it.

**Q6 — iPodLinux's place.** It is cut from 0.5 and marked experimental. §11 shows it as a disabled
checkbox with its reason. The alternative is that it does not appear at all until it works.

---

## 15. Implementation notes

**Slint 1.17.** Chosen over Iced and Dioxus Native for the reasons argued separately: a stable 1.x
API with a company behind it, releases every few weeks, a real styling and layout system, live
preview, and a licence (GPLv3) that matches this repository exactly.

- **The Screen** is `SharedPixelBuffer` → `Image::from_rgba8`, with `image-rendering: pixelated`,
  drawn at a floored integer scale. This path is stable API. The `unstable-wgpu-*` feature is **not**
  used and is not needed — see §12 on why there is no shader pipeline.
- **The model stays in `eapp-loader`.** `settings.rs`, `compose.rs`, `identity.rs` and `nor.rs` do
  not learn what a toolkit is. That separation is what made this redesign cost one file, and it is
  worth keeping for the next one.
- **The layout tests come across.** The current window has tests asserting that every screen can be
  opened from somewhere, that every wizard step draws and fits, that the surface does not move when
  you change your mind, and that every character in the file has a glyph. Those tests caught real
  regressions — including, within an hour of being written, twelve missing glyphs and then my own
  fold arrows. They are re-expressed against the new window, not dropped.
- **One thing to measure, not assume.** Immediate mode re-lays-out and repaints the whole window
  every frame while the CPU is emulating an ARM7; retained mode repaints only what changed. That
  *should* buy back time, but there is no measured GUI-versus-headless delta in this repository —
  the README's ~24% is headless. Measure it before and after, with pinned inputs in both arms, and
  put the number in the changelog.
