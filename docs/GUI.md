# The window

The design of the program's interface, written **before** the interface. When the window and this
document disagree, this document is what gets argued with first.

> **Status: agreed, being built.** §18 records every decision, including the ones this revision
> overturns and the reasoning behind each, so a reversal is cheap rather than archaeological. §19
> records what two adversarial readings broke, what changed because of them, and the four breaks
> that are accepted as limitations rather than fixed.

---

## 1. The thesis, and what this program is

**The iPod is not a picture inside the program. It is the program.**

One well, one device drawn at its own true size, one fixture holding it, one shelf under it that
speaks, and one drawer beside it that holds everything else. Pressing the device's own centre button
is what starts it. There is no chrome bar, no tab strip, no Start button, and no settings screen —
because every one of those is a thing standing between a person and the object they came to look at.

The 320 × 240 rectangle on that device's face is the only part of the window we are forbidden to
draw in. Everything else is ours.

### 1.1 Why the last one is being thrown away, again

The window shipped on `gui/slint-rebuild` is a three-tab shell. Its two live callbacks both print to
stderr (`start device {index} — not yet reconnected to the emulator`), `Settings::save()` is never
called, five whole modules compile with `#[allow(dead_code)]` and no caller, and the entire
compatibility engine — `compose.rs`, 636 lines and eight tests — has **zero call sites anywhere in
the workspace**. `grep -rn "compose::" tools --include='*.rs'` returns nothing outside `compose.rs`
itself.

That is not the problem. That is a half-built window and it was declared as one.

The problem is that the design and the window now disagree about the things a designer reads first,
and one of the disagreements is a live defect in the rule this project calls non-negotiable:

- **The framebuffer is being stretched right now.** `ipod.slint` draws the `Image` at
  `0.4866 * h` × `0.3672 * h` with `image-fit: fill`. At `h = 658` that is **320.18 × 241.62** for a
  320 × 240 buffer — a scale of **1.00057 horizontally and 1.00674 vertically**, non-uniform and
  non-integer. With `image-rendering: pixelated`, nearest-neighbour resolves the 0.674 % vertical
  excess by duplicating roughly 1.6 rows out of 240 at fixed positions, which lands on RetailOS's
  own 1 px separator rules.
- **The test written to catch that cannot fail.** `the_device_is_big_enough_to_show_every_pixel`
  asserts `drawn_w >= FB_W` and `drawn_h >= FB_H`. That is a *downscale* detector. It passes on a
  stretch while its own doc comment claims "presented at an integer scale, never smoothed and never
  stretched". It also hand-copies every constant it is meant to police out of the `.slint` files, so
  editing the markup leaves it green, and its `CAPTION: 90.0` is a guess against a caption stack
  whose own fixed heights already total 96 before the 30 px heading line is measured.
- **The old §13 describes a window that was deleted.** Master–detail with a 280 px list is still
  written down and still recorded in the decisions table; `window.slint`'s header says the sidebar
  was killed and the window is a carousel. The reversal's reasoning exists only in commit `05cfcc9`.
- **Two source comments describe code that is not there.** `window.slint` says `stage-area` pins
  `min-height` and `preferred-height` to zero; it sets neither. `ipod.slint` says the tests
  re-derive every ratio from Rockbox's SVG; the test says in its own words that it cannot read that
  file because `resources/` is gitignored.

Every one of those is the same failure the last twelve commits kept fixing in the machine — a claim
recorded in prose that outran its evidence — applied to the window's own documentation. This
revision fixes the arithmetic, deletes the false prose, and adopts one mechanical rule to stop it
recurring: **§16.9, no prose claim about the window without a check that can fail.**

**What is kept.** The toolkit still touches one file. `settings.rs`, `compose.rs`, `identity.rs`
and `nor.rs` know nothing about any toolkit and survive intact, as do `emu.rs`, `wheel.rs`,
`control.rs`, `png.rs` and `update.rs`. `ipod.slint`'s measured proportions survive with the
corrections in §6.6. The nouns are right. The surfaces are what change.

---

## 2. Principles

Nine. Every one is the scar of something that actually went wrong in this repository, and each names
it.

1. **The device is the program, and it does not move.** The drawn iPod sits at a constant distance
   above the shelf and at a constant **physical** size — `655.75` device pixels of body height,
   times a whole number — on every surface, in every phase, for as long as the window stays on one
   display at one size. Every pixel of slack the window has goes to the top margin. *(Earned twice:
   the body sat at 560 px for a while, the screen was drawn 253 px wide, a quarter of every frame
   was discarded, and nothing said so — because a downscaled 320 × 240 of a menu is still a legible
   picture of a menu. And again in §6.6: a constant expressed in **logical** pixels is not a
   constant, and on a 125 % display it silently quadrupled the black border it was supposed to
   hold at 11 px.)*

2. **Nothing moves that you did not move.** Every field, row and control reserves its space whether
   or not it currently has content. In Slint that is a mechanical choice per element and §16.3 says
   which. *(Earned: "things should take their space even when empty so things are not jumping
   around".)*

3. **No UI state is ever painted on the drawn device.** Selection, focus, disabled, hover and
   "startable" live on the **cradle** — the fixture the device sits in — never on the chassis. The
   single exception is a control's own physical depression, and it is split: **the press edge reads
   the pointer** (a real button moves when your finger does) and **the release edge reads the
   *emulated* input state** when there is a machine to read one. With no machine, a press restores
   after `tight` and nothing else happens. *(Earned: the moment the object is tinted to mean
   something, a screenshot of this window stops being a picture of an iPod, and `research/`,
   `docs/media/` and every bug report depend on it being one. The split is earned separately —
   `Stats::buttons` is a wheel-frame bitmask published by a **running** machine and is 0 whenever
   the phase is `Off`, so a rule that waited on it would leave the centre button visually stuck down
   through a four-minute first-run build.)*

4. **Disable with a reason. Do not hide.** An option that cannot be used stays visible, greyed, and
   carries its reason — on **focus** as well as hover, which is a specific mechanical requirement in
   this toolkit (§16.5). See §14.1: this is a deliberate reversal of a rule followed elsewhere.
   *(Earned: the compatibility matrix is this program's subject. A bootloader that silently vanishes
   when you pick a ROM teaches nothing.)*

5. **Nothing floats.** No modals, no dialogs, no popovers, no toasts, no tooltips carrying
   information you need. Contextual content **pushes**; it never covers. The one thing that may sit
   above content is a focus ring. *(Earned twice: the current `D` overlay covers the thing being
   debugged, and OpenEmu has a public issue filed against its floating HUD for exactly that.)*

6. **One name for one thing.** A device is a device on every surface, in every message, in the
   settings file and in the changelog. *(Earned: `current == name` was made to mean "running", and a
   device that had never started said it was running.)*

7. **Say what will happen, then say what happened.** Every action that touches the network, the
   disk, or minutes of the operator's time states its plan — with byte counts — before it starts,
   and reports which file it wrote and how big it was after. **One number per axis, from one
   source.** *(Earned: nobody has ever been shown what a download will cost before agreeing to it —
   and then, in this document's own first-run screen, they were shown three different numbers for
   one operation and refused on the wrong one. §10.1.)*

8. **Every surface can be left, and the way out is in the same place.** *(Earned: it was possible to
   get stuck inside settings, because settings and running were different screens and "no machine"
   was how the window knew which to draw.)*

9. **Fidelity is not a style choice.** The panel is 320 × 240 and is presented at an **exact integer
   number of physical device pixels per framebuffer pixel**, nearest-neighbour, never smoothed,
   never stretched, never letterboxed to a wrong aspect. The arithmetic is §6.6 and the test that
   enforces it is §16.10, which must be proved to fail before it is trusted.

---

## 3. The nouns

Settled. Not redesigned here. The model changes required before any markup is written are §3.1, §3.2
and §3.3, and §20 puts them in order.

| noun | what it is | type |
|---|---|---|
| **iPod** *(a boot ROM)* | **an iPod's identity** — model, capacity, serial, GUID, colour. Either **dumped** from a real device or **synthesised** from a seed. Provenance, not category | `Resource::Firmware(nor::Source)` |
| **Firmware** | an Apple `.ipsw`. **Makes a disk.** Not bootable itself | `Resource::Installer(PathBuf)` |
| **Bootloader** | **Goes in the firmware partition, which holds exactly one thing.** That single constraint is what the whole of `compose.rs` is downstream of | `Resource::Bootloader(PathBuf)` |
| **Software** | Rockbox, ZeroSlackr. **Installs onto a disk** | `Resource::Software(PathBuf)` |
| **Disk** | a drive image that knows what built it and what went onto it, in order — so a list of disks is a list of machines rather than a list of filenames | `Disk { name, path, built_from, installed }` |
| **Device** | a *name for one selection*: an iPod, a disk, and how to treat them. **A selection, not a copy** | `Device` |
| **Snapshot** | a parked machine's RAM and CPU, plus the frozen drive it pairs with. **~1.6 GB, and it is a thing on disk with a size** | `Config::snapshot` + `::frozen` |
| **Title** | a decrypted `.ipg`. Runs with **no boot ROM, no disk and no Apple OS** — §13 | 0.6 |
| **The machine** | one device or one title, running. There is exactly one | `emu::Phase` |

**There are four resource kinds, not three.** The shipped window renders three and silently drops
`Bootloader`. That is not cosmetic: *the firmware partition holds exactly one thing* is the reason
`compose.rs` exists, and a parts list that cannot show a bootloader cannot teach the rule the
program exists to teach.

**A snapshot is a noun and not an implementation detail.** It was not one in the previous revision,
and the consequence was a program that writes 1.6 GB per parked device, per window close, with no
listing, no total and no way to get the space back. §11.4 gives it a Parts group.

**A device refers to its parts by name.** Making a second device does not duplicate a 74 GB image
and deleting a device deletes nothing it pointed at. This is better than UTM, which bundles disks
inside the VM bundle so a second VM from the same base is a full copy — and for a program whose
images are sometimes the only copy of an iPod somebody owns, that is not a nicety, it is the safety
property. **§11 makes it visible**, because a safety property nobody can see is not one.

**`work_on_copy` is tri-state, and the third state is load-bearing — but it is not the state the
previous revision described.** `None` means *nobody has said*, and it resolves to **a copy, always**:
which of §7.5's sentences appears is `d.work_on_copy.unwrap_or(true)` and nothing else. What
`built_from` decides is something different — **whether an explicit choice to write gets the `warn`
colour**. The previous revision wrote those two rules as one and got the sentence backwards. Four
sentences about a drive that resolves, and a fifth for one that no longer does; §7.5 spells all
five.

**And the drive is resolved by `Settings::disk_of` rather than read off the device.** A device names
its drive by name — that is what makes the paragraph above true — so a device that has been through
the settings file once carries no `disk_path`, and `write_target()` read exactly that field until
2026-08-22. From the second launch on, row 3 said *no drive yet — nothing will be written* about
every saved device, while the `warn` colour, resolved the other way, was painted underneath those
words. One resolver, and one producer returning the sentence and the colour together.

### 3.1 A synthesised boot ROM is a resource, exactly like a dumped one

**Required in `settings.rs`, with its own tests, before the window is built.** This was declared
closed in the previous revision and was never done; `Device` carried both `firmware: Option<String>`
**and** `nor: Source`, with the migration case described in its own doc comment. **Done 2026-08-21**
(§20 item 1): the field is `firmware: String`, resolved through `Settings::nor_of` and nowhere else,
and the migration case went with the field — `Settings::parse` gives a device that carried its
recipe inline a **named** iPod instead. The rest of this section is why, and it still stands.

The consequence to say out loud, because it is a behaviour change for a real user: with the inline
copy gone there is nothing to fall back to, so a device whose dump has moved is **refused** rather
than booted as a silently substituted generated 5.5G. That substitution is what `unwrap_or(d.nor)`
did, and it is the whole argument for collapsing the pair.

Three things depend on it and none of them is cosmetic:

- **Composition step ① becomes one question.** "Pick an iPod, or make one" — symmetric with step ②'s
  "pick a disk, or build one" — rather than a differently-shaped radio inherited from history.
- **A title can reference an iPod for its identity without booting it.** Apple's DRM binds to the
  8-byte FireWire GUID in `sysinfo_t`, read from the NOR and never from the disk. So a `.ipg` needs
  an *identity* even though it needs no bootable ROM. §13 is not expressible without this.
- **One flow, two entrances.** Synthesising from inside the Composer and from Parts produce the same
  named thing, rather than two code paths that agree by hand.

**Presentation follows.** A synthesised ROM has no file size, and printing `recipe` where every other
row shows megabytes advertises it as a lesser kind of thing. Size is not the interesting fact about
a boot ROM; *which iPod it is* is. So that is what the row says, for both kinds.

### 3.2 A second model change: provenance

**Required before §11 can be honest.** The shipped window prints `fetched and verified` for every
`Installer` and every `Software`, and `dumped from a real iPod` for every `Firmware(File)`, as
**string literals**, regardless of where the file came from — and `Resource` carries only a path, so
the model cannot support the claim.

```rust
pub enum Provenance {
    Dumped,
    Synthesised { seed: u64 },
    Fetched { verified: Verification },
    Provided,
    /// Built here, out of a tree in this checkout.
    Built,
}
pub enum Verification { Sha256, SizeOnly, None }
```

`Item` gains a `from: Option<Provenance>`. The three download outcomes render as three distinct
strings and **`SizeOnly` is never silently upgraded to `verified`** — `firmware.rs` already keeps
that distinction and the window must not undo it. Until the field exists the row **says nothing**
rather than lying.

**Two departures from this block, both argued from its own sentences, both now shipped in
`settings.rs`.** *(Corrected 2026-08-21, when §20 item 2 was built.)*

- **Five variants, not four.** `Built` is what a file this program produced from a vendored tree is,
  and none of the other four is true of it. It carries a retirement condition in its own doc
  comment: §20 item 7 replaces the vendored `ipodloader2` with the fetched release, and if nothing
  ends up filing a resource with `Built` it is deleted rather than left standing.
- **`Option<Provenance>`, not `Provenance`.** "Says nothing rather than lying" needs a says-nothing
  state and none of the five is one — every settings file written before this field has one for
  every entry it holds. `None` renders as the empty string. It is deletable later: when every write
  path states a provenance it becomes unreachable and the `Option` comes off.

Two rules hold the field down, and they are the reason there are two verbs rather than one.
`Settings::file_away` fills in a `None` and **never overwrites a stated value**, so a fetch followed
by a Provide cannot flip a recorded fact; `Settings::record_provenance` is the separate verb for a
caller that has re-checked the bytes. And a `Firmware(Synthetic)` item's provenance is **always**
`Synthesised` with the recipe's own seed, whatever a caller or a hand-edit said — the seed is in the
file once, in the recipe, and `res.N.provenance` is not written for it at all.

**A provenance is a record of how a file arrived, not a measurement of the bytes now on disk**, and
the wording had to be corrected to say so (2026-08-21). An `Item` is keyed on its `Resource` — a
*path* — and no digest, size or mtime is stored beside the token, so replacing the file underneath
it (a partial re-download, a `curl -o`, a restore from a backup) leaves the claim standing. The row
read `fetched · SHA-256 verified`, present tense, about a file nobody had re-opened: the same shape
as the string literal this field exists to delete, one level down. It reads **`fetched · SHA-256
verified when it arrived`**, and `Provenance::is_verified` says in its own doc comment that it
answers about the filing rather than about the file. The path that re-establishes or refutes it does
exist and is named there — `firmware::cached(dir, true)` re-hashes, `firmware::provenance` maps the
answer, `Settings::record_provenance` files it — and it has **no caller yet**, because the Parts
page that would run it is Phase 6. A re-fetch must go through that verb and not through
`file_away`, whose whole rule is that it leaves a stated value alone: routed the wrong way, a
re-download that could only be size-checked keeps the SHA-256 badge from the first one.

### 3.3 A third: a device knows whether its parts are still on disk, and when it was parked

**Required before the cradle can be honest, which is before the bench can be drawn.** §7.3's closed
cradle set contains `cannot start — the disk is not where it was`, and the previous revision treated
that state as reached. **Nothing computed it.** `Settings::missing()` checked only
whether a *name* still resolved in `self.disks` and `self.resources`; it never touched the
filesystem. Delete an image in Finder and the entry was still in the list, so `missing()` returned
empty, the cradle stayed `accent`, the label promised `about 3 s`, and the machine started and
failed somewhere inside `emu`. It stats now — see the block below and the three decisions under it.

```rust
pub enum Absent {
    /// The device names a part the lists no longer hold.
    Unlisted(String),
    /// The lists hold it and the file it points at is gone.
    Gone(PathBuf),
}
pub fn missing(&self, d: &Device) -> Vec<Absent>   // stat() per resolved path
```

Two named cases rather than one boolean, because the sentences differ and the second one can name
the path — which is the whole of the answer. **A device missing more than one part gets its own
cradle row** (§7.3), because 24 px of centred `body` does not hold two filenames and the previous
revision specified both the cradle *and* shelf row 2 for the same string.

**Done 2026-08-21** (§20 item 3), with three decisions the block above does not carry:

- **The order is fixed** — firmware first, disk second — so the one-part sentence is stable.
- **`Presence` memoizes one pass's `stat` answers** and is dropped at the end of it, so N devices on
  one drive cost one call and there is no staleness window to invalidate. A path under a stale
  network mount blocks, so `missing()` **must not be called from a binding or a callback body**; it
  belongs in the same off-thread pass §11.4's `detect_mounted()` already needs. *(The sharing was
  specified and then not done: the one caller used `Settings::missing`, which mints a fresh
  `Presence` per call, so N devices on one drive cost N. Fixed 2026-08-21 — `device_rows` builds one
  and threads it through `summary`. **The off-thread half is still open**, and the pass runs before
  `window.run()`, so a share that is not up delays the first window rather than one row of it.)*
- **A `stat` that fails reads as present, unless the failure is itself a statement of absence.**
  `Path::exists()` folds every error into `false`, so a directory the user cannot traverse would be
  reported as `is not where it was` — the program asserting a fact about somebody's filesystem it
  never observed, which is the same defect class as the `fetched and verified` literal. The cost is
  that a permissions problem produces no diagnosis at all; §7.3's closed set has no row for it, and
  adding one is an operator decision. *(Corrected 2026-08-21: `NotFound` alone was too narrow the
  other way. `NotADirectory` — a path whose parent component is a regular file — and `InvalidInput`
  / `InvalidFilename` — a path the OS will not accept at all — are definite negatives, and calling
  them present hid a device that cannot start behind a cradle saying nothing.)*

§7.3 has no row worded for `Unlisted` — its one-part row names a *file*. The model keeps the
distinction; the window reuses the one-part row via `Absent::label()` until the design says
otherwise.

Two more fields, both cheap and both currently unrepresentable:

- **`Device::parked_at: Option<u64>`** — a unix timestamp. §7.5's shelf renders `parked · 4 min ago`
  and §12.4 treats "parked" as a device state, and `Device` carries no such flag and no such time.
  Derive both from the snapshot file's existence and mtime *or* store them; either is fine and the
  document has to say which. **It stores them**, because the snapshot path is a `Config` concern and
  the shelf should not have to know it. **Model half done 2026-08-21** (§20 item 4) — seconds since
  the epoch, with `now_unix`, `record_park`, `discard_park` and a saturating `parked_for`. It answers
  *when*, never *whether*: the authority on whether there is a restore point is
  `Config::may_restore()`, so `parked_at` is **not** cleared by a cold boot, a power-off, or a pair
  that has broken. **Nothing writes it yet** — `emu::Link` carries no `parked_at` and
  `write_restore_point` still reports nothing, so `record_park` has no caller.
- **`Stats::enters_by_core`** — conditional, and §17.Q10 is the question. Today `Stats::enters` is
  `[u64; WATCHED.len()]`: one flat array of five arrival counts, with no per-core dimension anywhere
  in `Stats` or `Out`. §12.8 draws **one** column until the model carries two, because the
  alternative is filling a `core 1` column with an invented zero, which is exactly the conflation
  the Gauge's three-state freshness exists to forbid.

---

## 4. The shape

**Three surfaces and a menu bar.** That is the whole navigation model.

```
                    ┌─────────────────────────────────────────────┐
                    │                                             │
                    │              T H E   B E N C H              │   the window opens here
                    │   one well · one device · one cradle ·      │   and there is nowhere else
                    │   one shelf.  Press ● to start.             │   it can open
                    │                                             │
                    └─────────────────────────────────────────────┘
                       │  MENU / handle / ⌘\ / ⌘,      ▲
                       ▼                               │  MENU at the root / Esc / handle
                    ┌──────────────────────────────────────────────┐
                    │  T H E  D R A W E R   420 px, right-anchored  │
                    │  it PUSHES — the well AND the shelf narrow to │
                    │  W − 420.  Full client height above the shelf.│
                    │  Max 3 levels deep.  Devices · Parts · Games ·│
                    │  Work · Readout · Settings · Reference        │
                    └──────────────────────────────────────────────┘

                       │  ⌃⌘F / F11                ▲  Esc / ⌃⌘F / F11
                       ▼                           │
                    ┌─────────────────────────────────────────────┐
                    │  F U L L S C R E E N — the panel alone, at  │
                    │  the largest whole-number scale that fits    │
                    └─────────────────────────────────────────────┘
```

**The bench is not a place you can leave**, because there is nowhere above it. The drawer is a set of
pages you visit. Fullscreen is a surface the machine has, not a mode the window is in.

**The bottom-right 420 × 88, decided 2026-08-21.** *The drawer is full client height above the
shelf* **and** *the shelf narrows to W − 420* cannot both be true of the same rectangle: between them
they leave the corner where the drawer's height stops and the shelf's width stops belonging to
nothing. §11.2's own arithmetic — 803 px on the operator's machine, 722 at the window minimum — is
computed as `client − SHELF`, so the drawer's height is not the negotiable half. **The shelf's band
background and its 1 px top rule span the full client width; only the shelf's three content rows
narrow to `W − 420`.** Nothing is unpainted, the drawer covers no shelf content, row 3 keeps the
narrowed measure §7.5 asks for, and the bottom band stays continuous as §7's own drawing shows it.

**And the drawer has to be *told* that height.** A component's root element cannot read `parent` —
Slint says `Cannot access id 'parent'` — and it must not read the window's own height either (§16.1),
so the client area is a wrapping element at `100%` and its height is passed in. Letting each use site
write `height: <something> - SHELF` would put this decision in two files and let them disagree.

**`Esc` is not on that diagram and that is the point.** It has exactly one definition, in §16.8, and
it only ever goes *outwards*. The previous revision listed it as a way *into* the drawer while also
defining it as the way out, and also as the way out of fullscreen — three meanings for one key, of
which the one that fired on an empty bench initiated a 1.6 GB write. There are already four routes
in (`⌘\`, the handle, the shelf's row-3 leading slot, the menu bar); `Esc` is not a fifth.

**The menu bar is a macOS convenience and nothing is menu-only** — every verb in it is also a drawer
row. §16.8 has the platform caveat and §17.Q4 is the open question about Windows and Linux.

**There is no carousel, and no list on the bench.** Both are overruled here; §18 records why.

---

## 5. The primitive vocabulary

Nine, closed. Anything the window shows is one of these. A tenth requires editing this document
first — and note that this revision adds three (**Cradle**, **Gauge**, **Scroll**) and retires one
(**Tile**), which is exactly the edit this rule demands.

| | what it is | Slint accessibility | used for |
|---|---|---|---|
| **Pane** | a full surface | `Main` (bench) / `Complementary` (drawer) | the bench, the drawer, fullscreen |
| **Cradle** | the fixture the device sits in. **Carries every piece of UI state the device is forbidden to carry**: startable, running, refused, focused | `Button` + `accessible-label` + `accessible-description` | the bench, and only there |
| **Row** | one thing, on a line, 44 px, label leading / value at 232 px / chevron trailing | `ListItem` inside a `List` | every drawer page |
| **Expand** | in-place detail for a Row; pushes the rows below it down | `accessible-expandable` + `-expanded` + `accessible-action-expand` | a refusal's paragraph, what is inside a ROM |
| **Field** | a labelled input, which may be **locked** and states why | `TextInput` / `Combobox` | identity, names, paths |
| **Gauge** | one measured number: label leading, value trailing in tabular mono, with a **three-state freshness** — live / stale / not measured. *(Built 2026-08-24. Four states, not three: `final` is *the machine ended here* against `stale`'s *we stopped looking*, and a two-state boolean is what makes them one)* | `Text` | the Readout, and nothing else |
| **Rail** | a stream of plan, progress and results | `Region` + `accessible-live-region: polite` | the drawer's Work page |
| **Scroll** | the **body** of a drawer page, between its fixed header and any pinned footer row. Never the bench, never the shelf, never the well | `Flickable`, `accessible-*` on its children | Readout, Parts, Composer, Games |
| **Screen** | the framebuffer, 320 × 240, exact integer physical scale, nearest neighbour. **Obeys different laws** | `Image` | the bench's glass, fullscreen, a ROM's boot-screen preview |

**Scroll is the ninth and it was missing, which was fatal.** §16.2's whole finding is that a Slint
layout smaller than the sum of its children's minimums neither shrinks an item pinned by an explicit
`height:` nor clips the surplus — the trailing children are simply positioned past the container's
bottom edge and drawn there. With no scroll container, §12.8's Readout (36 Row-shaped items, 7
headings, six inter-group gaps and a four-line paragraph — about 1 970 px) draws roughly 1 100 px of
itself over the well and off the bottom of the window, silently. §11.4's Parts and §11.2's Composer
are the same shape. That is the exact failure §16.2 names: *it looks fine and it is wrong.*

**Scroll's costs are real and are accepted with their numbers** (§16.11): an **8 px** drag threshold
(`i-slint-core-1.17.1/items/flickable.rs:366`), and a `Flickable` that has scrolled captures further
wheel events for **800 ms** (`:376`). *(A third — a **100 ms** press-forward delay — was accepted
here and is **retired**: it applies only to an interactive Flickable, and `Scroll` declares
`interactive: false`. What replaces it is no touch-drag-to-flick. See §16.11.)* §11.2 is still re-cut
into three depth levels, because 1 090 px does not fit in 722.

**Amended 2026-08-21: the 34 px reason slot belongs to every pressable thing, not to `Field`.** It
was written as `Field`'s alone, and §9.4's rule — *a disabled control states why, and stays in the
tab order* — applies to a `Row` as much as to an input. So the slot is reserved by the one
construction §16.5 specifies, which `Row` inherits: **a disabled `Row` is 44 + 34 = 78 px, not 44.**

That is a real cost and it is named rather than discovered. §11.2's page budget counts Rows at 44
and Fields at 44 + 34 separately, so any page sized against that arithmetic is short: the drawer's
own root page is 556 px rather than 380, which is what put it past the bottom of a 312 px drawer
(§16.11). The alternative — showing a `Row`'s reason on focus and hover *inside* the 44 px — is a
control that changes what it says without changing size, and there is nowhere in 44 px for two
lines of `label` beside a label and a value. This is the edit §5's own *"a tenth requires editing
this document first"* rule demands, made rather than left implicit.

**Retired: Tile and Sheet.** Tile went with the device grid (rejected) and the carousel (§18). Sheet
went because the drawer *is* the pushed surface, and having both would be two mechanisms for one
job.

**Slint's `AccessibleRole` has no `dialog` and no `log`** — verified against
`i-slint-common-1.17.1/enums.rs`. The previous revision assigned both. The roles above are what
exists. And note §16.7: **the `accessibility` feature is not compiled into this program today**
(`default-features = false`, and `grep -c accesskit Cargo.lock` returns 0), so every one of these is
aspirational until it is turned on. Turning it on is a prerequisite, not a polish item.

---

## 6. The visual system

### 6.1 Nostalgia: borrow the language, never the resolution — enforced by geometry

The window should feel like the iPod's world. That is why anyone comes. But the emulated screen
lives *inside* our window, and if our chrome also looks like RetailOS nobody can tell which layer a
screenshot is of — which `research/`, `docs/media/` and every bug report depend on.

So the rule is not a taste. It is three independent geometric tells, **any one of which is
sufficient**:

1. **The picture's long edge is an exact integer multiple of 320 device pixels.** Ours never is.
2. **Our type is the system UI face at 12 / 13 / 14 / 18 / 20 px.** RetailOS's is a 2005 face with
   visibly square pixels at any scale above 1×.
3. **10.5 physical pixels of `#08080a` glass surround the picture on all four sides**, and nothing
   of ours is ever drawn inside that rectangle. Not an error, not "no disk", not a hint, not a
   welcome.

**Tell 3 is stated in *physical* pixels and that is the correction that mattered most in this
revision.** It used to say "~11 px" and be computed from a logical constant, which meant that on a
125 % Windows display it was 43 px and on a 150 % display 97 px — four and nine times the number a
ruler was supposed to be able to check. At that point the drawn object stops being an iPod and
becomes an iPod-shaped frame around a small picture, which is the operator's already-recorded
rejection (*"it doesnt look like a proper ipod"*) reached by arithmetic rather than by bad drawing.
§6.6 is the fix; **10.49 physical px, four sides, every display, every scale factor** is the
invariant, and §16.10 asserts it.

| borrow | never borrow |
|---|---|
| the **palette**, sampled off a frame this emulator drew | pixel fonts, faux-LCD, scanlines, screen glass |
| the **material** — dark rule, bright band, falloff | type sizes derived from a 320 × 240 grid |
| the **row grammar** — label leading, value at a fixed column, trailing chevron, full-width selection | skeuomorphic plastic or brushed-metal texture |
| the **motion** — deeper slides in from the right, back slides out | any chrome outside the device body that could be mistaken for the device's screen |

**And every borrowed thing is placed somewhere a framebuffer physically cannot be** — the drawer is
420 px at desktop resolution and runs to the window's own top edge; the accent is on the cradle,
which is outside the body; the material is on drawer rows. That is what makes the borrow safe, and
it is checkable with a ruler.

**Brushed metal is the one texture Apple's own 2005 HIG would have licensed here** — it was reserved
for "programs that mimic the operation or interface of common real world devices", iTunes and iSync
being the cited cases. It is refused, and recorded as a considered refusal rather than an omission,
because somebody will propose it. It is a pixel-grid borrowing wearing a texture.

**No shadows anywhere.** Slint offers `drop-shadow-*` and `inner-shadow-*` on every element; a shadow
is the visual grammar of floating, and nothing here floats. Separation is a 1 px `line` rule. The
drawn iPod's own edge is a hairline.

### 6.2 Type — six roles, and the seventh was costing the panel 10 px

| role | size / line | weight | used for |
|---|---|---|---|
| `title` | 20 / 26 | 600 | the shelf's name row, drawer page headings |
| `readout` | 18 / 22 mono, **tabular figures** | 500 | every Gauge value, and nothing else |
| `body` | 14 / 20 | 400 | everything |
| `strong` | 14 / 20 | 600 | row names, the one word in a sentence that matters |
| `label` | 12 / 16 | 500 | field labels, provenance, group headings. Sentence case, never uppercase |
| `mono` | 13 / 18 | 400 | paths, hashes, serials, GUIDs, addresses, instruction counts |

**`display` is retired.** A 28–30 px device name floating over a 656 px rendering of that same
device is redundant, and its line box was eating 36 px of a vertical budget with none to spare
(§9.6). The shelf's name row is `title`.

**`readout` earns the seventh slot the sixth vacated**, because 25 live numbers want tabular figures
— without them a value that changes from `1 612 004 992` to `1 612 500 112` reflows its own digits,
which reads as motion and is not.

**Two of the columns above are not properties Slint has, and both got substitutes in the building
(2026-08-21).**

- **The `/ 26`, `/ 22`, `/ 20`, `/ 16`, `/ 18` line heights.** `Text` has **no `line-height`** in
  Slint 1.17 — zero hits across `builtins.slint` — and a `Text`'s vertical `LayoutInfo` is
  `min == preferred ==` its own font metric (`items/text.rs:684`). So **a line box is a container
  height**: `LINE_TITLE = 26`, `LINE_BODY = 20`, `LINE_LABEL = 16` are declared in
  `src/geometry.rs` and the row that carries the type sets them. The other two arrive with their
  first use site.
- **`readout`'s tabular figures.** There is no font-feature or font-variant control in the language
  at all. What is available is choosing a **monospace family**, which §6.2 already does for that
  role — the tabular digits then come from the family rather than from a feature flag. The reason
  the role exists is unchanged; only the mechanism is.

One family: the system UI font, because it is the only one right on three platforms and this program
should not ship a webfont to draw a settings page. One monospace, chosen per platform in Rust and
pushed in as a global string, because **Slint takes one `font-family` per element and has no
fallback list** (§16.6).

`font-weight: 600` is a role, never inlined. It is currently inlined in six places in
`window.slint`; that is corrected at the token file, not per call site.

### 6.3 Space — 4, 8, 12, 16, 24, 32, 48, and nothing between

- Inside a control: 8 vertical, 12 horizontal.
- Label to its field: 4. Between fields: 12. Between groups: 24. Page margin: 24 (the drawer) / 32
  (a wide page).

Five numbers on this surface are **geometry, not spacing**, and are declared as such so they do not
look like scale violations: the device's **`hero`** (a derived length, §6.6), the drawer's **420**,
the shelf's **88**, the cradle label's **24**, and the cradle's **10 px outward offset plus a 6 px
focus gap** — which §9.6 has to pay for **on both sides**, and did not. Every other hard-coded
length in `window.slint` today — 36, 44, 56, 60, 30, 20, 16, 7, 6, 2 — either resolves onto the
scale or is deleted.

### 6.4 Colour — measured, dark derived by a stated rule, and contrast computed against the surface each thing is actually on

The light palette is sampled from `docs/media/ipod-03-main-menu.png`, a frame this emulator drew.
The dark palette has nothing to sample — RetailOS has no dark mode — so it is **derived** from the
light values by one stated rule rather than invented: *preserve hue, invert lightness, and keep the
well a recess in both schemes.* Inventing values is the exact mistake the iPod's proportions already
made once.

| role | light | dark | used for |
|---|---|---|---|
| `bg` | `#ffffff` | `#121212` | drawer pages |
| `bg-raised` | `#f7f7ff` | `#1b1c1f` | the drawer body, an expanded row |
| `bg-band-top` → `-bottom` | `#eff3f7` → `#c6cbce` | `#24262b` → `#15171a` | the shelf, the drawer header |
| `bg-sunken` | `#c6cfd6` | `#0e1013` | **the well the iPod sits in**, and fullscreen |
| `fg` | `#000000` | `#f2f2f2` | body text, **and the cradle's focus ring** |
| `fg-dim` | `#5a616b` | `#9aa1ab` | labels, provenance, secondary detail, **and the cradle's inactive ring** |
| `fg-disabled` | `#9aa0a8` | `#61666e` | a control that cannot be used — **on `bg` and `bg-raised` only** |
| `line` | `#848ea5` | `#3a4050` | the only borders that exist |
| **`accent`** | **`#2969d6`** | **`#5292e7`** | focus ring · progress · the cradle when startable |
| material rule / top / bottom | `#2969d6` / `#5a9aef` / `#4a86de` | `#2a5fbf` / `#3f7fd6` / `#2f66b8` | see §6.5 |
| `warn` | `#9a6700` | `#d9a441` | `writes to <your>.img`, and `input_dropped > 0` |
| `danger` | `#b3261e` | `#e0564f` | `Remove`, and a `Stopped` machine |

The accent is **RetailOS's own selection blue** — the colour the iPod itself uses to say *this one*.

**Contrast is computed against the surface each thing is drawn on, which is the correction.** The
previous revision computed the accent on `#ffffff` and on `#121212` and never once against
`bg-sunken` — which is the **only** surface the cradle is ever drawn on, and the cradle is the sole
carrier of machine state under principle 3. Measured:

| on `bg-sunken` | light `#c6cfd6` | dark `#0e1013` | verdict |
|---|---|---|---|
| `line` at 30 % *(the previous inactive ring)* | **1.23 : 1** | **1.14 : 1** | **invisible.** Five of twelve cradle states |
| `line` at 100 % | 2.08 : 1 | 1.84 : 1 | still fails 3 : 1 |
| `fg-disabled` *(the previous "cannot start" ring)* | **1.67 : 1** | 3.30 : 1 | **fails in light.** The one state whose job is to teach |
| **`fg-dim`** | **3.96 : 1** | **7.31 : 1** | ✓ — the inactive and refused ring |
| **`accent`** | **3.25 : 1** | **6.01 : 1** | ✓ — startable |
| **`danger`** | **4.14 : 1** | **5.09 : 1** | ✓ — stopped |
| **`fg`** | **13.30 : 1** | **17.02 : 1** | ✓ — the cradle's focus ring |

On `bg` / `bg-raised` the older numbers stand and are kept: `#2969d6` is **5.14 : 1** on white,
`#5292e7` is **5.91 : 1** on `#121212`, white on the accent fill is **5.14 : 1**, and `fg-dim` is
**6.26 : 1** on white.

**So the cradle uses three colours and one shape, not four colours** (§7.3): `accent` when startable,
`fg-dim` otherwise, `danger` when stopped, and **a broken ring** — four arcs with gaps at the
corners — when the device cannot start. A refusal that cannot be seen is a hidden option, and
principle 4 forbids hidden options; but there is no fourth colour on this surface that clears 3 : 1
and does not already mean something else, and `Path` has no dash array (§16.6). A gap is a shape,
and a cradle that cannot hold the device having gaps in it is the drawing saying the thing.

**The cradle's focus ring is `fg`, and it is the one exception to "the focus ring is `accent`."**
Everywhere else the focus ring sits on `bg` or `bg-raised` where the accent clears 5 : 1. On the
cradle, a 2 px accent ring 4 px outside a 2 px accent state ring is a ring around a ring in one
colour, which is not a focus indicator. `fg` at 13.3 : 1 collides with no state.

**The accent is used for three things and no others**: the focus ring (except on the cradle),
progress, and the cradle when the device can be started. A window where four things are blue has no
primary action. The shipped window spends its entire accent budget on carousel page dots — the one
element the design never mentions — and puts no accent on the centre button at all.

The scheme comes from `SlintInternal.color-scheme`, a backend-provided reactive expression usable
directly in markup with no Rust round trip (verified at `i-slint-compiler-1.17.1/lookup.rs:824`). It
is an internal namespace and therefore outside the 1.x stability promise; that is a risk worth naming
and there is no supported alternative — it is what Slint's own styles use.

### 6.5 The material, and where it is allowed

Sampled down a vertical strip of RetailOS's own selection: a **1 px darker top rule** `#2969d6`, an
immediate **bright band** `#5a9aef`, a **gentle fall** to `#4a86de`. Dark edge, highlight beneath,
falloff — the whole of the era's vocabulary, and what makes a surface read as *of a time* rather than
as a flat rectangle with a period colour on it.

**Used for exactly three things:**

1. **The selected row in a drawer list.**
2. **The one primary row on a drawer page** — `Create`, `Resume`, a `Fix` (at 60 % opacity, because a
   fix is offered rather than urged). One per page, never two.

**And never on a control that is disabled** *(added 2026-08-24)*. `Pressable` drew the material on
`primary || selected` and never asked `enabled`, so a disabled primary kept the full-opacity accent
under a label in `Ink.fg-disabled`: `#9aa0a8` on `#5493e9` is **1.18 : 1**, which is not a label.
`_out/gui/composer-reading.png` is what shipped — a full-width blue `Create` with nothing legible on
it and *A device needs a name.* underneath. Off the material the same label is 2.47 : 1, which is
what every other disabled control in this window already draws at; `material-opacity` is deliberately
not used instead, because at 0.45 the label measures 1.6 : 1 and §14.1 has already recorded that
failure once, at 1.67 : 1.

**Nothing checked either half of this rule and both are now checked off the pixels.**
`Shot::material_bands` counts the bands of material down a drawer page in a shot and
`every_page_this_window_draws_can_be_shot_with_no_window` asserts at most two — use 1 plus use 2. A
source sweep could not: `primary: true` is a literal at four sites and a binding at two more, and how
many of them are *drawn at once* is a fact about the model each page was pushed. `devices.png` had
**three** — the selected shelf row, `Start` inside its open body, and the pinned `+ New device`
footer whose own comment called itself *the page's one material row*. `Start` gave it up; the footer
is the page's action in every state including the empty one, which is what §9.1 put it outside the
`Scroll` for.
3. **The one primary row on the too-short bench** (§9.5) — which is the *same* row as the cradle's
   own label and callback, wearing the material because on that display the cradle is not drawn and
   the program's single interactive element has to survive.

The shipped window gives the material to the selected **tab** and gives the selected device row a
flat `bg-raised` fill — the rule was rewritten in a source comment rather than in the document it
contradicts. That is corrected here, in the document.

**And it does not go on the centre button.** An earlier draft of this design put it there, on the
argument that a surface treatment on a moulded disc reads as the disc being lit rather than as a
widget stuck to a photograph. That argument is good and it loses to principle 3: a glossy blue disc
is UI state painted on the object, and a screenshot of it is no longer a picture of an iPod. **The
cradle carries "press this" instead** — the accent annotates the fixture, never the object.

### 6.6 The drawn iPod, and the fidelity arithmetic — twice corrected

Every dimension is a fraction of `body-height` and nothing else. That is what makes the device unable
to resize itself, and it means one number takes it from a drawer thumbnail to the hero.

**The ratios are measured from Rockbox's own scale drawing of this device**
(`manual/rockbox_interface/images/ipodvideo-front.svg`, a vector front elevation shipped with the
Rockbox manual). The parse is self-checking: on a 104.1 mm body the extracted display comes out
50.7 × 38.2 mm against the 50.8 × 38.1 mm a 2.5-inch 4:3 panel must be.

#### Correction one: the drawing's 0.2 % is not small

`0.4866 / 0.3672 = 1.32516`, and 4:3 is `1.33333`. The drawing's own error is 0.61 % — small on paper
and fatal in the one rule this project will not bend, because a screen well that is not 4:3 stretches
a 320 × 240 buffer *in one axis only*.

**Invert the dependency.** MAME solves exactly this by making the screen an explicitly bounded item
that the artwork is positioned *around*, rather than a fraction of the artwork. Here that means: the
SVG governs **where** the screen sits; the hardware governs **how big** it is.

| ratio | was | is | why |
|---|---|---|---|
| `SCREEN_W` | 0.4866 | **0.48799** | 50.8 mm / 104.1 mm |
| `SCREEN_H` | 0.3672 | **0.36599** | 38.1 mm / 104.1 mm |
| `SCREEN_W / SCREEN_H` | 1.32516 | **1.33333** | exactly 4:3, by construction |

The cost of the correction is one number and it is below the drawing's own error: the symmetric left
inset moves from `(0.5917 − 0.4866)/2 = 0.05255` to `0.05186`, a difference of `0.00065` of body
height — **0.43 px at hero, 0.07 mm on a real device.** The test's symmetry assertion (tolerance
0.002) still holds.

