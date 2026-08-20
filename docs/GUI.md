# The window

The design of the program's interface, written **before** the interface. When the window and this
document disagree, this document is what gets argued with first.

> **Status: agreed, being built.** §19 records what was decided and what is still reversible.

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
- **The one-button first run was lost.** The README still promises *"press the button and it
  synthesises a boot ROM and downloads Apple's firmware itself"*; the program grew a three-step form
  in front of it. Nobody decided that. It happened. See §11.

None of those are bugs in a screen. They are a window that has no model of itself.

**What is being kept.** The toolkit only ever touched one file. `settings.rs`, `compose.rs`,
`identity.rs` and `nor.rs` (the device model, the compatibility rules, the serial/GUID validation,
the ROM recipes) know nothing about any toolkit and survive intact — as do `emu.rs`, `wheel.rs`,
`control.rs` and `png.rs`. **The nouns are nearly right. The surfaces are wrong.** This is a redesign
of the window, not of the program.

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
   §18 — this is a deliberate reversal of a rule we follow elsewhere. *(Earned: the compatibility
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

Nearly all of this is already implemented and correct. One change, in §3.1.

| noun | what it is |
|---|---|
| **iPod** *(a boot ROM)* | **an iPod's identity.** Model, capacity, serial, GUID, colour. Either **dumped** from a real device or **synthesised** from a seed |
| **Firmware** | an Apple `.ipsw` bundle |
| **Software** | Rockbox, and later others — a thing installed *onto* a disk |
| **Disk** | a drive image. Knows what built it and what is installed on it |
| **Device** | a *name for one selection*: an iPod, a disk, and how to treat them. Cheap to make, cheap to keep |
| **Game** | a decrypted `.ipg` title. Runs with **no boot ROM and no disk** — see §16 |
| **The machine** | one device, running. There is exactly one, and only while you are on Running |

The single most important consequence: **a device is a selection, not a copy.** Making a second
device does not duplicate a 60 GB image, and deleting a device does not delete anything it pointed at.

### 3.1 A synthesised boot ROM is a resource, exactly like a dumped one

Settled here, because the old model was of two minds about it.

On real hardware **the NOR flash *is* the iPod** — model, serial and GUID all live in its SysCfg,
and the drive is swappable. So a boot ROM is not an ingredient like a firmware bundle is; it is *an
iPod's identity*. Whether its bytes came off a real device or out of the generator is
**provenance, not category.** Both kinds are listed together, in one group, with the same row shape.

This deletes a real inconsistency. `Device` today carries **both** `firmware: Option<String>` (a
named resource) **and** `nor: Source` (an inline recipe), where `None` means "the inline one
answers" — a shape the code itself describes as what a device migrated from an older settings file
has. Unifying on *the ROM is always a named resource* removes the split and the migration case with
it.

Two consequences, both improvements:

- **The Sheet's two steps become symmetric.** Step 2 was already "pick a disk you have, or build
  one". Step 1 becomes **"pick an iPod, or make one"** rather than a differently-shaped radio.
- **One flow, two entrances.** Synthesising from inside the device Sheet and from the Resources tab
  are the same flow producing the same named thing. Not two code paths that agree by hand.

**Presentation follows from this.** A synthesised ROM has no file size, and putting the word
"recipe" where every other row shows megabytes advertises it as a lesser kind of thing. Size is not
the interesting fact about a boot ROM. *Which iPod it is* is — so that is what the row says, for
both kinds.

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
          help · about · the program's own settings · licences · credit
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
| **Tile** | one thing, shown as an object — a drawn iPod, a cover | `button` / `listitem` | the device grid, later the title grid |
| **Row** | one thing, shown as a line, with its actions at the trailing edge | `listitem` | resources, disks, firmware |
| **Expand** | in-place detail for a Row; pushes the rows below it down | `button` + `aria-expanded` + `aria-controls` | what is inside a ROM or an `.ipsw` |
| **Rail** | a stream of progress and results along an edge; pushes, never covers | `log` | fetching, building, installing, the debug readout |
| **Field** | a labelled input, which may be **locked** and states why | native | identity, names, paths |

And one thing that is not a primitive because it obeys different laws:

| **Screen** | the iPod's framebuffer, 320×240, integer scale, nearest neighbour | `img` | Running, and each device Tile's thumbnail |

---

## 6. The window itself

Unspecified last time, which is how the current program got both *the tiny iPod* and *the device
that resizes itself*.

- **Minimum size: 900 × 860**, and the height is not a taste. Principle 8 says the framebuffer is
  presented at an integer scale; the drawn screen is 0.4866 of body height wide (§7); so a 320-pixel
  panel at 1:1 needs a **658 px device**, and 56 chrome + 658 + a caption + padding is 852. Below
  that the choice is a smaller window or a downscaled screen, and principle 8 settles which.
  2× would need a 1315 px body, which does not fit on a laptop, so 1:1 is the scale.
- **Default on first launch: 1100 × 880**, centred. Remembered afterwards.
- **The device is a constant, never a function of the window.** A size derived from a height it can
  itself influence oscillates — that is the self-resizing device of §6 wearing a different hat, and
  Slint reports it as a binding loop and warns it may panic.
- **The Screen's scale is a floored integer**, recomputed from the space available, and it **never
  feeds back into the window size.** The device does not resize the window; the window sizes the
  device. A layout that can grow its own container will oscillate, and this one did.
- **Resizing is continuous and never rearranges.** The grid reflows its columns; nothing moves
  between surfaces, nothing collapses into a different control at a breakpoint. There is one layout.
- **Fullscreen** is available on Running only, where it means the Screen at the largest integer
  scale that fits, centred on `bg-sunken`, chrome hidden until the pointer moves. It is not
  available on the Library, because a full-screen list is not a thing anyone wants.

---

## 7. The visual system

### Nostalgia: borrow the language, never the resolution

**The window should feel like the iPod's world.** This is an emulator for a 2005 object and the
nostalgia is the point — it is why anyone comes.

But there is one hazard, and it is not aesthetic. **The emulated screen lives inside our window.**
If our chrome also looks like RetailOS, nobody can tell what is emulated and what is ours — and that
breaks things this project depends on: the screenshots in `research/`, the films in `docs/media/`,
and every bug report where somebody sends a picture and the first question is whether the defect is
in Apple's code or in ours. Knowing which layer you are looking at is this project's whole
discipline.

So the rule is a line, not a dial:

| borrow | never borrow |
|---|---|
| the **palette**, sampled (above) | pixel fonts, faux-LCD, scanlines, screen glass |
| the **material** — dark rule, bright band, falloff | type sizes derived from a 320×240 grid |
| the **row grammar** — big left-aligned type, generous rows, full-width selection, trailing chevron | skeuomorphic plastic or brushed-metal textures |
| the **motion** — deeper slides in, back slides out | any chrome outside the device body that could be mistaken for the device's screen |

Everything in the left column is the design *language*, rendered at native desktop resolution, where
it cannot be confused with a 320×240 framebuffer. Everything in the right column imitates the
*pixel grid*, which is exactly what makes chrome mistakable for content — and is also what makes a
serious instrument read as a toy.

The drawn iPod body with a hard-edged 320×240 rectangle at integer scale inside it is already an
unmistakable boundary. Keep that, and the rest is safe.

**One thing already true by accident.** Principle 4 says nothing floats and everything pushes. That
is exactly how iPod menus move — deeper slides in from the right, back slides out. It was arrived at
from the no-floating-UI rule, not from the iPod, and it turns out to be the same grammar. Where the
iPod's grammar is good, this design already agrees with it.

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

### Colour — sampled from RetailOS, not chosen

The light palette is **measured off the artifact**, the same way every number in `research/` is:
sampled from `docs/media/ipod-03-main-menu.png`, a frame our own emulator drew.

| role | light | dark | used for |
|---|---|---|---|
| `bg` | `#ffffff` | near-black | the page |
| `bg-raised` | `#f7f7ff` | one step up | Sheets, rows, the list |
| `bg-band` | `#eff3f7` → `#c6cbce` | one step down | header bands, the title strip |
| `bg-sunken` | `#c6cfd6` | one step down | the well the iPod sits in |
| `fg` | `#000000` | near-white | body text |
| `fg-dim` | `#5a616b` | 60% | labels, subtitles, secondary detail |
| `fg-disabled` | 38% | 38% | a control that cannot be used |
| `line` | `#848ea5` | subtle | the only borders that exist |
| **`accent`** | **`#2969d6`** | **`#5292e7`** | focus rings, the primary action, progress |
| `warn` | amber | amber | "this will write to your image" |
| `danger` | red | red | destructive confirmation only |

**The accent is RetailOS's own selection blue** — the colour the iPod itself uses to say *this one*.
Contrast, computed: `#2969d6` is **5.14:1** on white, `#5292e7` is **5.91:1** on `#121212`. Both
clear 3:1 for non-text UI with room to spare, and white text on the light-mode fill clears AA.

**The accent is used for three things and no others**: the focus ring, the primary action on the
current surface, and progress. A window where four things are blue has no primary action.

### The material

2005 interfaces were made of *materials*, not flat fills, and RetailOS's selection is a material
with a precise structure. Sampled down a vertical strip:

| | |
|---|---|
| a **1 px darker top rule** | `#2969d6` |
| an immediate **bright band** under it | `#5a9aef` |
| a **gentle fall** to the bottom edge | `#4a86de` |

Dark edge, highlight beneath, falloff. That is the whole of the era's vocabulary, and it is what
makes a surface read as *of a time* rather than as a flat rectangle with a period colour on it.

**Used for exactly two things**: the primary action, and the selected row in a list. Everywhere else
is flat. A window where everything is glossy is a 2005 pastiche; a window where the *one thing you
are about to press* is glossy is a 2005 idea applied with judgement.

### Icons are drawn, never typed

**No icon is a font glyph.** Every one is a vector drawn by us, from a small named set, sized in the
space scale.

This is not aesthetics. The current window shipped **twelve missing glyphs** to the operator — `ⓘ`,
arrows, and others rendering as empty squares — and the test written to catch it caught two more of
my own within the hour. A glyph is a bet that three operating systems all have that codepoint in
some font they will actually choose. We lost that bet, visibly, twice.

The set, and it is closed: `back`, `close`, `add`, `remove`, `expand`, `collapse`, `power`,
`camera`, `readout`, `info`, `help`, `check`, `warning`, `folder`, `download`. Fifteen. Anything
else is a word.

**The glyph test survives the port** and is widened: no source file may contain a non-ASCII
character that is rendered as UI text unless the font in use is proven to have it.

### The iPod

The drawn device is this program's best asset and the current window wastes it in a list. It appears
at three sizes, all of them the same drawing:

- **thumbnail** (~48 px tall) — on a device Tile, in the right chassis colour, screen dark.
- **hero** (fills the Library's empty state) — at rest, the click-wheel outline showing.
- **full** (Running) — the live Screen at integer scale, the wheel and buttons live.

The chassis colour comes from the ROM unless the operator overrides it, which is already how
`Settings::chassis` works and is right.

---

## 8. Motion

**Springs only.** No `ease-in-out`, no fixed-duration curves, with one exception noted below.

| transition | motion |
|---|---|
| Sheet in / out | slide from the trailing edge, `gentle` |
| tab change | 4 px rise + crossfade, `tight` |
| Expand open / close | height, `gentle`; content fades in after the height settles |
| Rail appears | push from the edge, `gentle` |
| press | `scale(0.98)`, `tight`, restored on release |
| focus ring | appears instantly. A focus ring that animates is a focus ring you lose |
| **Library → Running** | **the showpiece.** The hero iPod is already the right size in the right place, so it does not grow — it **wakes up**: the screen lights, the chrome recedes, the list slides out, `lively`. Going back reverses it |
| progress | **linear, not sprung.** It is data. A progress bar that overshoots is lying |

`prefers-reduced-motion` collapses every one of these to an instant state change. The structural
change still happens; only its animation does not.

---

## 9. Four states, everywhere

Every surface that can show a list, run a task, or take input specifies all four. A surface that
only specifies the happy one is not designed.

**Empty** — never a bare "nothing here". It says what this is for, and offers the one action that
fills it. The Library's empty state is §11.

**Working** — an inline Rail entry naming what is happening and against what: `Fetching
iPod_25.1.3.ipsw from Apple — 6.5 MB`. Never a spinner alone. Long tasks are cancellable, and §10
says what cancelling costs.

**Failed** — stays on screen until dismissed. Says what was being attempted, what happened in the
program's own words, and **what to do next**. A failure that leaves the surface unchanged and shows
nothing is the bug that sent the wizard back to step one with no explanation. Five classes, each
with a different next step:

| class | example | next step offered |
|---|---|---|
| **network** | Apple's server did not answer | Retry · Provide the file yourself |
| **verification** | the download's SHA-256 does not match the recorded one | Retry · Report it — a mismatch is interesting and should not be shrugged off |
| **incompatible** | this bootloader cannot carry this OS on this drive | the reason, and the option that does work, pre-selected |
| **space** | 74 GB needed, 31 GB free | the two numbers, and where it would have been written |
| **permission** | the image is read-only, or the directory is not writable | the path, and what to change |

**Disabled** — visible, `fg-disabled`, non-interactive, and **carries its reason on focus and on
hover**. Note for implementation: in the current toolkit `.clicked()` is never true on a disabled
widget and a normal hover-text call is silently dropped on one; whatever replaces it must be checked
the same way.

---

## 10. Long work, and what cancelling costs

Building a drive for an 80 GB iPod writes tens of gigabytes and takes minutes. Three rules.

**Check before starting.** Free space is checked against the estimate before a byte is written, and
a shortfall is a *space* failure per §9 with both numbers — not a crash forty gigabytes in.

**Build to a temporary name; rename on success.** This is already the rule for fetched firmware —
*nothing is renamed into place until it verifies* — and it extends to disks unchanged. A cancelled
or failed build leaves **no partial file with a real name**, so there is nothing to mistake for a
working drive later.

**Cancelling deletes only our own temporary file.** Never the source image, never anything the
operator supplied, never anything that was already named. That rule has no exception and no
"unless".

### Whose file is about to be written to

The current program has a surface that says this out loud before the machine starts, and my first
draft of this document lost it. It is the thing standing between an afternoon and somebody's only
image of an iPod they own.

It appears in **two** places, saying the same sentence:

- on the device Tile, as one dim line — `writes to my-5.5g.img` or `works on a copy`;
- in Running's chrome bar for the first few seconds after start, and permanently in the readout Rail.

When the target is the operator's own supplied image and `work_on_copy` is off, the line is `warn`
coloured. That is the only routine use of `warn` in the program.

---

## 11. First run, and the one button

The README's promise is the product's whole first impression:

> **Press the button.** It synthesises a boot ROM and downloads Apple's firmware itself — then
> builds a drive from it and boots.

**That is restored and it is the design.** A person who has just downloaded this does not have a
boot ROM, does not have an `.ipsw`, does not know what either is, and should not meet a form.

```
┌───────────────────────────────────────────────────────────────────────┐
│  ipod-emulator                                                  ? ⓘ   │
├───────────────────────────────────────────────────────────────────────┤
│                                                                       │
│                              ▟▙▙▙▙▙▛                                  │
│                            ▐          ▌                               │
│                            ▐  ┌────┐  ▌            ← the hero iPod,   │
│                            ▐  │    │  ▌              at rest          │
│                            ▐  └────┘  ▌                               │
│                            ▐   ( ● )  ▌                               │
│                            ▜▙▙▙▙▙▙▙▙▙▛                                │
│                                                                       │
│                    You do not need an iPod,                           │
│                    or any files off one.                              │
│                                                                       │
│                    ┌───────────────────────┐                          │
│                    │   Make me an iPod     │                          │
│                    └───────────────────────┘                          │
│                                                                       │
│         Already have files?  Set one up yourself  ·  or drop them here │
└───────────────────────────────────────────────────────────────────────┘
```

**One button.** It synthesises a 5.5G boot ROM, fetches and verifies the matching firmware, builds
the drive, names the device, and starts it — narrating each step in the Rail as it goes, and leaving
every artifact it made in Resources under a name, so nothing is magic and everything is editable
afterwards.

**It is not a mode.** It produces exactly the objects the Sheet produces. Someone who presses it and
later wants to change the model opens the device and changes it.

The two smaller links are the escape hatches: the full Sheet for someone who knows what they want,
and §12 for someone holding files.

**After the first device exists, this screen is never seen again.** The Library's empty state
thereafter — reached by deleting every device — is the same hero iPod with `No devices yet` and both
paths offered equally, without the welcome copy.

---

## 12. Dropping files

The best thing the current program does, and it was missing from the first draft of this document
entirely.

**Drop anything, anywhere on the window, in any order.** A file is identified by **what it
contains**, not by which control you dropped it on. A 1 MB NOR dump is a boot ROM whether you meant
it to be; an `.ipsw` is firmware; a `.ipod` is software; a large image is a drive. There is no
"choose file type" step and there is no wrong target.

- **The whole window is the target.** While a drag is over it, the window shows one line naming
  what it thinks the file is — `boot ROM · 5.5G` — before you let go. Identification happens on
  hover, so a wrong guess is visible before it is committed.
- **Several at once, in any order.** Dropping a ROM and an `.ipsw` together produces one device.
  Order never matters. There is a test asserting exactly this and it comes across.
- **Nothing is moved or copied without saying so.** The drop reports where the file was filed and
  whether it was copied in or referenced in place.
- **Unrecognised files are named, not swallowed.** `That does not look like anything this program
  can use — 4.2 MB, no recognisable header.` Silence would leave you wondering whether it worked.

Dropping onto Running is the same: it files the resource and says so. It does not disturb the
machine.

---

## 13. Library — Devices

The default tab, and the program's front door once §11 has been through once.

**Master–detail, not a grid.** The list carries the borrowed row grammar from §7; the detail pane is
ours, and is where the drawn iPod lives at hero size.

```
┌────────────────────────┬──────────────────────────────────────────────────┐
│ ██ My 5.5G          ›  │                                                  │
│    Rockbox test     ›  │                    ▟▙▙▙▙▙▛                       │
│    Retail dump      ›  │                  ▐          ▌                    │
│                        │                  ▐  ┌────┐  ▌                    │
│                        │                  ▐  │    │  ▌                    │
│                        │                  ▐  └────┘  ▌                    │
│                        │                  ▐   ( ● )  ▌                    │
│                        │                  ▜▙▙▙▙▙▙▙▙▙▛                     │
│                        │                                                  │
│                        │                   My 5.5G                        │
│                        │        80 GB · Apple 25.1.3 · Rockbox 4.0        │
│                        │              parked · works on a copy            │
│                        │                                                  │
│                        │            ┏━━━━━━━━━━━━━━━━━━━━━┓               │
│                        │            ┃  Resume    Cold boot ┃              │
│ [ + New device ]       │            ┗━━━━━━━━━━━━━━━━━━━━━┛               │
└────────────────────────┴──────────────────────────────────────────────────┘
   the selected row wears        the iPod is always at hero size — this is
   the §7 material               the object the program is about
```

**Why this beat the grid.** Four reasons, and the fourth is the one that decided it:

- The drawn iPod is at **hero size always**, instead of a 48 px thumbnail on a tile.
- It **scales to any number of devices**, which retires the question of how many you will have.
- The detail pane has **room for the facts** a tile could not hold — provenance, what is installed,
  the write target from §10, the last boot.
- **The showpiece transition gets simpler and better.** The hero iPod does not grow into the running
  one; it is already the right size and in the right place. It *wakes up* — the screen lights, the
  chrome recedes, the list slides out. Nothing has to be tweened between two geometries.

**The list.** One row per device: the name, a trailing chevron, and nothing else — density is the
point. The selected row is full-width and wears the §7 material. Rows are `title` sized and
generously spaced, which is the borrowed grammar and is also just a good list.

**The detail pane** holds everything about the selected device and every action on it — Start,
Edit, Duplicate, Reveal disk, Remove. **This deletes a mechanism**: the earlier draft needed
right-click to morph a tile's action area into an inline strip, with its own way back out. A detail
pane simply has room, so there is no morph, no mode, and no way to get stuck in one.

**Remove asks**, names what will and will not be deleted (`Removes the device. The iPod and the disk
stay in Resources.`), and is the only place `danger` appears.

**Empty** is §11 on first run. Thereafter — reached by deleting every device — the list is empty,
the detail pane holds the hero iPod at rest with `No devices yet`, and both paths are offered.

---

## 14. Library — Resources

Everything a device is made from. Rows here, not Tiles: these are files, and files compare by name,
size and date, which is what a row is for.

```
┌───────────────────────────────────────────────────────────────────────┐
│  ipod-emulator                        Devices [Resources]       ? ⓘ   │
├───────────────────────────────────────────────────────────────────────┤
│   iPods                                 [ Add a dump… ][ Synthesise… ]│
│   ▸ From my 30 GB       5G   · 30 GB · white · dumped from a real iPod │
│   ▸ Black 5.5G          5.5G · 80 GB · black · synthesised · seed 4f2a│
│                                                                       │
│   Apple firmware                             [ Fetch… ][ Provide… ]   │
│   ▸ iPod_25.1.3.ipsw        6.5 MB   5.5G · fetched and verified      │
│   ▸ iPod_20.1.3.ipsw        6.5 MB   5G   · fetched and verified      │
│                                                                       │
│   Software                                            [ Fetch… ]      │
│   ▸ Rockbox 4.0             8.1 MB   5G/5.5G · fetched and verified   │
│                                                                       │
│   Disks                                    [ Build… ][ Provide… ]     │
│   ▸ my-5.5g.img            74.5 GB   from iPod_25.1.3 · Rockbox 4.0   │
│   ▸ rockbox-test.img       55.9 GB   from iPod_20.1.3 · Rockbox 4.0   │
└───────────────────────────────────────────────────────────────────────┘
```

Four groups, fixed order, **always all four present even when empty** — an empty group shows its
name, its actions, and one dim line saying what belongs there. A page whose sections come and go is
a page you have to re-learn every visit.

**`iPods` holds both kinds** per §3.1, in one list, in the same row shape. The trailing column is
provenance — `dumped from a real iPod` or `synthesised · seed …` — where the other three groups put
`fetched and verified` or `provided by you`. **Every row states where it came from**, because in
this program that is the interesting fact.

**`▸` expands in place** to show what is *inside* that file — the ROM's image directory and the
identity it declares; the `.ipsw` firmware versions and their checksums; the disk's partitions and
what is installed. This replaces the current separate Details and Firmware pages, both of which
exist only because there was nowhere to put this.

**Removing** a resource that a device depends on says which devices, by name, and offers to remove
it anyway or cancel. It never silently breaks a device. Removing a **synthesised** iPod warns that
the identity is regenerable only from its seed, and shows the seed so it can be written down.

---

## 15. The device Sheet

One Sheet, two modes: **new** (steps) and **edit** (all of it at once). Same layout, same fields,
same order — so what you learn making one you keep when changing one.

```
                            ┌────────────────────────────────────────┐
   Library stays visible    │  New device                    ✕ Close │
   and does not dim         ├────────────────────────────────────────┤
                            │  ① Which iPod                          │
                            │     ◉ One I have                       │
                            │       [ Black 5.5G               ▾]    │
                            │     ○ Make one                         │
                            │                                        │
                            │       Model  [ 5.5G ▾][ Black ▾]       │
                            │              [ 80 GB ▾]                │
                            │       Serial [ 7B4••••••X3N         ]  │
                            │       GUID   [ 000A27••••••••       ]  │
                            │         ⓘ Generated from the seed, so  │
                            │           the same iPod comes back.    │
                            ├────────────────────────────────────────┤
                            │  ② What it runs                        │
                            │     ◉ A disk I have                    │
                            │       [ my-5.5g.img              ▾]    │
                            │     ○ Build one                        │
                            │       from [ iPod_25.1.3.ipsw     ▾]   │
                            │       plus [✓] Rockbox 4.0             │
                            │            [ ] iPodLinux — experimental│
                            ├────────────────────────────────────────┤
                            │  ③ Name it                             │
                            │     [ My 5.5G                       ]  │
                            │     [✓] Work on a copy                 │
                            │       writes to my-5.5g-copy.img       │
                            ├────────────────────────────────────────┤
                            │              [ Cancel ] [ Create ]     │
                            └────────────────────────────────────────┘
```

**The two steps are symmetric** per §3.1: *have one, or make one*, in both. That symmetry is the
point — the previous draft had a differently-shaped question in each step for no reason but history.

**The iPod comes first and decides everything after it.** That ordering is settled and is not a
layout preference: an iPod *states* its model, capacity, serial and GUID, so choosing an existing
one fills those in and **locks** them; making one turns them into a choice, and the choice
constrains which firmware and which software can follow.

**Existing and new look identical.** Same controls, same positions, same heights — the existing case
simply has them locked with a reason attached. This is the only way the surface does not jump when
you switch between them, and it is why a locked dropdown is a dropdown and not a line of text.

**Changing the model regenerates the serial and the GUID**, and both are validated against the model
that is actually selected — a 5G serial is not a 5.5G serial, and the program knows the difference.
A typed serial is validated the same way and says specifically what is wrong.

**Impossible combinations are disabled with their reason attached**, and the best available option is
selected by default. Nothing disappears.

**Making an iPod here creates the resource.** It appears in §14 under its name the moment it is made
— not on completion of the whole device — so a cancelled device does not throw away the identity you
just tuned.

**In `edit` mode** the three numbered groups become three plain groups, all open, no step counter,
`Save` and `Cancel`. Changing the iPod of an existing device warns before it invalidates anything.

**Build failures land in this Sheet**, in the Rail, with the surface intact and the inputs still
filled. They do not close it, and they do not return you to step one.

---

## 16. Running — and the console

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

The iPod is the subject; the chrome is one bar — power, screenshot, readout, all drawn icons per §7.
The readout is a **Rail that pushes the iPod aside**, not an overlay; the current `D` overlay covers
the thing you are debugging.

**Progress is honest**: the denominator is this device's own last completed cold boot, which is why
it works for Rockbox (~100 M instructions) and iPodLinux (~21.5 G) without knowing which is on the
drive. Before a device has ever booted it says `booting` with no fraction rather than inventing one.

### Games — the console (0.6, designed now)

The older goal, and the reason the project exists: a purchased title runs directly, **with no boot
ROM, no disk and no Apple OS in the loop**. Designed in now so it is not bolted onto a finished
window later. Called **Games** in the window; `research/` keeps saying *title*.

**What is already known, so the design rests on facts rather than hope:**

- A `.ipg` is a **zip archive**; its executable is an eApp container — `eapp` magic, block table,
  load base `0x18000000`, named imports as `ldr pc,[pc,#N]` thunks. **20 decrypted titles are on
  hand**, plaintext, verified by their header.
- RetailOS **publishes** its framework surface: **8 frameworks, 433 functions**, each with a 16-byte
  interface hash. Those hashes are byte-identical to the ones `eapp-inspect` reads out of a title's
  own import blocks — the ABI confirmed independently from both sides. Pac-Man declares **98** of
  the 433.
- **`eapp_loader::Machine::bind_native()` already exists.** It matches each framework by interface
  hash and rewrites every import thunk to the real export, returning `(framework, bound, total)`.
  Unbound imports stay pointed at trap space on purpose, so a call that lands there is unambiguous.

**A game needs a framework, and that has the same grammar as a boot ROM.** *Apple's, out of an
`.ipsw`* or *ours, native Rust* — exactly retail-versus-synthesised, one layer up. So §15's Sheet
shape is reused. But the choice **surfaces only when it is the answer to a problem**: default to
whatever works and never make anyone learn the word "framework" to play Pac-Man.

**The readiness readout is this mode's compatibility matrix, and `bind_native`'s return value is
already it.** Not *"will this boot?"* but *"this title declares 98 functions; we can serve 98"* — or
*"…we can serve 61, and here are the 37 missing, by name."* **The trap table is the missing-function
list.** That is §2's third principle at its best.

**The UI barely moves.** Games is a third tab with the same master–detail as §13: list on the left,
the title's own cover art where the hero iPod goes, its readiness underneath, and Play. Same Running
surface, same Screen, same integer scale, same transition. A game is a thing you run, exactly like a
device, and making it feel like a different program would be a mistake.

**Two things it needs that a device does not:**

- **A cover.** It is in the zip. Drawn from the title's name if there is none — never a blank
  rectangle.
- **An identity without a boot ROM.** The DRM binds to the 8-byte **FireWire GUID in `sysinfo_t`,
  read from the NOR, never from the disk** — measured, down to the function. So an encrypted title
  needs an *identity* even though it needs no bootable ROM, and §3.1 is what makes that expressible:
  a game can reference an iPod for its identity without booting it.

**The keystore is unsolved and that work is not in this repository**, so this program takes
**decrypted** titles. The refusal must therefore be first-class and honest — the same one RetailOS
itself draws, in our own words, saying what it binds to and why we cannot help.

**And there is no boot.** No 2.4 G instructions, no seventy-five seconds. Press Play and the game is
there. That is the best demo this program will ever have.

**One thing not to promise.** With Apple's framework the calls are still emulated ARM at ~24 % of
real time, which may not be playable. With native implementations the 179 `OpenGLES` functions
become Rust and the emulated work collapses to the game's own logic — plausibly the first mode that
runs at real speed. That is a hypothesis with an obvious measurement, not a claim.

**What it must not grow**: a shader pipeline. The screen is 76,800 pixels. Any presentation effect
worth having costs nothing on the CPU, and rendering above 320×240 would change the machine rather
than the presentation, which principle 8 forbids.

---

## 17. The rest

### Keyboard

Every interaction has a keyboard route. Unspecified last time, and the current program's map grew by
accretion.

| | |
|---|---|
| `Tab` / `Shift-Tab` | focus, in document order. Never a positive `tabindex` |
| `Esc` | leave: closes a Sheet, exits an action strip, parks a running machine and returns |
| `Enter` | the primary action of the focused surface |
| `Space` | activate the focused control; on Running, the centre button |
| arrows | move within a grid, a list, or a group of fields; on Running, the wheel and buttons |
| `⌘,` / `Ctrl,` | Reference |
| `?` | Reference, on help |
| `⌘F` / `Ctrl F` | fullscreen, Running only |
| `S` | screenshot, Running only |
| `D` | the readout Rail, Running only |

**Running is a mode and says so**: while the machine has focus, letter keys drive the iPod rather
than the window, and the chrome bar shows which. This is the one place the program is modal, it is
unavoidable, and the way out is the same `Esc` as everywhere else.

### Where the program's own settings live

There is no Settings surface, because there are only three settings and they are not worth a place:

- **check for updates on start** — off unless asked for, and it stays that way.
- **default for *work on a copy*** — the per-device answer overrides it.
- **theme** — system, light, dark.

All three live in **Reference**, under the help and above the credits. Everything else that used to
be a setting is a property of a device and lives on the device.

### Screen readers

Slint carries AccessKit, so the roles in §5 are real rather than aspirational. The target is that
the Library, the Sheet and Reference are fully navigable and announced. **Running is not** — a live
framebuffer has nothing to announce — and it says so once, rather than pretending.

---

## 18. Where we deliberately differ

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

## 19. Decisions

Settled 2026-08-20. Each was a recommendation the operator did not overrule; each is one constant or
one section away from being reversed, and the reasoning is here so a reversal is cheap rather than
archaeological.

| | decided | why |
|---|---|---|
| **Devices: grid or list** | **master–detail**, §13 | the grid was chosen first and lost. Hero-size iPod, unbounded scaling, room for the facts, and a transition that needs no tweening |
| **Accent colour** | **`#2969d6` / `#5292e7`**, §7 | RetailOS's own selection blue, sampled off a frame we drew. Derived, not chosen |
| **The one button's default iPod** | **5.5G, 30 GB, black**, §11 | low conviction and genuinely close. Images are sparse, so capacity costs nothing — 30 GB is a real Late-2006 configuration and the friendlier default |
| **Games in 0.5** | **no — 0.6**, §16 | the framework work is not done. The design lands now so the shape is right when it does |
| **iPodLinux's place** | **visible, disabled, with its reason**, §15 | principle 3. But see below — it is a different *kind* of disabled |
| **`Reveal disk`** | **keep it**, §13 | fifteen lines, and this is a program about files that are hard to find. The expanded row also shows the full path, copyable |
| **Reference** | a place reached by `?`, not a Sheet | long prose reads better in one |
| **Theme** | follows the system, both fully specified | three platforms, three expectations |
| **Nostalgia** | **borrow the language, never the resolution**, §7 | the emulated screen is *inside* our window; chrome that imitates it destroys the ability to tell which layer a screenshot is of |

### The two kinds of disabled

Answering the iPodLinux question exposed a gap: §9's `Disabled` state covers two different things,
and greying them identically makes both ambiguous — which defeats the whole justification for
principle 3.

| | says | example | wording |
|---|---|---|---|
| **a machine rule** | *this cannot work* | `ipodloader2` reads FAT32 type `0x0B` and this drive is `0x0C` | state the rule. It is permanent, and teaching it is the point |
| **a project state** | *this is not finished* | iPodLinux boots, then its userland stalls at ZeroLauncher's last step | say what *does* work instead — `ipod-boot install-linux` builds that drive — and match what the README already says |

A machine rule is about the hardware and will never change. A project state is about us and should
read like it. They get different wording, and a project state always names the escape hatch.

---

## 20. Implementation notes

**Slint 1.17.** Chosen over Iced and Dioxus Native for the reasons argued separately: a stable 1.x
API with a company behind it, releases every few weeks, a real styling and layout system, live
preview, and a licence (GPLv3) that matches this repository exactly.

- **The Screen** is `SharedPixelBuffer` → `Image::from_rgba8`, with `image-rendering: pixelated`,
  drawn at a floored integer scale. This path is stable API. The `unstable-wgpu-*` feature is **not**
  used and is not needed — see §16 on why there is no shader pipeline.
- **The model stays in `eapp-loader`.** `settings.rs`, `compose.rs`, `identity.rs` and `nor.rs` do
  not learn what a toolkit is. That separation is what made this redesign cost one file, and it is
  worth keeping for the next one.
- **§3.1 is a model change, not just a presentation one.** `Device::firmware: Option<String>` and
  `Device::nor: Source` collapse into one named reference, and the migration case in the settings
  file goes with them. Do that in `settings.rs` *before* the window is built, with its own tests, so
  the port is not also a data migration.
- **The layout tests come across.** The current window has tests asserting that every screen can be
  opened from somewhere, that every wizard step draws and fits, that the surface does not move when
  you change your mind, and that every character in the file has a glyph. Those tests caught real
  regressions — including twelve missing glyphs, and then my own fold arrows within the hour. They
  are re-expressed against the new window, not dropped, and §7's icon rule makes the glyph one
  stricter rather than retiring it.
- **The drop test comes across too** — `dropped_files_route_themselves_in_either_order` is the
  guarantee behind §12 and it is the one feature nobody has ever complained about.
- **One thing to measure, not assume.** Immediate mode re-lays-out and repaints the whole window
  every frame while the CPU is emulating an ARM7; retained mode repaints only what changed. That
  *should* buy back time, but there is no measured GUI-versus-headless delta in this repository —
  the README's ~24% is headless. Measure it before and after, with pinned inputs in both arms, and
  put the number in the changelog.