#### Correction two: `hero` is a physical constant, and the glass is sized from the panel

The previous revision fixed `hero: 658px` as a constant in **logical** pixels and then computed the
panel in **physical** ones. Those are the same number only when the display scale factor is 1. At
sf = 1.25 the body is drawn 822 physical px tall, `k` still floors to 1, and the panel sits 320 × 240
inside a glass well 427 × 327 physical — **43 px of black each side and 35 top and bottom**, against
the 11 that §6.1 declares as a checkable invariant. At 150 % the panel fills 62 % of the glass's
width. Nothing in the previous revision's tables could see it, because all three tabulated rows
(1.00, 2.00, 1.25) treated the 1.25 remainder as acceptable rather than as the tell failing.

Two changes, and together they make the whole computation one number on one axis:

**(a) The body's height is `k × 655.751` *physical* pixels.** `320 / 0.48799 = 655.751`, and
`240 / 0.36599 = 655.756` — the same number to four figures, because the well is now exactly 4:3.
That is the body height at which one framebuffer pixel is one device pixel. `k` is a whole number and
the logical length pushed into markup is `k × 655.751 / sf`.

**(b) The glass is sized from the panel, not the panel from the glass.** The previous revision
computed `k` from `0.51999 h × 0.39799 h`, whose ratio is **1.3066** — not 4:3 — so the two ratios the
section exists to correct governed nothing except where the glass sits, and the section's headline
claim described a computation the code did not perform. It was harmless only because the glass
happened to be width-limited at every tabulated scale factor, which is a coincidence of the 0.032 h
bezel rather than a construction. The bezel is what is actually measured, and it is **the same on all
four sides**: `0.51999 − 0.48799 = 0.032` and `0.39799 − 0.36599 = 0.032`, so `0.016 h` per side.

```
BEZEL_PHYS  = 0.016 × 655.751 = 10.49 physical px, four sides, every k, every sf
GLASS_PHYS  = (k×320 + 20.98) × (k×240 + 20.98)
```

At k = 1 that is 341.0 × 261.0 physical, which is where the old 342.2 × 261.9 came from — and now it
is an *output*, so a future re-measurement of the bezel cannot silently flip which axis bounds `k`.
§16.10 asserts it anyway.

```rust
// One source of truth, in physical pixels, because that is the unit fidelity is defined in.
const HERO_PHYS_1X: f64 = 320.0 / 0.48799;   // 655.751
const CHROME_MIN:   f64 = 154.0;             // §9.6, logical: everything that is not the body
const BEZEL_RATIO:  f64 = 0.016;

let sf    = window.window().scale_factor() as f64;
let avail = client_height_logical();          // MEASURED (§9.6), not predicted from the screen
let k     = (((avail - CHROME_MIN) * sf) / HERO_PHYS_1X).floor().max(1.0);

let hero  = k * HERO_PHYS_1X / sf;            // logical length -> .slint
let pw    = next_up_until(k * 320.0, sf);     // logical; see "the f32 tail" below
let ph    = next_up_until(k * 240.0, sf);
window.set_hero(hero as f32);
window.set_screen_w(pw as f32);
window.set_screen_h(ph as f32);
window.set_screen_scale(k as i32);            // the shelf prints it
window.set_too_short(hero + CHROME_MIN > avail);   // with hysteresis, §16.1
```

**This is not a binding loop.** `scale_factor()` and `size()` are window properties set by the
platform and by the user, not sizes the layout decides — provided the Window's own `min-height` and
`preferred-height` are plain constants that never read `hero`. §16.1 states that condition as a rule
because it is the only thing keeping the arrow pointing one way.

| display scale | k on the operator's machine | body, physical | body, logical | glass remainder, physical |
|---|---|---|---|---|
| 1.00 | 1 | 655.75 | 655.75 | **10.49** each side |
| 1.25 | 1 | 655.75 | 524.60 | **10.49** each side |
| 1.50 | 1 | 655.75 | 437.17 | **10.49** each side |
| 1.75 | 2 *(if it fits)* | 1311.50 | 749.43 | **10.49** each side |
| 2.00 | 2 | 1311.50 | 655.75 | **10.49** each side |

One physical size everywhere, one remainder everywhere, and every tell in §6.1 holds. On the
operator's own 2× machine `hero` is 655.75 logical and `k` is 2 — which is what the window already
does today, so the correction costs that machine nothing and buys every fractional-scale machine the
drawing back.

**`k` is decided when the window is shown, and again on `ScaleFactorChanged` and on `Moved` to a
different display. A plain resize never changes it** — only the top margin, and past the floor the
too-short boolean. That is principle 1: dragging a window edge is not a request to redraw the iPod
at a different size. The cost is that a window dragged large enough for a higher `k` does not take it
until the next launch; the shelf's fidelity slot says which `k` is in force so it is never a mystery,
and §17.Q11 puts the trade in front of the operator.

**The f32 tail, and a finding that was wrong in its arithmetic and right in its class.** A reading of
this design held that `k × 320 / sf` stored as f32 and multiplied back by `sf` lands at
319.99999 physical at sf = 1.5, so Skia antialiases the edge columns even under `FilterMode::Nearest`
— because the Skia renderer rounds an image's destination **origin** to whole device pixels only when
the transform is a pure translation and **never rounds the destination size**
(`i-slint-renderer-skia-1.17.1/itemrenderer.rs:434, :530-546`). The renderer fact is correct. The
arithmetic is not: `Coord = f32` (`i-slint-core-1.17.1/lib.rs:104`) and euclid's `Scale<f32>`
multiply is a single correctly-rounded f32 operation, so `f32(320.0/1.5) × 1.5` is **exactly 320.0**.
Swept over `sf ∈ {1.0, 1.25, 1.5, 1.75, 2.0, 2.25, 2.5, 2.75, 3.0, 3.5}` and `k ∈ 1..8`, the
round-trip is exact in **79 of 80** cases.

The one that is not is **`sf = 2.75, k = 7`**, and arbitrary fractional scale factors of the kind
Wayland and X11 hand out (1.09, 1.12, 1.13 …) fail routinely — 327 of 1 204 sampled combinations. So
the defence is adopted anyway and it costs one function: **`next_up_until` walks the logical size up
by ULPs until `(w × sf) ≥ k × 320`**, and the sub-ULP excess falls inside the black glass where
nobody can see it. §16.10's test runs at all ten scale factors and `k` up to 8, **and it goes red
today at 2.75 / 7** — which satisfies the prove-it-can-fail obligation with a case that is real
rather than one that was assumed.

#### Everything else, unchanged and correct

| | ratio of body height | at `hero_phys = 655.75` | check |
|---|---|---|---|
| body width | 0.5917 | 388.0 px | |
| screen | 0.48799 × 0.36599 | **320.0 × 240.0 px** | 50.8 × 38.1 mm, exactly 4:3 |
| glass well | panel + 2 × 10.49 | **341.0 × 261.0 px** | an output, not an input |
| screen top inset | 0.05186 | 34.0 px | = the left inset, ± 0.00065 |
| wheel | 0.3675 | 241.0 px | 38.3 mm |
| wheel top | 0.5215 | 342.0 px | |
| centre button | 0.1228 | 80.5 px | 12.8 mm |
| corner radius | 0.0501 | 32.8 px | |
| **hold switch** | **0.100 × 0.024**, right edge inset 0.055 | 65.6 × 15.7 px | 11 × 2.5 mm, 5.5 mm inset — **published dimensions, a placeholder; §17.Q6** |

**2× in a window would need a 1311 px body and ~1466 px of client height, which fits on no laptop** —
so on a 1× display `k = 1` is the windowed scale and every higher one lives in fullscreen (§12.6).
On a 2× display `k = 2` *is* the windowed scale, for free, because the logical size is the same.

**The drawn iPod appears at three sizes, all the same drawing:** **hero** on the bench; **row** 40 px
in the drawer's Devices list, screen dark, chassis correct; **thumbnail** 24 px on a Parts row for an
iPod resource. `body-height` is the only input, and the two small ones are not framebuffers so they
have no `k`.

### 6.7 Icons are drawn, never typed

**No icon is a font glyph.** Every one is a vector `Path`, from a closed set, sized on the space
scale. This is not aesthetics: the previous window shipped **twelve missing glyphs** to the operator
as empty squares, and the test written to catch it caught two more within the hour.

The set, sixteen: `back` · `close` · `add` · `remove` · `expand` · `collapse` · `power` · `camera` ·
`readout` · `info` · `help` · `check` · `warning` · `folder` · `download` · **`fullscreen`**.
Anything else is a word.

`fullscreen` is the one addition and §12.6 is why. Note also that **`Path` has no dash array** in
Slint — so there are no dashed rules and no draw-on stroke animations in this design, and §7.3's
refused cradle is a **broken** ring (arcs with gaps) rather than a dashed one.

**The glyph test survives and widens**: no source file may contain a non-ASCII character rendered as
UI text unless the font in use is proven to have it. It caught the thing it was written after — the
shipped window built ` · ` (U+00B7) into UI strings with no coverage gate at all — and then caught
it a second time one crate over.

**The sweep reads the model as well as the window.** Half the sentences this font is asked to draw
are written in `eapp-loader`: `inspect::flash`'s verdict, the fact lists behind every Parts row,
`nor::Source::describe`, every `Provenance` line. Four of them joined their lists with U+00B7 and no
gate looked, because the sweep read `tools/ipod-gui/src` and that is the other crate. The line is now
the crate's own shape — `eapp-loader/src/*.rs` is the library the window links and is swept;
`eapp-loader/src/bin/*.rs` is a `main` that owns a terminal, in a font this program neither chooses
nor can interrogate, and is not. `bin/trace.rs` prints `✅` and `⚠️`; both are correct where they are.

---

## 7. The bench

The only surface the program can open on, because there is nowhere else.

```
  1180 × 846 client   ·   1 char ≈ 11 px   ·   1 line ≈ 22 px
 ┌─────────────────────────────────────────────────────────────────────────────────────────────────────────┐
 │                                                                                                         │ ▏ top margin, 24 px, the
 │                                  ┌ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ┐                                  │ ▏ only elastic term
 │                                  ╭───────────────────────────────────╮                                  │ ▏ the CRADLE — 2 px, 10 px
 │                                  │                        ▭▬▬        │                                  │ ▏ outside the body, accent
 │                                  │  ┌───────────────────────────────┐│                                  │ ▏ when startable. Focus
 │                                  │  │                               ││                                  │ ▏ ring 4 px outside that,
 │                                  │  │                               ││                                  │ ▏ in `fg`, so 16 px above
 │                                  │  │                               ││   the glass:  341.0 × 261.0 phys  │ ▏ the body is spoken for
 │                                  │  │        320 × 240, dark        ││   the Image:   k×320 × k×240 phys │
 │                                  │  │        exactly k device       ││   the black:  10.49 px, 4 sides   │
 │                                  │  │        pixels per frame-      ││                                   │
 │                                  ┤  │        buffer pixel           ││├                                  │ ▏ clamp marks, 3 × 28 px
 │                                  │  │                               ││                                  │
 │                                  │  │                               ││                                  │
 │                                  │  └───────────────────────────────┘│                                  │
 │                                  │                                   │                                  │
 │                                  │                                   │                                  │
 │                                  │          ╭─────────────╮          │                                  │
 │                                  │       ╭──╯    MENU     ╰──╮       │                                  │
 │                                  │      │                     │      │   the wheel: 241.0 px            │
 │                                  │      │       ╭─────╮       │      │   96 detents, five buttons,      │
 │                                  │     │   ◀◀   │  ●  │  ▶▶   │     │   all of them the MACHINE's      │
 │                                  │      │       ╰─────╯       │      │   centre drawn 80.5 px; its      │
 │                                  │      │                     │      │   hit region is WheelRing::      │
 │                                  │       ╰──╮      ▶‖     ╭──╯       │   select, already 39 % wider     │
 │                                  │          ╰─────────────╯          │                                  │
 │                                  │                                   │                                  │
 │                                  ╰───────────────────────────────────╯                                  │
 │                                  └ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ┘                                  │
 │                                        press ● to start · cold boot, about 75 s                         │ ▏ the cradle label, 24 px
 │                                                                                                         │ ▏ gap, 16 px
 ├─────────────────────────────────────────────────────────────────────────────────────────────────────────┤ ▏ 1 px line + the 3 px
 │  My 5.5G                                                                          parked · 4 min ago    │ ▏ progress bar, when there
 │  5.5G · 30 GB · black · Apple 25.1.3 · Rockbox 4.0             panel 1× · 320×240 · nearest neighbour   │ ▏ is one
 │  works on a copy of my-5.5g.img                        MENU ›  Devices · Parts · Games · Work · Readout │ ▏ THE SHELF, 88 px,
 └─────────────────────────────────────────────────────────────────────────────────────────────────────────┘ ▏ flush to the bottom
                                                                                                          ▲
                                                                                            the drawer handle, 12 × 96 px
```

### 7.1 The well

Full bleed, `bg-sunken`. It is a recess in both schemes, which is the rule that generates the dark
value rather than a taste. It holds one device and nothing else, ever.

### 7.2 The device, and where it is

**Pinned by its distance above the shelf, not centred in the well.** All vertical slack goes to the
top margin. Consequences, and they are the reason for the choice:

- Growing the window moves nothing.
- Shrinking the window eats the top margin first; when that is gone the layout is at its minimum and
  §9.5 takes over.
- Anything that pushes from the top costs the well its air, not the device its place.

**The bench shows the machine whenever there is one.** That sentence settles a question the previous
revision left open and which reproduced the disease this whole design is a cure for. "One device on
screen" plus a `←`/`→` that switched devices meant that looking at a second iPod either destroyed the
first or left an ARM7 executing behind a panel that was no longer drawn — and §12's opening claim,
*"Running is a state of the bench, not a place"*, is precisely what makes that visible, because a
state of the bench cannot survive the bench showing something else.

So:

- **While there is a machine, the bench draws it and only it.** There is no cradle state for
  "another device is the machine", because there is no way to reach one.
- **Other devices are inspected on the Devices drawer page**, whose rows carry the 40 px drawing, the
  parts list, and `Start` — disabled while a machine exists, with the machine-rule reason
  `My 5.5G is running. Stop it first.` You can read everything about a second device without touching
  the first.
- **`←`/`→` on the bench are therefore unambiguous.** They are the wheel while there is a machine, and
  previous/next device when there is not — because when there is not, there is nothing else for them
  to mean.

**Changing device** (only possible with no machine) cross-dissolves `chassis` and the wheel-markings
colour over `tight` (140 ms) and moves nothing; `hero` is a pushed constant and is **never** animated
(§8.2). The Screen is unbound for the duration of the dissolve, gated on a plain `settling: bool`
that Rust sets true on a change and false 140 ms later.

`Colour::Unspecified` is drawn `#E4E4E2` and never black — drawing an unknown chassis black would
invent a fact about somebody's iPod, and the model already refuses to.

#### A fact is not a reason, and it wraps *(added 2026-08-24)*

The five `Made of` lines — `iPod`, `Drive`, `Built from`, `Installed`, `Writes to` — are a labelled
two-column list inside the row's `Expand`, and a **fact is not §9.4's reason**: a reason elides on
purpose in one line of a fixed 34 px slot, and this list is inside an `Expand` that is already
variable height and has nothing to protect. So a fact that does not fit the column takes a second
line and the row grows.

What shipped did neither. The value was a `wrap: word-wrap` `Text` with `horizontal-stretch: 1` in a
`HorizontalLayout`, and Slint takes a wrapping `Text`'s **unwrapped** width as its preferred one — so
the cell was sized for one line and the second was **clipped with no ellipsis at all**.
`_out/gui/devices.png` drew `works on a copy of my-5.5g.img — nobody has` and stopped, with 20 px of
blank underneath and nothing on screen to say a word was missing. The same sentence draws whole on
the shelf three rows down in the same frame.

**The line count is arithmetic on a measured width**, not `preferred-height`: that answered 24 px for
a sentence that draws in two 16 px lines, so the row would still have been wrong and only *looked*
right, because the 8 px of `spacing` beneath it happened to absorb the difference. A hidden
non-wrapping twin gives the natural width, `Math.ceil` of it over the column gives the lines, and the
row is that many `LINE_LABEL` tall with `clip: true` under it.

`every_fact_this_page_draws_is_drawn_whole` is the gate, and it is the only probe in this window that
answers a **height**: `MainWindow.fact-height` builds the shipped `MadeOfLine` at a drawer page's
measure and reports what the row became. A width probe cannot see this defect at all.

### 7.3 The cradle — where every piece of UI state lives

A **2 px rounded outline** tracing the body's footprint offset outward by 10 px (408.0 × 675.8 px at
`hero_logical = 655.75`, radius 42.8 px), plus two **3 × 28 px clamp marks** at the body's mid-height
on either side. Its geometry is constant. Only its colour and its continuity change.

| state | ring | label (24 px, `body` 14/20, centred) |
|---|---|---|
| startable, never booted | `accent` | `press ● to start · cold boot, about 75 s` |
| startable, parked | `accent` | `press ● to resume · about 3 s` |
| **parked, pair broken** | `fg-dim` | `press ● to cold boot · the parked snapshot no longer matches this drive` |
| a title | `accent` | `press ● to play · there is no boot` |
| first run | `accent` | `press ● to make an iPod · 6.5 MB to download, about 28 MB on disk` |
| **first run, partly done** | `accent` | `press ● to finish making My 5.5G` |
| **booting** | `fg-dim` | `booting · 62 %` — or `booting · 412 M instructions` with no denominator — **and always** ` · press ● to stop` |
| running | `fg-dim` | `running` — or `running · wheel 41 queued` — or, where fullscreen is available and the strip is not drawable, `running · ⌃⌘F for 7× · Esc to come back` |
| working | `fg-dim` | `building · 41 % · fetching Rockbox 4.0` |
| parking | `fg-dim` | `parking · 0.7 of 1.6 GB` |
| stopped | `danger` | `stopped — Lost(0xe19b0000)` |
| **cannot start, one part gone** | `fg-dim`, **broken ring** | `cannot start — my-5.5g.img is not where it was` |
| **cannot start, more than one** | `fg-dim`, **broken ring** | `cannot start — two of its parts are missing · MENU › Devices ›` |
| **cannot start, not a 5.5G** | `fg-dim`, **broken ring** | `cannot start — that boot ROM is a nano-class device` |
| **a control was pressed and there is no machine** | unchanged | `the wheel and the buttons belong to the machine, and there is no machine` — held while the pointer is down |
| nothing mounted | `fg-dim` | `nothing is mounted` |
| focused | +2 px **`fg`** ring, 4 px outside, **instant** | unchanged |

That is the closed set. The label is the one place on the bench that says **what pressing will
cost**, before you press — and the cost is the whole reason it exists: the same gesture is 3 seconds
for a parked machine, 75 seconds for a cold boot, four minutes for a first run, and instant for a
title.

**Six of those rows are new and each one closes a hole.**

- **`parked, pair broken`.** §12.4 is precise about why a broken pair is dangerous — *"pairing
  restored RAM with a drive that kept moving is what produced the intermittent 'connect to computer'
  screen"* — and names `Config::pair_is_whole` as the single place that knows. It then never routed
  that knowledge anywhere, so a device whose image was modified by `ipod-boot put-files` between
  sessions promised `about 3 s` and delivered the failure mode the section was written to describe.
  `pair_is_whole()` is a stat and a one-line string compare; the cradle reads it before it draws.
  **The snapshot is kept, not deleted** — principle: never discard the operator's data without
  asking — and a `Discard the snapshot` row sits on the device's drawer page.
- **`booting … press ● to stop`.** There was no stop control on the bench at all. A user two minutes
  into a 21.5 G iPodLinux boot had an inert object in front of them for the next twenty-one minutes,
  `Esc` would have written a 1.6 GB snapshot of a half-booted machine, and `Cmd::PowerOff` was three
  navigations away on a page the drawer's root row list did not even include. The centre button is
  live during `Booting` and sends `Cmd::PowerOff`.
- **`first run, partly done`** — §10.3, and it is the difference between one iPod and three.
- **the two `cannot start` rows** — §3.3, and the reason there are two is that 24 px of centred
  `body` does not hold two filenames.
- **`not a 5.5G`** — §11.4, and it exists because `inspect::flash()` passes a 1 MiB nano-class dump.

**Accessibility.** The cradle is the `Button`. `accessible-label` is the device's name;
`accessible-description` is the label line; `accessible-enabled` follows startability. The drawn
device itself is `AccessibleRole::Image` with the panel's own description, and announces nothing
about the program.

**Three things the building added to this section (2026-08-21).**

- **`accessible-enabled` is the announcement, never the gate.** Neither the cradle's `Enter` / `Space`
  handler nor the drawn centre button may check `startable`: §7.4 keeps the drawn control live at all
  times, and Rust is the only thing that can say *which part is gone*. Gating the keyboard half and
  not the pointer half made `Return` a dead key on exactly the device §20 item 12 exists for, while
  a click on the same device filed the refusal.
- **The window has to hand the cradle focus, and a `forward-focus` cannot reach it.** The fixture is
  an internal element of a component, so the route is a `public function focus-cradle()` called from
  the window's `init`. Without it the window opens with focus on the root scope, where `Return` means
  nothing, and the one control the whole program is built around is a `Tab` away with no ring on it.
- **The `●` in every label above is a glyph nothing has proved.** U+25CF, on the one line the whole
  bench is built around, in the system UI font, on three platforms — and nothing in `.slint` or in
  Rust can ask whether a font has it (§16.6). §6.7's answer for a symbol is that it is **drawn**, and
  these labels come from Rust, which has no `Path`. Either the wording loses it or the cradle label
  gains a drawn mark beside a shorter sentence. **Unresolved, and it is an operator decision** — the
  table above is the design's own wording and this document is not the place to quietly reword it.

#### And two the model added, both measured rather than argued *(2026-08-23)*

`machine.rs` is this table evaluated — `machine::cradle(press, &Stand)` returns the ring, the
continuity and the label, so the window asks rather than deciding. Building it measured two things
about the table itself that no reading had.

- **`·` is not a character this program may type, and every caption above is built on it.**
  `geometry::GLYPHS` is a closed set of three — `—`, `…`, `§` — and the paragraph beside it says why
  `·` is deliberately *off* it: *"A symbol — `·`, `×`, `›` — is drawn as a `Path`, which
  `ui/bench.slint` does for the shelf's own MENU list."* Rust has no `Path`, and these labels come
  from Rust. So the shipped captions use an em dash and a comma where the table above writes a middle
  dot — `Press the centre button — cold boot, about 75 s`, `Press the centre button to stop —
  booting, 62 %`. **The same sentence in the vocabulary the window can render**, and the alternative
  was four `.notdef` squares on the one line the whole bench is built around. This is the `●`
  question one row down, answered the only way it can be answered before somebody draws a mark.
- **Two rows above do not fit the row they are drawn on, and losing the `●` costs a third.**
  `geometry::CRADLE_LABEL_MAX_CHARS` is **48** characters at the smallest window this program draws
  a device on, and nothing had measured the table against it. Counted:

  | row | as written, with `●` | shipped, with `Press the centre button` |
  |---|---|---|
  | `startable, never booted` | 40 | `— cold boot, about 75 s` — **47** |
  | `startable, parked` | 29 | `— resume, about 3 s` — **43** |
  | **`parked, pair broken`** | **71** | `— no resume, about 75 s` — **47** |
  | `booting`, with a fraction | 32 | `to stop — booting, 62 %` — **47** |
  | `booting`, counted | 46 | `to stop — booting, 412 M instr` — **54** |
  | `stopped` | as long as the `Stop` is | unchanged, unbudgeted |
  | **`a control was pressed…`** | **72** | unchanged; there is no press in it to shorten |

  **The measurement lands on the `●` question above, and it is the argument for keeping it.**
  `press ●` is seven characters and `Press the centre button` is twenty-three, so every press row
  pays sixteen for a glyph nothing can prove — enough to push the counted boot caption from 46 to
  54, over a 48-character row. Five of the seven fit as the table writes them. That is not a reason
  to type a glyph this program cannot verify; it is the size of what a **drawn** mark beside a short
  sentence would buy, which is the thing §6.7 says to do and nobody has done.

  Two of the shortenings are decisions rather than arithmetic. **The booting row is reversed**: the
  table writes `booting · 62 % · press ● to stop` and the stop is the half that elides at every
  width — which is the half the row was *added* for, against a twenty-one-minute boot with no stop
  control at all. Put first, it survives. **And `parked, pair broken` loses its explanation, not its
  warning**: `no resume` is what a person needs before pressing, and *why* the snapshot is not being
  used is a paragraph, which this section already puts on the device's drawer page beside `Discard
  the snapshot`.

### 7.4 The wheel, the buttons and the hold switch — all of them the machine's

**Every drawn control goes to the machine, always, and to nothing else.** With no machine the only
one that does anything is the **centre button, which powers on** — `Cmd::PowerOn`, which on a real
iPod is what a button press on a dead device does. MENU, Prev, Next, Play and the wheel do nothing
until there is a machine.

**Pressing one of the inert controls puts its sentence on the cradle label, held while the pointer is
down and released on pointer-up.** The previous revision put it in shelf row 2 *"for four seconds"* —
which is a self-dismissing timed message that replaces content, i.e. a toast, which principle 5 bans
by name; which made row 2 a fourth tenant beyond the facts line, the Rail line and a bench refusal;
and which meant a refusal like `the disk is not where it was` could be silently overwritten because
somebody brushed the wheel. The cradle already owns a closed set of transient machine-state sentences
and nothing else competes for that line. No timer, no fourth tenant, and row 2's precedence stays
unambiguous: facts, then the Rail line, then a refusal, never a notice.

That inertness is a deliberate deletion. An earlier draft had the wheel drive the drawer when nothing
was running. It is a mode whose meaning flips on a state the user did not set, and it disappears at
exactly the moment a new user has finished learning it. **The drawn controls are the hardware's and
they are never repurposed** — lying about a piece of hardware in a program whose subject is that
hardware is not a trade worth making.

- **96 detents**, `wheel::WheelRing::hit(x, y)` across outer / inner / select radii.
- **Five buttons** by quadrant, `wheel::quadrant(pos)`.
- **The centre button's hit region is `WheelRing::select`, which is what `hit()` already uses.** The
  previous revision also said "12 px larger than its drawing on every side", and the two rules
  disagree with each other and with the model. At hero the wheel's outer radius is 120.5 px, so
  `select` is `120.5 × 0.465 = 56.0` and `inner` is `120.5 × 0.52 = 62.7` (`wheel.rs:158-162`); the
  drawn button is `0.1228 h = 80.5` across, radius 40.3. So `hit()` already treats a region **39 %
  wider than the drawing** as Select — Delta's `extendedEdges` idea, arrived at from the hardware
  rather than borrowed — and "+12 px on the drawing" (52.3) would *shrink* the target it claimed to
  widen, while "+12 px on `select`" (68.0) would swallow `inner` and destroy the 6.7 px moulding dead
  band that `wheel.rs:152-155` documents as deliberate: *"a press landing in it is a press on neither
  — which is the honest answer, since on the hardware it is the moulding."* One rule, and it is the
  one the machine's own hit test uses.
- **The hold switch is drawn, on the top edge**, and it is a first-class concept and not a button:
  it has its own command (`holdsw on|off`), its own field (`Stats::hold`) and its own key (`H`).
  With no machine it sits in the off position, inert, with the reason `the hold switch belongs to
  the machine, and there is no machine`.

**The two real latencies are visible rather than hidden**, because the machine runs at ~24 % of real
time and pretending otherwise makes the drawing lie:

- A press's **release** is held for `MIN_BUTTON_HOLD` = 22 500 000 instructions ≈ **1.6 s of wall
  time** at the window's ~14 M instr/s. So the button depresses on pointer-down — a real button moves
  when your finger does — and, **when there is a machine**, stays depressed until `Stats::buttons`
  clears, which is when the machine actually saw the release. You can watch your click take a second
  and a half to reach Apple's firmware, which is a true and interesting fact about this emulator.
  **With no machine there is nothing to read**: `Stats::buttons` is published by a running machine and
  is 0 while the phase is `Off`, so a power command or a first-run build depresses the button for
  `tight` and restores it. Principle 3's exception says both halves for exactly this reason.
- The wheel drains one event per `click_gap` = 300 000 instructions ≈ 21 ms wall ≈ **47 clicks a
  second**, so a full 96-detent rotation takes about two seconds to deliver, and `MAX_QUEUE` is 96.
  **Momentum scrolling is therefore not viable and is not offered.** The backlog is shown on the
  **cradle label** — `running · wheel 41 queued` — and never on the wheel itself, and
  `input_dropped > 0` turns that clause `warn`, because a refused step is a lie about what you did.

#### Built 2026-08-24, and three corrections the building produced

**The gate is not in the markup, and that is the change worth naming.** The first version of this
put `if (root.has-machine) { …the machine… } else { …the sentence… }` on the ring and on the hold
switch, in `ipod.slint`. That is the drawing deciding two things it cannot see: *whether the wheel
reaches anything* and *what to say when it does not*. It is the same defect as the two refusal
sentences that used to be literals in that file, one paragraph up — and it broke in the same
direction the moment there was a machine to be wrong about. **Every drawn control now raises its
callback on every press**, and `machine::Life` decides where it goes. `has-machine` survives as what
§7.3 calls *the announcement rather than the gate*: it is the hold switch's `accessible-enabled` and
nothing else.

**The refusal is held while a KEY is down as well as a pointer**, which this section did not say
because §16.8's rows were not built when it was written. `M` on a bench with no machine is not
*nothing happens*; it is §7.4's sentence on the cradle label, arriving and leaving with the key.
§14.1 is the reason and it does not distinguish between a finger and a keystroke. The one row that
is **not** refused is `← →`, because §16.8 gives it a second job — see there.

**Measured: at `hero` the elision drops the noun.** `machine::NO_MACHINE` is 71 characters and this
section's own note predicts `the wheel and the buttons belong to the machine, and t…` at
`CRADLE_LABEL_MAX_CHARS` = 48. That is the *narrowest* window; at hero the label is wider and
`_out/gui/bench-refused.png` reads

> the wheel and the buttons belong to the machine, and there is no…

— which stops one word short of the word the sentence is about. The first clause survives, which is
the trade this section already accepted, so nothing is changed here; it is recorded because the
predicted elision and the drawn one are not the same string and only one of them had ever been
looked at.

### 7.5 The shelf — 88 px, three rows, flush to the bottom, full width

`bg-band` gradient over a 1 px `line` top rule. Padding 24 left and right, 12 top and bottom. The
3 px linear `accent` progress bar draws **on** the top rule when there is progress, and therefore
costs no height.

**The shelf narrows with the drawer, and the drawer runs above it.** The drawer occupies the full
client height *minus the shelf*, right-anchored, and the well and the shelf both narrow to
`W − 420`. The previous revision described the drawer only in terms of the well, and both readings of
the gap broke something: a full-height drawer covers the shelf's right 420 px, which is covering
rather than pushing and principle 5 forbids it; a drawer sitting above a full-width shelf leaves row 3
with 412 usable px to hold *both* a ~72-character write-target sentence **and** the 47-character menu
list, which do not both fit and one of which "never goes away".

The resolution is one line: **while the drawer is open, row 3's trailing slot is empty** —
`visible: false`, so §16.3 keeps its cell and nothing moves — because the menu list is a route into a
drawer you are already inside. Row 3's leading slot then keeps the full narrowed measure.

| row | h | leading | trailing |
|---|---|---|---|
| 1 | 26 | the name of the thing on the bench (`title`) | the state, and time since (`label`, `fg-dim`) |
| 2 | 20 | the facts, **or the reason**, or the current Rail line (`body`) | the fidelity fact (`mono` 13) |
| 3 | 16 | **the write target, permanently** (`label`) | `MENU ›  Devices · Parts · Games · Work · Readout` — **empty while the drawer is open** |

**Row 3 never goes away.** It is `write_target()` and it is the one line standing between an
afternoon and somebody's only image of an iPod they own. **Four sentences about a drive that
resolves, not three**, because `work_on_copy`'s `None` and `built_from`'s `Some`/`None` are two
different questions and the previous revision's prose fused them into one and got it backwards —
and two more for a device whose drive does not resolve:

```
works on a copy of my-5.5g.img                                             ← Some(true)
works on a copy of my-5.5g.img — nobody has said, so a copy it is          ← None, always a copy
writes to my-5.5g.img — we built it from iPod_25.1.3, so it is regenerable ← Some(false), built_from Some
writes to rockbox-test.img — you chose this, and we did not build it       ← Some(false), built_from None · warn
no drive yet — nothing will be written                                     ← names no drive
cannot write to mine — it is not in the library any more                   ← names a drive the library dropped
```

Which of the first four appears is `d.work_on_copy.unwrap_or(true)` and nothing else; `built_from`
decides only whether the `warn` colour appears on the explicit-write case.

**The last two are about the drive rather than about the choice, and they are two states, not one.**
A device that names no drive is *unfinished* and is finished by making one; a device naming a drive
the library no longer lists is *broken* and is repaired by finding it. Sharing a sentence between
them read as reassurance about a device the shelf was refusing in the same breath. `cannot write to`
carries no `warn` colour: `Settings::run_device` refuses that device, so there is no write to raise
an alarm about, and the alarm is raised where the remedy is — the cradle names the missing part.

**The sentence and the colour are one value.** They were two functions resolving the device's drive
two different ways, and from a device's second launch on they disagreed — in the direction that costs
somebody an afternoon. `write_target()` returns both out of one `match` now, so the arm that says
`writes to` is the arm that sets the flag. Seven tests hold that behaviour, one of which puts its
device through `render` + `parse` first, because a fixture carrying a shape no save produces is how
the defect stayed green for as long as it did.

**Row 3 has a long and a short form, and the verb is never what goes.** At the narrowed measure the
qualifier after the em-dash is dropped and lives on the device's drawer page instead. `works on a
copy of` and `writes to` are the first words on the line, so even a hard truncation preserves the
dangerous one.

The right-hand slot of row 2 always carries a number a bug report can quote:
`panel 1× · 320×240 · nearest neighbour`, or where `k` and the display scale differ,
`panel 1× · 320×240 physical · display scale 125 %`.

The left slot of row 3 is the program's entire discoverability and is also a control: clicking it
opens the drawer. That, the 12 px handle, `⌘\`, `⌘,` and the menu bar are the four routes in — `Esc`
is not one of them (§4).

### 7.6 The shelf states, the drawer explains

Row 2 is where a bench-level refusal goes, in `fg` rather than `fg-dim`, as **one elided line** with
a `why ›` control at the trailing edge that opens the drawer at the page owning the refusal — where
the full `Verdict::No.why` paragraph and its `Fix` live, under the control that caused them.

**This is a compromise and it is worth naming as one.** Principle 4 wants the reason next to the
control. `Verdict::No.why` runs 2–4 sentences (~55 words), which is three lines at the shelf's
measure. A shelf tall enough to hold that is 134 px. §17.Q1 puts the trade in front of the operator
with numbers that the §6.6 correction has already moved once.

---

## 8. Motion

**Slint 1.17 has no spring easing.** It has 32 named curves plus an authorable
`cubic-bezier(a, b, c, d)`, and overshoot is genuinely available — `ease-out-back` *is*
`cubic-bezier(0.34, 1.56, 0.64, 1.0)`, and `ease-out-elastic` and `ease-out-bounce` are computed in
Rust. So "no springs" does not mean "no overshoot", and the spring vocabulary is **retired outright
rather than kept as a fiction**. Four names, written into `tokens.slint` so nobody ever picks a raw
bezier:

| name | duration | curve | used for |
|---|---|---|---|
| `tight` | 140 ms | `ease-out-quad` | press, hover, cradle colour, device cross-dissolve, drop-band content |
| `gentle` | 320 ms | `ease-out-quart` | drawer in and out, the device's `x`, Expand height, scroll-into-view |
| `lively` | 260 ms | `ease-out-back` | **drawer page depth only** — nothing else, ever |
| `linear` | data-driven | `linear` | progress. It is data; a bar that overshoots is lying |

### 8.1 The complete transition list

1. **Drawer in / out.** The well and the shelf narrow to `W − 420`; the device's `x` animates to the
   new centre, `gentle`. **Only `x` animates.**
2. **Drawer page depth.** The current page slides left, the new one in from the right, `lively`. This
   is the borrowed iPod motion, and it is inside a 420 px panel where no framebuffer can be.
3. **Expand.** Height `gentle`; content fades in **after** the height settles, so text never slides.
   Inside a Scroll, an Expand that opens below the fold **scrolls its own top edge into view**
   (`gentle`) rather than letting the rows below travel under a stationary cursor — §11.3, and it is
   principle 2 arrived at through a mechanism §16.3 alone does not cover.
4. **Device change.** `chassis` and the markings colour cross-dissolve, `tight`. The Screen is
   unbound for the duration. Only reachable with no machine (§7.2).
5. **Cradle state.** Colour and ring continuity only, `tight`.
6. **Wake, Off → Booting — and it is the showpiece precisely because it has no geometry in it.** The
   glass's `Image` becomes visible and its opacity ramps 0 → 1 over 220 ms `ease-out-quad`: a
   backlight coming up, not a window opening. The cradle's `accent` fades to `fg-dim`. The label
   crossfades. The shelf's rows change text. **Nothing is tweened between two geometries, because
   there is only one geometry** — the device was already the right size in the right place. Reversed
   on park.
7. **Press.** The drawn control depresses on pointer-down, `tight`; with a machine it restores when
   `Stats::buttons` clears, up to 1.6 s later, and without one it restores after `tight` (§7.4).
8. **Gauge value change.** The row's background flashes `accent` at 12 % for 140 ms, `linear`, and
   ends. **The digits never tween.** A number that animates is a lying instrument.
9. **Focus ring.** Instant, always. A focus ring that animates is a focus ring you lose.
10. **Drop band.** The shelf's rows 1 and 2 crossfade to the identification, `tight`. **Nothing
    moves** — §11.4.

### 8.2 Two rules with teeth, both earned from measured defects

1. **Nothing that animates may feed the Screen's geometry.** The shipped carousel animates
   `body-height` from `hero * 0.55` to `hero` over 320 ms on every selection change, and every
   dimension in `ipod.slint` is a fraction of it — so the live framebuffer is drawn at a
   continuously varying non-integer scale for the whole animation. Here `hero` is a length pushed in
   from Rust and changed only on a display change, the Screen's size is two more pushed lengths, and
   none of the three is ever the target of an `animate` block.
2. **No overshooting curve may touch a property the Screen's geometry reads.** Overshoot momentarily
   *over*scales the panel. `lively` is confined to drawer page depth, which is inside the drawer and
   reads nothing of the device.

### 8.3 Never propose an animation that does not end

The winit event loop idles at `ControlFlow::Wait` and costs nothing — but `about_to_wait` calls
`request_redraw()` for **every window with an active animation**, and partial rendering is off by
default with the Skia renderer. So one perpetual spinner, pulse, shimmer or breathing highlight pins
the main thread at display refresh with **full-window** repaints, indefinitely, while an ARM7 is
being emulated at 24 % of real time.

That is the mechanical reason there is no spinner in this program. Working is a Rail line with a byte
counter; progress is a value that moves when data moves; a boot with no denominator is an
instruction count that moves.

### 8.4 Reduced motion

**Slint has none** — a grep for `reduced.motion` across `i-slint-core`, `i-slint-common` and
`builtins.slint` returns nothing. It is read per platform in Rust — macOS
`NSWorkspace.accessibilityDisplayShouldReduceMotion` (`objc2-app-kit 0.3.2` is already in the tree),
Windows `SPI_GETCLIENTAREAANIMATION`, otherwise false — into a single global `Motion.scale: float`,
1.0 or 0.0. Every duration is written `Metric.gentle * Motion.scale`. One global, one multiplication,
no duplicated markup. **The structural change still happens; only its animation does not** — including
scroll-into-view, which jumps rather than glides.

---

## 9. The five states, everywhere

Every surface specifies all of them. A surface that only specifies the happy one is not designed.

### 9.1 Empty

Never a bare "nothing here". It says what the surface is for and offers the one action that fills it.

| surface | empty |
|---|---|
| bench | §10 first run, once; thereafter the ghost iPod, `No devices yet`, cradle label `press ● to make an iPod`, no welcome copy |
| Devices | one non-interactive `fg-disabled` row `No devices yet.` above the always-present `+ New device ›` |
| Parts | **all six groups present**, each with its heading, its count `0`, its verbs, and one `fg-disabled` line naming what belongs there |
| Games | `No titles. A title is a .ipg file — drop one anywhere on this window.` **and**, when no `Installer` is filed, `A title's imports are matched against RetailOS's framework table, so inspecting one also needs Apple's firmware. Fetch… ›` |
| Work | `Nothing is happening. Fetches, builds and installs report here.` |
| Readout | **every heading and every row present, each value `—`**, plus `No machine is running. These are the counters a running one publishes.` |
| Settings | never empty; three rows, always |

### 9.2 Working

An inline Rail entry naming what is happening and **against what**, with real bytes:
`fetch  Apple's firmware — iPod_25.1.3.ipsw  ████████░░  4.1 MB of 6.5 MB`. Never a spinner (§8.3).
Long tasks are cancellable and §12.7 says what cancelling costs.

The bench mirrors it in one line: the cradle label carries `building · 41 % · fetching Rockbox 4.0`
and the shelf's top rule carries the bar.

### 9.3 Failed

**Stays until dismissed.** Says what was attempted, what happened in the program's own words, and
what to do next — with the next step as a **real pressable control**, never prose. **Ten classes**
*(the prose said "Seven" over a table of nine until 2026-08-21, and the tenth is the row below)*:

| class | example wording | next step |
|---|---|---|
| **network** | `Apple's server did not answer.` | `Retry` · `Provide a file…` |
| **not served** | `Apple no longer serves this release (403). Five of the 71 are refused; that is a fact about Apple's servers, not about your network.` | `Provide a file…` |
| **verification** | `The SHA-256 does not match the one on record. That is interesting and should not be shrugged off.` | `Retry` · **`Copy the details`** |
| **incompatible** | `Verdict::No.why`, verbatim | the `Fix` — one press, or two where it detaches a resource (§11.3) |
| **space, pre-flight** | `<dir> needs 95 MB to build in — 28 MB for the drive and 67 MB of room to work in — and /Volumes/Work has 51 MB free. Nothing has been written.` | `Choose a folder…` |
| **space, mid-write** | `Stopped at 41.2 GB. my-5.5g.img.part is 41.2 GB and cancelling deletes it.` | `Choose a folder…` · `Cancel` |
| **volume** | `That folder is on a FAT32 volume. FAT32 cannot hold a file larger than 4 GiB and has no sparse files, so an 8 GiB drive image would be written in full and would stop at exactly 4 294 967 296 bytes.` | `Choose a folder…` |
| **permission** | the path, and what to change | `Reveal` |
| **tool missing** | `7z is not on the path, and it is what this step runs.` | — · **a named command in `mono`**, one per tool |
| **missing** *(added 2026-08-21)* | `my-5.5g.img is not where it was — /Volumes/Work/my-5.5g.img.` | `Provide a file…` · `Devices` |

**`missing` is the tenth and §20 items 1 and 12 are why.** §3.3's refusal — *a part of this device
is no longer on disk* — is the one failure this program can already produce, and none of the nine
covers it. Filing it under `permission` would be the program asserting a fact about somebody's
filesystem it did not observe.

**Two things about it that are true today and are worth saying rather than discovering.** First,
`tool missing` is the one class with **no** next step at all: there is no control this program could
draw that installs 7-Zip, so what it carries instead is a command a person can paste, in `mono`,
under the paragraph. Second, `missing` currently resolves to **zero live controls**: `Provide a
file…` needs a file picker or a drop target and this build has neither, and `Devices` needs
a drawer page that is not built. Both are drawn **disabled with their reason** per §14.1. That is
the honest state of the first failure anybody will see, and §17.Q3 is the operator's call that
changes it.

**Two labels got shorter, and it is the same measurement §9.4 records** *(2026-08-23)*. Six of the
ten classes offer **two** next steps and `geometry::RAIL_NEXT_W` makes each one exactly half the
block, so a label is drawn in the same 146 px a reason is. `Provide the file yourself…` and
`Choose somewhere else…` did not fit: they drew as *Provide the file you…*, losing their own
trailing `…`, which is the one character that says a picker opens. They are `Provide a file…` and
`Choose a folder…` — §11.4 already calls the same acts `Provide…` and `Add a dump…` on Parts, so
this is that name with the object put back rather than a new vocabulary.

**Both of them carry a real escape hatch now, and three classes had none at all.** §9.4's rule for a
project state is *say what does work, and always name the escape hatch* — and after one retry
`verification` offered `Provide a file…` and `Copy the details`, both disabled, both with
nothing behind them; a live `403` offered `Provide` alone on the same terms; and `permission`
offered `Reveal`. Each of those is a person told *we have not built this* and given nothing else.
The commands exist in `ipod-boot` today: `ipod-boot firmware get <family>` lands the bundle in the
cache this program reads, which is what a picker would have done; `ipod-boot firmware cache
--verify` hashes every cached bundle and prints exactly what `Copy the details` would copy; and
`IPOD_EMULATOR_DATA=<path>` is what a person does after being shown a folder they cannot write in.
`Retry` still carries none, and that is a different kind of absence: `firmware::download` **is**
`curl`, so naming a command would be the phantom route in its original shape.
`no_failure_class_is_a_dead_end_in_this_build` sweeps all ten classes across three retry counts.

**`volume` is a class because free bytes are not the only thing a filesystem can refuse.** Unpack the
release zip onto a FAT32 USB stick — which is what a stick that also has to work in a car is — and
`settings.rs`'s first data-directory branch puts `data/` beside the executable, on FAT32. Then two
independent things go wrong that a free-space check cannot see: the 16 777 216-sector image is written
in full rather than "about 28 MB on disk", and the write dies at 4 GiB with an OS error that is none
of the other classes. **Query the target filesystem before the plan is drawn**, not after.

**`space` is split because a mid-write failure has a number and a consequence and the pre-flight one
has neither.** `Nothing has been written.` is true before the first byte and false 41 GB in, and
§12.7's cancellation contract needs to say which file it is about to delete and how big it is.

**`Copy the details` replaced `Report it`.** There is no network reporting path in this program, no
issue URL in this design, and a visible control that does nothing is the same class of defect this
document indicts twice — two dead tabs and twelve missing glyphs. `Copy the details` puts the release
name, the URL, the recorded size and SHA-256, the computed size and SHA-256, and the platform string
on the clipboard, which is what a bug report needs and is the same mechanism §12.8 already uses.
**`Retry` counts**: after the second mismatch it is replaced by `Provide a file…` with
`two downloads of this release have failed the same check; that points at the file, not the
connection.` — because `firmware.rs`'s present-but-wrong path prints "already here but does not
verify — downloading again" and will loop for as long as a mirror serves the wrong bytes.

Three external tools each gate a different capability and each gets a **named remedy**: `curl` (every
download), `7z` (ZeroSlackr only), `ffmpeg` (GIFs only).

**Failures accumulate on the Work page and are dismissed individually**, which is what stops one
scrolling away unread. The bench shows a one-line summary and never has to hold two.

### 9.4 Disabled

Visible, `fg-disabled`, non-interactive, and **carrying its reason on focus as well as hover**. §16.5
is the construction; it is not optional, because Slint forcibly clears `has_hover` on a disabled
`TouchArea` and a disabled `FocusScope` refuses focus *even programmatically*.

**Two kinds, worded differently**, because greying them identically makes both ambiguous and defeats
the entire justification for principle 4:

| | says | example |
|---|---|---|
| **a machine rule** | *this cannot work, ever.* State the rule; teaching it is the point | `That image's data partition is FAT32 type 0x0C, the LBA form, and ipodloader2 reads only 0x0B — it will report "No valid paritions found!". Both are legitimate FAT32; drives off real iPods are 0x0C and drives built here are 0x0B.` |
| **a project state** | *this is not finished, by us.* Say what does work, and **always name the escape hatch** | `iPodLinux boots — both partitions found, the root mounted, /bin/init run, no ATA error anywhere — and then ZeroLauncher stalls at "Finishing Up…".` `ipod-boot install-linux` **builds that drive if you want to look at it.** |

A machine rule carries a **`Fix` button**; a project state carries a **command in `mono`**. No new
colour, and you can tell them apart at a glance.

**A `Fix` that names a value the picker disables is itself disabled**, wearing the same reason and the
same escape hatch — §11.3, and it exists because the two surfaces contradicted each other on the first
refusal a curious user hits.

#### A reason is one clause, and the clause has a width *(added 2026-08-23)*

The examples above are paragraphs, and the slot is **34 px with `overflow: elide` and no wrap**. So
what those two sentences actually drew was `That image's data partition is FAT32 type 0x0C, th…` —
and §9.4's whole argument for disabling a control rather than hiding it is that the control says why.
A reason you cannot finish reading is not doing its job.

**The rule: every sentence in this slot is one clause that fits the slot it is drawn in.** Not
wrapped, not two lines, not a taller slot. §17.Q1 already made this trade once, for the shelf: *46 px
of permanent chrome to hold a paragraph is a bad trade*, and §9.6's whole vertical budget is built on
fixed row heights that a wrapping reason would make variable. The same answer applies one level down.

Four slots draw one: §9.3's next-step pair is **146** (`geometry::REASON_MEASURE`), §11.4's group
verbs **180** (`PARTS_VERB_W`), a Parts or Devices act **324** (`ACT_MEASURE`), a Settings row or a
`Field` **372** (`PAGE_REASON_MEASURE`, which is `REFUSAL_MEASURE`).

**A `Next` refusal is still written to 146**, because `rail::Next::reason` alone is drawn in three of
the four and a sentence written to the column it happens to be in today elides the first time it is
reused one column over. **That is the only sentence class with that property**, and holding the rest
to it was the half of the rule that did not survive contact: §11.3's `consequence` is read once, at
one control, before one press, and no arrangement of English says what a destructive act costs in
146 px — held to that number `parts::remove_consequence` could not name what §11.4 asks of it. So the
budget is the slot, and the sweep carries the slot with the sentence.

**And the two 372s are not reached the same way, which cost a sentence.** A `Field`'s row has no
padding of its own, so its `ReasonSlot` is the whole page body; a `Pressable` in the same column
indents by its own `pad`, which every use site sets to `page-margin`. On the Composer's identity page
they are four rows apart, and `composer::Lock`'s
`Read from the dump; a device's identity is the ROM's, not ours.` draws **whole** under `Serial` and
elides to *…not …* under `Model` — read it off `_out/gui/composer-ipod-dumped.png`. The difference is
kept, because a reason lines up under the label it is about and the two labels are genuinely not at
the same x; what changed is that `Field` used to reach `0px` by omission and now states it, both
primitives report `reason-measure`, and
`the_two_reason_slots_differ_by_exactly_the_pad_that_indents_one_of_them` builds one of each and
reads the renderer back. Same `pad`, same slot; different `pad`, exactly `2 × page-margin` apart.

**Measured, never counted.** `Add a dump…` is wider than `Synthesise…` at equal length (§17.Q12), so a
character budget is not a width. `MainWindow.reason-probe` is a `Text` at `label-size` /
`weight-label` with the eliding taken off, and
`every_reason_this_window_draws_fits_the_slot_it_is_drawn_in` sweeps every sentence the four
producers — `rail.rs`, `parts.rs`, `devices.rs`, `settings_page.rs` — can word into it, measures each
one through the renderer and fails on any that is wider. **Thirty-nine sentences**, and it prints
each one with the share of its own slot it spends, widest share first, so the ones an edit away from
eliding are at the top. The fullest spends 139 of 146 px.

**What this costs, and where the long form went.** Nowhere on screen. There is no `why ›` route to
put it behind: that control is the bench's (§7.6) and its job is to open the drawer page owning a
refusal — and these refusals are already *on* their drawer page, so it has nowhere to take you. The
long form lives in each producer's doc comment and in this section, which is where somebody who wants
the argument reads it. The drawn half is the half a person standing at the control needs:

**The consequences, which were the half nothing measured.** `ReasonSlot` draws
`enabled ? consequence : reason` — one slot, two producers — and only the refusal was ever swept.
Sixteen of thirty-eight sentences were over the moment the other one was. Every one of them was
spending the room on a fact drawn whole a few rows above the control, which `devices::start_row` had
already written the rule for and nobody had applied to its siblings: *putting them in this slot as
well would spend the one line the control has on a fact the reader can already see and lose the one
they cannot.* So they were **re-sited, not widened** — the slot costs §9.6 nothing and a taller one
would cost it 16 px per row on every page:

| was, and what it drew | is | and where the rest of it already is |
|---|---|---|
| `The entry goes. Its iPod A446, seed 6182160 and its drive stay in the library, and neither file is deleted.` — 545 px in 324, drew *…and its drive …* | `The entry goes; both files stay.` | `made_of`'s first two lines, `iPod  Black 5.5G, filed as A446, seed 6182160` and `Drive  my-5.5g`, at 372 px in the same open body |
| the same, with a park — **1124 px**, about a quarter of itself | `The entry goes; its iPod stays. The park goes, unlisted.` | the Snapshots group in Parts, with the sizes |
| `1 still name it — My 5.5G. They will say so rather than quietly losing it, and the file itself is not deleted. This iPod is a recipe: only seed 6182160 regenerates its identity.` — 849 px in 324 | `No file is deleted. Still named by My 5.5G.` | the `Seed` fact in the row's own open body — see §11.4 |
| `1 parked machine is forgotten. The RAM and the frozen drive behind them are not deleted by this — nothing in the library records where they are.` — 772 px in **180** | `Forgets 1. Files stay, unlisted.` | the Snapshots rows above it |
| `Changes the recipe to build from Apple's firmware instead.` — 310 px in 146 | `Rewrites the recipe.` | the button it is under, which says *build from Apple's firmware instead* |
| `Runs this step again, from the beginning.` — 219 px in 146, drew *…again, fro…* under a live blue `Retry` | `Runs the step again.` | — |

| was, and what it drew | is |
|---|---|
| `there is no file picker in this build yet, and nothing here accepts a dropped file` — 411 px, drew *…and not…* | `no file picker in this build` |
| `there is one palette in this build and nothing keys on a scheme, so the control would write a preference no pixel reads` — 623 px, drew *…so t…* | `one palette in this build` |
| `nothing on this page reaches a fetcher yet — the only download this build starts is the first run's own plan` — 558 px | `no fetcher on this page yet` |
| `every download in this program goes through curl, and it is not on this computer` — 423 px | `no curl on this computer` |
| `My 5.5G is running. Stop it first.` — 168 px, drew *…Stop it f…* | it kept the imperative; see below |

**The last one was shortened to `My 5.5G is running` and has since been given back its second
sentence**, and that is what the per-slot budget bought. 168 px was measured against 146, and 146 is
§9.3's next-step pair — a column `devices::running_rule` is drawn in **none** of. `edit_row`,
`remove_row`, `start_row` and `parts::inventory`'s `held` all draw it at **324**, where 168 fitted
with 156 px to spare, and §11.4 and §7.2 of this document had specified the long form the whole time.
One budget applied to four columns is how a document and its implementation came to disagree about a
sentence neither of them had measured in the right place.

It is still the shape to copy where a sentence carries the operator's own words:
` is running. Stop it first.` is about 148 px and the rest of the slot is the **name's** — about 176,
thirty characters or so. A device somebody called `Rockbox on a 5G, second try` still fits; a longer
one elides, and that is right. This window does not shorten a person's name for their own iPod, it
just does not spend the line on its own prose first.

**And the column has to be a column.** A reason budget is arithmetic about a row that does not obey
it unless both halves of a two-control row carry the half-share as a floor — otherwise the shrink
gives one half's pixels to the sibling whose `mono` escape hatch is wider, and the reason that fits
in theory elides in fact. `geometry::PARTS_VERB_W` and `geometry::RAIL_NEXT_W` are those floors; both
were measured off `_out/gui/*.png` after the sentences were already short, because a page is where
this is visible and no assertion in the suite could see it.

**Not yet applied to `composer.rs`**, which is the fifth producer of §9.4 sentences, words §11.1's
and §11.2's locks, and is the page the two 372s disagree on. Read off `_out/gui/composer-ipod-dumped.png`:
`Read from the dump; a device's identity is the ROM's, not ours.` draws as *…not …* under both locked
pickers, and `composer::NO_CLIPBOARD` as *this build has no clipboard, so there is nowhere for the
co…* — the same sentence, built the same way out of `rail::Next::CopyDetails`, that the Settings page
had. Same defect, same fix, not done here.

#### The escape hatch gets the block, because a command cannot be reworded *(added 2026-08-24)*

The paragraph that used to end this section said the `mono` hatch was **still elided**, *"named
rather than left to be discovered"* — 30 characters of monospace in a 180 px column with nothing to
shorten. Naming a defect is not fixing one, and what it shipped as is in
`_out/gui/work-failed.png`: the verify card drew `Provide a file…` and `Copy the details` side by
side, and under them `ipod-boot firmware get <family>` and `ipod-boot firmware cache --verify`
rendered as the **byte-identical** `ipod-boot firmwar…`. Two different commands, one string, neither
of them typeable — and §9.4's rule for a project state is that it *always names the escape hatch*.

**Measured**, through `MainWindow.hatch-probe` — a `Text` at `Metric.mono-family` /
`Metric.mono-size`, which is `ReasonSlot`'s own face for that line and not the label face
`reason-probe` uses:

| command | wants |
|---|---|
| `ipod-boot firmware cache --verify` | **259 px** |
| `ipod-boot firmware get <family>` | 243 |
| `IPOD_EMULATOR_DATA=<path>` | 196 |
| `ipod-boot rockbox-install` | 196 |
| `ipod-boot install-linux` | 181 |

Every one of them is over `REASON_MEASURE` 146, and three are over `PARTS_VERB_W` 180. **The rule
above does not reach this and cannot be made to.** *Every sentence in this slot is one clause that
fits the slot it is drawn in* is a rule about English, and a command is not English: `ipod-boot
firmware cache --verify` has to be typed exactly, so there is no shorter true version of it.

**So the column moves instead of the sentence: a control that names an escape hatch is drawn at its
block's full width, and a pair that contains one stacks.** `ui/rail.slint`'s `NextStep` and
`ui/parts.slint`'s `GroupVerb` are the two extractions that make that possible — one control, two
layouts — and both gate on `escape-hatch != ""`. Stacked, a `Next` gets `ACT_MEASURE` 324 and a
group verb gets `PAGE_REASON_MEASURE` 372, which holds the longest command with 65 px to spare.

It costs 78 px on a failure block whose two next steps both refuse, and the same on the three Parts
groups that offer a `Fetch…`. That is the trade §17.Q1 refused for the *shelf* — 46 px of permanent
chrome for a paragraph — taken here for the opposite reason: this is not permanent chrome and not a
paragraph. It is the one line on the page whose entire job is to be copied into a terminal, on a
control that has just refused, and half of it is worth nothing at all.

`every_escape_hatch_this_window_names_is_drawn_wide_enough_to_read` is the gate. It sweeps all three
producers — `rail::Next::escape_hatch`, `parts::Group::fetch_route` and `compose::{Os,Loader}` — and
fails on any command wider than the stacked slot it lands in. Proved red by pinning `stacked` to
`false`: seven of nine go over, the longest at 259 px in 146.

### 9.5 And a fifth, because the alternative is the 560 px bug again

> **This pane needs no startup suppression, and the note here used to say it did.** `too_short` did
> go true on every launch, for one event, on a display comfortably tall enough: Slint's
> `adjust_window_size_to_satisfy_constraints` clamps a not-yet-known size up to the declared minimum,
> so the window is *created* at `MIN_HEIGHT` and the first `Resized` carries that — 400, a size the
> window is resized away from before it is ever mapped. This note prescribed suppressing that first
> event. **Do not build that**: the height is now taken from `winit::Window::inner_size()` rather
> than off the `Resized` payload, so the first answer is already the right one and a suppression rule
> would swallow a legitimate event. Measured with `IPOD_LAYOUT=1`, 2026-08-21: one on-screen block at
> startup, `measured 846.0`, no *too short for 1:1*.
>
> The guard is `the_fit_is_computed_from_the_size_the_platform_reports`
> (`tools/ipod-gui/tests/startup_fit.rs`), which launches the real window — the only kind of test
> that can see this at all.

**The display is too short to show the panel at 1:1.** This is a designed state, not an overflow, and
it is reachable on real hardware people own — 1280 × 800 and 1366 × 768 can never satisfy it at any
scale factor, because the body alone needs 656 physical pixels and neither display has 810 of usable
window height.

The threshold is `hero_logical + 154 > client_logical`, evaluated in Rust from the **measured**
window (§9.6, §16.1) with hysteresis, and re-evaluated on `Resized`, `Moved` and
`ScaleFactorChanged`. When it is short, the well is replaced by — and this is **built**, as
`ShortPane` in `ui/bench.slint`; the drawing below is what `_out/gui/bench-too-short.png` shows:

```
 ┌─────────────────────────────────────────────────────────────────────────────────────┐
 │                                                                                     │
 │            This window is 735 pixels tall. The iPod at 1:1 needs 810.                │
 │                                                                                     │
 │        Drawing it here would throw away part of every frame, so it is not            │
 │        drawn. Nothing is wrong with your files and nothing is missing —              │
 │        the press that was on the centre button is the row below.                     │
 │                                                                                     │
 │      ▓▓▓▓▓▓▓▓▓▓▓  Press here — cold boot, about 75 s  ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓   │  ← 44 px, the material,
 │                                                                                     │     the cradle's own
 └─────────────────────────────────────────────────────────────────────────────────────┘     label and callback
        └── 420 px, `Geometry.short-measure` = `min-width − DRAWER_W − 2 × WELL_AIR`:
            the widest measure that cannot overflow the narrowest well this window has
```

**The primary row is the whole point of this state and the previous revision did not have it.** The
well is the only thing that contains the drawn iPod, and the drawn iPod's centre button is the only
start affordance in the entire program. Replace the well with a paragraph and shelf row 2 still
reads *"The centre button makes one: a 5.5G, 30 GB, black"* — pointing at a control that is not
drawn — while the only offered control was `[ Fullscreen ]`, which §12.6 disabled on any phase but
Running with the reason *"there is no machine running, so there is nothing to show."* The advertised
route was a phantom and the offered route was dead, on the one display class this document
explicitly designs a state for. So:

- **The panel carries a real 44 px primary Row** wearing the material — §6.5 use 3, *the one primary
  row on the too-short bench* — whose callback is exactly the centre button's and whose label is the
  cradle's own sentence. One source, so they cannot disagree, and `Space` / `Enter` reach it because
  it is the pane's primary action. `Bench::focus-cradle` routes to it below the threshold, so a
  window that opens on a 1280 × 800 display opens with the press focused rather than with focus on a
  fixture that is not drawn.
- **One tail, two prefixes**, which is what *one source* had to become in the building. The caption
  is one sentence in two halves — where to press, and what pressing costs — and only the first is a
  fact about the surface. `press ● to make an iPod` was written when `●` was to be a drawn mark
  inside the label; the shipped cradle spells it *Press the centre button*, and that on a pane which
  draws no centre button is this section's own indictment two inches lower. So `main::Press` is the
  prefix — `Press the centre button` / `Press here` — and every tail under it is written once, for
  both surfaces. `the_two_press_surfaces_share_every_tail` strips each surface's prefix and compares
  what is left; a refusal carries no press clause at all and both surfaces answer it identically.
- **`Fullscreen` is offered only when there is something on the glass** — Booting, Running, Stopped,
  or parked with a frame. Otherwise it is absent from this panel rather than present and dead, and
  §12.6's rule is restated to match. **So it is absent from every state this build can reach**:
  §12.2 puts this build in `Off` and nowhere else, which makes *absent* the correct rendering of
  this rule here rather than a gap in the pane. It arrives with §12.6, and the scale in its sentence
  is interpolated from the same function §12.6 uses, never a constant — on the operator's own
  machine that is 7×, not 3×.
- **The sentence says WINDOW where this section's draft said display**, and that is a correction
  rather than a rewording. §9.6 moved the threshold onto `winit::Window::inner_size()` — *the window
  we actually got* — precisely because the two disagree: drag the bottom edge up on a 1440 × 900
  display and the display gives 835 while the window is 500. A sentence naming the display would be
  false in the one case a person can do something about. It is pushed on **every** moment the pane is
  on screen rather than only when the fit crosses the threshold, because it is the one live
  measurement in this program and a stale one is an instrument lying.

The shelf and the drawer are unaffected, so the machine can still be composed, built and run — it is
only the drawn body that is withheld. **Which makes the shelf this section's second finding, and it
was still live until the pane landed**: row 2's empty-bench copy opened *The centre button makes
one*, and the shelf is deliberately left alone below the threshold, so that sentence was being read
against a bench with no device on it. It counts the press instead now — *One press makes one: a
5.5G, 30 GB, black* — which is true on both surfaces and drops a route the cradle line already
states. The route belongs where a route belongs (§7.3); row 2 was naming it twice.

**It replaces the layout rather than shrinking it**, and that is §9.6 applied rather than restated:
every term in that column is a fixed height, nothing gives, and a squeezed bench is the 560 px bug
again. Below the threshold the well is `visible: false` — not `if`, per §16.3, because the subtree
holds the cradle's `FocusScope` and the drawn device's own hover and press state, and because
`body-x` / `body-y` read `well.width` and `well.height` by name, which an `if` element cannot be
asked for. What it buys mechanically is that Slint takes the undrawn half out of the tab order and
out of the accessible tree — the same mechanism `drawer.slint` uses on a closed drawer — and that is
what `main::the_short_pane_replaces_the_bench_below_the_threshold_and_not_above_it` reads: the press
on screen and the drawn iPod not, below the threshold, and the reverse above it.

### 9.6 The vertical budget, and it is the argument for the whole layout

All logical pixels except the body, which is `k × 655.751` **physical**.

| | preferred | minimum | note |
|---|---|---|---|
| top margin | 24 | **0** | the only elastic term; absorbs all slack |
| **cradle overhang + focus ring, above** | **16** | **16** | 10 px outward offset + 6 px focus gap. **This term was missing** |
| **the device** | `k × 655.751 / sf` | same | fixed; contributes zero to Slint's shrink adjuster (§16.2) |
| **cradle overhang + focus ring, below** | **16** | **16** | |
| gap | 6 | 2 | |
| cradle label | 24 | 24 | |
| gap | 16 | 8 | |
| **the shelf** | **88** | **88** | flush to the bottom edge |
| **total** | body + **190** | body + **154** | at k = 1, sf = 1: **846** and **810** |

**The top term was the one that was missing, and it is the topmost thing on the bench.** The cradle
is an outline offset *outward* by 10 px and the focused state adds 2 px more, 4 px outside that — so
16 px above the body is spoken for, on both sides, and the previous revision's budget listed
`cradle overhang 10` exactly once. With the top margin's minimum declared 0, the cradle's top edge
and its focus ring had nowhere to go at minimum size and were positioned above the pane, where
§16.2's own finding says Slint neither shrinks nor clips them. Principle 3 makes the cradle the sole
carrier of UI state, and its top edge and its focus indicator were the first two things off the
screen.

**The client height is measured, not predicted.** The previous revision computed it as
`screen − 33 (menu bar) − 32 (title bar)` on macOS. That 33 was measured on this machine with
`com.apple.dock autohide = 1` — **still 1, verified** — which is why `visibleFrame` came out 923 of
956 with nothing else removed. With the Dock at its default visible size the client loses a further
70–90 px, so the design's headline answer to *"which displays can show the panel at 1:1"* was true
only for the one machine configuration it was measured on, and the document never said so. Two
changes:

- **Read the ceiling at runtime.** On macOS from `NSScreen.visibleFrame` (`objc2-app-kit 0.3.2` is
  already in the tree) and on Windows from `SPI_GETWORKAREA`. **winit's `MonitorHandle` has no
  work-area API** — only `size()` and `position()` (`winit-0.30.13/src/monitor.rs`) — so this is a
  per-platform call, not a portable one, and **on Wayland there is no work area at all**. That makes
  it the same shape as `set_outer_position`: a two-platform fact, stated in Reference rather than
  pretended.
- **Then stop relying on the prediction entirely.** The too-short boolean is computed from
  `winit::Window::inner_size()` — the window we actually got, asked of the platform — and re-computed
  on `Resized`, `Moved` and `ScaleFactorChanged`. The table below becomes documentation of what to
  expect, not a mechanism. **Not `slint::Window::size()`, which this line used to name**: the winit
  event filter runs *before* Slint applies the event that updates that cache
  (`event_loop.rs:192-194` calls the filter, `:222` writes the cache), so inside the filter it is one
  event old — and not the `Resized` payload either, which at startup is the size Slint's minimum
  clamp gave the window at creation. Three sizes, one of them true at the moment the question is
  asked; `IPOD_LAYOUT=1` prints all three so they can be compared.

| display | sf | client, Dock hidden | k | body, logical | needs | verdict |
|---|---|---|---|---|---|---|
| 1280 × 800 | 1.0 | 735 | 1 | 655.8 | 809.8 | ✗ **75 short** — §9.5, at every scale factor |
| **1366 × 768** Windows | 1.0 | 689 | 1 | 655.8 | 809.8 | ✗ **121 short** — §9.5, at every scale factor |
| 1440 × 900 | 1.0 | 835 | 1 | 655.8 | 809.8 | ✓ 25 spare — **✗ with the Dock shown** |
| **1470 × 956** (the operator's) | 2.0 | 891 | **2** | 655.8 | 809.8 | ✓ 81 spare — ✓ by 1 px with the Dock shown |
| 1512 × 982 (14″ MBP, notch bar ≈ 37) | 2.0 | 913 | 2 | 655.8 | 809.8 | ✓ 103 spare |
| 1920 × 1080 Windows @ 100 % | 1.0 | 1001 | 1 | 655.8 | 809.8 | ✓ 191 spare |
| 1920 × 1080 Windows @ 125 % → 1536 × 864 | 1.25 | 801 | 1 | **524.6** | **678.6** | ✓ **122 spare** |
| 1920 × 1080 Windows @ 150 % → 1280 × 720 | 1.5 | 667 | 1 | **437.2** | **591.2** | ✓ 76 spare |
| 2560 × 1440 @ 100 % | 1.0 | 1375 | 1 | 655.8 | 809.8 | ✓ (k = 2 would need 1465.5) |

**The physical-hero correction made the fractional-scale displays easier, not harder.** At 125 % the
previous revision's budget had 11 px of slack and a panel filling 56 % of its glass; this one has
122 px of slack and a panel filling all of it. Note also that the 125 % row was internally
inconsistent before — it subtracted 79 logical px of chrome at 100 % and only 63 at 125 %, which made
it ✗ by 5 px on its own arithmetic before any correction. It is computed with the same 79 here.

**The general rule, in one line a ruler can check**: a display needs
**`655.751 + 154 × sf` physical pixels of window height**. 810 at 100 %, 848 at 125 %, 887 at 150 %,
964 at 200 %. A 768-pixel-tall panel can never reach it, and saying so is more useful than a table.

**This budget is why the chrome bar is deleted and the caption is a shelf.** The shipped column wants
56 (chrome) + 16 + 658 + 16 + ~132 (caption stack) + 16 ≈ **894 px** against the operator's own
891 — three pixels short of its own fidelity rule, on the machine this program is developed on, with
a guard test that cannot see it because it hard-codes the caption at 90.

**`min-width: 880`**, and the derivation is one line rather than two disagreeing ones. The previous
revision's parenthetical — *"12 handle + 24 + 389.3 + 24"* — sums to 449.3, not 880, and stood beside
the real derivation as a second source of truth for a load-bearing constant, in the document whose
§16.9 rule is that constants live in one place. The real one:

```
420 (the drawer)
+ 388.0 (the body at k=1, sf=1: 0.5917 × 655.751)
+ 20    (the cradle, 10 each side)
+ 12    (the focus ring, 6 each side)
+ 40    (well air, 20 each side)
= 880.0
```

With the drawer open the well is `880 − 420 = 460` against a cradle-plus-ring of 420, so **the drawer
never widens the window** — unlike a design where opening a panel resizes the window under you. On a
fractional scale factor the body is narrower and the air is more generous; 880 is declared at the
widest logical case because a markup constant cannot vary.

**`min-height: 400`, and it is a floor rather than the mechanism.** A minimum tall enough to
guarantee the 1:1 panel would have to vary with `k` and `sf`, and a window `min-height` is a
constant. 400 is the height below which even §9.5's replacement pane cannot be laid out. Everything
above that is the too-short boolean's job, with hysteresis (drop below the threshold, restore 20 px
above it) so the boundary does not flutter under a drag. **`preferred: 1180 × 846`.**

**That sentence was arithmetic nobody had done, and it is done now.** 400 less the shelf's 88 is
312, and §9.5's column is `2 × WELL_AIR + 2 × LINE_BODY + 2 × s4 + ROW_H` = 136 — so the floor holds
the pane with room to spare, and `geometry::the_short_pane_fits_the_window_minimum` is what says so
if either number moves. Across, the binding case is this section's own *the drawer never widens the
window*: at `min-width` with the drawer open the well is 460, which is `Geometry.short-measure` 420
plus §7.1's air, exactly.

---

## 10. First run, in full

A person who has just unpacked this has no boot ROM, no `.ipsw`, and no vocabulary. **They meet no
form.**

### 10.1 What is on screen

The bench, at 1180 × 846, centred (macOS, Windows and X11; **Wayland cannot place a window** —
`set_outer_position` is documented Unsupported — so "centred on first launch, remembered afterwards"
is a three-platform promise and Reference says so rather than pretending).

- **A ghost iPod**: the full drawing in `Colour::Unspecified` `#E4E4E2` at 45 % opacity. Not black.
  An iPod at 45 % reads as *an iPod that has not been decided yet*, which is what it is.
- **The glass is dark and completely empty.** No welcome, no logo, no "press start". §6.1 has no
  first-run exemption.
- **The cradle is `accent`** and its label reads
  `press ● to make an iPod · 6.5 MB to download, about 28 MB on disk`.

  **What is drawn is `Press the centre button to make an iPod`, and the difference is two rules
  this document sets elsewhere.** `·` is U+00B7, which is outside §16.6's closed glyph set — the set
  is closed on purpose and widening it to make one label pass is the thing that rule forbids. And
  the sentence above is 65 characters against a 420 px row: [`geometry::CRADLE_LABEL_MAX_CHARS`]
  budgets 48, and even at the advance the renderer actually measured — 0.479, so about 62 — it
  elides, taking the cost off the end. The cost is on the shelf's third row and in the ledger, both
  of which are drawn on the same screen; the cradle says what pressing does. §7.3's table carries
  the same wording and the same deviation applies to it.
- **The shelf**:

```
 ├──────────────────────────────────────────────────────────────────────────────────────────────────────────┤
 │  No iPod yet                                                                            nothing mounted  │
 │  You do not need an iPod, or any files off one. One press makes one: a 5.5G, 30 GB, black —              │
 │  6.5 MB to download, about 28 MB on disk.      MENU ›  Parts, if you have files  ·  or drop them here    │
 └──────────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

**One number per axis, and both come from `Recipe::steps()`.** The previous revision put three
different sizes for one operation on the one screen principle 7 was written for: the shelf said
`about 300 MB, and four minutes`, the plan said `8 GiB sparse` / `about 240 MB on disk today`, and
the ledger said `8.02 GB needed`. *(Those three are quoted as they stood. A section that records
what was wrong has to keep the wrong words, and a later pass corrected them in place — which made
the paragraph accuse the old text of saying what the new text says.)* The actual download is a
single 6 533 633-byte `.ipsw` — **6.5 MB, not 300** — and the actual disk cost is about 28 MB.
Worse, the free-space gate was written against
the *apparent* size of a sparse file, so a person with 4.1 GB free was refused with the `space` class
on a machine with sixteen times the room the build needs, and the refusal was wrong.

So: **`6.5 MB to download · about 28 MB on disk` wherever the bill appears** — the cradle, the
shelf's third row and the ledger — the gate is against the **materialised** estimate, and 8 GiB
appears exactly once, in the `build` step's own sub-line, as the volume's apparent size, where it is
a fact about the drive rather than a bill.

**A step's own sub-line states that step's own cost, and that is not a second bill.** The build
costs `about 21 MB`; the bundle sitting in the firmware cache is the other 6.5 MB; 28 MB is the two
together and is what a person is being asked to agree to. Putting 28 MB on the build row would
attribute the download to the build, which is the same class of confusion this paragraph is about,
one level down. `the_first_run_screen_carries_one_bill_and_one_step_cost` is what holds the line:
**at most two distinct `… on disk` figures on the screen, the drive's drawn exactly once, and the
bill in every place a bill is drawn.** Where sparse files
are not available (§9.3's `volume` class) the number *is* 8.6 GB and the sub-line says why.

> **Where these three numbers come from.** All of them were wrong in an earlier revision of this
> section, in the direction of alarm, so each carries the recipe that produced it.
>
> - **28 MB on disk.** `compose::DRIVE_ON_DISK` is 20 987 904, measured on APFS: build a drive with
>   `ipsw::build_disk(&fw, out, ipsw::DEFAULT_SECTORS)` and read `settings::on_disk_size` of its
>   metadata. Add the 6 533 633-byte bundle, which stays in the firmware cache, and a completed first
>   run costs 27 521 537 bytes. Confirmed end to end by running it: `du -k` over the data directory
>   afterwards reports 20 496 KiB for the drive and 6 384 KiB for the bundle. The figure this replaced
>   was **240 MB** — out by an order of magnitude, and quoted at a person before they agreed to
>   anything.
> - **6 533 633 B.** `firmware::CATALOGUE`'s entry for `iPod_25.1.3.ipsw`, which is what `verify()`
>   refuses against; **6 500 352 matches no release in it**. Both render `6.5 MB` through `si`, which
>   is why nobody noticed.
> - **8.6 GB.** `ipsw::DEFAULT_SECTORS` is 16 777 216, and 16 777 216 × 512 is 8 589 934 592, which
>   `si` renders `8.6 GB`. The figure this replaced was `8.02 GB`, which is that number read as
>   gibibytes and printed with a decimal unit.

- **The drawer is open, on Work, showing the plan — before anything is pressed.** This is principle 7
  taken literally, and it costs one call to `Recipe::steps()` that already exists:

```
 ┌──────────────────────────────────────┐
 │ ‹ Close              Work            │
 │══════════════════════════════════════│
 │ This is what pressing ● does         │
 │                                      │
 │ ○ synthesise  a boot ROM             │
 │      5.5G, 30 GB, black · A446       │
 │      instant, nothing downloaded     │
 │ ○ fetch       Apple's firmware       │
 │      iPod_25.1.3.ipsw · 6 533 633 B  │
 │      from Apple, SHA-256 checked     │
 │ ○ build       a drive                │
 │      8 GiB volume, about 21 MB on    │
 │      disk — the file is sparse       │
 │ ○ install     Apple's software       │
 │      from the bundle above           │
 │ ○ start       cold boot, about 75 s  │
 │                                      │
 │ ──────────────────────────────────── │
 │ 6.5 MB to download                   │
 │ about 28 MB on disk · 312 GB free    │
 │ on /Users/…/Application Support      │
 │                                      │
 │ Nothing has been downloaded yet.     │
 └──────────────────────────────────────┘
```

**Nobody has ever been given that list before agreeing to a download.**

### 10.2 What the press does

**One press.** With the mouse, with `Space`, with `Enter`, by pressing centre on the wheel, or — on a
display too short to draw the wheel — on §9.5's primary row, which carries the same label and the
same callback. There is one interactive element on the bench and it is the centre button of the
object you came to look at. There is no Start button anywhere in this program, because the operator's
own instruction was *"just pressing on a button would start it so no need for any buttons"* and this
design takes it literally: **the button is on the iPod.**

In order, narrated in the Work Rail, as a `Recipe` with ticked `Step`s:

1. **The identity is minted and written.** `nor::Source::Synthetic { model: "A446", seed: <random>, .. }`
   is created, filed in Parts under `Black 5.5G`, and **`Settings::save()` is called here** — the
   first write the program makes. The body cross-dissolves from 45 % `Unspecified` to solid black the
   moment `Source::identity()` answers, which is before a single byte is fetched, and **nothing else
   moves**. **This step is idempotent**: if an in-flight first run already has a synthesised ROM, it is
   reused rather than re-minted.
2. **The target volume is checked, then free space.** The volume check is §9.3's `volume` class — can
   this filesystem hold the file at all, and does it do sparse — and it runs *at the press, before
   anything is fetched*. It cannot run before the plan is drawn, which is what this said first: the
   only honest way to find out whether a filesystem does sparse files is to make one and measure it,
   so the check itself writes an 8 GiB file — and §10.1's absolute is that nothing is written before
   you agree. The plan is therefore drawn assuming sparse; if the probe then answers otherwise, the
   gate refuses against the apparent size and the refusal carries the real number. Free space is then checked against the **materialised** estimate (about 28 MB,
   or 8.6 GB where sparse is unavailable). A shortfall is the *space, pre-flight* class with both
   numbers and the path.
3. `iPod_25.1.3.ipsw` is fetched to `<file>.part`, verified against the recorded size and SHA-256,
   and renamed into place **only then**.
4. An 8 GiB sparse image is built, with `aupd` marked applied so the first boot runs the OS rather
   than the updater.
5. The device is named `My 5.5G`. `Settings::save()` again — and after every completed step, which is
   what makes §10.3's resume real rather than aspirational.
6. It starts. The cradle label reads `booting · 412 M instructions · press ● to stop` with **no
   percentage**, because `Device::cold_boot_instructions` is `None` and inventing a fraction would be lying
   about a number the program does not have. About 75 seconds later RetailOS draws its language
   picker inside the glass, at exactly `k`.
7. `record_boot()` writes the denominator. Every subsequent boot of this device shows a real
   percentage — until the recipe changes, and §12.3 says what happens then.

**Everything it made is a named, editable resource.** `MENU › Parts` afterwards shows the synthesised
iPod under `Black 5.5G · synthesised · seed 9380292`, the `.ipsw` under its filename with
`fetched · SHA-256 verified`, and the drive under `my-5.5g.img · 8.0 GB · FAT32 0x0B · from
iPod_25.1.3`. Nothing was magic.

### 10.3 Failure, retry, and the flag that stops the wizard coming back

If a step fails, the Work page keeps the failure block with its class and its next step, the
completed steps stay ticked, **nothing is re-downloaded**, and the cradle reads
**`press ● to finish making My 5.5G`**. The surface never went anywhere, so it cannot return to step
one.

**The press resumes the existing `Recipe` from the first unticked `Step`, and that is not a nicety.**
The previous revision made step 1 unconditional, so a hotel wifi that drops three times left
`Black 5.5G`, `Black 5.5G (2)` and `Black 5.5G (3)` in Parts — three distinct FireWire GUIDs, each
`TitleAuth::Never`, each recoverable only from its own seed. §10.3's whole argument was that a
boolean stops the wizard re-running; the boolean stopped the *welcome copy* returning, not the
*identity* being re-minted. **Identity is the one decision this document calls permanent, and the
retry path was minting a fresh one per press.** Step 1 is idempotent and `Settings::save()` moved to
it, so the identity survives a crash as well as a retry.

**`Settings` gains `welcomed: bool`, written when the bench is wired, just before the first frame.** Not *the first time the bench is drawn*, which is what this said first: Slint 1.17 exposes no Rust-side first-render callback, so there is no "after it was drawn" moment to hook. The two failure modes are not symmetric — writing early loses one welcome if the process dies in between; writing late re-runs the welcome after any crash, which is this section's own bug verbatim. The welcome copy
never returns; a later empty bench is the same ghost iPod with `No devices yet` and both routes
offered equally, visibly the lesser of the two.

That flag exists specifically because **the old wizard inferred "offer me" from "the list is
empty", and a cancelled build empties the list** — so it re-opened itself forever. Inferring
first-run from emptiness is the bug, and a boolean is the fix.

**The flag chooses the copy and never the route, and the first implementation of it got that
backwards.** `welcomed` is written when the bench is wired, before any press — so opening the
program, looking at it and closing it was enough to reach the later-empty bench, and that bench was
given no way to make an iPod at all: the plan was not filed, the drawer stayed shut, the cradle was
drawn `fg-dim` and unpressable, and the press answered *there are no devices in the library yet, so
there is nothing to start* while shelf row 2 went on saying *the centre button makes one*. That is
this section's own bug inverted — not a wizard that returns to step one, but a bench with no step
one — and it is reachable in the commonest possible way. **Emptiness may only ever suppress the
welcome, never offer it; and the welcome may only ever be the copy.** `Offer` has four states for
that reason — `Welcome`, `Again`, `Finish`, `Quiet` — and three of the four carry the plan.

**Which row the press starts is asked per press, of the row that was pressed.** A session-wide
boolean got it wrong in both directions: with an empty library and the welcome already shown it was
false, and with a half-made first-run device sitting beside one somebody composed by hand it was
true for **every** row, so pressing the composed device resumed the first run instead of starting
it. Two rows route to the first run and no others: the empty bench, which has no device to start,
and the minted-but-unfinished device, which is a run that stopped part way.

### 10.4 The escape hatches, and the one that had to be built

`MENU › Parts` for someone who already has files, and **the whole window is a drop target** (§11.4).
Both lead to the same bench. `Make me an iPod` is disabled with a named remedy when `curl` is absent
— `Every download in this program goes through curl, and it is not on this computer. Install curl,
or set one up yourself from files you already have.` — and the second route still works, which is
what makes it a remedy rather than a dead end.

**Neither of those helps the person §10 is actually written for**, because both require files they do
not have. On a 1280 × 800 or 1366 × 768 laptop — the two most common display classes that fail
§9.6 — the previous revision left first run with no pressable route to a device at all: the well
replaced by a paragraph, the cradle undrawn, and the paragraph's one control disabled by §12.6 in
exactly the state it was offered in. That is the operator's rejected pattern in a worse form: not a
wizard that returns to step one, but a bench with no step one. **§9.5's primary row is the third
escape hatch and it is the one that matters**, because it is the only one that works with nothing on
disk.

---

## 11. Composing a device, and Parts

### 11.1 The ROM comes first, and that is not a layout preference

On real hardware **the NOR flash *is* the iPod**: model, capacity, serial, GUID and colour all live
in its SysCfg, and the drive is swappable. So an iPod *states* five facts, and those five decide
which firmware bundle can follow, which bootloader can carry which systems, and whether a purchased
title could ever be authorised.

Until an iPod is chosen, levels ② and ③ are `fg-disabled` with:

> An iPod states its model, capacity, serial and GUID, and those decide which firmware can follow.
> Choose one first.

### 11.2 The Composer — three levels, not three groups

A drawer page. **Three numbered levels, each one row deep from the root**, then the verdict, the plan,
and `Create`.

The previous revision drew all three groups open on one page and it does not fit. Counted from its
own drawing: ~14 rows at 44 px = 616, six Fields each reserving a 34 px two-line reason slot = 204,
the verdict region 54, the plan ≈ 110, `Create` 44, plus group rules and the page header ≈ 60 —
about **1 090 px** inside a drawer that is at most 803 px tall on the operator's own machine and 722
at the minimum. It had to scroll, and no surface in that revision had a scroll model. Now that Scroll
exists (§5, §16.11) it *could* scroll — and it still should not, because a `Flickable` costs every
control inside it 100 ms of press latency, and this is the page that gets pressed most.

So the three groups become three **depth levels**, which is what §11.2's own promise — *"the surface
never changes shape as you decide"* — already implied:

```
 ┌──────────────────────────────────────┐   420 px · 38 chars · rows 44 px
 │ ‹ Devices        New device          │
 │══════════════════════════════════════│
 │ ① Which iPod     Black 5.5G        › │
 │ ② What it runs   Apple + Rockbox   › │
 │ ③ Name it        My 5.5G           › │
 │══════════════════════════════════════│
 │ THE VERDICT — 54 px, always reserved │
 │ Starts Rockbox. Hold MENU at         │
 │ power-on for Apple's software.       │
 │══════════════════════════════════════│
 │ WILL DO, IN ORDER                    │
 │  fetch    Apple's firmware   6.5 MB  │
 │  build    a drive, 8 GiB volume      │
 │  fetch    Rockbox 4.0        9.1 MB  │
 │  install  Rockbox and its bootloader │
 │  16 MB to download                   │
 │  about 37 MB on disk · 312 GB free   │
 │▓▓▓▓▓▓▓▓▓▓▓▓ Create ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓ │   ← the material, one per page
 └──────────────────────────────────────┘
```

Three rows, a verdict, a plan and a button: **420 × 738 with room to spare, and no Flickable on the
path to `Create`.** Each `›` slides one level deeper, `lively`, and comes straight back. The verdict
and the plan are live at the root and update as you come back from a level, so you never lose sight
of what you are building.

**Level ② is the one that is long, and it is the one that may scroll** — the systems list, the
bootloader picker, and up to two refusal paragraphs with their `Fix` rows. §16.11's rules apply
there.

```
 ┌──────────────────────────────────────┐   ← level ①
 │ ‹ New device     Which iPod          │
 │══════════════════════════════════════│
 │  iPod            Make one          › │
 │  Model           5.5G, 30 GB       › │
 │  Colour          Black             › │
 │  Serial          7B4••••••X3N   Show │   masked by default — a screenshot
 │  GUID            000A27••••••••  Show│   of this page must not carry
 │                                      │   somebody's identifiers
 │  Generated from a seed, so the same  │
 │  iPod comes back next launch. It can │
 │  never authorise a purchased title — │   ← TitleAuth::Never, said where
 │  invented values match no purchase   │     the decision is made, not in
 │  ever made, on any machine.          │     a footnote
 └──────────────────────────────────────┘

 ┌──────────────────────────────────────┐   ← level ②
 │ ‹ New device     What it runs        │
 │══════════════════════════════════════│
 │  Disk            Build one         › │
 │  From            iPod_25.1.3       › │
 │                  iPod_20.1.3         │   ← fg-disabled, MACHINE RULE
 │                  the 5G's software;  │
 │                  this iPod is a 5.5G │
 │  Systems      ✓  Apple's software    │
 │               ✓  Rockbox 4.0         │
 │               ☐  iPodLinux           │   ← fg-disabled, PROJECT STATE
 │                  boots, then         │
 │                  ZeroLauncher stalls │
 │                  at "Finishing Up…". │
 │                  ipod-boot           │     ← the escape hatch, in mono
 │                  install-linux       │
 │                  builds that drive.  │
 │  Bootloader      Rockbox's         › │
 └──────────────────────────────────────┘
```

**Existing and new look identical.** *One I have* fills Model / Colour / Serial / GUID and **locks**
them, `fg-dim`, with `Read from the dump; a device's identity is the ROM's, not ours.` A locked
picker **stays a picker** rather than collapsing to a line of text — that is the only way the page
does not jump when you switch between the two.

**Level ③ is where a device is named, and it is the only place.** A new device takes its name here
on `Create`; an existing one is renamed here on `Save`, in place, through `Settings::rename_device`
— so it keeps its boot denominator and its park time, which discarding and re-adding it would drop,
and `current` moves with it. **There is no `Rename` control on any row, on any page.** A row control
travels as `row-action(int, int)` — two integers and no string — so a `Rename` row could never have
done the renaming; it could only have opened a page with a field on it, and `Edit…` already opens
this one. One destination does not need two doors.

**The iPod becomes a filed resource on `Create`**, under `<model>, seed <n>` — one entry per iPod,
*restated* rather than duplicated when a device that already names it is saved again. Filing at the
mint was the earlier design and it is withdrawn, because the two things it would have bought are
both already paid for elsewhere and it costs a third:

- *Cancelling keeps the identity you just tuned* was its point, and the mint's own confirmation
  already tells the truth instead — replacing an iPod costs two presses and names the seed, because
  **the seed is not kept**. Filing at the mint would make that sentence false: the old identity
  would still be in the library, and the control would be over-stating what a press destroys.
- Tuning an identity would then have to restate a filed entry on every keystroke in the `Serial`
  field, which is the settings file written per character typed.
- And a compose somebody abandons would leave an iPod in Parts that nobody asked for, removable
  only by going and removing it.

So the identity is filed when a device is made of it, and leaving level ① without `Create` discards
it. Nothing in the program disagrees: `Composer::make_one` mints into the page and touches no
library, `Composer::commit` files, and `Settings::restate_firmware` is what keeps the Edit route to
one entry.

**Validation returns the model's own sentences, verbatim**, into a **34 px two-line slot reserved
whether or not it is filled** — and that reservation is what sets the Field's height, which is what
makes principle 2 mechanical rather than aspirational:

- `The shape is LLYWWUUUCCC` — eleven characters.
- `O` would be read as a zero. Apple does not use it.
- `000A270014EFE726 is the shape. This one starts 001B63, so it belongs to some other maker's
  hardware.`

**With one carve-out, and it is a masking hole rather than a wording preference.** `identity.rs:386`
renders *"the 5.5G was made in 2006 or 2007, so its serial's third character is one of those — not
`3`"* — which quotes the offending character back, and therefore defeats the masking on this page the
moment a validation sentence fires. **While a field is masked, the reason renders the position rather
than the character**: *"the third character is the year, and 2006 or 2007 are the only ones this
generation was made in."* Press `Show` and the model's own sentence returns verbatim, because at that
point the identifier is already on screen and there is nothing left to protect.

Three behaviours in `identity.rs` differ and **the UI must not flatten them**: a typed non-Apple OUI
is **refused**; one read out of a real file **warns**, because a real dump is evidence and a typed
field is a claim; and `--serial` without `--guid` is refused outright, because *the GUID is the field
with teeth*.

**The identity consequence is permanent on the page**, because it is the one UI decision with an
irreversible consequence and `Identity::title_auth()` computes it today while nothing shows it:

| source | sentence |
|---|---|
| `Generated` → `TitleAuth::Never` | It can never authorise a purchased title — invented values match no purchase ever made, on any machine. |
| `Provided` → `IfGenuine` | Only if these are really this device's; we cannot tell by looking. |
| `RealDevice` → `Yes` | Yes, for the titles bought for this device. |

### 11.3 Impossible combinations, and the four rules a `Fix` obeys

**Every box ticks.** `compose.rs`'s own doctrine is that a checkbox you cannot tick is a question you
cannot ask: it tells you *that* something is impossible and never *why*, and the why is the whole of
what somebody learning this hardware wants. `Os::ALL` is 3 and `Loader::ALL` is 3, and all six are
offered — the two that are not in `OFFERED` appear **disabled with a project-state reason**, never
absent.

A refused value gets three things, all permanent, all in the flow:

1. **The value stays**, in place, at its own height, in `fg-disabled`.
2. **The reason is a paragraph under it**, `body` 14/20, `fg`, verbatim from `Verdict::No.why`,
   reserved via an `Expand` — 2–4 sentences, 4–6 lines at 388 px of measure.
3. **At most one `Fix` row**, wearing a 60 %-opacity material, carrying `Fix::label()`.

**And the `Fix` obeys four rules, because "one press applies it and the reason collapses" was not
enough.** A test already asserts that every refusal carries a fix and that applying it resolves the
*`Verdict`* — `every_fix_resolves_the_thing_it_is_offered_for` — and that test says nothing about
what the fix discards or whether the value it names is one the user is allowed to choose. (Three
earlier drafts of this document called that test
`every_refusal_carries_a_fix_and_applying_it_resolves`, which is not a name anything in `compose.rs`
has ever had. A document citing a test that cannot be run is the §1.1 shape exactly.)

| shape | presses | rule |
|---|---|---|
| `UseLoader` · `AddOs` · `RemoveOs` | **one** | it changes the recipe and nothing else |
| **`BuildFromIpsw`** | **two** | it changes which resource the device points at, and **says so in its own reason line before it is pressed** |
| **any `Fix` naming a value the picker disables** | **none — it is disabled too**, wearing the same project-state reason and the same `mono` escape hatch | |

**`BuildFromIpsw` needed the second press because it detaches an image.** `Fix::BuildFromIpsw`
replaces `Start::FromImage` with `Start::FromIpsw` (`compose::Fix::BuildFromIpsw`). Pick a 55.9 GB image dumped
off your own 5.5G, tick iPodLinux out of curiosity, watch `best_loader()` move to ipodloader2, get
the 0x0C paragraph, press the button — and the device silently stops pointing at the only copy of
your iPod and starts pointing at a drive that does not exist yet. §11.4 spends a paragraph on
`Remove` naming its dependents before it acts; this control detached a 55.9 GB reference with no
sentence at all. So the button reads its consequence: **`builds a new drive; rockbox-test.img stays
in Parts and this device stops using it`**, and the second press is the confirmation.

**The disabled-`Fix` rule existed because the two surfaces contradicted each other on the first
refusal a curious user hits, and §20 item 7 has now deleted the contradiction rather than
reconciling it.** Un-tick iPodLinux, re-tick it, and `check()`'s rule (1) offers `use ipodloader2` —
while the picker four rows above used to show `ipodloader2` `fg-disabled` with
`has not been built. `make` in resources/vendor/ipodloader2`. The picker refused the value and a
button below it set it in one press; and if the press was honoured, `install_linux` then wanted
`resources/vendor/ipodloader2/loader.bin` and errored out. The promise *"applying it resolves rather
than moving you from one dead end to another"* was false.

**`install-linux` now uses the fetched `ipodlinux::LOADER`** — v2.8.1, 56 912 B, SHA-256 on record —
via `ipodlinux::resolve_loader`, which consults `IPOD_LOADER=` and otherwise the download cache, and
never `resources/vendor/`. So there is no project state left for the picker to report: ipodloader2's
Bootloaders row is `not fetched yet` with the group's own `Fetch…`, exactly like Rockbox's. The
disabled-`Fix` rule stays in the table because it is a rule about *any* `Fix` naming a value the
picker disables — `iPodLinux` itself is still disabled, for ZeroLauncher's stall — and not because
of this one instance.

`best_loader()` runs on every change, so the bootloader **follows** rather than complaining — ticking
iPodLinux moves the loader to ipodloader2 rather than telling you the one you had is wrong. The
verdict is still there for combinations somebody drives into deliberately.

**The verdict region is 54 px, always reserved, and it has four renderings, not three.**

| verdict | rendering |
|---|---|
| `Verdict::Ok` | `Recipe::describe()` in `fg-dim`, in the order `install::loader_menu` actually writes it — `A boot menu: ZeroSlackr, Apple OS, Rockbox, Disk Mode, Sleep.` |
| `Verdict::No` | `why`, in `fg`, with the `Fix` below |
| **nothing chosen yet** | `compose::NOTHING_CHOSEN` — `nothing chosen yet` — in `fg-dim`. The window picks `fg-dim` from `Recipe::nothing_chosen()`, **not** from the verdict: `Verdict` gains no variant, and the string is a constant precisely so this row and the code cannot drift apart |
| **still reading** | `reading rockbox-test.img…` in `fg-dim` |

**The last two are the correction, and both were false claims in the always-reserved region.**
`Recipe::default()` is `Start::FromIpsw(String::new())` with `Loader::Apple` and `{Os::Apple}`, and
`check()` has no arm for an empty name — so before a firmware has been chosen at all the verdict read
`Starts Apple's software, the way the iPod shipped.` And `Start::FromImage { fat_type: None }` means
*"has not been looked at yet"*, while rule (2) only fires on `Some(0x0c)` — so picking a 55.9 GB image
on a slow external drive read `Ok` for several seconds and then flipped to the 0x0C refusal, which is
content moving under the user, which principle 2 forbids. §11.3's whole argument is that the verdict
is a teaching instrument; it was teaching two things that are not true. `Recipe::check()` gains
rule (0), returning `Verdict::No { why: NOTHING_CHOSEN, fix: None }` — a model change, so it is in
§20 — and the reserved 54 px absorbs both new strings at no layout cost.

**Rule (0) covers all three `Start` variants, not only `FromIpsw`** — `Recipe::nothing_chosen()` is
the predicate, and an empty `FromImage` path or `FromDisk` name is the same question with the same
answer. A second copy of that match, in the window, is where the third variant gets forgotten. It
carries **no `Fix`**: `Fix` has no payload that could name a firmware, and none is needed, because
the picker one row above is what resolves it. That is why
`every_fix_resolves_the_thing_it_is_offered_for` accepts the nothing-chosen state as a terminus —
a refusal with no fix, not a dead end with one that fails.

**And it is `check()` that gains the arm, not `check_parts()`.** `Recipe::loader_works` and
`Recipe::why_not` go through the latter, so a bootloader's tooltip stays about the bootloader; wired
to `check()`, every bootloader would grey out reading `nothing chosen yet` before a firmware is
picked, which is a non-sequitur in a bootloader tooltip.

**Who reads the drive — and what it cost that for a long time nobody did.** *Still reading* is a
rendering of a fact about a **background read**, and the read is `install::data_partition_type` on a
thread of its own (`work::Reads`), started from the Composer's own re-push and landed in `pump_once`.
Everything about that sentence was true of the design and false of the program until 2026-08-23:
`Composer::asked_for_reading` and `Composer::took_reading` were written, documented and tested, and
**called by nothing outside their own test module**. So `VolumeRead` never left `Idle`, *still
reading* was unreachable, and — the part that matters — `Recipe::volume_type()` was `None` for every
drive for ever. Rule (2a) refuses a drive whose MBR names no FAT32 data partition and rule (2)
refuses a `0x0C` one under `ipodloader2`; both fire on that field; so **every library disk verdicted
`Ok`**, including drives the plan cannot be carried out on at all. A verdict region that is never
stale because it never learns anything is not the promise.

Four things make the wired version honest, and each is a rule rather than a habit:

- **The read is armed in the page's own re-push**, not beside the disk picker. Four other routes
  reach a chosen drive — opening on an existing device, a `Fix`, a re-entered `+ New device`, the
  Rail's own `Fix` — and a read armed at the picker is a read four routes arrive without.
  `Composer::volume_to_read` answers `Some` only from `VolumeRead::Idle`, which is what makes asking
  on every frame cost one thread per chosen drive rather than one per frame.
- **The answer is tagged with the drive it is about**, and `Composer::took_reading_of` drops one
  about a drive that is no longer chosen. Picking a second disk before the first answers is one
  press; without the guard the second drive wears the first drive's partition type, which is a
  verdict about a file nobody read.
- **A drive being read holds the 10 Hz timer open exactly as a build does.** `pump_once` stopping on
  `!work.busy()` alone would stop the tick on the first frame after a pick, leaving *reading …* on
  screen until the next press — a spinner nothing stops, which is the one outcome that is certainly
  worse than either verdict.
- **A read that fails is not a refusal.** A drive nobody could read is not a drive that fails; the
  verdict goes on without it, which is also what a thread that could not be spawned reports.

**One correction to what §11.3 implies about which half of this a person can reach today.** Rule
(2)'s trigger is `ipodloader2` — selected or required — and `Os::OFFERED` and `Loader::OFFERED` hold
Apple and Rockbox only, so both of its triggers are drawn **disabled** in this build (§9.4, and
`KNOWN-BUGS.md` for why). The `0x0C` refusal is therefore correct, tested and **not reachable by any
press** until iPodLinux is offered. What *was* reachable, and was the live false `Ok`, is rule (2a):
a drive whose MBR names no FAT32 partition at all — a Mac-formatted iPod with an Apple Partition Map
rather than an MBR is the everyday way to hold one — verdicted `Ok` and would have been installed
onto.

**The plan is one list rendered twice**: `Recipe::steps()` as *this is what will be downloaded* here,
and as a ticking checklist on the Work page while it runs. One source, so they cannot disagree.

**On `Create`**: the settings file is written immediately, the drawer switches to Work, the Composer's
values lock with `building — this recipe is in use`, and **the device appears on the bench at once**,
in the `building` state with its progress on the cradle label. You can leave and start something
else. A five-minute, five-ways-to-fail operation must not own a screen — that is the structural
answer to *"a wizard that re-opens itself"*.

**And `Create` clears `Device::cold_boot_instructions` whenever `oses` or `loader` changed** — §12.3.
`Recipe::shape()` is what it compares: `compose::BootShape`, the bootloader and the systems and
deliberately not the drive.

### 11.4 Parts

**Six groups, fixed order, always all six present even when empty.** A page whose sections come and
go is a page you re-learn every visit.

| group | model | verb (from `Resource::verb()`) | actions |
|---|---|---|---|
| iPods | `Resource::Firmware(nor::Source)` | chosen by a device | `Add a dump…` `Synthesise…` |
| Apple firmware | `Resource::Installer` | makes a disk | `Fetch…` `Provide…` |
| **Bootloaders** | `Resource::Bootloader` | **goes in the firmware partition, which holds exactly one thing** | `Fetch…` `Provide…` |
| Software | `Resource::Software` | installs onto a disk | `Fetch…` `Provide…` |
| Disks | `Settings::disks` | what a device runs | `Build…` `Provide…` |
| **Snapshots** | the parked machines | **what `press ● to resume` resumes** | `Discard` |

**The two verbs this build draws live open the Composer.** `Synthesise…` under iPods and `Build…`
under Disks are enabled because `rail::Caps::composer` is true — a verb that rewrites a recipe needs
a surface that holds one, and this build has that surface — so `main.rs` routes both to it, the same
entrance `+ New device ›` uses. What a live verb must never say is `rail::Next`'s *reason* — those
sentences are why a control is **disabled**, and using one under a live press had `Synthesise…`
answering *there is no Composer in this build yet* one row from the page it opens.

**`Fetch…` is drawn DISABLED on all three groups that offer it, and it used to be live.** It asked
`Next::Retry`, which asks *is curl on this computer*; `curl` is measured at launch by running it; so
on every computer that has curl the verb was blue and every press failed, because there is no
per-part fetch behind it — the only download this build starts is the first run's own plan. **A
capability question is the wrong question when the mechanism behind the control does not exist**, and
asking it draws a live control over a hole. So it is §14.1's construction instead: disabled, wearing
§9.4's second kind — *nothing on this page reaches a fetcher yet* — and naming the group's own route,
which is `ipod-boot firmware get <family>` under Apple firmware and `ipod-boot rockbox-install` under
Bootloaders and Software. The fetchers themselves exist and work; what does not exist is a way here
to reach them.

**Both verbs carry `geometry::PARTS_VERB_W` as a floor**, because without one `Fetch…` drew as `F…`.
It is the **half-share** rather than the label's own budget, because the same shrink that ate the
label also eats the reason under it: at 104 the two halves were 256 and 104, and Apple firmware's
`no file picker in this build` elided in the narrow one while Bootloaders drew it whole — see §9.4.
The two share a row by `horizontal-stretch`, an eliding `Text` floors at one ellipsis, and Slint
shrinks a too-narrow row **by stretch** — the same pixels off each — so the half whose reason is a
sentence kept its width and the half whose label is a word lost it. §20 item 20 has the measurement.

**Snapshots is the sixth group and it is here because 1.6 GB per park was invisible.** Close the
window rather than quitting — which §12.4 says parks — four times across four devices, and 6.4 GB of
snapshots exist that the user never asked for, cannot see, cannot total and cannot delete, on a
machine that might have 18 GB free. Parts is where every other byte this program spends is visible;
these belong in it, with sizes, with the device each pairs with, and with a `Discard` verb.

```
 ┌──────────────────────────────────────┐
 │ ‹ MENU             Parts             │   the page body is a Scroll (§16.11);
 │══════════════════════════════════════│   the header above it does not move
 │ iPods                            2   │
 │  No iPod is plugged in               │  ← reserved, always. fg-disabled
 │▓▓Black 5.5G▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓› │  ← the material: the selected row
 │▓synthesised · seed 20266 · used by 2▓│
 │  From my 30 GB                     › │
 │  dumped from a real iPod · used by 1 │
 │  Add a dump…      Synthesise…        │
 │──────────────────────────────────────│
 │ Apple firmware                   2   │
 │  iPod_25.1.3.ipsw                  ▾ │
 │  6.5 MB · fetched · SHA-256 verified │
 │  ┌────────────────────────────────┐  │
 │  │ osos  7 559 680  checksum ok   │  │
 │  │ rsrc  5 242 880  checksum ok   │  │
 │  │ aupd    262 144  checksum ok   │  │
 │  │ identified by contents:        │  │  ← firmware::identify(), by hash
 │  │ Apple iPod_25.1.3              │  │     not by filename
 │  │ used by: My 5.5G               │  │
 │  │ Reveal   Copy path   Remove    │  │
 │  └────────────────────────────────┘  │
 │  iPod_20.1.3.ipsw                  › │
 │  6.5 MB · fetched · size only, no    │  ← NEVER renders as "verified"
 │  hash on record for this release yet │
 │  Fetch…           Provide…           │
 │──────────────────────────────────────│
 │ Bootloaders                      1   │  ← the fourth kind the shipped
 │  Rockbox's bootloader              › │     window drops entirely
 │  51 996 B · fetched · SHA-256        │
 │  ipodloader2 v2.8.1                  │  ← no longer a project state:
 │  56 912 B · not fetched yet          │     `Fetch…` is the whole answer
 │  Fetch…           Provide…           │
 │──────────────────────────────────────│
 │ Software                         1   │
 │  Rockbox 4.0                       › │
 │  9 090 335 B · SHA-256 verified      │
 │  Fetch…           Provide…           │
 │──────────────────────────────────────│
 │ Disks                            2   │
 │  my-5.5g.img                       › │
 │  74.5 GB · FAT32 0x0B · from         │  ← the FAT type is a first-class
 │  iPod_25.1.3 · Rockbox 4.0           │     fact: it is the byte compose.rs
 │  rockbox-test.img                  › │     reasons about
 │  55.9 GB · FAT32 0x0C · provided     │
 │  Build…           Provide…           │
 │──────────────────────────────────────│
 │ Snapshots                        1   │
 │  My 5.5G · parked 4 min ago        › │
 │  1.61 GB + 1.61 GB frozen drive      │
 │  Discard                             │
 └──────────────────────────────────────┘
```

**`used by N` is the reference-not-copy property made visible**, and it is the whole reason this
model beats UTM's. An expanded row names the devices; `Remove` names them before it acts and offers
`Remove anyway` in `danger` or `Cancel`. Removing a resource never deletes the file it points at;
removing a **disk** asks separately and explicitly about the image, defaults to no, and says the size.

**The seed is shown by the open row, not by `Remove`'s own sentence — and that is a measurement
rather than a preference.** This section used to say *removing a synthesised iPod additionally shows
the seed*, and `remove_consequence` did: `This iPod is a recipe: only seed 6182160 regenerates its
identity.` A seed is a whole `u64` and `18446744073709551615` is twenty digits, so that clause is 62
characters at its widest — **330 px in the 324 px slot the control draws in (§9.4)**, over budget with
nothing else in the sentence at all. There is no wording of it that fits, so the choice was never
between two sentences. What shows the seed instead is the **`Seed` fact inside the same open body**,
a few rows above the control, at 372 px and whole; and when the ROM was filed by the program rather
than named by hand, the row itself is called `A446, seed 6182160` and its fact line reads
`synthesised, seed 6182160, used by 1`, so it is drawn three more times before the row is opened.
The sentence keeps the half that is nowhere else on the page: `No file is deleted. Still named by
My 5.5G.` The names go last, so a long list elides where the point does not.

**And nothing that the machine is using can be removed while it is running.** One line, covering both
the resource case and the device case that had no rule at all: **while a device is the machine,
`Remove` on that device — and on every resource it references — is disabled with the machine-rule
reason `My 5.5G is running. Stop it first.`, and the device's drawer page carries `Power off`
immediately above it.** Without it the consequences were all live: the bench would have no device to
draw while a machine thread executed, `Settings::current` would dangle, §12.4's park would write
`<device>.parked.png` for a name that no longer exists, and §9.1's empty bench would be drawn while an
ARM7 was mid-boot. Same construction as the Composer's field locking, which already solves this shape.

**The plugged-in-iPod row is reserved, always.** `identity::detect_mounted()` scans `/Volumes`,
`/media` and `/run/media` one and two levels deep for iPod-shaped volumes and reads
`iPod_Control/Device/SysInfo` — **no dump, no driver, no privileges** — and it has no caller anywhere.
The previous revision had the `iPods` group *grow* a row when one appeared, which moves every row
below it on an event the user did not initiate at this surface: principle 2's exact prohibition, and
the operator's own words. So the group always carries a first row, `No iPod is plugged in` in
`fg-disabled`, which becomes `An iPod is plugged in — 7B4••••••X3N. Read its identity?` when one
appears. Three more rules with it: **`detect_mounted()` runs off the UI thread** on a 2 s poll while
Parts is open and never otherwise, because a `/Volumes` walk on a machine with an unresponsive SMB
mount blocks its caller; the offer says **`read only`**; and it carries `open-drive`'s honesty
forward — `this cannot tell whether anything else is writing to it`.

**`▸` expands in place**, pushing rows down, `gentle`, content faded in after the height settles —
and inside the Scroll, an Expand that opens below the fold scrolls its own top edge into view (§8.1):

- **a ROM** → `inspect::flash()`'s verdict with its own diagnosis (`0 bytes — Rockbox's Dump ROM
  contents writes its output at the end, so an iPod reset before it finishes leaves exactly this` ·
  `a 512 KiB dump is a nano-class device` · `2 MiB — a 6G Classic or a nano` · `word 0 is not an ARM
  branch` · `no flsh directory at 0xffe00`); the image directory and tags; the reset vector; the
  bootloader's own build string; the SysCfg identity resolved through libgpod's table (**197 rows —
  the README's "198" is wrong and should be corrected**) to capacity, colour and generation; a
  separate warning when `Mod#` and `HwVr` disagree about the generation; **the raw bytes of every
  record it could not decode**, because Rockbox names nine tags and this 5G NOR carries a tenth; the
  identity **masked** with a `Show`; `TitleAuth`; and `Show its boot screen ›`.

  **Two states the previous revision had no answer for, and both are machine rules that disable the
  ROM for use in a device:**

  | | says |
  |---|---|
  | the `Mod#` is not in the table | `Mod# 〈code〉 is not in the model table. HwVr says 〈code〉. This will not claim a generation it cannot look up.` |
  | the generation resolves and is not the 5th | `this is not a 5th-generation iPod — the table says 〈generation〉` |

  A 1 MiB NOR pulled off a nano passes every size-based diagnosis `flash()` has, and the bench draws
  whatever ROM a device points at using one `hero` and one drawing — so it would be rendered as a 5.5G
  with a 320 × 240 glass, which is inventing a fact about somebody's hardware. That is the exact thing
  `Colour::Unspecified`'s `#E4E4E2` exists to refuse. §7.3 has the matching cradle row.

- **an `.ipsw`** → firmware versions and checksums, and `firmware::identify()`'s answer **by
  contents, not filename**. `Unrecognised` is explicitly allowed and carries its paragraph: modified
  firmware is a legitimate reason to want an emulator, and it is reported so you know, not to stop
  you.
- **a disk** → the partition table; the FAT type spelled out; `built_from` and `installed` in order;
  whether the flash updater is armed (`armed — this drive boots the updater, not the OS`); what
  `volume_software()` found (`/.rockbox/rockbox-info.txt`, `/loader.cfg`, `/boot/vmlinux`); the full
  path in `mono`, copyable; and an **in-window FAT32 tree** from `ipod-boot fat tree`.
- **a snapshot** → the instruction count it was taken at, whether `Config::pair_is_whole()` still
  answers yes, and if not, why — `the drive has been written to since this was taken`.

**The boot-screen preview obeys the layer rule.** `nor::Source::boot_screen(320, 240)` is emulator
output, so it is drawn in a 320 × 240 rectangle with the same `#08080a` glass treatment and a
`preview · 320 × 240` caption — never flowed into our layout at an arbitrary size.

**Dropping is the best acquisition route and the whole window is the target**, because winit's file
events carry no cursor position (§16.4). While a drag is over the window, the shelf's rows 1 and 2
crossfade to what the program thinks it has — **nothing moves, and no pixels are spent**:

```
 ├─────────────────────────────────────────────────────────────────────────────────────────────────────────┤
 │  Eight files                                                                    let go to file these     │
 │  1 048 576 B · not a boot ROM · word 0 is not an ARM branch  ·  Apple firmware · iPod_25.1.3  ·  +6 more │
 │  works on a copy of my-5.5g.img                                                                          │
 └─────────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

Four rules, and three of them are corrections:

1. **A drop is a 150 ms coalescing window.** winit delivers `DroppedFile(PathBuf)` **one event per
   file, with no cursor position and no terminating event** — the previous revision recorded the first
   two facts and missed the third, so nothing in it could tell eight events apart from eight drops.
   All `DroppedFile` events within 150 ms are one drop.
2. **The band shows a count and at most two identifications, then `+N more`.** It was written for
   `Three files` with three identifications on one 20 px line; eight do not fit, and a band that
   reflows is a band that moves.
3. **Ambiguity files and does not compose.** A drop containing more than one ROM or more than one
   `.ipsw` files everything into Parts and creates **no** device, with the Work Rail saying
   `eight files filed; make a device from them in Devices ›` and naming each. The old promise — *"a
   ROM and an `.ipsw` dropped together, in either order, produce one device"* — is undefined with two
   of each, and its test, `dropped_files_route_themselves_in_either_order`, is a two-file test that
   was carried across unchanged. It stays, and it keeps its two-file scope; the ambiguous case gets
   its own.
4. **For a ROM-sized file the band shows `inspect::flash()`'s verdict, not `inspect::Kind`.**
   `Kind::Rom` is "exactly 1 MiB" and nothing else, so a 1 048 576-byte JPEG classified as `Rom` and
   the band rendered `boot ROM · 5.5G · 1 048 576 B` — an affirmative claim, with a generation
   attached, for a photograph. The real verdicts (`word 0 is not an ARM branch`, `no flsh directory
   at 0xffe00`) existed only inside the Parts row's `Expand`, which the user has to go and open. **A
   size class is a hypothesis and the band is where it gets tested**, because that is the last moment
   before the file acquires a name. Note the contrast case the previous revision handled correctly —
   a 4.2 MB unknown reads `no recognisable header — named, not filed` — and the dangerous one did not.

Identification is **by content**, never by extension: `inspect::Kind` first (a `.ipod` image is
verified by its big-endian checksum seeded with 5 — before that check existed, `rockbox.ipod` at
7.5 MB fell through the size test and an OS was handed to the disk parser), then `firmware::identify`
by hash. **Identification during `HoveredFile` is size-gated and hashing happens after the drop, not
during it** — a SHA-256 of the 101 MB ZeroSlackr archive on the UI thread while a hover is in
progress is a frozen window. An Apple Partition Map is **named** (`this is a Mac-formatted iPod`)
rather than reported as "no MBR". On drop, each file is filed and the Work Rail says where it went
and **whether it was copied in or referenced in place**.

### 11.5 The platform split, and why the FAT32 tree is a requirement

`open-drive` uses `hdiutil attach -imagekey diskimage-class=CRawDiskImage` on macOS and
`udisksctl loop-setup` on Linux, probed with `mount::available()` rather than trusted from
`cfg!(target_os)` — and **cannot work on Windows at all**, because Windows mounts ISO and VHD, not a
raw image. So the in-window FAT32 tree is **Windows's only route to putting files on a volume**,
which makes it a requirement rather than a nicety, and it is offered on every platform so the two are
not separate mental models.

`Reveal the disk` is disabled while that device runs, with the honest sentence:
`Two writers on one filesystem is how a volume gets corrupted. This cannot check that the iPod is
powered off, so it says so rather than pretending to.`

### 11.6 Settings — one page, three rows, and it is not a settings app

`Settings::chassis`, `Settings::check_updates_on_start` and the theme were settled decisions with no
surface at all. §15 said *"there are three program settings and this is one of them"* and §14.3
argued the drawer is not a sidebar partly because *"this program has five resource groups and three
settings"* — and then no page owned any of them. The operator rejected **a settings screen reachable
only by destroying the running machine**, not settings.

```
 ┌──────────────────────────────────────┐
 │ ‹ MENU             Settings          │
 │══════════════════════════════════════│
 │  Theme           System            › │   system / light / dark
 │  Check for updates on launch     ☐   │   Settings::check_updates_on_start
 │  Settings file   ~/…/settings.txt  ⧉ │   Settings::path() — a preference
 └──────────────────────────────────────┘   nobody can find is one nobody
                                             can reset
```

Three rows, reachable while the machine runs, which is the whole cure. **`chassis` is not one of
them** — it is an override on the *device's* page, because the model's own comment says it is *"the
window's iPod, not the machine's identity"*, and a global override for a per-device fact is how the
same setting comes to mean two things. `Settings::mode` is §17.Q8 and is not on this page until it
has a job.

---

## 12. Running

**Running is a *state of the bench*, not a place.** Nothing about the layout changes when the machine
starts. That is the direct cure for the original disease — settings could only be reached by
destroying the machine, because settings and running were different screens and *"no machine"* was
how the window knew which to draw.

Four things change and only four: **the screen lights**, the cradle's ring and label change, the
shelf's rows change text, and the drawer's Composer fields **lock in place** with
`The machine is running. Stop it to change what it is made of.` — visible, same size, same position,
greyed. Principle 4 applied to the exact case that caused the bug.

### 12.1 The screen

`Out.fb` is 320 × 240 RGB888, refreshed by the emulator at most 60 Hz with an `fb_seq` counter so an
unchanged frame skips the texture upload. Its size is §6.6's `k × 320` physical, and **nothing that
animates feeds it**.

Two mechanical corrections before this ships, both measurable:

- **`Image::from_rgba8_premultiplied`, not `from_rgb8`.** Skia cannot take RGB888, so the RGB path
  runs `pixels.as_bytes().chunks(3).flat_map(...).collect::<Vec<u8>>()` — 76 800 iterations building
  a 307 200-byte `Vec` — and copies it again into a `skia_safe::Data`. **It is not cached**: a
  programmatically built `SharedPixelBuffer` gets `ImageCacheKey::Invalid`, and
  `replace_cached_image` returns immediately on an Invalid key, so both allocations re-run on **every
  draw**, not once per frame.
- **Try `SLINT_SKIA_PARTIAL_RENDERING=1`.** Partial rendering is off by default with Skia, so pushing
  frames at 60 Hz repaints the whole window at 60 Hz.

Measure both arms with `SLINT_DEBUG_PERFORMANCE=refresh_lazy,console` against the recorded **17.4 M
instr/s headless and ~14 M with a window open** (README vs KNOWN-BUGS — a ~20 % loss that the
previous revision of this document called unmeasured while two other files already disagreed by that
much), and put the number in the changelog.

### 12.2 The four phases, and Off is genuinely one of them

| phase | cradle | glass | shelf row 1 trailing |
|---|---|---|---|
| `Off` | `accent`, or a broken ring | **dark** | `off` |
| `Booting { target }` | `fg-dim` | the ROM's boot screen | `booting · 62 %` or the instruction count |
| `Running` | `fg-dim` | live | `running · 14.2 M instr/s · 24 % of real` |
| `Stopped(reason)` | `danger` | **its last frame, kept** | `stopped` |

`Off` means **no machine exists, nothing is executing, and the panel is dark** — the state a 5G is in
with a flat battery. It is not a pause and it is not drawn as one: a powered-off iPod's glass is
`#08080a` and empty, never a frozen last frame, because a frozen frame is a paused machine
pretending.

*(**Built, toolkit-free** — `tools/ipod-gui/src/machine.rs`, 2026-08-23. `Life` is this table with the
evidence each phase is entitled to attached, and `Life::read` — which takes an `emu::Out` — is its
only constructor, so it is this table evaluated and never a second opinion about what the machine is
doing. Three states the previous shape allowed are gone by construction rather than by discipline:
`Off` carries no pace, so a dead machine cannot draw a speed; `Stopped` carries a `Reason` that is
never empty, so a `danger` ring cannot sit over a blank sentence; and `Progress` splits `Counted`
from `Fraction { of: NonZeroU64 }`, so §12.3's *"no fraction and no bar"* is a variant rather than a
rule somebody has to remember. `Glass` is the third column, `Life::shelf` the fourth, and
`machine::cradle` is §7.3. **One trap the building found**: the `target` in the `Booting { target }`
row above is `Config::snap_at`, the instruction count the *snapshot* is taken at — not §12.3's
denominator, which is `Device::cold_boot_instructions`. Two numbers, two questions, and each is a
plausible-looking substitute for the other — a bar drawn over the wrong one is still a bar, moving
at a plausible rate, and nothing would report it.
`the_boot_bar_divides_by_the_last_cold_boot_and_not_by_the_snapshot_instant` is the test, and it
carries the substitution as its own control so a reading that proves nothing cannot pass it.
**Wired 2026-08-24.** `main::life` reads `Out.phase` through `Life::read` and nothing else in the
window decides a phase; `machine::cradle` is every §7.3 caption on the bench; `machine::Glass` is
what is on the panel; `machine::centre` and `machine::permits` are what the centre button means and
whether a power command is physical. The press builds an `emu::Config` from the device
`Settings::run_device` resolved and spawns `emu::run` on its own thread. Three of the table's four
rows are reachable from the window today — `Off`, `Booting` and `Stopped`; `Running` needs a drive
with an OS on it, which is a fixture no test suite can carry. **Two things in the table were still
not drawn**, and both are now: §12.4's `parking` caption has a producer — `Esc` from `Running` sets
`Link::save_on_quit` and the cradle reads `Link::saving` — and the `stalled` line below the table is
a Gauge on §12.8's Readout, which is built. **The glass gained a fifth state with the park**:
`panel-lit` is `Glass != Dark` rather than `phase != Off`, because a device that is off with a
restore point beside it has §12.4's frame on it and an unlit panel draws it at `opacity: 0`.)*

`Stopped` is the opposite and for the opposite reason: **the last frame is evidence** and is kept.
Row 2 carries the reason in `fg`, row 3's trailing slot offers `Cold boot` and `Copy the reason`, and
it stays until dismissed.

**A fifth thing that is not a phase, and needs its own treatment**: `stalled_secs > 2.0` turns the
Readout's stalled Gauge `warn` and adds one line — *the instruction count has not moved for 6.4 s;
the core is halted waiting for an interrupt with no deadline armed.* One session sat dead at
2 791 999 952 instructions and was only noticed because two `state` replies happened to be compared
by hand.

### 12.3 Progress, honestly — and what happens when the recipe changes

The denominator is `Device::cold_boot_instructions` — **this device's own last completed cold boot**,
which is why one bar is honest across Rockbox (~100 M), RetailOS (~1.6 G) and iPodLinux (~21.5 G)
without detecting which is on the drive. Before a device has ever booted there is **no fraction and
no bar**: the cradle label carries an instruction count that moves. A 4 px indeterminate rule that
does not animate would be acceptable; a spinner would not (§8.3).

**That honesty is conditional on the device never changing what it boots, and §11.3 exists precisely
so it can.** A device that learned ~1.6 G on RetailOS and then has Rockbox installed reaches its menu
at ~100 M against a 1.6 G denominator, so the cradle reads `booting · 6 %` at the moment the machine
is finished. Boot the other way and the bar passes 100 % and keeps going. Both are the specific
defect `cold_boot_instructions` replaced `snap_at` to fix, reintroduced through the edit path.

So: **`Create` clears `Device::cold_boot_instructions` whenever the recipe's `oses` or `loader` changed**,
and the first boot after an edit has no fraction, exactly like a device that has never booted. One
line in `Create` — plus a model method that owns the rule, because a device is edited from more than
one entrance and a rule with nothing computing it is the shape §16.9 exists to delete.

**The comparison is `compose::BootShape`**, which `Recipe::shape()` produces: the bootloader and the
set of systems, and **not** the drive it starts from. Excluding the drive is what makes a *rename*
distinguishable from an *edit*. Three reasons, in order of weight: `Start` carries
`fat_type: Option<u8>`, which goes from `None` to `Some(_)` when a background read of the volume
finishes — a discovery, not an edit — so a whole-`Recipe` comparison would drop a good denominator
because a read completed, which is a number changing for a reason the user did not cause; renaming
the `.ipsw` moves RetailOS's cold boot by a few per cent, not by the order of magnitude this is
about; and the next completed boot overwrites the number anyway.

The device stores the shape it last learned its denominator on — `Device::boot_shape:
Option<BootShape>`, one settings key, the bootloader then the systems comma-separated — and
`Settings::set_boot_shape` is the single place the rule runs: same shape, keep the number; different
shape, store the new one and take the number; no such device, do nothing. `None` means nobody
recorded it, which is what every device from an existing settings file reads as, and an unverifiable
denominator is dropped once rather than trusted for ever.

*(**Built.** `compose::BootShape` and `Recipe::shape()` shipped 2026-08-21; `Device::boot_shape`,
`Settings::set_boot_shape` and the `as_device` carry-forward line followed; and `Composer::commit`
became the production writer — the last of the four, and the one whose absence made the other three
unreachable. It calls `set_boot_shape` rather than clearing `cold_boot_instructions` itself, so the rule
runs in one place: `a_composed_device_records_what_it_boots_and_reopens_on_it` measures the write,
the file and `recipe_of`'s authority branch in one pass, and
`a_device_with_no_recorded_shape_drops_its_denominator_once` measures the once-only drop that every
device written before this pays.)*

*(**And the denominator itself had no writer at all until 2026-08-24, which made every word above
about a number nothing produced.** `Settings::record_boot` existed and nothing in the window called
it, so `expected_boot` answered `None` for every device that has ever existed and `Progress::
Fraction` was a variant the shipped program could not construct: every boot drew the counted form,
and this section's honesty was the honesty of having nothing to be dishonest with.*

*The trap in wiring it is that **the boot phase ends two ways one line apart in the run loop** —
the machine going quiet, which is an observation, and `executed >= snap_at`, which is the
fallback for the case that never happens. A writer that watched for the phase change and took
`Stats::executed` records `snap_at` — the 1.6 G constant this whole section exists to stop being the
denominator — on every machine that never settled, and files it as "this device's own last
completed cold boot". `emu::boot_end` is the one function that tells the two apart; it answers
`Some(None)` for the fallback and `Some(Some(n))` for the observation, `Out::booted_at` carries only
the second, and `a_boot_that_ended_on_the_fallback_teaches_the_denominator_nothing` carries the
substitution as its own control. A restored machine never enters `Booting` at all, so a resume
cannot teach the denominator what a cold boot costs.*

*(**The observation was `0x8001052a` — RetailOS asking the click wheel for frames — until
2026-08-25, and it was wrong by 387x.** That command's first arrival is the **boot ROM's**, at
`@2 211 983` of an 872 M cold boot, with the drive not yet answered and the panel black; the
denominator it taught draws a bar full at 0.12 %. It is now `emu::Quiet`: an 8 M-step trailing
window of the machine's own steps that is 95 % **halted**, with at least one ATA command issued —
because a booted machine halts and a booting one does not, whatever is on the drive, which is the
same property this section needs to be honest across Rockbox, RetailOS and iPodLinux without
detecting which is on it. Over the whole cold boot the halted fraction never exceeds 61.7 %; from
823.6 M it holds 99.7 %. `research/10` Addendum 32 is the measurement, and `KNOWN-BUGS.md` carries
what the old signal cost. **The settings key moved with the meaning** —
`device.N.cold_boot_instructions` — so every denominator learned by the old signal is read by
nobody and gone at the next save.)*

*And the difference is **drawn** rather than only modelled: a 4 px determinate rule in the cradle's
own band, under the body, when there is a fraction — and nothing at all when there is not, which is
this section's "no fraction and no bar" as a picture. It costs zero vertical budget: the band
between the body's bottom edge and the cradle's bottom ring line is `CRADLE_BAND − FOCUS_GAP −
FOCUS_RING_W` = 8 px of fixture that already exists at every size. `_out/gui/bench-booting.png` is
the photograph, measured at 62.0 % of a 387 px track.)*

### 12.4 Parking, and what it costs

`Esc` from `Running`, the drawer, or closing the window sets `Link::save_on_quit`. It is a **~1.6 GB
write**, so `Link::saving` drives a real sentence — `parking · 0.7 of 1.6 GB` — rather than the
window appearing to hang, and the bench is usable throughout.

**Free space is checked against the snapshot size before `save_on_quit` is set, and if it is short
the window closes without parking.** The park that fails for lack of space fails *at window close*,
when there is by definition no window left to show §9.3's `space` class in and `Link::saving` has
nowhere to land — so the report goes to the only surface that still exists, the next launch's Work
page: `My 5.5G was not parked at last close — 1.6 GB needed, 0.9 GB free.`

A snapshot is **RAM and CPU only**, so the drive is frozen beside it (`Config::frozen`): pairing
restored RAM with a drive that kept moving is what produced the intermittent "connect to computer"
screen, and `Config::pair_is_whole()` is the single place that knows. **The cradle reads it before it
draws a parked state** (§7.3) — it is one `stat` and one line compare on a copy, or a file-existence
test on a frozen clone, so it costs nothing to ask. Resume is ~3 s against ~75.

**A parked device's glass shows the frame it stopped on**, at exactly `k × 320`, with `parked` in the
shelf. Park writes `<device>.parked.png` beside the snapshot; `png.rs` — currently unreferenced — is
the writer, and this retires one of the five `#[allow(dead_code)]` modules with a real caller. If the
PNG is absent the glass is dark, which is the honest fallback.

`Resume` and `Cold boot` are **two separate, both-visible rows** on the device's drawer page, joined
by `Discard the snapshot` when the pair is broken. Never a modifier you have to know.

**Two things this section asks for that nothing in the program can answer yet** *(measured
2026-08-23, building `machine.rs`)*.

- **`parking · 0.7 of 1.6 GB` has no numerator and no denominator.** `Link::saving` is an
  `AtomicBool`; nothing anywhere publishes bytes written or bytes to write, and the snapshot writer
  does not count as it goes. So the cradle reads **`parking`** and stops — a fraction invented from
  the snapshot's nominal size would be a bar moving at a rate nobody measured, which is the one
  thing §12.3 is written about. The sentence becomes possible when the writer reports progress;
  until then the honest row is the short one.
- **A resume is reachable only by *building* the machine thread, so `Resume` is not a command.**
  `Cmd::PowerOn`'s own doc says *"always a cold boot, never a restore"*, and the only code that
  restores is `emu::run`'s entry, gated on `Config::may_restore(first)` with `first` false for every
  power cycle inside a session. A window that has built a machine, powered it off, and then wants
  the parked state back has no route to it: `machine::Launch::Resume` returns `None` for its command
  rather than quietly handing back `PowerOn`, which would be the bench cold-booting a machine whose
  own label had promised three seconds. **The fix is a `Cmd::Resume` the run loop honours by
  rebuilding**, and it is not written.

**`Device::parked_at` is a stored field, not an inference** (§3.3). The shelf renders `parked ·
4 min ago` and the model carried nothing to render it from — the same shape as the `fetched and
verified` string literals §3.2 was written to delete.

*(**Built and wired, 2026-08-24 — and the ~1.6 GB in this section is wrong about the RAM half.**
`machine_config` gives every device its own restore point at `Settings::restore_point(name)`; `Esc`
from `Running` and the window close both set `Link::save_on_quit` after `machine::Park` has checked
free space against `Link::snapshot_bytes` — this machine's own memory, summed off the format that
writes it rather than quoted from here. **The pair measures 156 657 372 bytes, about 149 MB**, on a
5.5G through `Machine::snapshot`; the 1.6 GB figure is the frozen **drive** in copy mode, which is a
reflink on APFS and 12 KB of real space. `a_park_and_a_restore_are_a_round_trip_and_the_frame_comes_
back` builds a real machine, parks it, and asks the questions the next launch asks.*

*§17 Q7's frame is written — `<snapshot>.parked.png`, and `png::encode` has its first production
caller, which is what retires that module's dead-code allow. It is **read** by
`slint::Image::load_from_path`: this document's own note said nothing in the program could decode a
PNG, and that was true of `png.rs` and false of the crate — `slint/std` turns on
`i-slint-core/image-decoders`, which is `image 0.25` with `png`, already compiled. `_out/gui/bench-
parked.png` is a device with no machine at all, showing the frame it was put down on, under
`Press the centre button — resume, about 3 s`.*

*Two things this section asked for are still not there. The **numerator** is still absent, so the
cradle reads `parking` and stops. And **`Cmd::Resume` is still not written**: what the press does is
build the machine thread, which is where the restore happens — so a device that has never been
started resumes in about 3 s, and a machine that has been built and powered off inside the same
session still has no route back to its snapshot. The one thing that has changed is that **`Discard
the snapshot` now deletes files**, because there are now files: it cleared a timestamp and left the
pair on disk, which would have left `Config::may_restore` — which asks the files, not the library —
resuming a machine somebody had just discarded.)*

### 12.5 Power and boot targets

`Cmd::PowerOff | PowerOn | PowerCycle | Boot(BootTarget)` live on the device's drawer page and in the
Machine menu — **and `PowerOff` is also the centre button while `Booting`** (§7.3), because a
twenty-one-minute boot with no stop control is not a design, it is a hostage situation.

Power off is real — the machine is dropped and re-entered at the reset vector, not restored and
pretended.

`BootTarget::Nor("diag" | "disk")` are reached by **power-cycling**, because that is how the hardware
reaches them, and the row's verb is `Start into`, never `Switch to`. MENU + SELECT is **not** offered
as a restart: nothing measured in this project shows RetailOS acting on that combo, and a control
claiming to be the hardware combo while actually restarting the emulator would be the window lying.

| target | state |
|---|---|
| `Apple's software` | ✓ |
| `Diagnostics` | ✓ on a dumped ROM. On a synthesised one, **disabled, machine rule**: `Diagnostics lives inside the boot ROM's image directory, and a generated ROM has none.` |
| `Target disk mode` | **disabled, project state**: `Faults after about 128 000 instructions — Lost(0xe19b0000). It is a USB feature and USB is unmodelled.` |
| `An image…` | any raw ARM image expecting 0x10000000 — `rb-main.raw`, `ipodloader2` |

### 12.6 Fullscreen — the only place a scale above `k` exists

`⌃⌘F` on macOS, `F11` on Windows and Linux — the one row of §16.8's table that is not one column, and
§16.8 says why. `Window.full-screen` is a plain in-out boolean, so this is one property.

The whole display, `bg-sunken`, the framebuffer at
`K = floor(min(W_phys / 320, H_phys / 240))`, centred, `image-rendering: pixelated`, hard-edged. **No
glass, no border, no glow, no shader.**

**Recomputed in physical pixels, because the previous revision's table was not.** Every row of it was
computed in logical pixels while §6.6's own Rust computed `K` in physical ones from `scale_factor()`
— so the document contradicted itself, and the wrong half was the one rendered into a user-facing
string on §9.5's screen.

| display | sf | backing store, physical | K | drawn | chrome strip? `K × 240 + 44 × sf ≤ H_phys` |
|---|---|---|---|---|---|
| 1280 × 800 | 1.0 | 1280 × 800 | **3** | 960 × 720 | 764 ≤ 800 ✓ |
| 1366 × 768 | 1.0 | 1366 × 768 | **3** | 960 × 720 | 764 ≤ 768 ✓ |
| 1440 × 900 | 1.0 | 1440 × 900 | **3** | 960 × 720 | 764 ≤ 900 ✓ |
| **1470 × 956** (the operator's) | 2.0 | **2940 × 1912** | **7** | **2240 × 1680** | 1768 ≤ 1912 ✓ |
| 1512 × 982 (14″ MBP) | 2.0 | **3024 × 1964** | **8** | **2560 × 1920** | 2008 > 1964 ✗ |
| 1920 × 1080 | 1.0 or 1.25 | 1920 × 1080 | **4** | 1280 × 960 | 1004 ≤ 1080 ✓ |

The previous revision promised the operator's own machine 3× / 960 × 720 and printed that as a
sentence. It is **7× / 2240 × 1680**. The 14″ MacBook Pro was 4× and is 8×. The conclusion that only
the 14″ fails the chrome-strip test survives — reached by arithmetic that applies this time — and
**§9.5's sentence interpolates `K` from the same function rather than quoting a constant.**

**The chrome strip is 44 logical px, top.** It fades in on pointer movement, out 2.5 s after it
stops, and it sits on the black surround **outside the drawn rectangle**, never over it. Where it
cannot be drawn, `Esc` is the way out — and §7.3 now has a cradle row that says so before you enter
(`running · ⌃⌘F for 7× · Esc to come back`), which the previous revision claimed as its cover while
its own closed cradle set rendered the single word `running`. **Principle 9 outranks principle 5's
convenience, and this is where they meet.**

Keyboard is unchanged, which is exactly why the map has to be complete: the drawn wheel is not on
screen.

**Fullscreen is available whenever the glass has something on it** — `Booting`, `Running`, `Stopped`
with its last frame, or parked with a `.parked.png`. Otherwise it is disabled with its reason —
`Fullscreen shows the panel at the largest whole-number scale that fits. There is nothing on the
panel yet.` The previous revision made it `Running` only, which is why §9.5 offered a control that
§12.6 disabled in exactly the state §9.5 offered it in. A key that silently does nothing is worse
than one that says why; a key that is offered and refused in the same breath is worse than both.

### 12.7 Screenshots, and what cancelling costs

**Two keys, two files, two names**, because a program whose discipline is knowing which layer a
picture is of should not make you guess which one you got:

- `S` → the panel alone, **exactly 320 × 240**, no scaling, no body → `<device>-<n>-panel.png`. This
  is what `research/` and a bug report want.
- `⇧S` → the window as drawn → `<device>-<n>-window.png`. This is what `docs/media/` wants.

Both report the path and the size afterwards, per principle 7. **`⇧S` respects §11.2's masking**: a
window screenshot taken with the Composer open captures whatever is on screen, so the masking is what
protects it, and §11.2's position-only reason wording is what closes the last hole.

**Long work.** The target volume is checked, then free space, both before a byte is written. Builds go
to `<name>.img.part` and are renamed only on success, so a cancelled build leaves **no partial file
with a real name**. **Cancelling deletes only our own temporary file** — never the source, never
anything supplied, never anything already named. That rule has no exception and no "unless", and the
Work page says which file cancelling will delete **and how big it is right now** before you press it.

### 12.8 The Readout

A drawer page, `Region` + `accessible-live-region: polite`, built from **Gauges** and nothing else.
It **pushes the device aside; it never covers it** (principle 5, and the specific complaint filed
against OpenEmu's HUD).

**Its body is a Scroll and it has to be.** Thirty-six Row-shaped items at 44 px is 1 584 px of rows
alone, about 1 970 with headings and gaps, against a drawer body of roughly 715–800 px. The page
header stays fixed above it and the two action rows stay pinned below it, so the two things you reach
for are always in the same place and never scroll away.

**The Gauge's three-state freshness is the whole ethic and it is a property of the primitive, not a
discipline:**

| state | rendering |
|---|---|
| **live** — sampled within 500 ms | value in `fg` |
| **stale** — older, or the machine is off | value in `fg-disabled`; the group heading gains ` · stale` |
| **not measured** | **`—`, never `0`** |
| **final** — the machine stopped here | value in `fg`, group heading gains ` · final` |

A zero and an unmeasured are different facts and this repository has been burned by conflating them.
`final` and `stale` are different too: stale means we stopped looking, final means the machine ended
there.

Seven groups, fixed order, all present always:

```
 ┌──────────────────────────────────────┐
 │ ‹ MENU             Readout           │   ← fixed header
 │══════════════════════════════════════│
 │ MACHINE                           ▲  │   ← the Scroll starts here
 │  phase                    running    │
 │  instructions      1 612 004 992     │
 │  this session ·      487 220 016     │
 │  simulated                21.5 s     │
 │  wall                     34.8 s     │
 │  speed          14.2 M instr/s       │
 │  ratio           24.1 % of real      │
 │  stalled                   0.0 s     │
 │                                      │
 │ CORES                     arrivals   │   ONE column. `Stats::enters` is
 │  frame decoder               1 041   │   [u64; WATCHED.len()] — a flat
 │  button edge                12 880   │   array with no per-core dimension
 │  scroll accumulator          3 204   │   anywhere in Stats or Out. A second
 │  button event                  218   │   column could only be filled with
 │  wheel event                 3 201   │   an invented zero, and a zero and an
 │                                      │   unmeasured are different facts
 │ PANEL                                │
 │  shown surface       0x000e0000      │
 │  shown moved                yes      │
 │  other surface       0x00100000      │   a restored machine can be one
 │  other moved                 no      │   page-flip out of phase, and this
 │  lit pixels              74 057      │   is the only place that says so
 │  backlight              16 / 32      │
 │  steps up / down       214 / 198     │   a level that is not moving and a
 │  frames posted ·          1 041      │   pin that is not pulsing are
 │  dropped / suppressed     0 / 12     │   different diagnoses
 │                                      │
 │ INPUT                                │
 │  wheel position         41 / 96      │
 │  touched                    yes      │
 │  buttons                     ——      │
 │  hold switch                 off     │
 │  frames asked for ·          yes     │
 │  steps dropped                12  ⚠  │  ← warn above zero
 │                                      │
 │ BUS                                  │
 │  ata commands               611      │
 │  ready                      611      │
 │  irqs                   183 452      │
 │  co-proc frames ·         1 041      │
 │  co-proc commands ·         302      │
 │                                      │
 │ MEMORY                               │
 │  unmapped pages               3      │
 │  0x7000c000 0x7000c100 0x60000000    │   the addresses, not a count: the
 │                                      │   question it settles wants them
 │ PROVENANCE                           │
 │  restored from a snapshot at         │
 │  1 612 000 000 instructions.         │
 │  · marks a counter that starts at    │   ← the label that stops a healthy
 │  zero after a restore. The picture   │     restored machine reading as
 │  on the panel is real.            ▼  │     "RetailOS has never drawn"
 │══════════════════════════════════════│
 │  Screenshot the panel  320×240     › │   ← pinned footer, never scrolls
 │  Copy this readout                 › │
 └──────────────────────────────────────┘
```

**The CORES group is one column until the model carries two.** The previous revision drew it as
`core 0  core 1` with a literal `0` in every core-1 cell and captioned it *"this is where what the
two cores are doing lives"* — which is exactly the conflation the Gauge's own three-state rule
forbids, committed in the group whose caption claims the most. §17.Q10 is whether the run loop can
even attribute an arrival to a core; §3.3 has the field it would need.

`Copy this readout` puts the whole thing on the clipboard as text, which is what a bug report
actually needs.

*(**Built and wired, 2026-08-24**, and §17 Q10 is answered in the strongest form: **no**. It is not
that `Stats::enters` lacks a second dimension — it is that the only code that records an arrival at
all is inside `Machine::run`, the **CPU's** loop. `Machine::run_cop` is a reduced loop whose own doc
says *"the instruments do not see the COP: a `--calls` ring, a `--profile` or a `--storelog`
describes the CPU alone"*, and it carries no arrival capture. Every arrival ever counted is a CPU
arrival by construction, so a second column could hold only an invented zero. **The one column
stands and the caption claims nothing about two cores.***

*(**The BUS group's first row was the click wheel's, corrected 2026-08-24.** `ata commands` and
`ready` were `Stats::data_reads` and `Stats::data_reads_ready` — word reads of the **click wheel's**
DATA register, which `--selftest` prints as *"DATA reads N (M with a frame waiting)"* — so the row
that claimed to count commands issued to the drive counted serial traffic off the wheel, and a
machine whose drive had never answered would have shown a healthy four-figure number there. The tell
is in the sketch above: two rows claiming different things and carrying **611** twice. `Stats` now
carries `ata_commands`, which is `Ata::commands.seen()` — a census, not `sample().len()`, which is a
cap of 256 against a retail cold boot's ~706 — and the wheel keeps its own three rows under the name
of the register they come off. What it buys: research/04 row 9's A/B is **102** ATA commands for a
retail cold boot against **24** for the same boot with the IDE interrupt latch ablated, where 24 is
Apple's bootloader painting its own screen and never handing RetailOS the disk. The difference
between a boot and a bootloader is exactly that number, and until now it was not on the page.)*

*`readout.rs` is the model and `ui/readout.slint` draws it; `Gauge` is §5's sixth primitive, built.
Four decisions of this section are acted on rather than deferred: `Stats::queued` is **deleted**
rather than allowed, because a stated retirement condition met with a *no* is a dead field;
`sim_usec_here` earns its row and its allow comes off; the **ratio** row is still absent for want of
a stated divisor; and the group heading carries `— stale` / `— final` in the em dash, because `·` is
not in `geometry::GLYPHS` and §6.7's answer for a symbol is a drawn `Path` that Rust cannot make —
the marker for a counter that restarts after a restore is `*`, with `PROVENANCE` carrying the
legend.*

*One thing this sketch asks for that reading the picture deleted: **`instructions` / `this session ·`
and `simulated` / `simulated ·` are the same number on a cold boot**, because a restore is the only
thing that can make either pair differ. Drawn unconditionally they print one number twice, four
pixels apart, in the group that claims the most — which `_out/gui/readout.png` showed and no
assertion could, each producer being right on its own. The `·` rows are drawn only on a restored
machine, and the `PROVENANCE` paragraph is the legend for their absence.)*

**`Stats::sim_usec_here` and `Stats::queued` are `#[allow(dead_code)]` today, and this design
decides.** `sim_usec_here` earns a row — it is the honest simulated-versus-wall ratio now that idle
costs what running costs. `queued` does not: `input_dropped` is the number that matters, because a
refused step is a lie about what you did and a deep queue is only ever the reason for one. The allow
comes off both.

**Two corrections to the picture above, both found by trying to compute it** *(2026-08-23)*.

- **`ratio · 24.1 % of real` has no stated divisor, and the numbers in this very diagram match
  neither candidate.** `Config::clock`'s own doc says *"5 is what every recipe uses; 75 is real"*, so
  `14.2 M instr/s` against a real 5G is **18.9 %**. `simulated` against `wall` — 21.5 s against
  34.8 s, both read off this diagram — is **61.8 %**. And the diagram is not consistent with itself:
  `487 220 016` instructions in `21.5 s` of simulated time is **22.7** instructions per simulated
  microsecond, which is neither 5 nor 75. One of those four rows is wrong and nothing here says
  which. `machine::Pace` therefore publishes the speed it can measure — `here / wall_secs`, which is
  a division of two numbers the run loop actually keeps — and **no ratio at all**. The row comes back
  when somebody writes down what it divides by.
- **§7.3 wants `queued` on the cradle and this section refuses it a row.** The cradle's running line
  is specified as `running` — *or* `running · wheel 41 queued`, and `Stats::queued` is the field that
  would fill it. Two sections of one document asking opposite things of one number is the shape §16.9
  exists to delete. This section's argument is the stronger one and holds: `machine::Life` reads
  `queued` nowhere, and the running caption carries the measured speed instead. **§7.3's row should
  lose the clause**; it is left in place here rather than edited silently, because that table is the
  design's own wording.

**Cost discipline.** The run loop takes the `out` lock once per `SLICE` = 250 000 instructions —
about 56 times a second at 14 M instr/s — and memcpys 230 400 bytes into `out.fb` on refresh frames.
**The Readout polls at 8 Hz, not per frame**, and copies the scalar `Stats` out under the lock in one
go. A UI thread holding that lock blocks the machine directly. `fb_seq` gates the texture upload.

### 12.9 What the window will not become

Two things the machine publishes that this window is **not** going to grow surfaces for, stated so
their absence reads as a carve-out rather than an omission:

- `trace`'s ~90 measurement flags, `dis`, `tcb`, `eapp-inspect`, `ghidra`, `ipod-film`, the seven
  boot recipes, and the Unix control socket. These are terminal instruments for a person already
  holding a hypothesis, and putting them in a window would make it a debugger — which is the one
  thing this thesis must not become. The socket is additionally **absent by default on purpose**: *a
  socket that appears without being asked for is an interface nobody audited, on a program that reads
  a NOR dump and a drive image.*
- `put-zip` and `put-files`, because they modify the disk they are given, which is a different
  contract from everything else here. **And because they do**, a device whose image they touched
  between sessions has a broken snapshot pair — which §7.3 and §12.4 now say out loud rather than
  promising three seconds.

**The honest bridge is one row** on the device's drawer page: `Copy the command line for this
device ›`. **What it emits today is `ipod-boot make-nor --model <m> --seed <n> <m>-<n>.bin`** — the
recipe, and nothing else. That is one command and it runs; this section used to describe a second,
wider one, and the window shipped a third that was neither.

- The design was `ipod-boot --print`'s already-shell-quoted argv, with its provenance annotation for
  every input path (`# NOR dump: /path — environment | setup screen | repository default`). Nothing
  built it, and it is still the right shape for *running* a device from a terminal. It is not what
  the row does.
- The window emitted `ipod-boot retail --nor-model <m> --nor-seed <n>`. **Neither flag has ever
  existed**, and `retail` forwards a flag it does not recognise to `trace` unchanged — so the copied
  line did not fail. It booted whichever NOR the setup screen was pointed at, dropped both flags, and
  exited 0: a command that reproduced somebody else's iPod in silence. The composer's own test
  asserted that the string contained `--nor-seed`, which is a test agreeing with a string.
- `make-nor` is the spelling that moved, because it is the only subcommand that builds an iPod out of
  a model and a seed — `retail` boots a ROM, it does not mint one — and `GETTING-THE-FILES.md`
  already documented it. `settings::reproduce_command` composes the argv, the window joins it, and
  `ipod-boot`'s own test **executes** it and reads the identity back out of the ROM it wrote.

**And it masks by default, because the other surface does** — which the shipped row reaches by
carrying no identifier at all rather than by eliding one. `--print` shell-quotes the full argv, which
for a synthesised ROM includes `--serial` and `--guid` in the clear and for a dumped one names the
dump's path, so the program was protecting those identifiers on §11.2 and putting them on the
clipboard here with no sentence at all. When that wider bridge is built, the row states what it is
about to copy, `--serial` and `--guid` are elided as `…`, and a sibling toggle `include the serial
and GUID`, **default off**, turns them back on for the person who actually needs to reproduce a run.
Until then there is nothing to elide: `make-nor --model --seed` is a recipe, a typed identity has no
seed that reproduces it, and that case is refused with a sentence instead of copied.

**What does earn a surface**: `facts`, `syscfg`, `fat` browsing, `rsrc`, the firmware cache,
`make-nor --preview`, and `open-drive`. Separately: **five `ipod-boot` subcommands are missing from
its own `--help`** — `syscfg`, `ghidra`, `firmware`, `fat`, `rsrc`. It was six. `make-nor` came off
that list when the row above started handing people a `make-nor` command line: a command the window
copies has to be findable in the program's own help, or the first thing a reader does with it is
look it up and conclude it does not exist. Four of the five get a window; the help text is still a
bug and should be fixed.

---

## 13. Games (0.6), designed in

**A title is another thing that stands on the bench**, and that single sentence is why nothing has to
be bolted on. The bench has one body, one 320 × 240 glass, one wheel and one centre button, and
*press the centre button to run the thing in front of you* is already the whole interaction. So
Games costs **one drawer page and one cradle-label state**.

### 13.1 What is already true, so the design rests on facts

- A `.ipg` is a **zip** whose executable is an `eapp` container at load base `0x18000000`, with named
  imports as `ldr pc,[pc,#N]` thunks. **20 decrypted titles are on hand.**
- RetailOS **publishes** its framework surface at `0x000793fc`–`0x00079ce0`: **8 frameworks, 433
  functions** — `OpenGLES` 179, `Metadata` 152, `Audio` 61, `AsyncFileIO` 17, `miscTBD` 15,
  `Filesytem` 4, `Settings` 3, `InputEvents` 2 — each with a 16-byte interface hash **byte-identical**
  to the ones `eapp-inspect` reads out of a title's own import blocks. The ABI is confirmed from both
  sides at once. Pac-Man declares **98** of the 433.
- `Machine::bind_native(&EApp, only) -> Vec<(String, usize, usize)>` matches each framework by hash,
  rewrites every import thunk's literal slot to the real export, and **removes the corresponding
  trap** so a call that still lands in trap space is unambiguous.

**And one thing that is not true yet, stated as such.** Nothing in this tree reads a `.ipg` zip's
members: `grep -rni "ipg" --include='*.rs' tools/` returns **nothing at all**, and `cover` appears in
the codebase only as an anti-aliasing coverage variable in `nor.rs`. §13.2 draws the cover from the
title's name unconditionally, and **whether a `.ipg` carries cover art, and under what member name,
is §17.Q9** — with the 20 decrypted titles named as the way to answer it. A structural claim about a
file format the program does not parse is exactly what §16.9 exists to stop.

### 13.2 The readiness matrix is `bind_native`'s return value, drawn

Not *"will this boot?"* but *"this title declares 98 functions; we can serve 98"* — or *"we can serve
61, and here are the 37, by name."* **The trap table is the missing-function list**, exactly because a
bound import loses its trap.

That is principle 4 at the highest resolution this program will ever achieve, and it is the same
instrument as the boot matrix one layer up.

```
 ┌──────────────────────────────────────┐
 │ ‹ Games          Pac-Man             │
 │══════════════════════════════════════│
 │        ┌────────────────┐            │   drawn from the title's name.
 │        │   PAC-MAN      │            │   Whether the zip carries art is
 │        │                │            │   §17.Q9 — never a blank rectangle,
 │        └────────────────┘            │   never a stock icon
 │                                      │
 │  98 of 98 functions bound            │
 │══════════════════════════════════════│
 │ FRAMEWORKS                           │
 │  OpenGLES            179 / 179     › │
 │  Metadata            152 / 152     › │
 │  Audio                61 /  61     › │
 │  AsyncFileIO          17 /  17     › │
 │  miscTBD              15 /  15     › │
 │  Filesytem             4 /   4     › │
 │  Settings              3 /   3     › │
 │  InputEvents           2 /   2     › │
 │──────────────────────────────────────│
 │ SERVED BY                            │
 │  Frameworks   Apple's, from        › │   retail-versus-synthesised, one
 │               iPod_25.1.3            │   layer up. Same grammar as §11.
 │──────────────────────────────────────│
 │ IDENTITY                             │
 │  iPod         Black 5.5G           › │   §3.1 is what makes this
 │  Authorises   never — this identity  │   expressible: a title references
 │               is generated, and      │   an iPod for its GUID without
 │               matches no purchase    │   booting it
 │               ever made. A decrypted │
 │               title runs anyway.     │
 │══════════════════════════════════════│
 │ There is no boot. Press the centre   │
 │ button and the title is there.       │
 └──────────────────────────────────────┘
```

**The matrix has a dependency and the page states it.** `bind_native` matches a title's imports
against **RetailOS's** published framework table at `0x000793fc` — so with no `Installer` filed there
is no RetailOS image, nothing to match against, and the page's central figure cannot be computed at
all. Previously the page rendered `SERVED BY → Frameworks: Apple's, from iPod_25.1.3` as if the
dependency were incidental, and §9.1's Games empty state never mentioned it. So:

> **not measured** — a title's imports are matched against RetailOS's framework table, and no Apple
> firmware is filed.  `Fetch… ›`

in exactly §9.3's shape, with the same next-step-is-a-control rule.

**And the matrix is computed on a throwaway `Machine` seeded from the firmware bundle, never on the
one that will run the title.** `bind_native` **mutates** — it rewrites import thunks and removes traps
— so computing the matrix for a page you can open without pressing anything would have a side effect
on the machine, and opening a page would change what pressing ● does. Free inspection has to actually
be free.

**Framework choice surfaces only when it is the answer to a problem.** It defaults to whatever binds
most and collapses to one `fg-dim` line until something is short. Nobody should have to learn the
word "framework" to play Pac-Man.

### 13.3 And there is no boot

No 2.4 G instructions, no seventy-five seconds, no learned denominator. The cradle label goes
`press ● to play · there is no boot` → `running` in one frame, and the wake is the same 220 ms
backlight ramp — which now reads as *instant*.

**That is the best demo this program will ever have**, and this layout is the one that shows it: the
same iPod, the same button, and the wait simply is not there.

### 13.4 The refusals, and the one thing not promised

**An encrypted title is refused first-class, in our own words**, because the keystore work is not in
this repository and a refusal that reads as a bug report is a bug:

> This title is encrypted. It binds to the 8-byte FireWire GUID in the iPod's NOR — the one it was
> purchased against, not the one on this bench — and the keystore that mints those keys is not part
> of this program. A decrypted title runs; this one cannot, and we cannot help with that.

**A partly-bound title is not disabled at all.** `61 of 98` runs, in `warn`. Watching where a call
lands in trap space *is* the mode, and refusing to run it would delete the instrument.

**Before 0.6, the `Games` root row is a project state**: `0.6. bind_native() resolves a title's
imports against RetailOS's eight frameworks today; the surface that runs one is not built.
eapp-inspect reads a title's import table now.`

**Speed is a hypothesis with an obvious measurement, not a claim.** With Apple's frameworks the calls
are still emulated ARM at ~24 % of real time and the title may not be playable. With native
implementations the 179 `OpenGLES` functions become Rust and the emulated work collapses to the
game's own logic — plausibly the first mode that runs at real speed. The Readout's ratio Gauge is
exactly the instrument that settles it, and it is on screen while you play. Around 25 functions carry
all rendering across twenty titles, so the practical first cut is far smaller than 433 — the matrix
will read `25 of 98` long before it reads `98 of 98`, and the row already knows how to say it.

**What it must not grow: a shader pipeline.** The screen is 76 800 pixels. Any presentation effect
worth having costs nothing on the CPU, and rendering above 320 × 240 would change the machine rather
than the presentation, which principle 9 forbids as flatly as `unstable-wgpu-*` being off.

---

## 14. Where this deliberately differs from convention

### 14.1 Disable with a reason, where the rule elsewhere is hide-don't-disable

Mainstream guidance is against principle 4, and its reasons are specific: disabled controls "appear
clickable but provide no response", grey-on-grey often fails contrast, and screen readers frequently
cannot reach them. Those are real and they are why §9.4 and §16.5 are as prescriptive as they are —
**and the grey-on-grey one bit this design in §6.4**, where the "cannot start" cradle ring was
1.67 : 1 against the surface it is drawn on. A refusal that cannot be seen is a hidden option, which
is the rule this section exists to reject, arrived at by not doing the arithmetic.

**This program does the opposite on purpose**, because its subject *is* the compatibility matrix.
Which bootloader can carry which operating system on which ROM is not incidental complexity to be
smoothed away — it is the thing the user came to find out, and much of `research/` exists to
establish it. An option that silently vanishes teaches nothing. An option that is visible, greyed,
and says *"ipodloader2 reads FAT32 type 0x0B and this drive is 0x0C"* has taught the reader something
true about the hardware.

86Box — the closest hardware-accuracy analogue on the PC side — chose **hide**: it lists only CPUs
compatible with the selected machine. `compose.rs` is better-shaped for the opposite choice, because
it produces a paragraph and a remedy rather than a filtered list.

### 14.2 The drawn device is a control surface, and desktop accuracy emulators have declined that

VICE closed a virtual-keyboard request denied in 2020 — *"doesn't make sense on a proper OS"* — and
the device-as-control-surface is a touch-platform idiom (Delta, Provenance, `.deltaskin`). The
justification here is different from theirs and belongs on the record so nobody re-litigates it:
**the click wheel is not a keyboard.** It is four marks and a ring, it is the single recognisable
thing about this machine, and pressing its centre button is a better Start affordance than a chrome
button precisely because a 2005 iPod had no chrome.

**But this is not a skin format.** Delta's `.deltaskin` (JSON `items[]` with `frame`/`inputs`/
`extendedEdges`, `screens[]` with `inputFrame`/`outputFrame`, resizable PDF art) is right when you
serve dozens of consoles and hundreds of community skins. This program has one device, two model
variants and fourteen chassis colours. `ipod.slint` deriving everything from one `body-height` is
already the correct amount of abstraction; a data format would be a second source of truth for a
single drawing.

**Two things are stolen from that world anyway** and named: Delta's `extendedEdges` — **already
satisfied by `wheel.rs`**, whose `select` radius is 39 % wider than the drawn button because that is
the hardware's own membrane, so the borrowing is a recognition rather than an addition (§7.4) — and
MAME's `inputtag` idea, a drawn control's appearance reading the *emulated* state rather than the
pointer, which principle 3 now states as a two-edge rule because the emulated state does not exist
when the machine does not.

### 14.3 No sidebar, and the drawer is not one wearing a hat

The operator rejected a 280 px sidebar. The drawer is 420 px on the other edge, and the difference is
checkable rather than rhetorical:

- **It is not on screen by default.** The bench opens with it closed and one key closes it.
- **It holds pages you visit**, not a permanently-visible list of things to switch between.
- **It cannot shrink the subject.** The device is a constant physical size whether it is open or not,
  and `min-width: 880` already accommodates it open — the window never resizes under you.
- **A sidebar's cost is proportional to the configuration surface it serves.** UTM has ~10
  configuration categories across two backends and is converging *on* a sidebar for that reason. This
  program has six resource groups and one Settings page with three rows.

### 14.4 Nothing floats, and the genre agrees

Everything Slint gives away for free that floats is unusable here: `PopupWindow`, `ContextMenu`,
`TooltipArea` (whose 500 ms delay and 8 px offset are hard-coded and marked internal). Expand, Rail
and every disabled reason are built from ordinary layout elements, and that is more work than it
looks.

The external confirmation is worth citing rather than asserting: OpenEmu's own tracker carries an
open issue proposing that its floating HUD be replaced with toolbar controls, because it interferes
with the content — the same complaint that applies to this program's current `D` overlay.

**And "nothing floats" is why there are no toasts**, which the previous revision violated once, in
§7.4, with a four-second self-dismissing message on shelf row 2. It is easy to violate by accident;
the rule holds anyway.

### 14.5 Reference, not copy — and it is made visible

UTM bundles disks inside the `.utm` bundle, so a second VM from the same base image is a full copy.
Here `Device` holds **names** into shared lists. For a program whose images are 74 GB and are
sometimes the only copy of somebody's iPod, that is the safety property, and §11.4's `used by N` is
what makes it visible — as is §11.3's rule that the one control which quietly *changes* a reference
has to say so first.

### 14.6 The chassis has no off switch, and Apple's own simulator disagrees

`Show Device Bezels` is a menu item in the iOS Simulator, and turning it off is the documented fix
when the frame fights the scale. Here the escape hatch exists and it is called **fullscreen**
(§12.6): the panel alone, no body, at 3× to 8×, which is strictly better than the same thing at `k`
in a window. There is no separate bezels toggle, because it would be a third way to look at one
thing.

---

## 15. What is deliberately not in 0.5

Each for a stated reason, not for lack of time.

| out | why |
|---|---|
| **Games** (§13) | 0.6. The framework work is not done and the keystore is in a private repository. The design lands now so the shape is right when it does. `bind_native()` already resolves imports; `eapp-inspect` already reads a title's table |
| **iPodLinux as an offered system** | `Os::OFFERED` is 2. Its kernel boot is clean and ZeroLauncher stalls at "Finishing Up…" after a 101 MB download. Offered as a **disabled project state with its escape hatch**, never hidden — and §20 moves the escape hatch off a `make`d vendor binary onto the fetched `ipodlinux::LOADER`, which is what makes it an escape hatch rather than a second dead end |
| **Target disk mode as a working target** | a USB feature, and USB is unmodelled. `Lost(0xe19b0000)` after 127 952 instructions. Offered as a **disabled project state** |
| **Audio** | a 1.0 condition. The Wolfson codec is unmodelled |
| **The ~90 trace instruments, `dis`, `tcb`, `ghidra`, `ipod-film`, the boot recipes, the control socket** | §12.9. Terminal instruments for a person already holding a hypothesis. The bridge is `Copy the command line for this device`, masked by default |
| **A second window, tear-off panels, multiple machines** | there is exactly one machine, by design. §7.2 is what makes "look at a second device" possible without one |
| **A shader pipeline, any scaler above nearest** | §13.4 and principle 9 |
| **Remembering window position on Wayland** | `set_outer_position` is documented Unsupported there, and there is no work-area query either. Stated in Reference rather than pretended |
| **A theme beyond system / light / dark** | §11.6 has all three program settings on one page; a fourth theme is not one of them |
| **Re-deciding `k` on a plain window resize** | principle 1. §17.Q11 |

---

## 16. Implementation notes — Slint 1.17

Chosen over Iced, Dioxus Native, egui and Tauri: a semver-stable 1.x with a release every few weeks,
a real layout and styling system, live preview, and GPLv3 matching this repository. `backend-winit` +
`renderer-skia` only; `unstable-wgpu-*` deliberately off.

### 16.1 The pushed `hero`, and the binding-loop trap

**Reading a size that the layout itself decides is a binding loop.** The window's height comes from
its layout, the layout from the content, the content from `hero` — so `property <length> hero:
root.height - 200px` is a hard compile error in the strict case and a deprecation warning that says
it "may cause panic at runtime" in the inherited case. Slint's own test fixture
(`tests/syntax/analysis/binding_loop_layout_if.slint`) is exactly this construct.

Consequences, and they are constraints rather than advice:

- **`hero` is a length pushed in from Rust and never read from the window inside markup.** It is
  computed from `scale_factor()` and a measured `size()` (§6.6) — both **window** properties set by
  the platform, not sizes the layout decides.
- **The arrow only points one way as long as the Window's own `min-height` and `preferred-height` are
  plain constants that do not read `hero`.** They are: 400 and 846. If either were bound to `hero` in
  the same frame the loop would close and oscillate, and this is the sentence that stops somebody
  "tidying" it.

  **And the compiler will not catch that one — measured 2026-08-21, three ways.** With
  `preferred-height: root.hero + Geometry.chrome-pref` on the Window,
  `cargo build -p ipod-gui 2>&1 | grep -ci 'binding loop'` prints **0**; so does a version reading a
  property bound to the client's height. A genuine two-property cycle prints **3**
  (*"The binding for the property 'pb' is part of a binding loop (root.pa -> root.pb -> root.pa)"*),
  and a deliberate `visible: false` on the root reaches the same grep as a `cargo:warning=` — so the
  plumbing is real and the *window-layout* case simply is not a cycle in the property graph: `hero`
  is an `in property` and reads nothing back. **The grep is a general regression net and is not the
  guard for this paragraph.** The guard is the text sweep
  `the_window_constants_do_not_read_the_hero`, which does exist and does go red on all three shapes.
- **The container that holds the device must declare no intrinsic height** — `min-height: 0px;
  preferred-height: 0px` — so the arrow points one way there too. Every container between the window
  and the device does it now: `Bench`, the drawer page's `Scroll`, and the `Flickable` inside it.
- **`root.height` is banned in markup; `root.width` is not.** The vertical axis is where the loop
  closes; the horizontal one has no pushed-in size the layout influences, and the drawer *pushes*, so
  the well's width has to come from the window's.
  `nothing_in_the_markup_reads_the_windows_own_height` is the mechanical form.
- **There are no responsive breakpoints and there cannot be.** "One layout, nothing collapses at a
  breakpoint" is not a taste rule, it is the only thing Slint permits. §9.5's too-short state is
  therefore decided **in Rust** and pushed in as a boolean.

**And the boolean is recomputed on `Resized` and `Moved`, not only on startup and
`ScaleFactorChanged`.** The previous revision promised it was decided *"once, on startup and on
`ScaleFactorChanged`, never while you are looking at it"*, and that promise is not one the platform
can keep. Two ways to break it, both ordinary:

- **Drag the window's bottom edge up** to make room for a terminal. Neither event fires. Every term
  in §9.6's column except the top margin is a fixed `height:`, so §16.2's adjuster can take nothing
  from any of them, the loop exits oversized, and the trailing children — the shelf, carrying
  `write_target()` — are positioned past the container's bottom edge and drawn there. The user is now
  writing to a disk with the warning off-screen.
- **Drag the window onto a second monitor of the same scale factor.** No `ScaleFactorChanged` fires,
  the window keeps its 846 px height on a 735 px client, and the same thing happens.

So: recompute on `Resized`, `Moved` and `ScaleFactorChanged`, with hysteresis (drop below the
threshold, restore 20 px above it), and **delete the "never while you are looking at it" claim**.
`k` itself is *not* recomputed on `Resized` — only on `Moved` and `ScaleFactorChanged` — which is
principle 1 and §17.Q11.

### 16.2 Nothing gives, so shrinking must be designed

Setting `height:` on a layout child sets `min = max = that value`. When a layout is smaller than the
sum of its children's minimums, the `Shrink` adjuster can only take `size − min` from each item —
**an item pinned by an explicit `height:` contributes zero** — the loop exits still oversized, and
the trailing children are positioned **past the container's bottom edge**
(`i-slint-core-1.17.1/layout.rs:189-202, :232-268`). Layouts do not clip by default, so the surplus
draws outside the pane, silently.

That is the same class of failure as the 560 px hero: it looks fine and it is wrong. §9.6's budget
gives exactly one elastic term (the top margin), §9.5 is what happens when it runs out, and **§5's
Scroll is what happens on a drawer page**, where the content is genuinely unbounded and no budget can
be drawn.

Also: **stretch factors are inert unless the layout's alignment is `stretch`.** With
`alignment: center | start | end | space-*` and available space ≥ preferred, every item gets exactly
its preferred size and `vertical-stretch` is never consulted. The comment on the chrome bar's
`HorizontalLayout` in `window.slint` — *"setting one packs the children and makes
`horizontal-stretch` inert"* — is correct, and this is the mechanism.

### 16.3 `visible` versus `if`, decided per element

`visible: false` lowers to a wrapping `Clip` element with `clip: !visible`, and that pass runs at
step **180**, *after* layout lowering at step **156** — so **the layout has already allocated the
cell and the element keeps its full space.** That is principle 2, mechanically, and it is the default
here: optional captions, the cradle label's content, the shelf's row-3 trailing slot while the drawer
is open, the `No iPod is plugged in` row, every line that comes and goes.

`if` becomes a conditional repeater whose cell the layout genuinely omits **and it destroys the
subtree** — which is why the shipped `if tab == 0:` throws away all carousel state, hover and focus
on every tab change. `if` is used here only where an unbounded list genuinely has no rows.

And: **`visible` on a component's root element is silently ignored** and only warns. It must be set
at the use site.

### 16.4 Drag and drop is a winit hook, not `DropArea`

**Slint's `DropArea` cannot carry file paths at all.** `DataTransfer` holds an image, plain text and
an internal `Rc<dyn Any>`; there is no file-path or uri-list variant.

The route is winit's raw events through `WinitWindowAccessor::on_winit_window_event` or
`Backend::with_custom_application_handler` — **the same escape hatch `opaque_window()` already
uses**. winit 0.30.13 delivers `DroppedFile(PathBuf)`, `HoveredFile(PathBuf)` and
`HoveredFileCancelled`, **one event per file, carrying no cursor position, and with no event that
says the drop is over.**

Three design consequences, and the third one was missed:

1. A window-wide target is the only kind that can be built, which makes §11.4's *"there is no wrong
   target"* the only implementable design rather than merely a good one.
2. Identification has to be cheap enough for a hover, so §11.4 size-gates it and defers hashing.
3. **Multiplicity has to be coalesced by the program**, because nothing in the event stream marks the
   boundary between one drop of eight files and eight drops of one. §11.4's 150 ms window.

**Windows note**: winit's file drag-and-drop uses apartment-threaded COM and "will interfere with
other crates that use multi-threaded COM API on the same thread". Any file-picker crate added later
must agree with that apartment model. **There is no file picker in the dependency graph today** —
`cargo tree -p ipod-gui | grep -iE "rfd|native-dialog|ashpd"` is empty — and adding `rfd` pulls GTK
or xdg-desktop-portal on Linux. That is a real dependency decision (§17.Q3), not a checkbox.

### 16.5 The disabled construction, specified once

Two traps, both verified in the toolkit source:

- A `TouchArea` with `enabled: false` **forcibly sets `has_hover = false`** and forwards the event to
  whatever is underneath (`i-slint-core-1.17.1/items/input_items.rs:81`).
- A `FocusScope` with `enabled: false` refuses focus "neither via click nor via tab focus traversal,
  **not even programmatically**".

So a disabled control that carries its own reason is unbuildable the obvious way. The shape, and it
is written once so it is not reinvented per control:

> **An always-enabled outer `TouchArea` + `FocusScope`** carrying hover, the focus ring and the
> reason, **wrapping an action gated by a plain boolean.** Never `enabled: false` on either.

**Proof obligation**: a disabled control must state its reason under **keyboard focus alone**, with
the mouse untouched. That is also NN/g's specific complaint about disabled controls answered rather
than dismissed.

Note that `ipod.slint` walks into this today: `centre-touch` is `enabled: pressable` and line 165
reads `centre-touch.has-hover` through the same flag.

### 16.6 Text, fonts and the glyph rule

Slint takes **one `font-family` string per element with no fallback list**. Slint 1.17's text stack
is parley 0.10 + fontique 0.10; script-based fallback exists inside fontique but runtime font
registration is behind `unstable-fontique-010`, and **nothing in `.slint` can ask whether a glyph
exists.** That is the mechanical reason §6.7's icon set is closed and drawn, and why the glyph test
widens rather than retires.

`Path` supports SVG `commands` or declarative children, with `fill` (a brush, so gradients work),
`stroke`, caps and joins — and **no dash array**. `commands` "can only be set in a binding and cannot
be accessed in an expression". That is why §7.3's refused cradle is a **broken** ring — four arcs
with gaps — rather than a dashed one: a gap is a shape and a shape is buildable.

### 16.7 Accessibility, and what it does not buy

`accessibility` is in slint's default feature set, and `tools/ipod-gui/Cargo.toml` set
`default-features = false` and did not list it. `grep -c accesskit Cargo.lock` returned **0** at
`5cf06c7`. Every ARIA claim in the previous revision was false as built.

**Turned on 2026-08-21** (§20 item 11), as a named feature that is on by default — named rather than
unconditional so that `cargo test -p ipod-gui --no-default-features` is a control that makes
`accessibility_is_compiled_in` go red. Two tests, deliberately not one: that one asks whether the
feature is set, and `the_lockfile_carries_accesskit` asks whether the crate is actually **resolved**,
which is the thing the ARIA claims were false about. Under `--no-default-features` the first goes red
and the second stays green, because a lockfile records the union of all features.

Measured cost, aarch64-apple-darwin, `--release` with this workspace's `lto = "thin"`: **+5 crates**
(`accesskit`, `accesskit_consumer`, `accesskit_macos`, `accesskit_winit`, `uuid`). **Linux is not
that cheap**: `accesskit_unix` pulls the AT-SPI stack and the graph grows by **16**, not 5 — worth
knowing before the first Linux CI job, and worth not reporting one platform's number as the number.

Then: `Region` / `Complementary` / `Main` for surfaces,
`accessible-live-region: polite` for the Rail and the Readout, `accessible-expandable` / `-expanded` /
`accessible-action-expand` for Expand, `accessible-item-selectable` / `accessible-item-selected` /
`accessible-item-index` / `accessible-item-count` for lists, `accessible-enabled` and
`accessible-description` for every disabled control.

*(**The list spellings are corrected 2026-08-21.** This section abbreviated them to `-index` and
`-count`, which are not properties: `accessible-index` and `accessible-count` do not compile.
`no_accessible_property_is_set_without_a_constant_role` checks the spelling as well as the role,
because the abbreviation is the sort of thing that gets copied out of a design document.)*

**`ItemRc::is_visible()` is a GEOMETRY test, not a read of the `visible` property**, and anything
resting on ARIA has to know it. It intersects an item's absolute rect with its absolute clip rect
(`i-slint-core-1.17.1/item_tree.rs:399-408`) and never consults `visible` at all; `visible: false`
reaches the accessible tree only because it lowers to a `Clip` element that empties that rect
(`passes/visible.rs`). Two consequences that bit:

- An element hidden with `opacity: 0` is **fully focusable and fully clickable** — `Opacity`'s input
  filter is `ForwardAndIgnore` and `is_visible()` never looks at it. `opacity` is not a way to hide
  a control.
- An element clipped away by an ancestor is out of the accessible tree **and out of the tab order**,
  with one exception: `WindowInner::move_focus` calls `is_visible_or_clipped_by_flickable()`
  (`window.rs:1328`), which recovers an item only when the hiding ancestor is a `Flickable`. A
  Rectangle's `clip` has no such recovery — see §16.11's `MenuPage` note.

So a closed drawer parked at exactly the client's edge is out of the tree **by a zero-width margin**,
which is not a guarantee. It carries an explicit `open` boolean as well, and
`the_closed_drawer_gate_is_a_boolean_and_not_only_geometry` is what keeps that term from being tidied
away — the behavioural test alone passes with it deleted, because the parking happens to be exact.

**One correction, made when the feature was turned on.** This section used to say
`AccessibleRole::TextInput` additionally has a *behavioural* job — that §16.8's shortcut suppression
is keyed on it, making the feature a prerequisite for the keyboard as well as the screen reader.
That is not true as built. `accessible-role` is a compiler-level property with no `cfg` near it
(`i-slint-compiler-1.17.1/builtins.slint:1636`) and nothing in `i-slint-core` gates it on
`feature = "accessibility"`; more to the point, §16.8's mechanism does not query the role at all —
the suppression is a root `FocusScope` that a focused `TextInput` consumes character keys ahead of.
That is focus, and it works with the feature off. There is also no supported Rust route to ask what
role has focus: `ItemRc::accessible_role()` exists (`i-slint-core-1.17.1/item_tree.rs:505`) but
`ItemRc` is not public API. The feature is still required — §16.7's rule is that an ARIA claim must
be true as built — but for the **announced** half, which is the honest reason.

**The honest gap**: a 96-detent ring has no announced equivalent — Slint has `Slider` but a wheel is
not one, and the `↑` / `↓` keys are the accessible route and are a fallback, not a peer. Say so once
rather than pretending.

### 16.8 Keyboard, shortcuts and the menu bar

**⌘ and Ctrl are one binding.** On Apple platforms Slint's winit backend swaps them — ⌘ (winit
`Super`) is delivered as `Control` (`i-slint-backend-winit-1.17.1/event_loop.rs:258-274`) — and the
compiler explicitly rejects `Cmd` / `Command` / `Win`. So `@keys(Control + ",")` is ⌘, on macOS and
Ctrl+, elsewhere; write the table with one column and use `Platform.os` only for the printed hint.

| key | does |
|---|---|
| `Tab` / `⇧Tab` | focus, in document order. Never a positive tabindex |
| `Esc` | **one definition, outwards, in order**: leaves fullscreen · then closes an Expand · then **goes back one drawer level** · then closes the drawer · then, from `Running`, parks. **From `Booting` it powers off** |
| `Enter` / `Space` | the primary action — on the bench, the centre button; on §9.5's bench, its primary row |
| `←` `→` | the wheel while there is a machine; previous / next device when there is not |
| `↑` `↓` | the wheel, always |
| `M` `P` `N` `B` | MENU, Play, Next, Prev — only while there is a machine |
| `H` | the hold switch |
| `⌘,` · `?` | Reference · Reference on help |
| `⌘\` | the drawer |
| **`⌃⌘F` / `F11`** | fullscreen — **the one row that is not one column**; see below |
| `S` · `⇧S` | the panel · the window |
| `D` | the Readout page |

**The four machine rows were built 2026-08-24, and the table understated two of them.**

- **`M` `P` `N` `B` and `H` are not silent on an empty bench.** The table says *only while there is a
  machine*, which is true of what they send and says nothing about what happens instead — and *what
  happens instead* is the whole of §14.1. They put §7.4's sentence on the cradle label, held while
  the key is down, exactly as a press on the drawn control does. `↑` `↓` are the same.
- **`← →` are the one row that must not refuse**, because they have a second job: with no machine
  they are previous / next device, and a machine's half that claimed the key would delete §7.2's
  only keyboard route to another device, silently. So the machine answers *not mine* for those two
  keys alone and the bench's own shortcut gets them — which is the shape the row was already written
  in, made mechanical.
- **A modifier ends the claim.** `event.text` for `⌘M` is still `m`, so without a guard a ⌘-chord
  this program does not define would press MENU on the emulated iPod. Only unmodified keys reach the
  machine's table; `Tab` and `Esc` never arrive as text and `⌘,` / `⌘\` are `KeyBinding`s, which run
  ahead of `key-pressed` entirely.
- **Lower case only.** `m` is the key the table means; `⇧M` is a different keystroke and the table
  does not claim it. The `S` · `⇧S` row is the proof that the shift matters in this program, so
  answering both from one arm would be inventing a rule.

**And a key that reaches the machine comes up as well.** A press with no release is a stuck finger,
and the wheel's `Touch` needs a `Release` for the same reason — so the root `FocusScope` handles
`key-released` too, which it did not before. Holding `↓` is therefore **one** contact with a stream
of clicks in it, at the platform's own auto-repeat rate rather than one this program invents.

**Which surfaced a hole in the exception above, and it is measured rather than reasoned.**
`TextInput` consumes a character key's **press** — which is what makes the exception focus rather
than a mode — and **ignores its release**: `i-slint-core-1.17.1/items/text.rs:1106-1115`'s
`KeyReleased` arm answers `EventIgnored` unless the field's own `key-released` callback accepts, and
by default it rejects. So the release bubbles to the root scope for a press that never got there,
and typing `m` into the Composer's Name field over a running machine would have put MENU **up** on
it. Harmless today — it clears a bit that is already clear — and not harmless the day MENU is held
by something else. **A release is therefore honoured only for a press this scope took**, which is
the half focus cannot cover, and it is the one place in §16.8 where the window keeps state about a
key at all.

**`Esc` had three incompatible definitions and now has one.** §4 listed it as a way *into* the
drawer, §16.8 defined it as a pure exit, and §12.6 said it was the way out of fullscreen — so on a
14″ MacBook Pro, where §12.6's chrome strip cannot be drawn and `Esc` is the only exit, the same key
might instead have initiated a 1.6 GB park. It is deleted from §4's list of entrances (there are
already four) and the order above is total. And **from `Booting` it powers off rather than parking**,
because parking a boot is a 1.6 GB write of a state nobody wants.

**Amended 2026-08-21 to five steps**, in the shape §5 used when it admitted a ninth primitive: the
drawer-level pop was in the built order and not in this table, so the document promised a different
`Esc` from the one the program has. It mirrors the header's `‹`, which is what makes the key and the
control agree; the cost is that the deepest surface takes **six** presses to reach the bench
(fullscreen, an Expand, three levels, close) rather than the three an earlier draft of the test
asserted. `every_surface_can_be_left` derives its bound from those terms — `2 + DRAWER_MAX_DEPTH + 1`
— and asserts the deepest surface reaches it exactly, so the bound is met rather than merely
respected. **Neither `Esc` nor `⌘\` ever writes `depth`**, so reopening returns you where you were.

**One exception, and it is focus rather than modality.** *Single-letter shortcuts and `Space` are
suppressed while an element with `AccessibleRole::TextInput` holds focus.* Only modified keys
(`⌘,`, `⌘\`, `⌃⌘F` / `F11`), `Tab`, `Esc` and `Enter` survive.

Without that line the Composer is unusable and the previous revision's claim — *"There is no modal
keyboard… Every other key is the window's, **in every state**"* — is false the moment §11.2 exists.
Type `My 5.5G` into the Name field and you fire `M` (MENU), `Space` (the centre button, i.e.
`Cmd::PowerOn`) and `S` (a screenshot written to disk); type a serial and you fire `S`, `M`, `S` and
`⇧S` on the shift-held capitals. The Composer has four text inputs and every one of them was
unusable. Slint offers exactly two shapes and no third: declared with `KeyBinding` or
`capture_key_pressed`, the shortcuts intercept window-down and the field cannot be typed into; on a
bubbling root `key_pressed`, the `TextInput` consumes them first and the shortcuts silently stop
working. The second is the correct one, and it is delivered through `FocusScope` bubbling with
`reject` — **that is focus, not a mode**, and it is the "in every state" claim that has to go.

**The fullscreen row is the one exception to the one-column rule, and the reason is mechanical.**
`⌃⌘F` needs both modifiers, so on Apple platforms it is `@keys(Control + Meta + "F")`. The swap only
applies on Apple platforms, so on Windows and Linux that same binding is **Win+Ctrl+F** — reserved by
the Windows shell and by most Linux compositors, so the keystroke never reaches the program. It is
platform-selected in `main.rs`: `⌃⌘F` on macOS, `F11` elsewhere, which is each platform's own
convention rather than a compromise between them.

**The menu bar.** Slint 1.17 has a real `MenuBar` / `Menu` / `MenuItem`, and `muda 0.19.3` is already
in the tree (`i-slint-backend-winit` depends on it for `macos` and `windows`). Slint's own
documentation says **the Window's `width` and `height` define the client area, excluding the menu
bar**, so on macOS it costs nothing from §9.6's budget.

Two caveats, both real:

- ~~**`MenuBar` "must not be in a `for` or an `if`"**~~ — **corrected 2026-08-21, checked in the
  compiler.** Only `for` is rejected: `process_window` errors under
  `if !repeated.is_conditional_element` (`passes/lower_menus.rs:465-471`) and threads the condition
  into the generated `MenuBarImpl`. So
  `if Platform.os == OperatingSystemType.macos : MenuBar { … }` compiles, and the
  two-top-level-components workaround is **not needed for the menu bar**. It is still needed for
  nothing else here, so it goes.
- **On Linux there is no muda**, so Slint renders the bar itself — which costs outer height and may
  need a style this crate does not compile in. **Measure it before trusting it.** §17.Q4.

Every menu item is also a drawer row. Nothing is menu-only, which is what makes the menu bar a
convenience rather than a dependency.

### 16.9 Model updates, and one mechanical rule

**Do not rebuild `ModelRc`.** `window.set_devices(ModelRc::from(Rc::new(VecModel::from(rows))))`
replaces the model wholesale, tearing down and reconstructing every repeater instance — losing focus,
hover and any in-flight animation. That is exactly the jumping principle 2 forbids, and `main.rs`
does it today in both `device_rows` and `resource_rows`. Keep one retained `Rc<VecModel<_>>` per list
and call `set_row_data`. It matters most for the Rail, which appends constantly.

**And the rule this whole revision exists to install: no prose claim about the window without a check
that can fail.** Every drift found in §1.1 is the same shape — a document describing a deleted
layout, a comment describing an unset property, a test citing a file it cannot read, a README
advertising removed features — and §19 shows the shape survived one full rewrite: a cover-art claim
about a file format nothing parses, a `core 1` column with no per-core field behind it, a
`parked · 4 min ago` with no timestamp in the model, a cradle state nothing computes. The design layer
gets the discipline `research/` already has: **the constants live in one place, the tests read them,
and a claim without a check is deleted rather than left standing.**

Mechanically: `build.rs` emits every ratio and every geometry constant from **one Rust source of
truth** — `tools/ipod-gui/src/geometry.rs` — into a generated `.slint` file, which the markup imports
as `@geometry`. The tests read that same Rust module directly, so the test reads what the markup
reads rather than hand-copying it. `min-width: 880` is one of them, and its derivation lives beside
it (§9.6) rather than in a parenthetical that sums to 449.

*(Built 2026-08-21. This paragraph used to say "into **both** a generated `.slint` file and a Rust
`const` module". Only the `.slint` is generated: the Rust `const` module **is** the source, and
`build.rs` compiles that same file via `#[path = "src/geometry.rs"]`. Generating a second Rust copy
of a Rust source would add an artifact that can go stale without adding a check that can fail —
which is the failure mode this section exists to close.)*

### 16.10 The tests, and proving they can fail

**Re-express, do not drop.** The current window's tests caught real regressions — twelve missing
glyphs, then two more within the hour.

| test | asserts |
|---|---|
| `the_panel_is_an_exact_integer_number_of_device_pixels` | drawn width is an exact integer multiple of **320 physical px**; drawn height is the **same** multiple of 240; the aspect is 4:3 to a float epsilon. **Runs at `sf ∈ {1.0, 1.25, 1.5, 1.75, 2.0, 2.25, 2.5, 2.75, 3.0, 3.5}` × `k ∈ 1..8`** — and **it goes red at 2.75 / 7 today**, which is the prove-it-can-fail obligation satisfied by a real case rather than an assumed one (§6.6). Also **prove the ratio arm**: set the ratios back to `0.4866 / 0.3672` and watch it go red before trusting it |
| **`the_glass_is_the_panel_plus_ten_and_a_half_physical_pixels`** | the black surround is `0.016 × hero_phys` on **all four** sides at every `k` and every `sf`, and the glass is computed **from** the panel rather than from a ratio — so a re-measured bezel cannot silently flip which axis bounds `k` (§6.6) |
| `the_column_terms_sum_to_the_declared_chrome` | derives §9.6's terms from the generated constants — not from typed copies — and asserts they sum to `CHROME_MIN` and `CHROME_PREF`, **including the cradle's overhang and focus ring above the body as well as below**. Drop the top band and it reports 138 against 154, which is the omission §9.6 describes. *(This row used to name a test `the_column_fits_the_declared_minimum` asserting `sum ≤ min_height`. Under §9.6's own numbers that is `809.751 ≤ 400`, which holds at no `k` and no `sf` — a test that can only fail, and would then be weakened until it passed. What §9.6 establishes is the sum, so that is what is asserted; `the_minimum_height_is_a_floor_not_a_fit` carries the other half.)* |
| `the_drawn_ipod_is_the_shape_of_a_real_one` | survives unchanged except for the two corrected ratios and the added hold switch |
| `every_surface_can_be_left` | every drawer page reaches the bench in ≤ 3 presses of one key |
| `every_disabled_control_states_its_reason_under_keyboard_focus_alone` | §16.5's proof obligation, with the mouse untouched |
| **`a_single_letter_shortcut_does_not_reach_the_window_while_a_field_has_focus`** | §16.8's one exception, and it must be shown to fail with the suppression removed |
| **`every_cradle_state_clears_three_to_one_against_bg_sunken`** | §6.4's table, computed rather than eyeballed, in both schemes |
| `dropped_files_route_themselves_in_either_order` | comes across unchanged, and keeps its two-file scope |
| **`an_ambiguous_drop_files_and_makes_no_device`** | §11.4's rule for two ROMs or two `.ipsw`s |
| **`a_fix_that_changes_a_resource_reference_says_so_and_needs_two_presses`** | §11.3, and it is the assertion `every_fix_resolves_the_thing_it_is_offered_for` does not make |
| `no_ui_string_carries_a_glyph_outside_the_closed_set` | widened per §6.7, twice: markup, then this crate's Rust, then the model's library. It caught the shipped ` · ` in the window and then four more in `eapp-loader`. *(Listed as `no_ui_string_contains_a_glyph_the_font_is_not_proven_to_have` until 2026-08-23; no test was ever called that.)* |
| `the_menu_preview_is_in_the_order_the_loader_writes` | already exists in `compose.rs`; the window renders it verbatim. (Earlier drafts called this `the_verdict_preview_matches_what_the_installer_writes`, which no test has ever been called) |
| `every_fix_resolves_the_thing_it_is_offered_for` | already exists in `compose.rs`, and is now swept over three `Start`s — a fix that lands on rule (0)'s nothing-chosen state is a terminus, and one that lands anywhere else carrying a fix still fails |
| **`a_zero_c_volume_refuses_ipodlinux_whatever_bootloader_is_showing`** | §11.3 rule (2), which is a fact about the volume and so must fire on every bootloader **and from either way of choosing a drive** — the guard that made rule (1)'s fix chain into rule (2)'s refusal, and the `Start::FromDisk` half it skipped |
| **`the_panel_is_an_exact_integer_number_of_device_pixels`** | §20 item 10. Swept `SF_SWEEP × 1..=K_MAX`, and it found `next_up_until` stepping f64 ULPs across an f32 grid: 40 of the 160 cases drew the panel short of its own framebuffer |
| **`the_display_decides_k_and_the_window_decides_the_warning`** | §9.5 / §16.1. `k` from the display, the too-short boolean from the window — the two were one value, so a window dragged short and then moved reported room it did not have |
| **`the_default_recipe_says_nothing_is_chosen_yet`** | §11.3 rule (0). It **replaces** `the_default_recipe_works`, which asserted the opposite and was wrong |
| **`the_bootloader_tooltip_is_about_the_bootloader_even_before_a_firmware_is_chosen`** | the `check` / `check_parts` split, without which the whole bootloader picker greys out reading `nothing chosen yet` |
| **`a_volume_type_discovered_later_is_not_a_change_to_what_a_device_boots`** | §12.3: `BootShape` excludes `Start`, so a background read completing does not throw away a denominator |
| **`the_loader_override_wins_and_an_override_that_points_at_nothing_is_an_error`** | §20 item 7: `IPOD_LOADER=` is honoured or refused, never quietly ignored |
| **`nothing_reaches_for_the_vendored_loader_any_more`** | §20 item 7: no `.rs` under `tools/` joins `resources/vendor/ipodloader2`, so the path that worked only inside this checkout cannot come back |
| **`the_fit_is_computed_from_the_size_the_platform_reports`** | §9.5 / §9.6 / §16.1, and it is the only test in this workspace that **launches the window**. Every other geometry test checks a number *about* one, and the headless backend cannot stand in: `i-slint-backend-testing` applies no minimum clamp, so it reads the same whether or not the defect is present. It reads `IPOD_LAYOUT=1`, and it **drives the window from outside the process** — a size this program cannot predict, because the two assertions that only compare the program with itself are both satisfied by a constant |

**`IPOD_LAYOUT=1` exists now** (2026-08-21). It was documented in `docs/DEVELOPING.md` and
implemented nowhere — `grep -rn 'IPOD_LAYOUT' tools/` returned nothing — so it was built rather than
deleted. `client_height::dump_layout` prints the work-area answer this build can give, the measured
display height, the window, the fit, the threshold, the glass, the inset, and every constant in
`geometry::ALL` once. On the operator's own machine, at the first event after startup:

```
── IPOD_LAYOUT ────────────────────────────────────────────
  work area   VisibleFrame — the usable height of the display this window is on, …
  display     923.0 logical px usable
  window      2360 x 1692 physical — Slint's cached size, which inside the event filter is one event old
  platform    2360 x 1692 physical, 1180.0 x 846.0 logical at scale 2 — winit::Window::inner_size(), asked now
  measured    846.0 logical — the height the fit below was computed from
  fit         k = 2, body 655.751 logical (1311.502 physical), panel 320.0000 x 240.0000
  needs       809.8 logical / 1619.5 physical for k = 2
  glass       661.0 x 501.0 physical, 10.49 px surround on all four sides
  inset       0.05186 of body height at the sides, 0.05250 at the top
```

That is §6.6's operator row and §9.6's `923` measured rather than asserted — and it is what caught
the one real defect in the wiring: with `k` decided from `min(window, display)`, the size winit
reports during window creation dragged `k` down to 1 and raised the too-short flag before a later
event corrected it. `k` is decided from the display; the too-short boolean from the window.

**`window`, `platform` and `measured` are three lines because they are three different sizes**, and
printing two of them as one is what turned a real defect into a bug report for a defect that does not
exist. `window` is Slint's cache; `platform` is the platform, asked now; `measured` is the height the
fit was computed from. For two revisions the block printed the first and the third, so at startup it
read as *too short for 1:1* beside a window comfortably tall enough and then the reverse one event
later — which four separate investigations read as *"the window collapses to 880 × 400"*. **It never
did.** Measured from outside the process with the accessibility API, the window is 1180 × 878 outer
from 0.5 s to 5 s after launch and at no point anything else; the 880 × 400 was the *creation* size
printed against a stale cache. The real defect underneath was that the fit was computed from that
creation size, and it is fixed (§9.5's box, §9.6). `measured` is now compared against `platform`
rather than against `window`, so a difference printed on that line means a defect rather than the
one-event lag the cache always has.

**And that fix was half-made**, which is worth recording because the prose above was written as
though it were whole. Splitting the two inputs was done in `main.rs`, where `ceiling_logical` reads
the display and `own_height_logical` reads the window — but `fit::Moment` still carried **one**
`client_logical` per variant, so `Fitter::apply` fed the display's number to the too-short
comparison on `Shown`, `Moved` and `ScaleFactorChanged`. Only a plain `Resized` ever reached it with
the window's own height. Each variant carries both measurements now (2026-08-21), and
`the_display_decides_k_and_the_window_decides_the_warning` is the case that could not be written
before: drag short, then move to a taller display, and the warning has to stay up.

### 16.11 Scroll, and the price of a `Flickable`

**Every drawer page's body is a `Flickable`**, between a fixed page header and any pinned footer row.
The bench is not one; the shelf is not one; the well is not one. That is the whole of where Scroll
is allowed, and §5 says why it had to exist at all.

Three costs, all verified in `i-slint-core-1.17.1/items/flickable.rs`, all accepted with their
numbers rather than discovered later:

| | value | consequence |
|---|---|---|
| `FORWARD_DELAY` | **100 ms** (`:370`) | **retired — see below.** It applies only to an *interactive* Flickable |
| `DISTANCE_THRESHOLD` | **8 px** (`:366`) | a drag under 8 px is a press, over it is a scroll. Fine for rows; it is the reason nothing draggable lives inside a Scroll |
| `SCROLL_FILTER_DURATION` | **800 ms** (`:376`) | once it has scrolled, it captures further wheel events for 800 ms. A nested scroll would be unusable, so there are none |

**Corrected 2026-08-21, in the building: `FORWARD_DELAY` is not paid.** `Scroll` declares
`interactive: false`, and `Flickable::input_event_filter_before_children` returns `ForwardAndIgnore`
for every non-wheel event on that condition (`flickable.rs:155-158`), as does `input_event`
(`:167-170`) — so `handle_mouse_filter`, and with it `DelayForwarding(FORWARD_DELAY)`, is never
reached. **Wheel events fall through in both places**, so the wheel and the trackpad still scroll.
What replaces the cost is **no touch-drag-to-flick**, which matters on a touchscreen laptop and not
on a trackpad. §11.2's three-level re-cut still stands on its other leg: 1 090 px does not fit in
722.

**And a page whose own content overflows the drawer needs one.** `MenuPage` — seven rows, six of
them disabled and therefore `ROW_H + FIELD_REASON` = 78 px each — is 556 px inside a drawer that is
312 at the declared window minimum, and it shipped without a Scroll. A clipped item is not merely
invisible: `WindowInner::move_focus` gates tab navigation on `is_visible_or_clipped_by_flickable()`
(`window.rs:1328`), which recovers an item only when the hiding ancestor **is** a Flickable, and the
drawer clips with a Rectangle. Three rows were unreachable by pointer, by keyboard and by an
assistive technology at once, with no indicator that they existed.

**The keyboard route into a Scroll is `Tab`, and that is an honest gap.** §16.8 gives `↑`/`↓` to the
machine's wheel, always, so scroll-into-view is driven from each `Pressable`'s `focus-gained`. That
reaches every interactive thing and nothing else: a page of pure `Gauge`s would be unreachable by
keyboard below the fold.

Two design rules on top:

- **An `Expand` that opens below the fold scrolls its own top edge into view**, `gentle`, rather than
  letting the rows below it travel under a stationary cursor. Without that rule, §11.3's expanding
  refusal is principle 2 violated through a mechanism §16.3 does not cover — `visible` keeps a cell,
  but a scroll offset is not a cell.
- **A pinned footer is outside the Flickable**, so `Create`, `Screenshot the panel` and
  `Copy this readout` are always in the same place. A primary action that scrolls away is a primary
  action you hunt for.

---

## 17. Open questions for the operator

Real ones, each with a recommendation.

**Q1 — the shelf's height, and what a refusal gets.** The shelf is 88 px and a bench refusal is one
elided line plus `why ›`. A 134 px shelf holds the whole `Verdict::No.why` paragraph and its `Fix`
inline. **The cost changed with §6.6 and is now smaller than it was**: the column goes
`body + 154` → `body + 200`, which at `k = 1, sf = 1` is 810 → 856. That loses **1440 × 900 with the
Dock hidden** and nothing else — 1920 × 1080 at 125 % now has 122 px of slack instead of 11, and the
operator's own machine has 81. One display class, not two.
**Recommendation: keep 88.** The full paragraph is one keypress away, under the control that caused
it; 46 px of permanent chrome to hold a paragraph that is one keypress away is a bad trade even at
one display. But the number moved, so the trade is worth re-reading rather than inheriting.

> **Answered for the drawer, 2026-08-23, and the answer is this one applied consistently.** Every
> §9.4 reason on every drawer page was drawing the same way this shelf line does — elided, mid-word —
> and the choice was the same: grow the slot, or shorten the sentence. It shortens, **and it shortens
> against the slot each sentence is drawn in rather than against the narrowest of the four** — the
> correction of 2026-08-23, made when §11.3's consequences were measured for the first time and
> sixteen of thirty-eight came back over. §9.4 carries the rule, the four measures, and the
> thirty-nine sentences the sweep now holds to them. **What does not transfer is the second half of
> this recommendation:** a bench refusal really is
> one keypress from its paragraph, and a drawer refusal is not — `why ›` opens the page that owns a
> refusal, and these refusals are already on their page. So the long form is not one keypress away;
> it is in the source and in §9.4. That is the cost, and it is named rather than glossed. This
> question stays open for the shelf itself.

**Q2 — the cradle.** A permanent 2 px outline 10 px around the device, plus two clamp marks, plus a
broken-ring variant for refusals and an `fg` focus ring 4 px outside. It is the mechanism that keeps
UI state off the object (principle 3) and it is the largest untested aesthetic bet in this document —
and it now costs 32 px of the vertical budget (16 above, 16 below) rather than 10. The alternatives
are: clamps only, no outline; or accept a tinted device and lose principle 3.
**Recommendation: build it, look at it, and decide from a screenshot rather than from this
paragraph.** If it reads as clutter, drop to clamps-only before dropping principle 3 — and note that
clamps-only returns 32 px to the budget, which is most of what Q1 wants.

**Q3 — a file picker.** `Provide…`, `Add a dump…` and `Choose a folder…` all need one — and
`Choose a folder…` is now the next step for **three** failure classes, not one, because §9.3
split `space` and added `volume`. Nothing in the dependency graph provides it; `rfd` pulls GTK or
xdg-desktop-portal on Linux.
**Recommendation: add `rfd`.** Drag-and-drop is the better route and it is window-wide, but "the only
way to give this program a file is to drag it" is not a program, and a Linux portal dependency is a
smaller cost than that.

**Q4 — the menu bar on Windows and Linux.** macOS is free (§16.8). On Linux, Slint renders it and it
costs outer height. **The physical-hero correction bought the fractional-scale displays enough slack
that this is no longer likely to be decisive** — 1920 × 1080 at 125 % went from 11 px of headroom to
122 — but it has still never been measured.
**Recommendation: macOS only in 0.5**, via two top-level components and a `cfg` — which §16.8's
fullscreen row needs anyway, so the mechanism is not a new cost. Everything in it is a drawer row.
Revisit when the Linux cost has actually been measured rather than estimated.

**Q5 — a second device on the bench.** Settled, not open, and it is worth saying why it moved. The
previous revision left "one device on screen" and a `←`/`→` that switched devices coexisting, which
meant looking at a second iPod either destroyed the first or left it executing behind a panel that
was no longer drawn. §7.2 decides it: **the bench shows the machine whenever there is one, and other
devices are inspected on the Devices page**, with `Start` disabled and a reason. The carousel stays
deleted because it animates `body-height` on a live framebuffer (§8.2).
**Recommendation: as written.** If a carousel is ever wanted back, it must not animate anything the
Screen reads, and it must not imply that the bench can show a device that is not the machine.

**Q6 — the hold switch's proportions.** §6.6's numbers (0.100 × 0.024 h, right edge inset 0.055 h)
come from **published dimensions, not from Rockbox's drawing**, because the SVG is a front elevation
and may not carry the top edge. An invented proportion is exactly the mistake this drawing already
made once — wrong by 66 % on where the screen sits.
**Recommendation: mark them a placeholder in the source, and make the test refuse to pass until
they are derived from a measured source.** If the SVG does not carry the top edge, say so in the
comment and cite where the numbers did come from.

**Q7 — the parked frame.** §12.4 writes a 320 × 240 PNG beside the snapshot so a parked device's
glass shows the frame it stopped on. It costs one write per park and retires `png.rs`'s dead-code
allow — and §11.4's Snapshots group now makes both it and the 149 MB behind it visible and
deletable.
**Recommendation: do it.** A shelf of parked machines that look parked rather than off is worth one
PNG, and it is drawn at exactly `k` so it costs nothing in fidelity.

> **Done, 2026-08-24.** One write per park, 230 723 bytes, and `png.rs`'s allow is off the module —
> what is left on it is `encode_ppm`, with its own condition beside it. The decoder is Slint's own,
> which this document did not know it had. `_out/gui/bench-parked.png` is the picture. **And the
> figure behind it was wrong here**: the pair is about **149 MB**, not 1.6 GB — see §12.4.

**Q8 — `Settings::mode` (User / Debug).** The model carries it, nothing reaches it, and this design
has no use for it: the Readout is a drawer page anyone can open, and there is no second presentation.
§11.6's Settings page deliberately does **not** include it.
**Recommendation: delete it from the model**, or give it one job — hiding the Readout row from the
drawer root — and say which. A field with no mechanism is a landmine.

**Q9 — does a `.ipg` carry cover art, and under what member name?** Nothing in this tree reads a
`.ipg` zip's members; `grep -rni "ipg" --include='*.rs' tools/` returns nothing. §13.2 draws the
cover from the title's name unconditionally in 0.6 rather than claiming a structure that has not been
looked at.
**Recommendation: open one of the 20 decrypted titles and list its members before 0.6 designs
anything else around them.** It is a `unzip -l` away, and it is the difference between a design and a
guess.

**Q10 — can an arrival at a `WATCHED` PC be attributed to a core?** `Stats::enters` is
`[u64; WATCHED.len()]` with no per-core dimension anywhere in `Stats` or `Out`, and §12.8 draws one
column because of it. The second column is worth having — the second core cost 24 % until its message
was found never to arrive — but only if the run loop actually knows which core executed the
instruction.
**Recommendation: answer it before §12.8 is built.** If yes, `enters_by_core: [[u64; WATCHED.len()];
2]` joins §20's list beside §3.1 and §3.2. If no, the one column is the honest drawing and the
caption stops claiming to show what two cores are doing.

> **Answered, 2026-08-24: no** — and by reading the run loop rather than the struct. The arrival
> capture (`enter_bloom` / `enter_pcs` / `enter_log.push`) lives inside `Machine::run`, which is the
> CPU's loop; `Machine::run_cop` is a **reduced** loop and its own doc states the consequence in as
> many words: *"the instruments do not see the COP."* So `enters` has no per-core dimension because
> nothing could fill one — a second column is not a missing field, it is a measurement that is not
> taken. `enters_by_core` would need the capture added to `run_cop` first, which is a change to what
> the coprocessor costs per instruction and therefore to every number in `research/`. The one column
> is drawn and the caption claims nothing.

**Q11 — should a plain window resize re-decide `k`?** Today it does not (§6.6): `k` is fixed when the
window is shown and re-decided only on `ScaleFactorChanged` and `Moved`. A window dragged large
enough for `k = 2` does not take it until the next launch. Re-deciding on `Resized` would give it
immediately and would make the iPod change size under a drag, which is principle 1.
**Recommendation: leave it, and put the fact where it can be seen** — the shelf's fidelity slot says
which `k` is in force, so it is a stated limitation rather than a mystery. If it turns out to annoy,
the cheapest fix is a one-line offer in that slot (`2× fits this window — relaunch to use it`), not a
live recompute.

**Q12 — five numbers in `src/geometry.rs` that this document does not state.** Added 2026-08-21,
because they are load-bearing and were derived rather than read. **The verb columns are answered**;
the shelf is still open.

| constant | value | where it came from |
|---|---|---|
| `DRAWER_HEADER_H` | 44 | one `Row`, so a header and the rows under it share a rhythm. §11.2's *"≈ 60"* covers *the page header **and** the group rules together* — an approximation inside a total, not a declaration |
| `WORK_FOOTER_H` | 72 | §10.1's three `label` line boxes, plus 12 above and 12 below |
| `RAIL_VERB_W` | **88** — *answered 2026-08-21* | it shipped at 64, which this question said nobody had measured. Budgeted at `10 × 14 × 0.62 = 86.8` and rounded up; **measured at 67.0** by `verb-probe` in `ui/window.slint`, which `IPOD_LAYOUT=1` prints and `tests/startup_fit.rs` asserts against the real binary. The prediction — *the longest verb is `synthesise`, it appears at first-run step 1, and if it elides it elides there* — was right about 64 and the column is now comfortable |
| `PARTS_VERB_W` | **180** — *added 2026-08-22 at 104, raised 2026-08-23* | §11.4's group verbs, which had no constant at all. The 104 was the label budget — `11 × 14 × 0.62 = 95.5` plus `Metric.s2`, **measured at 95** for `Add a dump…` at `weight-strong` — and it holds the verb. It does not hold the half of the row the verb's **refusal** is drawn in, because 104 is not the half-share and the shrink takes the difference: with every reason already cut to 146 px, `_out/gui/parts.png` still drew **Apple firmware**'s `no file picker in this build` as *no file picker in this …* while **Bootloaders** drew it whole, and the only difference between the two rows is the width of the `mono` command under the *other* half. So it is now `(372 − 12) / 2`, and two halves and their gutter are the row exactly |
| `RAIL_NEXT_W` | **170** — *added 2026-08-23* | §9.3's next-step pair, which had no floor at all — the same defect one surface over, and photographed before it was believed: `_out/gui/work-failed.png` drew a live **`Retry` in 98 px** with its own consequence elided to *Runs this step …*, beside a 194 px sibling, and `Copy the details` in **114** on the entry below. `(372 − 2 × 12 − 8) / 2`, so `170 + 8 + 170` is the failure block exactly. `REASON_MEASURE` — the 146 px budget a `rail::Next` sentence is written to, and the narrowest of §9.4's four slots — is this less the control's own padding |
| `ACT_MEASURE` / `PAGE_REASON_MEASURE` | **324** and **372** — *added 2026-08-23* | the two §9.4 slots a drawer page draws side by side, and the 48 px between them was invisible because neither had a name. A `Pressable` inside an already-inset page body indents again by its own `pad`, so it gets `420 − 4 × PAGE_MARGIN`; a `Field` in the same column has no padding of its own and gets `REFUSAL_MEASURE`. `_out/gui/composer-ipod-dumped.png` is the photograph: one sentence, whole under `Serial` and elided under `Model`. Both are now measured off real controls by `MainWindow.field-reason-w` / `act-reason-w` / `padded-field-reason-w` rather than derived, and the third of those is what proves `Field` is passing its `pad` down at all |
| `SHELF`'s decomposition | 12 + 26 + 20 + 16 + 12 = **86**, or 87 with the top rule, against a declared **88** | §7.5's own parts do not sum to §7.5's own total, and `CHROME_MIN` (154) and `CHROME_PREF` (190) are both built on the 88, so the 88 is the load-bearing half. Two pixels sit below row 3 inside the bottom padding |

**A verb did elide, and this question was pointing at the wrong column when it happened.** The one
that elided was a group verb on Parts — `Fetch…`, drawn as `F…` — and `RAIL_VERB_W` had nothing to do
with it. §11.4's two verbs share one row, each carrying `horizontal-stretch: 1` and no floor; a
`Text` set to elide reports one ellipsis as its minimum width, and when the row is too narrow Slint
shrinks **by stretch** — the same number of pixels off each. So the half whose reason is a sentence
kept its width and the half whose label is a word lost it. **The disabled sibling's reason was the
cause; the absent floor was the defect.** All four numbers were measured: the row holds 360 px,
`Provide…`'s disabled half prefers **411** because a disabled control prefers the width of its
*reason* rather than of its label, `Fetch…`'s prefers **52** because it is a word, and 463 shed into
360 asks 51.5 off each — so the one that had 52 hit its ellipsis floor. Widening a column would have
fixed nothing, which is the useful half of this: a measurement that was never taken is a good suspect
and not a verdict.

**What remains open is the shelf**, and it is the one that was a disagreement rather than an
invention: two numbers in one section that do not agree. **Do not invent a `SHELF_SPARE` term to
absorb it** — that is a second source of truth inside the section whose rule is that constants live in
one place. `the_shelf_rows_and_its_padding_fit_the_declared_shelf` measures the leftover and fails if
it grows past 4 px, so it is visible rather than forgotten.

---

## 18. Decisions, including the ones this revision overturns

Each is one constant or one section away from being reversed, and the reasoning is here so a reversal
is cheap rather than archaeological.

### 18.1 Overturned

| | was | is | why |
|---|---|---|---|
| **The panel's ratios** | `0.4866 / 0.3672`, measured from the SVG | **`0.48799 / 0.36599`**, from the hardware | The SVG's own 0.2 % error makes the well 1.32516 instead of 4:3, which stretches the framebuffer 1.00674 vertically. The drawing governs where the screen sits; the hardware governs how big it is. Cost: 0.43 px on the left inset |
| **`hero`** | `658px`, a **logical** constant | **`k × 655.751` physical, pushed in from Rust** | A logical constant is not a constant. At 125 % the glass held 43 px of black each side against a declared invariant of 11, and at 150 % the panel filled 62 % of its own well. §6.6 |
| **The glass** | `0.51999 h × 0.39799 h`, and `k` computed from it | **panel + `0.016 h` on four sides** | The glass ratio is 1.3066, not 4:3, so the two ratios the fidelity section exists to correct governed nothing but placement. It was harmless only by coincidence of the bezel |
| **The vertical budget** | 790 min / 826 preferred, `cradle overhang 10` counted once | **`body + 154` / `body + 190`**, the cradle's overhang and focus ring counted **above and below** | The topmost thing on the bench was 16 px above the body and the budget paid for none of it. §16.2 neither shrinks nor clips it |
| **Devices: master–detail** | a 280 px list + detail pane | **one device on the bench; the list is a drawer page** | The operator rejected the sidebar in their own words. The detail pane's job is done by the shelf and the drawer's device page |
| **Devices: the carousel** | a horizontal row of drawn iPods | **one device, cross-dissolve** | It animates `body-height` with a live framebuffer attached, so the panel is drawn at a continuously varying non-integer scale for 320 ms on every change |
| **The material on the centre button** | proposed, an earlier draft | **on the cradle instead, as accent** | Principle 3: a glossy blue disc is UI state painted on the object, and a screenshot of it stops being a picture of an iPod |
| **The cradle's inactive ring** | `line` at 30 % | **`fg-dim` at 100 %** | 1.23 : 1 against `bg-sunken`, which is the only surface it is ever drawn on. Five of twelve states were invisible |
| **The cradle's refused ring** | `fg-disabled` | **`fg-dim`, broken into four arcs** | 1.67 : 1, on the one state whose whole job is to teach. And a fourth colour that clears 3 : 1 and means nothing else does not exist on this surface |
| **The cradle's focus ring** | a second `accent` ring, 4 px outside the first | **`fg`** | A ring around a ring in one colour is not a focus indicator |
| **`Esc`** | a way *into* the drawer, *and* a pure exit, *and* the way out of fullscreen | **one ordered outward definition** (§16.8) | Three meanings, of which the one that fired on an empty bench started a 1.6 GB write |
| **The keyboard's "in every state"** | asserted | **one exception: `TextInput` focus** | Four text fields in the Composer, every one of them unusable, and Slint offers no third shape |
| **The centre button's hit region** | "12 px larger than its drawing" **and** `WheelRing::hit` | **`WheelRing::select`, and only that** | Two rules for one control, neither matching the model; one would have shrunk the target and the other would have eaten a deliberate dead band |
| **The eight-primitive vocabulary** | eight, no scroll container | **nine — Scroll** | Three drawer pages overflow their pane by 30–60 % and Slint neither shrinks nor clips |
| **The `space` failure class** | one class, `Nothing has been written.` | **`space, pre-flight` · `space, mid-write` · `volume`** | The wording is false 41 GB in, and free bytes are not the only thing FAT32 refuses |
| **`Fix`** | "one press applies it" | **one press, except `BuildFromIpsw`, and none when the value is disabled** | One of the four shapes silently detached a 55.9 GB reference; another set a value the picker forbids |
| **When the iPod is filed** | at the mint — *"the moment it is made, not on `Create`"* (§11.2) | **on `Create`**, restated rather than duplicated on a re-save | It was asserted for months and built by nothing, in the design and in two doc comments in `settings.rs`. Its own promise is already kept better by the mint's two-press confirmation naming the seed, and filing at the mint would make that sentence false, restate an entry per keystroke in `Serial`, and leave an iPod in Parts behind every abandoned compose |
| **`Rename` as a row control** | the seventh of nine `RowAction`s, with a label and two refusals | **deleted; §11.2 level ③ is the route** | Nothing built the row on either page, so the two *exhaustive* arms refusing it were free to disagree about whose control it was — and did. A row control carries two integers; a name is text |

### 18.2 Settled

| | decided | why |
|---|---|---|
| **The thesis** | the iPod is the program; three surfaces | it is the only shape that fits the panel at 1:1 on the operator's own 891 px machine, and it is what was asked for |
| **The device's position** | pinned by its distance above the shelf | all slack goes to one place, so growing the window moves nothing |
| **The device's size** | `k × 655.751` **physical** pixels, `k` fixed for a session | one number, both axes, every display scale; and the drawing does not resize under a drag |
| **Accent** | `#2969d6` / `#5292e7` | RetailOS's own selection blue, sampled off a frame we drew. Derived, not chosen |
| **Accent's three uses** | focus ring (except on the cradle) · progress · the cradle when startable | four blue things is no primary action |
| **The material's three uses** | the selected drawer row · the one primary row per page · §9.5's primary row | everything glossy is a pastiche; the third is the same row as the second, on a bench that cannot draw a cradle |
| **The one button's default iPod** | 5.5G, 30 GB, black | a real Late-2006 configuration; images are sparse so capacity costs nothing |
| **`display` type role** | retired | 36 px of a budget with none to spare, over a rendering of the same thing |
| **`readout` type role** | added | 25 numbers want tabular figures; without them a changing value reflows its own digits |
| **Cradle, Gauge and Scroll** | added to the closed vocabulary | principle 3 needs somewhere to put state; the Readout needs a not-measured that is not a zero; three drawer pages do not fit in a drawer |
| **Tile and Sheet** | retired | Tile went with the grid and the carousel; the drawer *is* the pushed surface |
| **The drawer's geometry** | full client height above the shelf; the well **and** the shelf narrow to `W − 420` | pushing, not covering; and row 3 keeps its measure because the trailing menu list is redundant while you are inside it |
| **The wheel** | always the machine's, never the window's | a mode whose meaning flips on a state the user did not set, and which vanishes when learned |
| **The bench shows the machine** | whenever there is one; other devices live on the Devices page | to look at a second thing you must not destroy the first — that is the disease this whole design cures |
| **Games** | 0.6, designed now | one drawer page and one cradle state, because a title is a thing that stands on the bench |
| **iPodLinux** | visible, disabled, project-state wording, escape hatch named **and made reachable** | principle 4, and an escape hatch that needs a `make` in the repository is not one |
| **Settings** | a drawer page with exactly three rows | three homeless settled decisions, and the operator rejected a settings screen you reach by destroying the machine, not settings |
| **Reference** | a drawer page, not a surface | a page you can leave cannot be a place you get stuck in |
| **Theme** | follows the system, **both fully specified in values** | the previous revision's dark column was qualitative and therefore not a specification |
| **Nostalgia** | borrow the language, never the resolution — **enforced by three geometric tells, one of them now in physical pixels** | it is checkable with a ruler rather than by taste, and the ruler has to measure the right unit |
| **Brushed metal** | refused, and recorded as refused | the one texture Apple's 2005 HIG would have licensed here, and a pixel-grid borrowing wearing a texture |
| **§3.1, §3.2 and §3.3** | model changes, in `settings.rs`, **before the window** | the first two were declared closed once and skipped; the third is what four different surfaces were already assuming |

---

## 19. What the critics found

This document was read twice, adversarially, by readers whose job was to break it. They found fifty
things. Five were fatal — states the design could reach in which it offered no working action, or
arithmetic that produced the wrong drawing. This section lists what changed, what was checked and
found not to hold, and what is accepted as a limitation, so that the next reader knows the argument
has already been had.

### 19.1 The five that were fatal

| | what broke | where it is fixed |
|---|---|---|
| **First run on a short display had no route to a device** | §9.5 replaced the well, the well holds the only interactive element in the program, and the only control the replacement offered was disabled by §12.6 in exactly the state it was offered in | §9.5 carries a real primary row with the cradle's own label and callback; §12.6's availability rule becomes "something on the glass"; 1366 × 768 added to §9.6 |
| **Every text field in the Composer was unusable** | bare-letter shortcuts declared "in every state" while `M`, `S`, `H`, `D` and `Space` are letters in ordinary names | §16.8's one exception, keyed on `AccessibleRole::TextInput`, with a test that must be shown to fail |
| **`hero` was a logical constant** | at 125 % the black glass was 43 px a side against a declared invariant of 11; at 150 % the panel filled 62 % of its well | §6.6: `hero` is `k × 655.751` physical, pushed in; the glass is sized from the panel; §9.6 re-derived |
| **The drawer pages overflowed the drawer** | the Readout is ~1 970 px in a ~750 px pane, and Slint neither shrinks a pinned child nor clips the surplus | §5's ninth primitive; §16.11's three costs; §11.2 re-cut into three depth levels |
| **The too-short state was only evaluated at startup** | drag the bottom edge up, or move to a second monitor of the same scale factor, and the shelf leaves the window with `write_target()` on it | §16.1: recompute on `Resized` and `Moved`, with hysteresis; the "never while you are looking at it" promise deleted |

### 19.2 The changes worth naming individually

- **Four surfaces assumed a model that does not exist.** `Settings::missing()` never touched the
  filesystem, so `cannot start — the disk is not where it was` was unreachable; `Device` had no
  parked timestamp behind `parked · 4 min ago`; `Stats` had no per-core dimension behind a `core 1`
  column; and nothing in the tree parses a `.ipg`, behind a claim about cover art. §3.3, §12.8 and
  §17.Q9.
- **Three numbers for one operation, and a refusal computed from the wrong one.** The first-run screen
  said 300 MB, 8 GiB and 8.02 GB for a 6.5 MB download and a 21 MB build, and gated free space on the
  sparse file's apparent size. Those are the figures as they stood; the corrections are in §10.1
  with the recipe that measured each. §10.1.
- **The retry path re-minted the identity.** Three failed first runs left three iPods with three
  FireWire GUIDs. Identity is the one permanent decision in this program. §10.2, §10.3.
- **A one-press `Fix` detached a 55.9 GB reference with no sentence.** §11.3.
- **A `Fix` offered a value the picker four rows above it refuses.** §11.3, and §20 deletes the
  project state rather than papering it.
- **Two verdicts in the always-reserved region were false**, one of them before anything had been
  chosen and one of them flipping several seconds after a pick. §11.3.
- **The cradle was invisible.** `line` at 30 % is 1.23 : 1 against the only surface it is ever drawn
  on, and `fg-disabled` is 1.67 : 1 — on the state whose job is to teach. §6.4.
- **`Esc` had three definitions**, `⌃⌘F` was unreachable on Windows and Linux, and `→` deleted the
  keyboard route to a second device while a machine ran. §16.8, §7.2.
- **A 1 MiB JPEG identified as a boot ROM with a generation attached**, and an eight-file drop had no
  defined behaviour at all. §11.4.
- **A running machine's device and its resources could be removed**, and a booting machine could not
  be stopped. §11.4, §7.3, §12.5.
- **A recipe edit left a stale boot denominator**, reintroducing the exact defect `cold_boot_instructions`
  replaced `snap_at` to fix. §12.3.
- **§12.6's fullscreen table was computed in logical pixels** and printed 3× where the answer is 7×.
- **A four-second self-dismissing message on shelf row 2** is a toast, banned by name in this
  document's own principle 5. §7.4.
- **`Copy the command line` put the identifiers §11.2 masks onto the clipboard**, and the masked
  validation sentence quoted the offending character back. §11.2, §12.9.
- **Two of level ①'s safety affordances were computed on every frame and drawn by nothing.**
  `Make one` over an existing iPod was **one unconfirmed press** that discarded the identity on
  screen; and the OUI warning this section says in so many words the UI *must not flatten* was
  flattened. Both because the window read the level's rows one field at a time and never mentioned
  two of them — the producer was right, the markup was ready, and no instrument in the tree could
  see the gap. Level ① is also the page nothing had ever taken a picture of. §11.2.
- **A locked picker reserved §5's 34 px and said nothing in it.** `Read from the dump; a device's
  identity is the ROM's, not ours.` was written into the slot a `Pressable` draws only while it is
  *enabled*, two rows above a `Field` drawing the same sentence correctly — §9.4's own rule, broken
  on the page it was written for, and visible the first time level ① was photographed. §11.2, §9.4.
- **Parking had no budget, no listing and no eviction**, and the park that fails for lack of space
  fails when there is no window left to say so in. §11.4's sixth group, §12.4.
- **`work_on_copy`'s three sentences were two rules fused**, and the prose implied the opposite of
  what three tests hold. §7.5, four sentences.

### 19.3 One finding that was checked and did not hold

**The f32 sub-pixel claim.** A reading held that `k × 320 / sf` stored as f32 and multiplied back by
`sf` lands at 319.99999 physical at sf = 1.5 and 1.75, so Skia antialiases the edge columns even
under `FilterMode::Nearest` — on the two most common Windows scalings.

The renderer half is correct and is now cited in §6.6: Skia rounds an image's destination *origin* to
whole device pixels only for a pure translation and **never rounds the destination size**. The
arithmetic half is not. `Coord = f32` (`i-slint-core-1.17.1/lib.rs:104`) and euclid's `Scale<f32>`
multiply is one correctly-rounded f32 operation, so `f32(320.0 / 1.5) × 1.5` is **exactly 320.0**.
Swept over ten real-world scale factors and `k ∈ 1..8`, the round-trip is exact in **79 of 80** cases.

**The defence is adopted anyway**, because the class is real even though the instance was not: the
one failing case is `sf = 2.75, k = 7`, and arbitrary fractional scale factors of the kind Wayland
and X11 hand out fail in 327 of 1 204 sampled combinations. `next_up_until` costs one function, and
the widened test now has a case that genuinely goes red — which is worth more than a test that passes
because the failure was imagined.

### 19.4 Accepted as limitations, with reasons

| | why it is not fixed |
|---|---|
| **1280 × 800 and 1366 × 768 cannot show the panel at 1:1, at any scale factor** | The body alone needs 656 physical pixels and neither display has 810 of usable window height. Drawing it smaller discards pixels the emulator produced, which principle 9 forbids. §9.5 is a designed state with a working action, not a failure — and fullscreen gives both displays 3× |
| **`k` is not re-decided on a plain window resize** | Principle 1. A drawn iPod that changes size while you drag an edge is worse than one that takes the larger scale at the next launch. The shelf says which `k` is in force. §17.Q11 |
| ~~**Every control inside a drawer page's Scroll is 100 ms late to a press**~~ **RETIRED 2026-08-21** | It was believed `Flickable` had no way to opt out of `FORWARD_DELAY`. `interactive: false` skips it entirely and the wheel still scrolls — see §16.11. The cost that replaces it is **no touch-drag-to-flick**, which matters on a touchscreen laptop and not on a trackpad |
| **"Centred on first launch" and the work-area read are two-platform** | Wayland has neither `set_outer_position` nor a work-area query. Mitigated by measuring the window we actually got rather than predicting it, so the *too-short* state is right on all three; only the initial placement is not |
| **The 96-detent ring has no announced accessible equivalent** | Slint has `Slider` and a wheel is not one. `↑`/`↓` are the route and they are a fallback, not a peer. Said once rather than pretended |
| **`BuildFromIpsw` breaks the one-press `Fix` rule** | It is the only `Fix` shape that changes which resource a device points at, and a silent detachment of somebody's only image is the worse failure. The rule is stated rather than the exception hidden |

---

## 20. What has to be true before any markup is written

In order, because each depends on the one before it.

1. **DONE.** `Device::firmware` + `Device::nor` collapsed into one named reference (§3.1):
   `firmware: String`, resolved through `Settings::nor_of`, with no second inline copy and no
   fallback. The migration case went with the field — a device that carried its recipe inline is
   given a **named** iPod by `Settings::parse` instead, through `adopt_inline_roms`, so the old keys
   are read for ever and written never. **One behaviour change for a real user**: a device whose dump
   has moved stops silently booting a substituted generated 5.5G and starts being refused by
   `run_device` and named by `missing()`. Between this and item 12's Rail that refusal has nowhere to
   be shown — `on_start_device` is still an `eprintln!`.
   **The disk got the same rule on 2026-08-21**, having been left with the old one: `run_device`
   fell through to `Device::disk_path` when the disk's *name* did not resolve, and after one round
   trip through the settings file there is no `disk_path` to fall through to — so the machine
   started with no drive at all while `missing()` was already reporting the name as absent. Three of
   the four `disk_of` outcomes had the two functions disagreeing. It refuses now, and naming **no**
   disk still starts, because that is an unfinished device rather than a broken one.
   **And §7.5's row 3 got it on 2026-08-22**, being the last place still reading `Device::disk_path`
   directly: it told every saved device from its second launch on that there was *no drive yet —
   nothing will be written*, while `writes_to_your_own_image` resolved the same drive by name and
   painted the `warn` colour under those words. `Settings::disk_of` is public now — one function
   knows how a device becomes an image — and the sentence and the colour come out of one `match`
   together, so there is no second producer left to disagree with the first.
2. **DONE.** `Item` gains `from: Option<Provenance>` (§3.2), and the hard-coded
   `fetched and verified` / `dumped from a real iPod` strings in `main.rs` are deleted — every
   trailing column is `Provenance::line()` now, and an item nobody recorded one for contributes the
   empty string. `firmware::provenance(CacheState)` is the bridge that makes the default listing
   path — which hashes nothing — file its results honestly.
3. **DONE.** `Settings::missing()` stats every resolved path (§3.3) and returns
   `Vec<Absent>` — `Gone(PathBuf)` or `Unlisted(String)`, firmware first, disk second, so the
   cradle's one-part sentence is stable. `Presence` memoizes one pass's answers and treats a
   permission, a timeout or a device error as **present**, because none of those is an observation
   of absence. Corrected 2026-08-21: the `false` arm is `NotFound`, `NotADirectory`, `InvalidInput`
   and `InvalidFilename`, not `NotFound` alone — a path whose parent component is a regular file,
   or one the OS will not even accept, is a definite negative, and folding those into "present"
   swallowed a device that cannot start. `device_rows` now builds **one** `Presence` and threads it
   through `summary`, which is the sharing the design is written around; it called
   `Settings::missing` per device, minting a fresh cache each time. `missing()` may block on a
   stale network mount, so it is not callable from a binding — that rule is in `Presence`'s doc
   comment, nothing enforces it, and the row-rebuild pass is still on the UI thread.
4. **Model half DONE.** `Device::parked_at: Option<u64>` (§3.3) — seconds since the Unix epoch, with
   `now_unix`, `Settings::record_park`, `Settings::discard_park` and `parked_for` (saturating, so a
   clock that stepped backwards reads as `0` rather than as 584 942 417 355 years). The **writer**
   is not built: `emu::Link` has no `parked_at` and `write_restore_point` still returns `()`, so
   nothing calls `record_park` yet. **`DeviceRow.parked` is deleted** (2026-08-21): it was computed and
   bound to nothing in any markup file, which is item 15's defect in a new place. What the shelf
   actually needs is §7.5's *state, and time since*, and `DeviceRow.state` carries it now — `off`, or
   `off, parked 4 min ago`, from `phase()` and `parked_for`. The authority on whether there is a
   restore point to **resume** is still `Config::may_restore()`, and the window has no `Config`.
5. **DONE.** `Recipe::check()` gains rule (0) — `Recipe::nothing_chosen()`, which covers all three
   `Start` variants and not only `FromIpsw` — returning `Verdict::No { why: NOTHING_CHOSEN,
   fix: None }` (§11.3), so the always-reserved verdict region stops asserting a plan for a firmware
   nobody has chosen. The old body moved to a private `check_parts()`, which is what
   `loader_works`/`why_not` call, so a bootloader tooltip stays about the bootloader. It came with
   one companion fix: rule (2)'s `0x0C` guard now fires whenever `ipodloader2` is *required*, not
   only when it is selected, because otherwise rule (1)'s fix chained into rule (2)'s refusal.
   **A second half of that guard was still missing** and was closed 2026-08-21: it matched
   `Start::FromImage` alone, so a disk out of the **library** — `Start::FromDisk`, carrying the same
   `fat_type`, and the drives most likely to be `0x0C` because they come off real iPods, which is
   what the refusal's own text says — verdicted `Ok` for a recipe `install::install_linux` refuses.
   Both variants are read through one `Recipe::volume_type()` rather than matched twice, for the
   same reason `nothing_chosen` enumerates all three in one place, and
   `every_fix_resolves_the_thing_it_is_offered_for` now sweeps five `Start`s rather than three.
6. **DONE.** `Create` clears `Device::cold_boot_instructions` when `oses` or `loader` changed (§12.3),
   and it does it by **calling** `Settings::set_boot_shape` rather than by keeping one line of its
   own: same shape, keep the number; different shape, store the new one and take the number. That is
   what makes the one-bar-across-three-operating-systems claim true rather than conditional.
   `compose::BootShape`, `Recipe::shape()`, `Device::boot_shape`, `Settings::set_boot_shape` and the
   `as_device` carry-forward line all landed before the caller did — and **the missing caller is
   what made the other five look built and behave as though they were not**: `Composer::commit`
   cleared the number and recorded no shape, so `Settings::recipe_of` — which treats `boot_shape` as
   the **authority** on what a device boots — could never take that branch, and every Edit
   re-derived the recipe from the drive's install list instead. The trap this item named for whoever
   finished it was `Settings::as_device`, and it was real: without
   `boot_shape: existing.and_then(|d| d.boot_shape.clone())` beside the `cold_boot_instructions` and
   `parked_at` lines, every `run_device`/`remember_as` round trip loses the shape and the next
   `Create` throws away a good denominator. It is there, and `commit` files **before** it calls
   `remember_as` for the same class of reason.
   **One cost, stated rather than discovered.** A device that recorded no shape — every device that
   existed before this — has a number measured on something nobody wrote down, so the first save
   through the Composer drops it once and records the shape. The save after that keeps it.
   **The `main.rs` half is done** (2026-08-21): `DeviceRow.state` read `never started`, which is a
   claim about history from a field that is a progress-bar **denominator** — a device booted a dozen
   times renders it the moment `set_boot_shape` clears the number.
   **The replacement was wrong too, and was corrected the same day.** It read `no boot time learned
   yet`, which is still a fact about the *progress bar* rather than about the machine — and by then
   that string was drawn on shelf row 1, which §12.2 gives to the **phase**: `off` / `booting` /
   `running` / `stopped`, with §7.5's *time since* beside it. `phase()` already answered `Off` and
   reached that slot nowhere. It reads `off` now, and `off, parked 4 min ago` for a parked device.
   The denominator is not a thing the shelf says at all.
7. **DONE.** `install-linux` uses the fetched `ipodlinux::LOADER` — v2.8.1, 56 912 B, SHA-256 on
   record — rather than `resources/vendor/ipodloader2/loader.bin` (§11.3), through
   `ipodlinux::resolve_loader`, with `IPOD_LOADER=` as the override for a build somebody made. That
   turns a project state into a working path for anybody not working inside this checkout, and
   deletes a contradiction between two surfaces rather than reconciling them. **It also changes
   which bootloader people get**: every number in research/17 was measured on the vendored 2.9.0d,
   and the command now installs 2.8.1.
   **The loader half is done and the command still cannot complete**, and item 7 must not be read as
   claiming otherwise: `install::install_linux` refuses at a firmware-partition packing step —
   `no room: moving the later images by 57344 bytes needs 13952512 of a 13895680-byte partition` —
   on every drive tried, one built by `make-disk` and one off real hardware. It predates this
   change and the arithmetic says the partition is packed to within one 512-byte sector, so no
   bootloader of any size fits. `KNOWN-BUGS.md` carries it, and the `0x0C` refusal in `install.rs`
   dropped its *"and works"* on 2026-08-21 for the same reason.
8. **The client-height reader exists**: `NSScreen.visibleFrame` on macOS (`objc2-app-kit 0.3.2` is
   already in the tree), `SPI_GETWORKAREA` on Windows, nothing on Wayland — and §9.6's too-short
   boolean is computed from the **measured** `winit::Window::inner_size()` on `Resized`, `Moved` and
   `ScaleFactorChanged` (§16.1). **Done 2026-08-21**, as `client_height.rs` + `fit.rs`. Two notes
   the doing added: *already in the tree* is not *importable* — `objc2-app-kit` had to be declared
   as a direct dependency (it costs no extra compilation, only a `use` that resolves); and winit
   hands `ns_screen()` over **unretained**, so the pointer is retained before the first message
   send. `k` is decided from the display and the too-short boolean from the window — see §16.10's
   `IPOD_LAYOUT` note for what taking the smaller of the two did instead.
   **Corrected 2026-08-21**: it was written that way and built the other way. `fit::Moment` carried
   one `client_logical` per variant and `Fitter::apply` fed that single value to both answers, so on
   three of the four moments — `Shown`, `Moved`, `ScaleFactorChanged` — the boolean came from the
   **display**. Drag the bottom edge up and then move the window and it reported room the window did
   not have, which is §16.1's own failure with the shelf drawn past the bottom edge. Each variant
   now carries `display_logical` **and** `window_logical`, `Resized` carries only the window
   (a drag is not evidence about a screen), and
   `fit::the_display_decides_k_and_the_window_decides_the_warning` is the case that could not be
   expressed before.
9. **`build.rs` emits every ratio and geometry constant from one Rust source** into a generated
   `.slint` (§16.9) — including `HERO_PHYS_1X`, `BEZEL_RATIO`, `CHROME_MIN` and `min-width`.
   **Done 2026-08-21**, as `tools/ipod-gui/src/geometry.rs`. *(Not "and a Rust `const` module": the
   Rust `const` module is the source, and `build.rs` compiles that same file. See §16.9.)* The
   markup's hand-written geometry is gone and so is `main.rs`'s hand-copied test block — which had
   already drifted, carrying a second copy of `SCREEN_W`/`SCREEN_H` at the pre-§6.6 values inside
   the very test that claims to verify the drawing.
10. **`the_panel_is_an_exact_integer_number_of_device_pixels` is written and proved to fail** — twice,
    once against the old `0.4866 / 0.3672` pair and once at `sf = 2.75, k = 7`, which it does today
    (§16.10). **Done 2026-08-21**, in `geometry.rs`, sweeping `SF_SWEEP × 1..=K_MAX` — 160 cases —
    and it found a real one immediately. `next_up_until` stepped **`f64::next_up`** across an
    **f32** grid: an f64 ULP is a relative 2⁻⁵², about 2⁻²⁹ of one step of the grid it was crossing,
    so sixty-four of them could not move the value and the loop always handed back exactly what it
    was given. It read as working because 120 of the 160 cases are already exact; the other 40 —
    every `sf` of 1.5, 1.75, 2.25, 3.0, 3.5 and some of 2.75 — drew the panel a fraction of a pixel
    **short** of `k × 320`, which is a framebuffer column thrown away wherever the renderer
    truncates rather than rounds. Stepping in f32 fixes all 40, worst overshoot 0.00015 px. The
    ratio arm is its own test, `the_old_ratios_would_draw_the_panel_off_its_own_framebuffer`, which
    hands the checker the pair that shipped and requires it to reject them — because
    `panel_logical` does not read the ratios at all, so the sweep alone could never catch a
    re-measurement gone wrong.
11. **The `accessibility` feature is turned on** and `cargo tree | grep accesskit` returns something
    (§16.7). **Done 2026-08-21.** *(The stated reason was corrected in the doing: §16.8's shortcut
    suppression does **not** key on `AccessibleRole::TextInput` — it is a root `FocusScope` and a
    focused `TextInput` consuming character keys ahead of it, which works with the feature off. The
    feature is a prerequisite for the announced half, not for the keyboard. See §16.7.)*
12. **DONE 2026-08-21. The Rail exists before the first button is wired**, and `on_start_device` is
    wired after it. `rail.rs` is toolkit-free: ten failure classes, each with its own words and its
    next steps, `Caps` gating every step whose mechanism this build does not have, and no `std::time`
    anywhere in the file — so nothing in it can expire. `ui/rail.slint` draws it, `ui/work.slint`
    pins §10.1's ledger under it, and the drawer opens on the Work page when a press is refused.
    **A device whose ROM or drive has left the library is refused, and the refusal is on screen.**

    Four things the doing found, each of which had shipped green:
    - **The registered handler panicked on the path that WORKS.** `on_start_device` held
      `settings.borrow_mut()` alive across a `match` — a scrutinee temporary lives to the end of the
      match in every edition — and then took `settings.borrow()` in the `Ok` arm. Every refusal test
      passed, because only the success path took it. No test could see inside that closure at all:
      the callbacks are registered in a `wire()` function now, on a window a test can make, and
      `the_registered_centre_button_handler_survives_a_device_that_resolves` drives the real one.
    - **`Class::ToolMissing`'s named command reached no pixel.** `RailRow` had no field for it, so
      §9.3's one class with no next step would have rendered as a paragraph, two invisible controls
      holding a 78 px band open, and `Dismiss`. `RailRow.mono` carries it now and `ui/preview.slint`
      draws that row.
    - **`why ›` opened the Work page on "Nothing is happening."** The shelf's refusal is a fact about
      the device, computed when its row was built; the Rail only ever got an entry from a press. It
      carries the device index now and files the sentence before it opens the page.
    - **The Rail grew without bound.** The module's own claim that it does not was true of no code
      path — `collapse_finished` is called only from `plan`, which nothing calls. A note or a failure
      identical to the one before it is now one entry; two *different* ones are still two.
13. **`Settings::save()` acquires callers**: **at first-run step 1**, after every completed first-run
    step, after any Composer `Create` or `Save`, after any Parts add or remove, and on close (together
    with `record_boot()` and the remembered geometry). `Settings::load()` called `seed_resources()`,
    which mutates, and never wrote the answer back — a list that put back what you removed, every
    launch — and the same save points fix it.
    **Half done 2026-08-21, and the half is narrower than it first was.** `seed_resources` returns
    whether it changed anything and **`Settings::load_and_seed`** persists it; `Settings::load`
    stays a pure read. The first attempt put the write inside `load`, and that is a save on a path
    nobody asked to save on: `ipod-boot` calls `load` from five places that only want the default
    drive, one of which is `<recipe> --print`, documented as *showing the command line and running
    nothing*. It rewrote the operator's settings, and `render` is generated from the model — so
    every comment they had added went with it. Two more things fixed with it: `save` writes through
    a `.part` and a rename, because `fs::write` truncates first and a process that died between the
    two left a device list that was half a file; and it returns `io::Result`, because a read-only
    home used to be swallowed and `ipod-boot setup` printed *"Saved to …"* about a file it had not
    written. One constraint nothing enforces: `load_and_seed` writes only into a file that already
    existed, because `migrate_legacy` declines the moment one exists here and a minted file would
    block a carry-forward for ever.
    **The first-run save points are built** (2026-08-21): at step 1, where the identity is minted
    and filed — and the ordering there is load-bearing, because a save that fails must leave the
    same iPod for the next press — and after every completed step, in `Queue::pump`. The Composer
    and Parts ones wait on those surfaces.
    **`migrate_legacy` has its first caller as of 2026-08-21**, and it runs before the first read, in
    `fn main`, ahead of `load_and_seed`. **Two of the window's save points are built**: after a
    successful resolve — where the window is still on screen and a failure becomes a Rail entry — and
    on `on_close_requested` as a backstop, whose failure has nowhere left to be shown and says so in
    the one `eprintln!` left in `main.rs`. The first-run, Composer and Parts save points wait on those
    surfaces.
14. **`README.md`, `docs/DEVELOPING.md` and `docs/GETTING-THE-FILES.md` are corrected** where they
    describe a window that does not exist: drop-anywhere, the `S` / `D` / `Esc` keys, parking on
    close, `IPOD_LAYOUT`, and the model table's **197** rows against the README's 198.
15. **DONE.** §9.5's replacement pane, which became load-bearing when item 9 landed and stayed
    open for two revisions after it. `min-height` moved from a hand-written `860px` to
    `Geometry.min-height` = **400**, which is right — §9.6 argues a window minimum is a floor and
    not a fit, and 860 was a minimum no 1280 × 800 display can satisfy, so it guarded the drag and
    never the display class §9.5 is actually about. But the boolean that replaces the layout below
    the threshold was **computed correctly, pushed in on every change, and drawn by nothing**, so
    between 400 and ~810 logical the 655.751 px device was positioned past the bottom edge and
    drawn there. The entry that stood here recorded that, and recorded that the previous notes
    about it had been wrong twice.
    **`ShortPane` is in `ui/bench.slint` now**: the measurement, the paragraph, and §9.5's 44 px
    primary Row carrying the cradle's own callback. It **replaces** rather than shrinks — the well
    goes `visible: false`, per §16.3 and for the two reasons §9.5 now gives — and
    `Bench::focus-cradle` routes to the Row below the threshold so the window opens with the press
    focused on the display class this state exists for. **`Fullscreen` is absent rather than
    disabled**, which is §9.5's own rule read against a build §12.2 puts in `Off`.
    **Three things the building found, none of which was the pane.** §9.5's *one source* for the
    label could not be taken literally: the shipped cradle spells the press *Press the centre
    button*, and a pane that draws no centre button repeating it is the phantom §9.5 exists to
    delete. `main::Press` splits the caption into a per-surface prefix and a shared tail, and
    `the_two_press_surfaces_share_every_tail` is what stops a third wording. **Shelf row 2 was the
    same defect and §9.5 names it** — *The centre button makes one* — read against a bench with no
    device on it, because the shelf is deliberately left alone below the threshold; it counts the
    press now. And the pane's headline says **window**, not display: §9.6 moved the threshold onto
    `winit::Window::inner_size()` precisely because the two disagree, so the draft's wording was
    false in the one case a person can do something about.
    The test that asserted the gap — `the_too_short_state_is_an_input_with_nothing_reading_it`,
    which said of itself *the moment §9.5's pane reads it, this goes red* — is deleted, and
    `main::the_short_pane_replaces_the_bench_below_the_threshold_and_not_above_it` stands where it
    stood, reading the accessible tree rather than the markup. `_out/gui/bench-too-short.png` is the
    picture.
16. **Parts shows `Resource::Bootloader`.** `resource_rows` rendered four groups — iPods, Apple
    firmware, Software, Disks — and dropped the fourth kind on the floor, which is §3's own named
    complaint and §11.4's six-group requirement. **`resource_rows` and `ResourceRow` are deleted**
    (2026-08-21) with the Resources tab; per §16.10 the assertion they carried was re-expressed
    rather than dropped, onto `Provenance::line()` and onto a window-side test that an item nobody
    recorded a provenance for contributes the empty string. This item is now about the Parts page,
    which does not exist. `Settings::parse` will produce one from
    `res.N.kind = bootloader`, and item 2 gave it an honest provenance column that nothing can
    display. **A clean-looking Parts page is not evidence that no bootloader is filed**, and that
    is the sentence this item exists to delete.

17. **What the drawer and the Rail landed with, and what is still open behind them** (2026-08-21).
    Built: the drawer and its push, `MenuPage` (seven rows, six disabled with their reason and their
    escape hatch, in a `Scroll`), the Work page with §10.1's ledger pinned under it, the Rail, the
    eight-primitive vocabulary's `Pressable` / `Row` / `Scroll` / `Icon`, `nav::Stack` as the single
    writer of where you are, and §8.4's reduced motion read per platform into one global.

    **Deferred with what each waits on**, so that none of them is mistaken for finished: `bench.rs`'s
    cradle-state table and `machine.rs` (every §7.3 row but three needs a `Config` this window does
    not hold); `drops.rs` (§16.4's winit hook); `keys.rs` (§16.8's bare-letter half needs a
    `field-focused` property, and declaring one nothing writes is item 15's defect in a new place);
    `persist.rs`; and §9.5's pane. Of §16.8's eleven keyboard rows, five are wired — `Tab`, `Esc`,
    `←`/`→`, `Enter`/`Space`, `⌘\` — and `⌘,` opens the **menu** rather than Reference, because
    pointing it at a page that does not exist landed on a blank panel with no header and no way out,
    bypassing the very row that states the gap. That table should be read as the design and not as a
    description of the program.

18. **DONE 2026-08-21. §10's first run runs**, end to end, from the drawn centre button: a boot ROM
    minted once, Apple's firmware fetched and SHA-256 checked, a drive built, Apple's software
    installed and read back as bootable, and the boot handed off to Phase 7. `work.rs` is the queue
    and it is toolkit-free; `volume.rs` measures what a filesystem will do with an 8 GiB file by
    making one; `tooling.rs` asks whether this computer can fetch anything by running the tool.
    The shelf's 3 px bar, `Rail::line`'s working half and `RailRow.sub` all have producers now, and
    the two live-but-inert next steps — `Cancel` and the `Fix` — are wired or disabled with a
    reason.

    Six things the doing found, each of which had shipped green:
    - **The later-empty bench had no route to an iPod.** §10.3's third paragraph is the whole of it.
    - **The press was routed by a session-wide boolean rather than by the row that was pressed**, so
      a device somebody composed by hand resumed the first run instead of starting it.
    - **A resumed run ticked nothing it skipped.** The bundle was in the cache, the build and the
      install ran, and `fetch Apple's firmware` sat `Planned` for ever — which left `first_unticked`
      stuck behind it, so §12.2's handoff could never fire and the window said nothing at all when
      the drive was finished.
    - **A `Done` sent on the worker's way out could be dropped**, after which `pump_once` stopped the
      timer and nothing ever drained again: a finished drive on disk that the library never learned
      about, and a next press building `my-5.5g (2).img` beside the orphan. `Queue::stop` threw the
      same report away on the close path.
    - **The whole worked run executed in no test a release run performs.** All six tests that reached
      a worker were `#[ignore]`d because they reach Apple's servers, so the build, the install, the
      `aupd` marking, the rename, the read-back and both cancellation boundaries were reached by
      nothing. They run offline now, against a synthetic `.ipsw` the test module builds, in about
      160 ms; only the download is still behind `#[ignore]`.
    - **`build_volume`'s own pre-write refusal was 32× under the real FAT32 floor**, so a drive too
      small to hold a FAT32 volume passed the check that exists to catch exactly that and failed
      afterwards, with the file already created and sized.

    **Still open**, and none of them is this phase's to close: §9.5's pane, so a 1280 × 800 display
    still has no drawn route to a press (§10.4 calls it the escape hatch that matters); `Verb::Start`
    itself, which hands off rather than booting; `geometry::BODY_ADVANCE`, which is a budget rather
    than a measurement and which `RAIL_VERB_W`, `PARTS_VERB_W` and `CRADLE_LABEL_MAX_CHARS` are all
    three derived from — **and which item 20 found is generous at `weight-body` and not at
    `weight-strong`**, where the measured advance is 0.617 against the 0.62 budgeted; and the
    8 GiB-versus-30 GB question in §10.1, which is an operator decision.

19. **DONE 2026-08-22. The three drawer pages are joined to their producers.** `parts.rs`,
    `devices.rs` and `settings_page.rs` shipped tested and reachable from nothing: **six** callbacks
    were declared, fired by controls drawn enabled, and registered by nobody — `device-expand`,
    `device-row-action`, `parts-expand`, `parts-group-action`, `parts-row-action`,
    `setting-toggled` — and **eighteen** `in property` declarations had no setter anywhere in the
    program, so each drew its type's default. Parts was a header over three empty models; Settings
    drew three rows with empty labels, two of them disabled carrying an empty `reason`; and Devices
    never opened a row, which is where §7.2 puts `Start`, so the one control that page has was
    unreachable.

    `every_window_property_is_pushed_and_every_callback_registered` is the gate: every `in property`
    on `MainWindow` has a `set_`, every `callback` an `on_`, with a dated exception list that is
    asserted to hold **exactly** the properties that legitimately have neither. It holds one —
    `running`, which §12.2 will fill when the bench starts a machine.

    What the wiring found, none of it visible until the pages drew:

    - **`ui/devices.slint` drew every act in `Ink.danger` and `ui/parts.slint` drew the same struct
      in `Ink.accent`** — one `DetailRow`, one flattener, two colours — so `Edit…` was red. The
      colour is a fact about the act, so `parts::RowAction::destructive` answers it and both files
      bind the answer. `Remove` is the only true one.
    - **`ui/devices.slint` had no paragraph branch**, so §9.4's machine rule — the one line on that
      page whose teaching is the point — was drawn as a value in the fact column, indented past an
      empty label at `label-size`.
    - **`Synthesise…` refused with the sentence that says the Composer does not exist**, one row from
      the page it now opens. See §11.4.
    - **`Esc` closed the `Stack`'s expand id and nothing underneath it**, so the row a person had
      just closed was drawn open again on the next push. `nav::Escape::ClosedExpand` closes the
      cursor of whichever page owns it.
    - **`on_escape_pressed` was the one thing that moved the stack without re-pushing the page it
      moved to**, and `on_drawer_toggled` was the second: the drawer never writes `depth`, so closing
      and reopening it returns to a page that was told nothing while it was away.
    - **`ui/composer.slint` carried `back: "Devices"` as a literal.** The Composer has two entrances
      now, and a `‹` naming a page the press does not return to is the one control whose whole job
      is to say where you are going. `nav::Stack::under` is the answer and `drawer.slint` words it.

    **Still open**: §12.2's `running` is the one property with no producer. The two this item left
    open at Parts — `Fetch…` drawn live and only refusing, and a group verb eliding to `F…` — are
    item 20. The prediction made here was that the fix was *in `Pressable`'s own width*; it was not.
    See below.

20. **DONE 2026-08-22. The group verb that elided, and the one that only refused.** Two defects one
    row apart on §11.4, and the first was not where it looked.

    **`Fetch…` elided to `F…`, and `RAIL_VERB_W` was not the reason.** §17.Q12 had named that
    constant as the one nobody measured and predicted a verb would elide; a verb did, and it was a
    different column. §11.4's two group verbs share one `HorizontalLayout`, each carrying
    `horizontal-stretch: 1` and nothing else. A `Text` set to `overflow: elide` reports **one
    ellipsis** as its minimum width, so each control's floor was about four pixels; and when the row
    cannot hold both preferred widths Slint shrinks **by stretch**, which for two equal stretches is
    the same number of pixels off each. `Provide…`'s half prefers **411 px**, which is its reason
    drawn as one line rather than its label — a disabled control prefers the width of what it has to
    say — and `Fetch…`'s prefers **52**, which is its label. 463 px shed into a 360 px row asks 51.5 off each, and the half that had 52
    hit its ellipsis floor. **The sibling's reason was the cause and the absent floor was the
    defect.** `geometry::PARTS_VERB_W` is that floor, both controls carry it, and §17.Q12 now records
    the measurement rather than the question.

    **`Fetch…` was drawn live on all three groups that offer it and every press failed.** The verb
    asked `rail::Next::Retry`, which asks *is curl on this computer*; `caps.download` answers that by
    running `curl --version`; so on every computer with curl the control was blue over a hole. There
    is no per-part fetch — the only download this build starts is the first run's own plan — so it is
    now **disabled with a reason and a route** (§14.1, §9.4's second kind): the fetchers exist
    (`firmware::download`, `rockbox::download`) and nothing on this page reaches them, and the
    command that does it from a terminal is the group's own — `ipod-boot firmware get <family>` for
    Apple firmware, `ipod-boot rockbox-install` for a bootloader or for Rockbox. **A capability
    question is the wrong question when the mechanism behind the control does not exist**, and asking
    it draws a live control over a hole; that is the general form and it is worth more than the
    instance.

21. **DONE 2026-08-22. Three guards that read source text instead of watching the program, and two
    of them were green through a working defect.** The fourth of the family was found in
    `compose.rs` the day before — an `include_str!("install.rs")` + `.contains(…)` that stayed green
    when `(0..4)` became `(0..1)`, which makes every real iPod fail to find its data partition, and
    **panicked** when `.unwrap_or(0)` was respelled `.unwrap_or(0u8)`, which changes nothing. A
    sweep classified the rest. Each was measured blind first, then measured again against the same
    plant.

    - **`the_rail_never_dismisses_itself`'s model half** greps `src/rail.rs` for `std::time`,
      `Instant`, `SystemTime`, `Duration`. A `Rail` stamping `note` and `failed` from
      `eapp_loader::settings::now_unix()` and a `line()` that skips an entry four seconds old — a
      failure fading off the bench out from under somebody reading it, **§14.4's ban by name** —
      passed it and passed **all 342 tests in the crate**. The clock a program reaches for is the
      one its own workspace already has, and a vocabulary of four `std` spellings could not see it.
      `nothing_the_rail_says_changes_because_time_passed` watches instead: one Rail read twice
      across four seconds and a fifth, and a twin driven by the same calls compared against it. The
      grep survives, widened to that vocabulary and **renamed `no_clock_is_named_anywhere_in_rail_rs`
      so nobody mistakes it for protection**; the two bound different things, and the doc on each
      names the other.
    - **`the_window_computes_no_compatibility_rule_of_its_own`** refused the literals `0x0b`/`0x0c`
      in `composer.rs`. `if t == 11u8 { self.recipe.loader = compose::Loader::Apple; }` in
      `took_reading` — a volume type resetting somebody's bootloader — was green across the whole
      crate; the bytes `0x0b` in a **doc comment** saying volume types are not this page's business
      made it panic. It keeps its name and drives the page now, over 216 recipes: every answer it
      gives about a recipe is the model's answer for that recipe, and **every recipe it produces is
      one the model produced** — the second half being the one a duplicated rule cannot pass, since
      `set_volume_type` moves no bootloader.
    - **`the_identity_is_stored_before_anything_that_can_fail`** was the honest one, and its defect
      is narrower: `unwrap_or(usize::MAX)` read a needle that was not there as *infinitely late*,
      which compares as fine. Writing `Worker::spawn (plan, …)` with one extra space left it green
      while it had stopped checking the spawn at all. Needles are required now. Its behavioural half
      **exists after all**: its own note said the data directory could not be made unwritable
      because every test in the binary reads it, and `crate::data_dir_lock` — one re-entrant lock
      over `IPOD_EMULATOR_DATA` for the whole binary — is what made that false.
      `a_press_whose_save_fails_still_holds_the_ipod_it_minted` puts a regular file where the data
      directory should be, presses three times and finds one GUID. The source-order lock is
      **renamed `press_names_no_way_out_above_the_store_but_the_two_that_mint_nothing`** and kept for
      the half behaviour cannot reach: a way out that does not exist yet.

    **The general form.** A test that reads source text fails on spelling and passes on behaviour,
    which is the exact inverse of what a test is for; and where one is genuinely the right
    instrument — markup, an ordering, a word that must not appear — it has to say so in its own name
    and name the test that watches. Two further greps in this family survive on purpose and are
    recorded rather than fixed: `the_verdict_region_reads_the_predicate_and_not_the_string` already
    carries a behavioural half over all three `Start` variants, and `composer.rs`'s one-line
    `!body.contains("fn raw(")` sits inside a test that has already proved the mask holds.

22. **DONE 2026-08-22. A page of this window can be drawn to a PNG with no window, so looking at the
    program never costs the operator their focus again.** Six pages — the bench, the menu, Devices,
    Parts, Work, Settings — land in `_out/gui/*.png` from `cargo test --release -p ipod-gui
    every_page_this_window_draws_can_be_shot_with_no_window`. Nothing opens, nothing takes focus,
    and there is no event loop.

    **The mechanism, found by reading `i-slint-backend-testing-1.17.1` rather than guessed at.**
    `TestingBackendOptions::renderer_name` (`testing_backend.rs:150`) is the whole of it: its own doc
    says *"windows embed a real rasterizer so headless rendering (e.g. `Window::take_snapshot`)
    works"*. `init_no_event_loop()` leaves it `None`, which is a **mock** renderer with fixed
    test-font metrics whose `take_snapshot` answers `Err("WindowAdapter::take_snapshot is not
    implemented by the platform")`. Set to `"skia"` it resolves to `SkiaRenderer::default_software`,
    whose `take_snapshot` wraps a `SharedPixelBuffer` with `skia_safe::surfaces::wrap_pixels` and
    renders into it — never touching `self.surface`, which is why no surface and no window are
    needed. `a_window` sets the two fields `init_no_event_loop` sets and that third one; every window
    test in `main.rs` now runs on a real rasterizer, and any of them can take a shot.

    **It costs no crates.** `cargo tree -p ipod-gui --prefix none | sed 's/ (\*)//' | sort -u | wc -l`
    is **353 before and 353 after**: `i-slint-renderer-skia` is already compiled for the window's own
    `renderer-skia`, and the feature only makes the testing backend hand it out. `renderer-software`
    was measured too — **355**, and it would also be a second rasterizer, drawing the shot with code
    the program never ships.

    **A size and a root-item geometry are two numbers, and the assertion that reads the first cannot
    see the second.** Measured by removing `shoot`'s two setup lines one at a time: with neither, the
    shot is 800x600; with the clock tick alone it is a **1180x846 buffer holding a window laid out at
    800x600** — right dimensions, 1 451 colours, and two flat edges where the window stopped. Only
    `show()`'s `set_window_item_geometry` (`i-slint-core-1.17.1/window.rs:1641`) supplies the second
    number. The *size* has two suppliers, `show()` and the single-shot `Timer` that
    `WindowPropertiesTracker` arms and `mock_elapsed_time` runs, which is why it is the assertion
    that survives the most breakage and therefore proves the least.

    So the test asserts four things and each was shown red by breaking the drawing: the size (remove
    both lines), *no page is one flat colour* (hand back a buffer of the window background instead of
    the renderer's — `1 colour`), *the far edges are drawn* (remove the `show()` — the two outer
    bands read **1** colour where a drawn page reads 73 to 82), and *no two pages are the same
    picture* (remove the `push_nav`; and again, differently, remove the clock tick, which parks the
    drawer off screen). The far-quadrant version of the third was tried first and **discarded on
    measurement**: `work` drawn is 673 colours and `bench` stopped short is 611, so there is no
    threshold. On the two edges there is nothing but a number to pick.

**Conditional on §17.Q10**: `Stats::enters_by_core: [[u64; WATCHED.len()]; 2]`, if the run loop can
attribute an arrival to a core. Until it is answered, §12.8 draws one column.


