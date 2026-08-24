//! The Composer's state and the rules — `docs/GUI.md` §11.1, §11.2, §11.3.
//!
//! **One [`compose::Recipe`], and it is private.** The fields, the verdict, the plan and the two
//! totals are four views of it, and nothing here derives the same fact twice. Every writer is a
//! method that ends in [`Composer::recompute`], which rewrites the verdict, the plan and the cost
//! together before control returns to the caller — so no frame can render a recipe beside another
//! recipe's verdict, and nothing has to remember to refresh anything.
//!
//! **Toolkit-free**, like every file in this crate but `main.rs` and `client_height.rs`. It names no
//! Slint type, and the flattened row structs at the bottom exist so that the one file which does
//! can copy fields across without asking a question about compatibility on the way.
//!
//! ## What "never stale" means here, precisely
//!
//! The verdict region is always reserved, so it always says something, so it must never say
//! something it will take back. Three things make that mechanical rather than careful:
//!
//! 1. **There is one writer per field**, and every one of them ends in `recompute()`. There is no
//!    path that edits the recipe without rewriting what is drawn about it.
//! 2. **`best_loader()` runs before the verdict is computed**, not after — so ticking a system that
//!    needs a different bootloader moves the bootloader rather than drawing, for one frame, a
//!    refusal the program is about to fix itself.
//! 3. **Recomputation happens exactly twice**, and both are that same function: on an edit, and when
//!    a background volume read lands through [`Composer::took_reading`]. There is no timer on this
//!    path, so nothing changes without an edit or a completed read. `main.rs` spawns that read off
//!    [`Composer::volume_to_read`] and lands it through [`Composer::took_reading_of`], which is the
//!    same verb with the drive's name on it — an answer about a file that is no longer the chosen
//!    one is dropped rather than written onto the one that is.
//!
//! And the region says *reading …* rather than a verdict for exactly as long as the answer can still
//! change it — [`compose::Recipe::volume_decides`] is the predicate, and it is a three-way trial
//! rather than a guess about which answer is coming.

// ## What is silenced here, and by what
//
// **Nothing is silenced at all any more.** This file carries **no `#[allow(dead_code)]`**, on an
// item or otherwise. The five that were left all named the same missing piece of window — the two
// halves of the volume read and the three [`VolumeRead`] states they construct — and `main.rs` now
// spawns that read, so every one of their retirement conditions is met and all five are gone rather
// than reworded.
//
// **Both counts this paragraph used to carry were wrong, which is the argument against writing
// counts in prose.** It said *six* allows over five, because `Composer::editing`'s was deleted the
// day `devices.rs` gave it a caller and nobody re-read the summary two hundred lines up; and it
// said *eight* `#[cfg(test)]` over seven, because `Lock::presses` stopped being one the day `Pick`
// grew a press count. Neither drift could go red: a number in a comment is nothing's contract.
// Zero is the one count that stays true by itself, and the allows are at zero now.
//
// - **Seven `#[cfg(test)]`**, measured: six accessors and [`Region::claims_a_plan`], all of them
//   things only this file's tests can reach — the row structs carry the same values across, and
//   the tests use the accessors to check that the row and the recipe say one thing rather than
//   two. `geometry.rs` states the precedent twice: a thing kept alive by an allow is the shape
//   §16.9 deletes, and something nothing outside a test can call is one of those.
//
// **The window registers ten Composer callbacks**, in one run in `main.rs`: the nine `composer-*`
// ones `window.slint` declares, and `device-new`, which is one of this page's two entrances. The
// other is the Rail's `Fix`, which pushes `Page::Composer` from `take_next_step`; no press a person
// can make reaches it in this build, and `main.rs` says so beside the arm rather than pretending
// the arm is not there. Both entrances construct `Composer::new()`.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use eapp_loader::compose::{self, Cost, Fix, Holes, Loader, Os, Recipe, Start, Step, Verdict};
use eapp_loader::firmware;
use eapp_loader::identity::{self, Identity, Model, Refusal, TitleAuth, MODELS};
use eapp_loader::nor;
use eapp_loader::settings::{self, Resource, Settings};

/// Whether this is a new device or one that already exists.
///
/// **It decides nothing about what is locked.** GUI.md §11.1's *existing and new look identical* is
/// only true if the lock comes from the ROM and from whether a build is running — see [`Lock`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Mode {
    New,
    Editing { device: String },
}

/// Which of the four pages is on screen. Three levels, one row deep from the root.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Root,
    WhichIpod,
    WhatItRuns,
    NameIt,
}

/// Which control a callback is about.
///
/// Crosses the boundary into the markup as an `int`, so the vocabulary stays in Rust — a Slint enum
/// would put it in the markup, where nothing can be swept.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Field {
    Ipod,
    Model,
    Colour,
    Serial,
    Guid,
    Disk,
    From,
    Systems,
    Bootloader,
    Name,
}

impl Field {
    pub const ALL: [Field; 10] = [
        Field::Ipod,
        Field::Model,
        Field::Colour,
        Field::Serial,
        Field::Guid,
        Field::Disk,
        Field::From,
        Field::Systems,
        Field::Bootloader,
        Field::Name,
    ];

    /// `None` for anything outside the list, so a stray `int` from the markup is a no-op rather than
    /// a panic or, worse, a different control.
    pub fn from_i32(n: i32) -> Option<Field> {
        usize::try_from(n).ok().and_then(|i| Field::ALL.get(i)).copied()
    }

    /// Its index in [`Field::ALL`], which is the number the markup carries.
    pub fn as_i32(self) -> i32 {
        Field::ALL
            .iter()
            .position(|f| *f == self)
            .expect("ALL holds every variant") as i32
    }

    /// Which level draws it. The root draws none of them; it draws three rows that lead here.
    pub fn level(self) -> Level {
        match self {
            Field::Ipod | Field::Model | Field::Colour | Field::Serial | Field::Guid => {
                Level::WhichIpod
            }
            Field::Disk | Field::From | Field::Systems | Field::Bootloader => Level::WhatItRuns,
            Field::Name => Level::NameIt,
        }
    }

    /// Whether this control states a fact about **which iPod this is** rather than about the drive.
    ///
    /// The identity fields are the ones a dump fills and locks, and the ones a shared ROM warns
    /// about before they are edited — see [`Lock`].
    pub fn is_identity(self) -> bool {
        matches!(
            self,
            Field::Model | Field::Colour | Field::Serial | Field::Guid
        )
    }

    /// The label in the left column.
    pub fn label(self) -> &'static str {
        match self {
            Field::Ipod => "iPod",
            Field::Model => "Model",
            Field::Colour => "Colour",
            Field::Serial => "Serial",
            Field::Guid => "GUID",
            Field::Disk => "Disk",
            Field::From => "From",
            Field::Systems => "Systems",
            Field::Bootloader => "Bootloader",
            Field::Name => "Name",
        }
    }
}

/// What a control is, and why — GUI.md §11.1's *existing and new look identical*.
///
/// **Derived from the ROM and from whether a build is running, and never from [`Mode`].** That is
/// the whole of the promise: a retail dump and a synthesised iPod draw the same controls in the same
/// places at the same heights, and the retail case is simply locked with a reason attached. A lock
/// keyed on *am I editing* would jump the surface every time somebody switched between the two.
///
/// Precedence is `Building > Dump > Shared > Open`, and it is total: two of them can be true at once
/// and the reason a person needs is the one furthest up.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Lock {
    /// Editable, with nothing to say about it.
    Open,
    /// A retail dump states this, so it is read out rather than chosen.
    Dump,
    /// Editable — **and every device made of this iPod changes with it**, which is the point of
    /// composing rather than copying, and is said before the edit rather than discovered after it.
    Shared { devices: usize },
    /// The build this recipe describes is running.
    Building,
}

impl Lock {
    /// Whether the control refuses input. **`Shared` does not**: it is a consequence, not a wall —
    /// `Settings::run_device`'s own model is that the named resource wins, so refusing the edit
    /// would be the window contradicting the model.
    pub fn locked(&self) -> bool {
        matches!(self, Lock::Dump | Lock::Building)
    }

    /// The sentence in the reserved slot under the control. Empty only for [`Lock::Open`].
    pub fn reason(&self) -> String {
        match self {
            Lock::Open => String::new(),
            Lock::Dump => "Read from the dump; a device's identity is the ROM's, not ours.".into(),
            Lock::Shared { devices } => format!(
                "{devices} devices are made of this iPod and will change with it."
            ),
            Lock::Building => "building — this recipe is in use".into(),
        }
    }

    /// How many presses a control in this state takes before it acts.
    ///
    /// Two for [`Lock::Shared`], for the same reason [`compose::Fix::BuildFromIpsw`] takes two: the
    /// press changes something other than the thing under the finger. **Every device made of this
    /// iPod changes with it**, and a rule that is stated in a reason line and enforced nowhere is
    /// a sentence, not a confirmation.
    ///
    /// **Its retirement condition was met by [`Pick`] growing a tenth field.** The doc this
    /// replaces said *`#[cfg(test)]`, and what retires it is `Pick` growing a press count* — it
    /// has one, `Pick::presses`, which `to_pick` copies into `PickRow` and `PickerRow` binds onto
    /// the `Row`'s inherited `Pressable::presses`. So the answer computed here is now the answer
    /// on screen, which it was not for the whole of the time this said it could not yet be drawn.
    ///
    /// **[`FieldState`] deliberately does not get one, and that is not the same omission.** Serial
    /// and GUID are typed into, and a `TextInput` has no press to count; what a shared identity
    /// owes a person there is the sentence, which [`Lock::reason`] supplies into the `Field`'s own
    /// reserved slot and has all along.
    pub fn presses(&self) -> u8 {
        match self {
            Lock::Shared { .. } => 2,
            _ => 1,
        }
    }
}

/// The verdict region's four renderings — GUI.md §11.3.
///
/// **A type in the window and not a fourth [`compose::Verdict`] variant**, because two of the four
/// are not verdicts: *nothing chosen yet* is a fact about the recipe and *reading …* is a fact about
/// a background read. The window picks its colour from the variant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Region {
    /// `fg-dim`. Nobody has said where the drive comes from.
    Nothing(&'static str),
    /// `fg-dim`. A volume read is outstanding and its answer can still change the verdict.
    Reading(String),
    /// `fg-dim`. What booting this drive will be like.
    Ok(String),
    /// `fg`. Why it will not work, and the one press that resolves it.
    No { why: String, fix: Option<Fix> },
}

impl Region {
    /// The text, whichever rendering this is — so a caller that only wants to draw a string does
    /// not have to match four ways to find one.
    pub fn text(&self) -> &str {
        match self {
            Region::Nothing(s) => s,
            Region::Reading(s) | Region::Ok(s) => s,
            Region::No { why, .. } => why,
        }
    }

    /// Whether it is drawn in `fg` rather than `fg-dim` — true for [`Region::No`] alone.
    pub fn emphatic(&self) -> bool {
        matches!(self, Region::No { .. })
    }

    /// Whether it asserts a plan. **False for three of the four**, which is the whole correction:
    /// the region used to read `Starts Apple's software, the way the iPod shipped.` before anybody
    /// had chosen a firmware.
    ///
    /// `#[cfg(test)]`: the window picks its colour from [`Region::emphatic`] and its words from
    /// [`Region::text`], and asks nothing else — so this predicate states the rule for the tests
    /// that check it and has nowhere else to be called from.
    #[cfg(test)]
    pub fn claims_a_plan(&self) -> bool {
        matches!(self, Region::Ok(_))
    }
}

/// Where a background read of the drive's data partition has got to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VolumeRead {
    /// Nothing has been asked.
    Idle,
    /// A read is outstanding.
    Pending,
    /// It answered.
    Read(u8),
    /// It could not be done, and said why. **Not a refusal of the recipe** — a drive nobody could
    /// read is not a drive that fails, and the verdict goes on without it.
    Failed(String),
}

/// The verdict region for a recipe and a read — **a free function, so the recompute and a test call
/// the same thing.**
///
/// The order of the four tests is load-bearing:
///
/// 1. **Nothing chosen** is first, because before anything is chosen there is nothing to read, and
///    a *reading …* there would be a claim about a file nobody picked. The predicate is
///    [`compose::Recipe::nothing_chosen`] and never a comparison against
///    [`compose::NOTHING_CHOSEN`] — the constant exists so this row and the model cannot drift, and
///    a string comparison would put the drift back.
/// 2. **Reading** is second, and only while the answer can still change the verdict. Saying it about
///    a settled fact is a spinner in front of an answer.
/// 3. and 4. are the model's own two, verbatim.
pub fn region(r: &Recipe, read: &VolumeRead) -> Region {
    if r.nothing_chosen() {
        return Region::Nothing(compose::NOTHING_CHOSEN);
    }
    if matches!(read, VolumeRead::Pending) && r.volume_decides() {
        return Region::Reading(format!("reading {}…", r.start.label()));
    }
    match r.check() {
        Verdict::Ok(s) => Region::Ok(s),
        Verdict::No { why, fix } => Region::No { why, fix },
    }
}

/// Which kind of identifier a [`Secret`] holds, so one type can mask both.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SecretKind {
    Serial,
    Guid,
}

/// The masking boundary — GUID.md §11.2. **There is no `raw()` and there must never be one.**
///
/// A screenshot of the identity page must not carry somebody's identifiers, so the value is masked
/// by default and `Show` reveals it. The two readers are deliberately different shapes:
///
/// * [`Secret::text`] is what is **drawn**, and it is masked unless revealed.
/// * [`Secret::editable`] is what crosses into the markup as an editable string, and it is `None`
///   while masked — so while a field is masked the markup holds no identifier at all, and a value
///   that is not on screen cannot be selected, copied or read out of the accessible tree.
///
/// The full value reaches the machine by an entirely different path — `nor::Source` into the ROM
/// image — and the two never cross.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Secret {
    full: String,
    revealed: bool,
    kind: SecretKind,
}

impl Secret {
    pub fn serial(full: &str, revealed: bool) -> Secret {
        Secret {
            full: full.to_string(),
            revealed,
            kind: SecretKind::Serial,
        }
    }

    pub fn guid(full: &str, revealed: bool) -> Secret {
        Secret {
            full: full.to_string(),
            revealed,
            kind: SecretKind::Guid,
        }
    }

    /// The only reader of the value, and it is masked unless revealed.
    pub fn text(&self) -> String {
        if self.revealed {
            return self.full.clone();
        }
        match self.kind {
            SecretKind::Serial => identity::mask_serial(&self.full),
            SecretKind::Guid => identity::mask_guid_hex(&self.full),
        }
    }

    /// `Some(full)` only while revealed. This is what becomes an editable string in the markup.
    ///
    /// **`Show` reveals and enables in one act**, so the drawn text and the editable text are never
    /// two different things. The cost is named: a serial cannot be typed without pressing `Show`
    /// first. The alternative is either drawing what you type as bullets — which is a password
    /// field, and this is not a secret — or drawing the value while claiming it is masked.
    pub fn editable(&self) -> Option<&str> {
        self.revealed.then_some(self.full.as_str())
    }

    pub fn revealed(&self) -> bool {
        self.revealed
    }

    /// The control's own label: what pressing it will do next.
    pub fn action(&self) -> &'static str {
        if self.revealed {
            "Hide"
        } else {
            "Show"
        }
    }
}

/// What a completed [`Composer::commit`] did, for the caller that has to build it.
///
/// **The Composer does not start a build**, because a `Worker` is singular and this file knows
/// nothing about the one that may already be running. It files the device and hands back what the
/// queue needs — which is also what makes *save first, spawn second* the order rather than a hope.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Commit {
    /// The name the device was filed under.
    pub device: String,
    /// The recipe to build, exactly as the verdict was computed from.
    pub recipe: Recipe,
    /// What it will boot — GUI.md §12.3's denominator key. [`Composer::commit`] records it on the
    /// device through [`Settings::set_boot_shape`], so this is a copy of a stored fact rather than
    /// the only place it exists.
    pub shape: compose::BootShape,
    /// Whether that differs from what the device booted when the Composer **opened**.
    ///
    /// **It is not the question that clears the denominator**, and it stopped being it the moment
    /// `Settings::set_boot_shape` was given its caller. That function compares against the shape
    /// the device *recorded*, which is the only thing a stored number can be vouched for by; a
    /// device that recorded none loses its number once whatever this says. This is the Composer's
    /// own question — did what this device boots move while the page was open — and it is `false`
    /// for a new device, which has no denominator to lose.
    pub shape_changed: bool,
    /// `Some((old, new))` when this commit renamed an existing device.
    pub renamed: Option<(String, String)>,
}

/// The recipe under construction, and everything the four pages read.
pub struct Composer {
    mode: Mode,
    level: Level,
    open: Option<Field>,
    /// **The iPod.** Level ①'s five controls read this and not the recipe: a recipe carries no boot
    /// ROM, and the ROM is what constrains which firmware and which software can follow.
    rom: Option<nor::Source>,
    /// The name the ROM is filed under in the resources, once it has been filed.
    filed_as: String,
    /// **Private, and no `&mut` is ever handed out.** Every writer below is a method that ends in
    /// `recompute()`.
    recipe: Recipe,
    read: VolumeRead,
    name: String,
    /// Per-field, cleared whenever level ① is left, and **never persisted** — a `Show` that survived
    /// a relaunch would defeat the mask on the next screenshot.
    revealed: BTreeSet<Field>,
    serial_refusal: Option<Refusal>,
    guid_refusal: Option<Refusal>,
    /// What the device booted when this opened, for GUI.md §12.3. `None` for a new device, which
    /// has no denominator to lose.
    opened_with: Option<compose::BootShape>,
    /// Said once, when the device this was editing left underneath it.
    vanished: bool,

    // ── derived, rewritten together by `recompute()` ──
    region: Region,
    steps: Vec<Step>,
    cost: Cost,
    generation: u32,
}

impl Composer {
    /// A new device, with no iPod yet.
    ///
    /// **It mints nothing.** `nor::mint_seed` is the one irreversible call in this program — the
    /// seed *is* the iPod — so it happens when somebody presses `Make one`, not when a page opens.
    /// Three cancelled visits to this page must leave zero iPods, not three.
    pub fn new() -> Composer {
        let recipe = Recipe {
            // **Not `Recipe::default()`.** That names Apple's bundle with an empty string, which is
            // the nothing-chosen state — and it is the state this opens in — but writing it out
            // here says so, where `default()` reads as though a choice had been made.
            start: Start::FromIpsw(String::new()),
            loader: Loader::Apple,
            oses: [Os::Apple].into_iter().collect(),
        };
        let mut c = Composer {
            mode: Mode::New,
            level: Level::Root,
            open: None,
            rom: None,
            filed_as: String::new(),
            recipe,
            read: VolumeRead::Idle,
            name: String::new(),
            revealed: BTreeSet::new(),
            serial_refusal: None,
            guid_refusal: None,
            opened_with: None,
            vanished: false,
            region: Region::Nothing(compose::NOTHING_CHOSEN),
            steps: Vec::new(),
            cost: Cost::NONE,
            generation: 0,
        };
        c.recompute();
        c
    }

    /// Open on a device that already exists.
    ///
    /// `recipe` is the caller's, resolved through the settings — the model owns the rules for
    /// turning a `Device` into a `Recipe`, and a second copy of them here is the drift this file
    /// exists to prevent. `None` when there is no device of that name.
    ///
    /// **Its retirement condition is met.** The allow this carried said *retired when something
    /// outside this file calls it*, and `devices::Devices::editor` now does — §11.2's *existing
    /// and new look identical* has the surface it was missing, and the `Mode::Editing` title
    /// `push_composer` has always drawn is finally constructed by something.
    pub fn editing(s: &Settings, device: &str, recipe: Recipe) -> Option<Composer> {
        let d = s.devices.iter().find(|d| d.name == device)?;
        let rom = s.nor_of(d).cloned();
        let mut c = Composer::new();
        c.mode = Mode::Editing {
            device: device.to_string(),
        };
        c.name = device.to_string();
        c.filed_as = d.firmware.clone();
        c.rom = rom;
        c.opened_with = Some(recipe.shape());
        c.recipe = recipe;
        c.recompute();
        Some(c)
    }

    // ── read-only accessors ───────────────────────────────────────────────────────────────────

    pub fn mode(&self) -> &Mode {
        &self.mode
    }
    pub fn level(&self) -> Level {
        self.level
    }
    /// **Shared, never `&mut`.** The only writers are the setters below.
    pub fn recipe(&self) -> &Recipe {
        &self.recipe
    }
    /// Bumped by every recompute. A two-press control armed against one recipe must not fire
    /// against another, and the markup has no other way to know one arrived.
    pub fn generation(&self) -> u32 {
        self.generation
    }
    /// **The five below are `#[cfg(test)]`, and the sixth in the run is not.** Each is a private
    /// field's accessor, and the window never asks for one: [`Root`] hands it the region, the plan
    /// and the two totals; [`Which`] hands it what the ROM resolved to, since `Which::ipod` is
    /// `suggest_ipod_name` of it; and `filed_as` is read by [`Composer::devices_sharing`] and by
    /// [`Composer::commit`], which hands it to `Settings::restate_firmware` — both a level below
    /// anything the window asks for. What the tests do with these is check that the row and the recipe say
    /// the same thing rather than two things — `consistent`, which
    /// `the_verdict_the_plan_and_the_recipe_are_one_recipe` calls after every kind of edit.
    ///
    /// [`Composer::open`] sits in the middle of the run and is **not** gated: `push_composer` reads
    /// it on every frame, through `set_composer_open_field` at `main.rs:5099`.
    #[cfg(test)]
    pub fn region(&self) -> &Region {
        &self.region
    }
    #[cfg(test)]
    pub fn plan(&self) -> &[Step] {
        &self.steps
    }
    #[cfg(test)]
    pub fn cost(&self) -> Cost {
        self.cost
    }
    #[cfg(test)]
    pub fn rom(&self) -> Option<&nor::Source> {
        self.rom.as_ref()
    }
    #[cfg(test)]
    pub fn filed_as(&self) -> &str {
        &self.filed_as
    }
    pub fn open(&self) -> Option<Field> {
        self.open
    }
    /// `#[cfg(test)]` for the same reason as the five above, and one more: the window's half of the
    /// volume read is the pair that *writes* it — [`Composer::asked_for_reading`] and
    /// [`Composer::took_reading`] — and neither hands the state back out.
    #[cfg(test)]
    pub fn read(&self) -> &VolumeRead {
        &self.read
    }

    /// The identity this iPod will present, when there is one and it can be resolved.
    pub fn identity(&self) -> Option<Identity> {
        self.rom.as_ref().and_then(|r| r.identity().ok())
    }

    /// The model, from the ROM. **Never from the recipe** — a recipe carries no boot ROM.
    pub fn model(&self) -> Option<&'static Model> {
        self.rom.as_ref().and_then(|r| r.model())
    }

    /// The serial this iPod will present, **or the one somebody typed that it will not.**
    ///
    /// `nor::Source::identity()` builds a `Provided` identity only when a GUID is set, and falls
    /// back to `Identity::generate` otherwise — so a serial typed without a GUID is *ignored*, and
    /// reading this row through `identity()` alone would draw the seed's serial back at somebody who
    /// had just typed a different one and make it look as though the typing had not landed.
    ///
    /// It is stored and it is drawn; [`Composer::can_commit`] is where it is refused, with the
    /// sentence that says why.
    pub fn serial(&self) -> Secret {
        let typed = match &self.rom {
            Some(nor::Source::Synthetic {
                serial: Some(s), ..
            }) => Some(s.clone()),
            _ => None,
        };
        Secret::serial(
            typed
                .or_else(|| self.identity().and_then(|i| i.serial))
                .unwrap_or_default()
                .as_str(),
            self.revealed.contains(&Field::Serial),
        )
    }

    /// The GUID, read the same way and for the same reason — a `Provided` identity that fails to
    /// build (because the serial beside it is malformed) must not blank the field somebody is
    /// looking at.
    pub fn guid(&self) -> Secret {
        let typed = match &self.rom {
            Some(nor::Source::Synthetic { guid: Some(g), .. }) => Some(format!("{g:016X}")),
            _ => None,
        };
        Secret::guid(
            typed
                .or_else(|| self.identity().map(|i| i.guid_hex()))
                .unwrap_or_default()
                .as_str(),
            self.revealed.contains(&Field::Guid),
        )
    }

    /// The sentence under the Serial field: the outstanding refusal, worded for the field's own
    /// masking state.
    pub fn serial_reason(&self) -> String {
        self.serial_refusal
            .as_ref()
            .map(|r| r.text(self.revealed.contains(&Field::Serial)).to_string())
            .unwrap_or_default()
    }

    pub fn guid_reason(&self) -> String {
        self.guid_refusal
            .as_ref()
            .map(|r| r.text(self.revealed.contains(&Field::Guid)).to_string())
            .unwrap_or_default()
    }

    /// What could ever be authorised against this identity, **and the sentence for it**, said where
    /// the decision is made rather than in a footnote.
    pub fn title_auth(&self) -> Option<(TitleAuth, &'static str)> {
        let t = self.identity()?.title_auth();
        Some((t, t.line()))
    }

    /// The strongest evidence there is that a dump did not parse, when there is any.
    pub fn oui_warning(&self) -> Option<String> {
        self.identity()?.oui_warning()
    }

    /// What a control is in, and why — see [`Lock`]. `building` is whether the queue's current run
    /// belongs to this device; it is an argument rather than a question this file asks, so a gate
    /// wired to a phase nothing computes cannot pretend to fire.
    pub fn lock(&self, f: Field, s: &Settings, building: bool) -> Lock {
        if building {
            return Lock::Building;
        }
        if !f.is_identity() {
            return Lock::Open;
        }
        if matches!(self.rom, Some(nor::Source::File(_))) {
            return Lock::Dump;
        }
        let users = self.devices_sharing(s);
        if users > 1 {
            return Lock::Shared { devices: users };
        }
        Lock::Open
    }

    /// How many devices are made of this iPod. `0` when it has not been filed yet.
    pub fn devices_sharing(&self, s: &Settings) -> usize {
        if self.filed_as.is_empty() {
            return 0;
        }
        s.devices
            .iter()
            .filter(|d| d.firmware == self.filed_as)
            .count()
    }

    /// `Create` for a new device, `Save` for one that exists.
    pub fn footer_label(&self) -> &'static str {
        match self.mode {
            Mode::New => "Create",
            Mode::Editing { .. } => "Save",
        }
    }

    /// The filename stem the drive will get, so the Name field can state it before it exists.
    pub fn stem(&self) -> String {
        settings::file_stem_of(&self.name)
    }

    /// Whether committing would change what the device boots — GUI.md §12.3.
    ///
    /// `false` for a new device: there is no denominator to lose. `false` for an unchanged recipe,
    /// which is what keeps a re-save from throwing away a good number.
    pub fn shape_changed(&self) -> bool {
        match &self.opened_with {
            None => false,
            Some(before) => *before != self.recipe.shape(),
        }
    }

    /// [`Composer::can_commit`] plus the one question that needs the library.
    ///
    /// **`Create` says a name is taken before it is pressed, not after.** `Settings::remember_as`
    /// replaces a device of the same name outright, so a refusal that only happened on the press
    /// would be a control that looks live and is not — and [`Composer::commit`] goes through this
    /// too, so the sentence the button carries and the sentence the press produces are one string.
    pub fn can_commit_in(&self, s: &Settings) -> Result<(), String> {
        self.can_commit()?;
        let name = one_line_name(&self.name);
        let mine = match &self.mode {
            Mode::Editing { device } => Some(device.as_str()),
            Mode::New => None,
        };
        if s.devices
            .iter()
            .any(|d| d.name == name && mine != Some(d.name.as_str()))
        {
            return Err(format!(
                "There is already a device called {name}. Give this one another name."
            ));
        }
        Ok(())
    }

    /// Whether `Create` can be pressed, and the sentence for it when it cannot.
    ///
    /// The order is the order somebody works in, so the reason names the next thing to do rather
    /// than the last: the iPod, then what it runs, then the name. The one question it cannot answer
    /// is whether the name is taken — see [`Composer::can_commit_in`].
    pub fn can_commit(&self) -> Result<(), String> {
        if self.rom.is_none() {
            return Err(NO_IPOD.into());
        }
        // A typed identity that does not check is refused here as well as under its own field,
        // because `Create` is the control somebody presses after ignoring the sentence.
        if let Some(r) = &self.serial_refusal {
            return Err(r.text(self.revealed.contains(&Field::Serial)).to_string());
        }
        if let Some(r) = &self.guid_refusal {
            return Err(r.text(self.revealed.contains(&Field::Guid)).to_string());
        }
        // **GUI.md §11.2: `--serial` without `--guid` is refused, because the GUID is the field
        // with teeth.** `nor::Source` would silently ignore the serial and generate a fresh
        // identity from the seed, which is a typed value quietly not taking.
        if let Some(nor::Source::Synthetic {
            serial: Some(s),
            guid: None,
            ..
        }) = &self.rom
        {
            if !s.trim().is_empty() {
                return Err(SERIAL_NEEDS_GUID.into());
            }
        }
        if let Verdict::No { why, .. } = self.recipe.check() {
            return Err(why);
        }
        if self.name.trim().is_empty() {
            return Err("A device needs a name.".into());
        }
        Ok(())
    }

    // ── the only writers, and every one of them ends in `recompute()` ─────────────────────────

    pub fn set_level(&mut self, l: Level) {
        // Leaving level ① re-masks every identifier. A reveal is for the moment it is needed.
        if self.level == Level::WhichIpod && l != Level::WhichIpod {
            self.revealed.clear();
        }
        self.level = l;
        self.open = None;
        self.recompute();
    }

    /// Open one picker, which closes whichever was open. Only one at a time — two open Expands is
    /// two claims about where you are.
    pub fn set_open(&mut self, f: Option<Field>) {
        self.open = f;
        self.recompute();
    }

    /// Reveal or re-mask one identifier.
    pub fn set_reveal(&mut self, f: Field) {
        if !self.revealed.insert(f) {
            self.revealed.remove(&f);
        }
        self.recompute();
    }

    /// Mint an iPod — **the one irreversible act on this page**, because the seed is the iPod.
    ///
    /// Making a *second* one takes two presses and says what it costs; see
    /// [`Composer::make_one_row`]. This function does the minting and nothing else, so the press
    /// count lives in one place and the caller cannot invent a different one.
    pub fn make_one(&mut self) {
        let model = self
            .model()
            .map(|m| m.number.to_string())
            .unwrap_or_else(|| nor::DEFAULT_MODEL.to_string());
        self.rom = Some(nor::Source::Synthetic {
            model,
            seed: nor::mint_seed(),
            serial: None,
            guid: None,
            splash: None,
        });
        // A new iPod is not the old one: whatever it was filed under is not this.
        self.filed_as = String::new();
        self.serial_refusal = None;
        self.guid_refusal = None;
        self.revealed.clear();
        if self.name.trim().is_empty() {
            if let Some(r) = &self.rom {
                self.name = settings::suggest_device_name(r);
            }
        }
        self.follow_the_model();
        self.recompute();
    }

    /// What the `Make one` control costs, before it is pressed.
    ///
    /// Empty — and one press — while there is no iPod. Once there is, it is two presses and names
    /// the seed, because a seed is the whole of what makes an identity come back and the one that
    /// is about to be replaced is not recoverable from anything on screen.
    ///
    /// **The loss leads the sentence, and that is a measurement rather than a style.** `ReasonSlot`
    /// draws one `line-label` with `overflow: elide`, and at the drawer's 420 px that is about
    /// fifty characters — measured off `_out/gui/composer-ipod-dumped.png`, where the `sub` line
    /// beside it cuts at *"…comes back nex…"*. The wording this replaces put `seed {n}` at
    /// character 84 of 101, so the one word this control exists to say was the one word that never
    /// reached the screen. A sentence that elides before its point did not make it.
    pub fn make_one_row(&self) -> FixRow {
        let (presses, consequence) = match &self.rom {
            Some(nor::Source::Synthetic { seed, .. }) => (
                2,
                format!("seed {seed} is not kept; a new serial and FireWire GUID"),
            ),
            Some(nor::Source::File(_)) => (
                2,
                "this device stops using the dump; a synthesised iPod replaces it".to_string(),
            ),
            None => (1, String::new()),
        };
        FixRow {
            label: MAKE_ONE.into(),
            enabled: true,
            reason: String::new(),
            escape: String::new(),
            machine_rule: false,
            presses,
            consequence,
        }
    }

    /// Choose a different iPod, by model number.
    ///
    /// **This changes the serial and the FireWire GUID**, and the control says so before it is
    /// pressed — see [`Composer::model_row`]. `Identity::generate` mixes the model number into the
    /// seed precisely so that choosing a different iPod produces a different one, and a control
    /// that quietly did that while wearing a cosmetic label would be the surface lying about what a
    /// press does.
    ///
    /// A serial the user typed is **revalidated, not discarded**: the model decides which endings
    /// and which year digits are real, so the same string can be right for one iPod and wrong for
    /// the next, and throwing it away would make correcting the model cost the typing too.
    pub fn set_model(&mut self, number: &str) -> Result<(), String> {
        let m = Model::lookup(number)
            .ok_or_else(|| format!("{number} is not a model number this program knows"))?;
        match &mut self.rom {
            Some(nor::Source::Synthetic { model, .. }) => *model = m.number.to_string(),
            Some(nor::Source::File(_)) => {
                return Err("A dump states its own model; it is read, not chosen.".into())
            }
            None => return Err(NO_IPOD.into()),
        }
        self.revalidate_identity();
        self.follow_the_model();
        self.recompute();
        Ok(())
    }

    /// Type a serial. Empty clears it back to the seed's own.
    pub fn set_serial(&mut self, typed: &str) -> Result<(), String> {
        let t = typed.trim();
        match &self.rom {
            None => return Err(NO_IPOD.to_string()),
            Some(nor::Source::File(_)) => {
                return Err("A dump states its own serial; it is read, not typed.".into())
            }
            Some(nor::Source::Synthetic { .. }) => {}
        }
        let up = (!t.is_empty()).then(|| t.to_ascii_uppercase());
        if let Some(nor::Source::Synthetic { serial, .. }) = &mut self.rom {
            *serial = up;
        }
        self.revalidate_identity();
        self.recompute();
        match &self.serial_refusal {
            Some(r) => Err(r.text(self.revealed.contains(&Field::Serial)).to_string()),
            None => Ok(()),
        }
    }

    /// Type a FireWire GUID, as sixteen hex digits.
    pub fn set_guid(&mut self, typed: &str) -> Result<(), String> {
        let t = typed.trim();
        if !matches!(self.rom, Some(nor::Source::Synthetic { .. })) {
            return Err(match self.rom {
                Some(_) => "A dump states its own GUID; it is read, not typed.".into(),
                None => NO_IPOD.to_string(),
            });
        }
        if t.is_empty() {
            if let Some(nor::Source::Synthetic { guid, .. }) = &mut self.rom {
                *guid = None;
            }
            self.guid_refusal = None;
            self.revalidate_identity();
            self.recompute();
            return Ok(());
        }
        // **Refused, not warned.** A typed field is a claim; a read one is evidence. The check is
        // the model's, so the Composer and `ipod-boot` refuse the same values for the same reason.
        match Identity::check_guid_at(t) {
            Err(r) => {
                self.guid_refusal = Some(r.clone());
                self.recompute();
                Err(r.text(self.revealed.contains(&Field::Guid)).to_string())
            }
            Ok(()) => {
                let v = u64::from_str_radix(t.trim_start_matches("0x"), 16)
                    .expect("check_guid_at accepted it");
                if let Some(nor::Source::Synthetic { guid, .. }) = &mut self.rom {
                    *guid = Some(v);
                }
                self.guid_refusal = None;
                self.revalidate_identity();
                self.recompute();
                Ok(())
            }
        }
    }

    /// Apply the option at `index` of one picker's own list.
    ///
    /// **Resolved through the same iterators [`Composer::options`] drew from**, so the window never
    /// has to know that index 0 of the Disk picker means *build one* or that the iPod list skips
    /// every resource that is not a boot ROM. An index the list does not have is refused rather than
    /// clamped: clamping picks *something*, and the something is somebody else's iPod.
    pub fn choose(&mut self, s: &Settings, f: Field, index: usize) -> Result<(), String> {
        let out_of = |what: &str| format!("there is no {what} at position {index}");
        // **A row drawn disabled refuses to be picked.** A disabled control that acts when it is
        // somehow pressed is the same defect as a disabled `Fix` that applies — the surface said one
        // thing and the program did another. The reason it refuses with is the row's own.
        let drawn = self.options_of(s, f);
        match drawn.get(index) {
            None => return Err(out_of("option")),
            Some(o) if !o.enabled => {
                return Err(if o.reason.is_empty() {
                    format!("{} cannot be chosen", o.label)
                } else {
                    o.reason.clone()
                })
            }
            Some(_) => {}
        }
        match f {
            Field::Ipod => {
                if index == 0 {
                    self.make_one();
                    return Ok(());
                }
                let (name, src) = filed_ipods(s)
                    .nth(index - 1)
                    .ok_or_else(|| out_of("iPod"))?;
                self.rom = Some(src.clone());
                self.filed_as = name.to_string();
                self.serial_refusal = None;
                self.guid_refusal = None;
                self.revealed.clear();
                self.revalidate_identity();
                self.follow_the_model();
                self.recompute();
                Ok(())
            }
            Field::Model => {
                let m = emulable().nth(index).ok_or_else(|| out_of("model"))?;
                self.set_model(m.number)
            }
            Field::Colour => {
                let here = self.model().ok_or(NO_IPOD)?;
                let m = siblings(here).nth(index).ok_or_else(|| out_of("colour"))?;
                self.set_model(m.number)
            }
            Field::Disk => {
                if index == 0 {
                    // **Keep whichever bundle was already chosen.** Coming back to *build one* after
                    // looking at the library is not a reason to un-choose the firmware.
                    let keep = match &self.recipe.start {
                        Start::FromIpsw(f) => f.clone(),
                        _ => String::new(),
                    };
                    self.set_start(Start::FromIpsw(keep));
                    return Ok(());
                }
                let d = s.disks.get(index - 1).ok_or_else(|| out_of("disk"))?;
                self.set_start(Start::FromDisk {
                    name: d.name.clone(),
                    fat_type: None,
                });
                Ok(())
            }
            Field::From => {
                let r = video_releases().nth(index).ok_or_else(|| out_of("release"))?;
                self.set_start(Start::FromIpsw(r.file.to_string()));
                Ok(())
            }
            Field::Bootloader => {
                let l = *Loader::ALL.get(index).ok_or_else(|| out_of("bootloader"))?;
                self.set_loader(l);
                Ok(())
            }
            // Not pickers: two are typed into and one is a list of tick boxes.
            Field::Serial | Field::Guid | Field::Name | Field::Systems => {
                Err(format!("{} is not a picker", f.label()))
            }
        }
    }

    /// Where the drive comes from. A new start has not been read, so the read state resets with it.
    pub fn set_start(&mut self, st: Start) {
        self.recipe.start = st;
        self.read = VolumeRead::Idle;
        self.recompute();
    }

    pub fn set_loader(&mut self, l: Loader) {
        self.recipe.loader = l;
        self.recompute();
    }

    /// Tick or un-tick a system — **and the bootloader follows**, rather than the page telling you
    /// the one you had is wrong.
    pub fn set_os(&mut self, o: Os, on: bool) {
        if on {
            self.recipe.oses.insert(o);
        } else {
            self.recipe.oses.remove(&o);
        }
        // Before the verdict, not after: see `recompute`.
        self.recipe.loader = self.recipe.best_loader();
        self.recompute();
    }

    pub fn set_name(&mut self, n: &str) {
        self.name = n.to_string();
        self.recompute();
    }

    /// Apply the verdict's own fix. `false` when there is none to apply, or when it names a value
    /// the picker refuses — **a disabled control that acted when pressed would be worse than one
    /// that is not disabled at all.**
    pub fn apply_fix(&mut self) -> bool {
        let Region::No { fix: Some(f), .. } = &self.region else {
            return false;
        };
        let f = f.clone();
        if !f.offered() {
            return false;
        }
        self.recipe.apply(&f);
        // `Fix::BuildFromIpsw` detaches the volume, so whatever was read about it is about a drive
        // this recipe no longer starts from.
        if matches!(f, Fix::BuildFromIpsw) {
            self.read = VolumeRead::Idle;
        }
        self.recompute();
        true
    }

    /// Whether the recipe starts from a volume nobody has read.
    ///
    /// **One copy, and both readers of it are below.** [`Composer::asked_for_reading`] arms on it
    /// and [`Composer::volume_to_read`] answers the file on it, and they have to agree or the page
    /// spawns a read it never renders, or renders a read it never spawned.
    ///
    /// It is deliberately *not* [`compose::Recipe::volume_decides`], which is a stricter question
    /// — *would the answer change the verdict* — and is the one [`region`] draws *reading …* on.
    /// The read is asked for whenever there is something to read, because the answer is written
    /// onto the recipe and is worth having whether or not today's rules turn on it; what a person
    /// is told they are waiting for is the narrower question.
    fn unread_volume(&self) -> bool {
        self.recipe.has_a_volume() && self.recipe.volume_type().is_none()
    }

    /// Say that a volume read has been asked for. The region goes to *reading …* only if the answer
    /// can still change the verdict — [`compose::Recipe::volume_decides`] decides that, not this.
    pub fn asked_for_reading(&mut self) {
        if self.unread_volume() {
            self.read = VolumeRead::Pending;
            self.recompute();
        }
    }

    /// The drive file a background read is **due on**, or `None`.
    ///
    /// Pure, and the caller's cue to call [`Composer::asked_for_reading`] and spawn one. Three
    /// things have to hold: nothing has been asked yet, the chosen volume has not been read, and it
    /// resolves to a file. The first is what makes it safe to ask on every re-push — a page that
    /// asked twice would spawn a second thread for the same drive on the next frame, for ever.
    ///
    /// `set_start` puts the read back to `VolumeRead::Idle` whenever the drive moves, so choosing a
    /// different disk is what makes the next read due; nothing else does.
    pub fn volume_to_read(&self, s: &Settings) -> Option<PathBuf> {
        if !matches!(self.read, VolumeRead::Idle) || !self.unread_volume() {
            return None;
        }
        self.volume_on_disk(s)
    }

    /// The file the chosen volume **is**, whatever has been read about it.
    ///
    /// `None` for a bundle, which has no volume yet, and `None` for a library disk the settings no
    /// longer hold — a device can name a disk that has been removed from the library, and a read
    /// of a drive nobody can resolve is a read of nothing.
    ///
    /// **All three variants, in one match, once.** [`compose::Recipe::volume_type`]'s own doc says
    /// a second copy of this match is where the third variant gets forgotten; this is the window's
    /// half of the same question and it is written out here rather than at each caller.
    fn volume_on_disk(&self, s: &Settings) -> Option<PathBuf> {
        match &self.recipe.start {
            Start::FromIpsw(_) => None,
            Start::FromImage { path, .. } => {
                (!path.is_empty()).then(|| PathBuf::from(path.as_str()))
            }
            Start::FromDisk { name, .. } => s
                .disks
                .iter()
                .find(|d| d.name == *name)
                .map(|d| d.path.clone()),
        }
    }

    /// A background read landed, **about a named drive**. `true` when it was taken.
    ///
    /// This is the guard [`Composer::took_reading`] cannot carry, and it is not hypothetical: the
    /// read is asked for the moment a disk is chosen and answers whenever the filesystem gets round
    /// to it, and a person choosing a second disk in that window would otherwise have the first
    /// drive's partition type written onto the second drive's recipe. That is a verdict about a
    /// file nobody read — the exact shape of the false `Ok` this whole path exists to stop, arrived
    /// at from the other end.
    ///
    /// **Compared against the drive as it is now**, not against what was armed, so a person who
    /// picks a drive, picks another and picks the first one again is answered rather than ignored.
    pub fn took_reading_of(&mut self, s: &Settings, file: &Path, r: Result<u8, String>) -> bool {
        if self.volume_on_disk(s).as_deref() != Some(file) {
            return false;
        }
        self.took_reading(r);
        true
    }

    /// A background read landed.
    ///
    /// **A failure leaves the verdict alone rather than refusing.** A drive nobody could read is not
    /// a drive that fails — and leaving the region in `Reading` for ever is the one outcome that is
    /// certainly wrong, because the region would then be a spinner nothing ever stops.
    pub fn took_reading(&mut self, r: Result<u8, String>) {
        match r {
            Ok(t) => {
                self.recipe.set_volume_type(t);
                self.read = VolumeRead::Read(t);
            }
            Err(e) => self.read = VolumeRead::Failed(e),
        }
        self.recompute();
    }

    /// The device this was editing has gone. Returns the sentence to file, **once**.
    pub fn device_vanished(&mut self, s: &Settings) -> Option<String> {
        let Mode::Editing { device } = &self.mode else {
            return None;
        };
        if s.devices.iter().any(|d| d.name == *device) {
            return None;
        }
        if self.vanished {
            return None;
        }
        self.vanished = true;
        let was = device.clone();
        // It becomes a new device rather than an edit of one that is not there: pressing `Save`
        // against a device that has gone would file a device somebody deleted.
        self.mode = Mode::New;
        self.opened_with = None;
        self.recompute();
        Some(format!(
            "{was} is no longer in the list, so this is a new device now."
        ))
    }

    /// File the device — GUI.md §11.2's `Create` and `Save`, model half.
    ///
    /// **It does not build.** A `Worker` is singular and this file knows nothing about one that may
    /// already be running, so the order is *save first, hand the recipe over second* and the caller
    /// decides what the queue does with it.
    pub fn commit(&mut self, s: &mut Settings) -> Result<Commit, String> {
        // **Never overwrite a device somebody made by hand**, and refuse it in the words the
        // button was already wearing — `remember_as` replaces a device of the same name outright.
        self.can_commit_in(s)?;
        let name = one_line_name(&self.name);
        let previous = match &self.mode {
            Mode::New => None,
            Mode::Editing { device } => Some(device.clone()),
        };

        // **`Settings::rename_device` owns the rule, and this is its production caller.** It
        // renames in place — so the device keeps its boot denominator and its park time, which
        // `forget` plus `remember_as` would drop — and it moves `current` with the device. Two of
        // its three refusals are already spent above: `can_commit_in` refuses an empty name and a
        // name a *different* device holds, in the words the button was wearing. The third is the
        // one the hand-written rename got wrong — `false` because **the device is not there any
        // more** — and `renamed` then says nothing happened rather than reporting a rename of a
        // device somebody deleted while this page was open.
        let mut renamed = None;
        if let Some(old) = &previous {
            if *old != name && s.rename_device(old, &name) {
                renamed = Some((old.clone(), name.clone()));
            }
        }

        let rom = self.rom.clone().ok_or(NO_IPOD)?;
        s.nor = rom.clone();
        s.chassis = rom.model().map(|m| m.colour());
        // A drive already in the library is referenced; one that has still to be built is not there
        // to reference, and the device is *unfinished* rather than broken until it is.
        s.disk = match &self.recipe.start {
            Start::FromDisk { name, .. } => s
                .disks
                .iter()
                .find(|d| d.name == *name)
                .map(|d| d.path.clone()),
            Start::FromImage { .. } | Start::FromIpsw(_) => None,
        };
        // **What this iPod IS, restated — not a second one filed beside it.**
        //
        // §11.2's level ① tunes a filed identity, and `Lock::Shared` draws the consequence two
        // rows above the control it tunes: *N devices are made of this iPod and will change with
        // it.* That sentence was false. `file_away` files by **value**, an edited ROM is a value
        // nobody has filed, so a shared edit minted a second entry and repointed **this** device
        // alone — leaving the other N-1 on the old entry, unchanged, having been told otherwise.
        // `Settings::restate_firmware` is the verb that makes it true, and its own doc is the
        // argument for it: *editing one changes every device made of it, which is the point of
        // composing rather than copying.*
        //
        // **Before `remember_as` and not after, and that order is the whole of why one entry comes
        // out.** `remember_as` files the live ROM itself, by value, first thing — so run it before
        // this and the edited value is already in the list under a second name by the time
        // anything can restate the first, and restating then leaves two entries holding one iPod.
        // Restate first and `remember_as`'s own filing finds the value already there and mints
        // nothing.
        //
        // **An empty `filed_as` is a mint**, because `Composer::make_one` clears it — a new iPod is
        // not the old one, and that is the one case where two entries are two iPods. It is refused
        // *there* and not tested for here: an empty name matches `parse`'s unnamed `res.N.`
        // placeholders, so the rule is `restate_firmware`'s to own beside `filed_under`'s, and a
        // second copy of it in this file is a second answer to *has this iPod been filed*. `None`
        // back is that, or a name that no longer resolves; filing is the honest answer to both.
        //
        // **`suggest_nor_name` and not `suggest_ipod_name`, and the reorder is what made it
        // load-bearing.** While filing ran *after* `remember_as`, the value was always already in
        // the list and the suggested name was unreachable — so `Black 5.5G` sat here for as long
        // as it never named anything. It names things now, and §11.2 says what the name is:
        // `<model>, seed <n>`. `remember_as`, `seed` and the inline-recipe migration all spell it
        // through `suggest_nor_name`; a fourth spelling here would file one iPod under two names
        // depending on which route reached it first.
        let filed = match s.restate_firmware(&self.filed_as, rom.clone()) {
            Some(n) => n,
            None => s.file_away(
                Resource::Firmware(rom.clone()),
                &settings::suggest_nor_name(&rom),
                None,
            ),
        };
        s.remember_as(&name);

        // **`as_device` keeps whatever the stored device already named, and the Composer's whole job
        // is to change it.** That preference is right for a re-save — it is what stops a switch
        // between devices quietly cutting one loose from what it was made of — and wrong the moment
        // somebody chose a different iPod or a different drive here. So both references are stated
        // rather than inherited.
        let disk_name = match &self.recipe.start {
            Start::FromDisk { name, .. } => Some(name.clone()),
            _ => None,
        };
        let disk_path = s.disk.clone();
        let shape = self.recipe.shape();
        let shape_changed = self.shape_changed();
        if let Some(d) = s.devices.iter_mut().find(|d| d.name == name) {
            d.firmware = filed.clone();
            d.disk = disk_name;
            d.disk_path = disk_path;
            // **The one thing that tells this device apart from the first run's**, and without it
            // nothing could. A device filed here carries a synthesised boot ROM with a minted seed,
            // which is the whole of what `work::minted` asks — so the window read a composed device
            // as a half-made first run and offered to *finish* it by running the fixed first-run
            // plan, which consults no `Recipe`. It is set on a re-save as well as on a `Create`:
            // the fact is where the device came from, and a second save does not change it.
            d.composed = true;
        }
        // **What it boots, recorded — GUI.md §12.3, and this is the writer that field never had.**
        //
        // `Settings::set_boot_shape` is the single place the rule runs: same shape, keep the
        // number; different shape, store the new one and take the number. What stood here was the
        // second half alone — the denominator was cleared and the shape it moved to was recorded
        // nowhere — so `Device::boot_shape` was written by nothing in the program, and
        // `Settings::recipe_of`, which treats it as the **authority** on what a device boots, could
        // never take that branch. Every Edit re-derived the recipe from the drive's install list,
        // and a device composed as Rockbox-only whose drive records no install re-opened as
        // Apple-by-default.
        //
        // **It asks a different question from `shape_changed` and that is deliberate.** This
        // compares against the shape the device *recorded*, which is the only thing a stored number
        // can be vouched for by. A device written before the field existed has no recorded shape,
        // so it loses its number once, here, whatever the Composer thinks moved. `set_boot_shape`'s
        // own note is the argument; `a_device_with_no_recorded_shape_drops_its_denominator_once` is
        // the measurement.
        s.set_boot_shape(&name, &shape);
        self.filed_as = filed;

        self.mode = Mode::Editing {
            device: name.clone(),
        };
        self.name = name.clone();
        self.opened_with = Some(self.recipe.shape());
        self.recompute();
        Ok(Commit {
            device: name,
            recipe: self.recipe.clone(),
            shape: self.recipe.shape(),
            shape_changed,
            renamed,
        })
    }

    // ── the private half ──────────────────────────────────────────────────────────────────────

    /// **The single recomputation, and the order in it is load-bearing.**
    ///
    /// `best_loader()` has already run in [`Composer::set_os`], which is the only writer that can
    /// invalidate the bootloader — doing it there rather than here keeps this function a pure
    /// function of the recipe, so a test can call it twice and get the same answer.
    ///
    /// `Holes::Sparse` and not the probe's answer: the probe writes an 8 GiB file to find out, and
    /// nothing may be written before a person has agreed to the plan.
    fn recompute(&mut self) {
        self.region = region(&self.recipe, &self.read);
        self.steps = self.recipe.steps(Holes::Sparse);
        self.cost = self.recipe.cost(Holes::Sparse);
        self.generation = self.generation.wrapping_add(1);
    }

    /// Re-run the identity checks against whatever the ROM now says it is.
    fn revalidate_identity(&mut self) {
        let model = self.model();
        self.serial_refusal = match &self.rom {
            Some(nor::Source::Synthetic {
                serial: Some(s), ..
            }) => Identity::check_serial_at(s, model).err(),
            _ => None,
        };
        // The GUID's refusal is set where it is typed; a model change cannot invalidate it, because
        // the check is about Apple's OUI and nothing else.
    }

    /// Keep the chosen firmware honest about which iPod this is.
    ///
    /// GUI.md §11.1: an iPod states its model, and the model decides which bundles can follow. A
    /// bundle for the other generation is not a bundle for this one, so choosing a different iPod
    /// drops it rather than leaving a plan that names somebody else's software.
    fn follow_the_model(&mut self) {
        let Start::FromIpsw(file) = &self.recipe.start else {
            return;
        };
        if file.is_empty() {
            return;
        }
        // **A name this build does not know is left alone.** It cannot be said to belong to the
        // other generation, and clearing on *cannot tell* would throw away a choice on a
        // hand-edited settings file or a catalogue this build has not caught up with.
        let Some(rel) = firmware::by_file(file) else {
            return;
        };
        let families = self
            .model()
            .map(|m| m.generation.updater_families())
            .unwrap_or(&[]);
        if !families.contains(&(rel.updater_family as u32)) {
            self.recipe.start = Start::FromIpsw(String::new());
        }
    }
}

impl Default for Composer {
    fn default() -> Composer {
        Composer::new()
    }
}

/// The one spelling of the mint control's name.
///
/// It is the `iPod` row's *value* while there is no iPod (§11.2's mock: `iPod   Make one ›`) and
/// the *label* of option 0 of the picker that row opens, which is the control itself. Two literals
/// for one control is how the row and the option come to disagree about what the button is called.
const MAKE_ONE: &str = "Make one";

/// The sentence GUI.md §11.1 puts on levels ② and ③ until an iPod has been chosen.
pub const NO_IPOD: &str = "An iPod states its model, capacity, serial and GUID, and those decide \
                           which firmware can follow. Choose one first.";

/// Why nothing on this page can be copied, when the build has no pasteboard.
///
/// **It opens with `rail.rs`'s own words for the same fact** — `Next::CopyDetails`'s reason — and
/// then says what that costs *here*. One fact worded two ways on two surfaces is how a person comes
/// to believe they are two different problems, and `a_build_with_no_clipboard_does_not_offer_the_copy`
/// pins the shared half by reading it out of `rail.rs`.
pub const NO_CLIPBOARD: &str =
    "this build has no clipboard, so there is nowhere for the command to go.";

/// Whether this build can reach a pasteboard: `rail::Caps::clipboard`, **as its own type**.
///
/// It is not a second `bool` beside `building` because `which(&s, false, false)` says nothing about
/// which `false` is which, and the two mean opposite kinds of thing — one is *a build is running on
/// this machine right now*, the other *this program was compiled without a route to a clipboard*.
///
/// It is not `rail::Caps` itself because this module does not depend on `rail` and must not start
/// to: `Caps` is seven booleans about the whole window and one of them is measured per launch, and
/// a page that took the lot would be reading six answers it has no business asking. `main::wire`
/// converts at the boundary, which is the one place that holds both. (The original reason was that
/// `tests/composer.rs` mounted this file standalone with no `crate::rail` to name; that harness is
/// deleted — `main.rs` declares `mod composer;` — and the reason above outlived it.)
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Clipboard {
    Present,
    Absent,
}

impl From<bool> for Clipboard {
    fn from(present: bool) -> Clipboard {
        if present {
            Clipboard::Present
        } else {
            Clipboard::Absent
        }
    }
}

/// GUI.md §11.2's third behaviour, and the UI must not flatten it.
pub const SERIAL_NEEDS_GUID: &str =
    "A typed serial needs a GUID too: the GUID is the field with teeth, and without one the serial \
     would be quietly ignored and the seed's own identity used instead.";

// ─────────────────────────────────────────────────────────────────────────────────────────────
// The flattened rows the boundary reads.
//
// Plain data, one struct per shape the markup draws. They exist so that the one file which names a
// toolkit type copies fields across without asking a question about compatibility on the way — a
// question asked there is a rule that lives outside this file.
// ─────────────────────────────────────────────────────────────────────────────────────────────

/// One option inside an open picker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Choice {
    pub id: u32,
    pub label: String,
    pub sub: String,
    pub enabled: bool,
    pub chosen: bool,
    pub reason: String,
    pub escape: String,
    /// `true` for *this cannot work, ever*; `false` for *this is not finished, by us*.
    pub machine_rule: bool,
    /// **An option may cost two presses**, and exactly one in this program does: `Make one`, which
    /// mints over an identity already on screen. An option row is a `Row`, a `Row` is a
    /// `Pressable`, and a `Pressable` has counted presses since §11.3 — so the press count belongs
    /// on the option rather than on a control invented beside the picker to carry it.
    ///
    /// [`Composer::make_one_row`] is the one producer of this pair; see [`Composer::ipod_options`].
    pub presses: u8,
    /// What the second press will do, drawn **before** the first — §11.3's rule, and the whole of
    /// why two presses is a confirmation rather than a nuisance.
    pub consequence: String,
}

/// A row that opens a picker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pick {
    pub field: Field,
    pub label: String,
    pub value: String,
    pub locked: bool,
    pub note: String,
    pub reason: String,
    pub escape: String,
    pub machine_rule: bool,
    pub open: bool,
    /// [`Lock::presses`]'s answer — **two while N devices are made of this iPod.** `note` is the
    /// lock's own sentence and is what the `Row` draws as its `consequence`, so the count and the
    /// sentence it confirms arrive from the same [`Lock`] and cannot come apart.
    pub presses: u8,
}

/// A row somebody types into.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldState {
    pub field: Field,
    pub label: String,
    /// **What is drawn** — already masked when `masked`.
    pub value: String,
    /// The editable string, and `""` while masked. Never both.
    pub raw: String,
    pub masked: bool,
    pub locked: bool,
    pub mono: bool,
    pub note: String,
    pub reason: String,
    /// `Show` / `Hide` / `""`.
    pub action: String,
}

/// One system's tick box.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tick {
    pub os: Os,
    pub label: String,
    pub on: bool,
    pub enabled: bool,
    pub reason: String,
    pub escape: String,
}

/// A control that acts, with its press count and what that press costs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FixRow {
    pub label: String,
    pub enabled: bool,
    pub reason: String,
    pub escape: String,
    pub machine_rule: bool,
    pub presses: u8,
    pub consequence: String,
}

impl FixRow {
    /// The row for a [`compose::Fix`] the verdict offered — **every one of its four rules, read
    /// from the model rather than decided here.**
    pub fn of(fix: &Fix, start: &Start) -> FixRow {
        FixRow {
            label: fix.label(),
            enabled: fix.offered(),
            reason: fix.why_not_offered().into(),
            escape: fix.escape_hatch().into(),
            // An un-offered value is a project state — *we have not finished this* — and carries a
            // command, never a second Fix.
            machine_rule: false,
            presses: fix.presses(),
            consequence: fix.consequence(start),
        }
    }
}

/// One refusal, with the one `Fix` it may carry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refused {
    pub why: String,
    pub fix: Option<FixRow>,
}

/// Level ① — which iPod this is.
///
/// **Eight fields, and `push_composer` destructures every one of them by name.** It read seven of
/// nine and dropped `make_one` and `warning` — both safety affordances, both computed on every
/// frame, neither reaching a pixel — for as long as this struct was flattened field by field.
/// The exhaustive `let Which { .. }` in `main.rs` is what makes that unrepeatable: a field added
/// here stops the crate compiling until somebody names it, and a field named and then not used is
/// an `unused_variables` warning, which is an error under the `-D warnings` this workspace builds
/// with. `settings_page::View` reaches the same guarantee by counting markup declarations in a
/// test; a compile error is the stronger half of that pair and costs nothing.
///
/// **`make_one` is gone from this struct rather than given a property**, and that is the finding
/// rather than a shortcut. The control it described is not on this page — it is option 0 of the
/// iPod picker, the row [`Composer::choose`] answers by minting. Giving the page a second
/// `Make one` button to carry the count would have put two mint controls on one surface with one
/// of them lying about what it costs. So [`Composer::make_one_row`] keeps its single-producer job
/// and [`Composer::ipod_options`] is its caller; the press count and the seed sentence ride on the
/// [`Choice`] a person actually presses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Which {
    pub ipod: Pick,
    pub model: Pick,
    pub colour: Pick,
    pub serial: FieldState,
    pub guid: FieldState,
    pub title_auth: String,
    /// §11.2's OUI warning: **the strongest evidence there is that a dump did not parse**, said
    /// beside the value rather than on a stderr the window cannot see. Empty for a synthesised
    /// identity, which carries Apple's OUI by construction, and for a typed one, which
    /// `Identity::check_guid` refuses outright — §11.2's three behaviours, unflattened.
    pub warning: String,
    pub copy_command: FixRow,
}

/// Level ② — what it runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Runs {
    pub disk: Pick,
    pub from: Pick,
    pub systems: Vec<Tick>,
    pub loader: Pick,
    pub refusals: Vec<Refused>,
    /// GUI.md §11.1: `fg-disabled` with one sentence until an iPod has been chosen.
    pub disabled_reason: String,
}

/// Level ③ — the name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Named {
    pub name: FieldState,
    /// The filename stem the drive will get.
    pub stem: String,
    /// Non-empty when another device already holds this name.
    pub taken: String,
}

/// One of the root's three rows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RowValue {
    pub label: String,
    pub value: String,
    pub enabled: bool,
    pub reason: String,
}

/// The root page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Root {
    pub which: RowValue,
    pub runs: RowValue,
    pub named: RowValue,
    pub region: Region,
    pub plan: Vec<Step>,
    /// **The plan's two totals, unformatted.** The window has exactly one function that turns a
    /// `Cost` and the free space into the ledger's two lines, and the Work page already calls it —
    /// so this hands over the number and not the sentence, and the two surfaces cannot print two
    /// bills for one press.
    pub cost: Cost,
    pub create: FixRow,
}

impl Composer {
    /// The option list of the one open picker, or empty.
    pub fn options(&self, s: &Settings) -> Vec<Choice> {
        match self.open {
            None => Vec::new(),
            Some(f) => self.options_of(s, f),
        }
    }

    /// One picker's list, whether or not it is the open one — **the same function
    /// [`Composer::choose`] resolves an index against**, so the row that was drawn disabled is the
    /// row that refuses to be picked.
    pub fn options_of(&self, s: &Settings, f: Field) -> Vec<Choice> {
        match f {
            Field::Ipod => self.ipod_options(s),
            Field::Model => self.model_options(),
            Field::Colour => self.colour_options(),
            Field::Disk => self.disk_options(s),
            Field::From => self.firmware_options(),
            Field::Bootloader => self.loader_options(),
            // Not pickers: two are typed into and one is a list of tick boxes.
            Field::Serial | Field::Guid | Field::Name | Field::Systems => Vec::new(),
        }
    }

    /// **Option 0 is the mint, and it is [`Composer::make_one_row`]'s row.**
    ///
    /// [`Composer::choose`] answers index 0 by calling [`Composer::make_one`] — the one
    /// irreversible act on this page — so the press count and the sentence naming the seed have to
    /// be on *this* row and nowhere else. They were computed and then dropped: `Which::make_one`
    /// carried them, `push_composer` never read it, and replacing an iPod was one unconfirmed press
    /// that discarded the identity on screen.
    ///
    /// The row is built out of the same `FixRow` rather than beside it, so there is no second
    /// spelling of `Make one` and no second answer to what it costs.
    fn ipod_options(&self, s: &Settings) -> Vec<Choice> {
        let mint = self.make_one_row();
        let mut v = vec![Choice {
            id: 0,
            label: mint.label,
            sub: "generated from a seed, so the same iPod comes back next launch".into(),
            enabled: mint.enabled,
            chosen: matches!(self.rom, Some(nor::Source::Synthetic { .. })),
            reason: mint.reason,
            escape: mint.escape,
            machine_rule: mint.machine_rule,
            presses: mint.presses,
            consequence: mint.consequence,
        }];
        for (i, (name, src)) in filed_ipods(s).enumerate() {
            v.push(Choice {
                id: i as u32 + 1,
                label: name.to_string(),
                // **Never `nor::Source::describe`**, which interpolates the serial into a sentence.
                sub: settings::suggest_ipod_name(src),
                enabled: true,
                chosen: self.rom.as_ref() == Some(src),
                reason: String::new(),
                escape: String::new(),
                machine_rule: false,
                // Choosing a filed iPod replaces the one on screen too — but it replaces it with
                // something the library still holds, so nothing is destroyed and one press is the
                // honest count. `Make one` is the only option here that ends an identity.
                presses: 1,
                consequence: String::new(),
            });
        }
        v
    }

    /// **Only iPods this program can be.** The machine is a PP5021C with a 320x240 panel, and a
    /// Nano is not one — offering 197 rows of libgpod's table would be offering 190 machines this
    /// emulator cannot start.
    fn model_options(&self) -> Vec<Choice> {
        let chosen = self.model();
        let mut v = Vec::new();
        for (i, m) in emulable().enumerate() {
            v.push(Choice {
                id: i as u32,
                label: format!("{}, {} GB", m.generation.label(), m.capacity_gb),
                sub: m.apple_number(),
                enabled: true,
                chosen: chosen.is_some_and(|c| c.number == m.number),
                reason: String::new(),
                escape: String::new(),
                machine_rule: false,
                // **Not a second answer to the shared-iPod question.** A `Lock` decides whether
                // this picker may be opened at all, and its press count rides on the `Pick` that
                // opens it — see [`Composer::pick`]. Once it is open, choosing a row is one press.
                presses: 1,
                consequence: String::new(),
            });
        }
        v
    }

    /// **Only colours that generation and capacity shipped in.**
    ///
    /// A colour is a property of the model *number* — `A444` white, `A446` black, `A664` U2 are
    /// three rows and not one row with three colours — so choosing one is choosing a different iPod,
    /// and [`Composer::colour_row`] says so before it is pressed.
    fn colour_options(&self) -> Vec<Choice> {
        let Some(m) = self.model() else {
            return Vec::new();
        };
        let mut v = Vec::new();
        for (i, row) in siblings(m).enumerate() {
            v.push(Choice {
                id: i as u32,
                label: row.colour().label().into(),
                sub: row.apple_number(),
                enabled: true,
                chosen: row.number == m.number,
                reason: String::new(),
                escape: String::new(),
                machine_rule: false,
                presses: 1,
                consequence: String::new(),
            });
        }
        v
    }

    fn disk_options(&self, s: &Settings) -> Vec<Choice> {
        let mut v = vec![Choice {
            id: 0,
            label: "Build one".into(),
            sub: "from an Apple firmware bundle".into(),
            enabled: true,
            chosen: matches!(self.recipe.start, Start::FromIpsw(_)),
            reason: String::new(),
            escape: String::new(),
            machine_rule: false,
            presses: 1,
            consequence: String::new(),
        }];
        for (i, d) in s.disks.iter().enumerate() {
            v.push(Choice {
                id: i as u32 + 1,
                label: d.name.clone(),
                sub: if d.installed.is_empty() {
                    "from the library — referenced, not copied".into()
                } else {
                    format!("{} — referenced, not copied", d.installed.join(", "))
                },
                enabled: true,
                chosen: matches!(&self.recipe.start, Start::FromDisk { name, .. } if *name == d.name),
                reason: String::new(),
                escape: String::new(),
                machine_rule: false,
                // §11.3's two-press case is `Fix::BuildFromIpsw`, which DETACHES the drive a device
                // points at. Attaching one here points the recipe somewhere without unpointing
                // anything, and the drive is referenced rather than copied — nothing is spent.
                presses: 1,
                consequence: String::new(),
            });
        }
        v
    }

    /// **The offered firmware follows the chosen model** — GUI.md §11.1, made mechanical.
    ///
    /// The other generation's bundles are **drawn and disabled**, never absent: *never absent* is
    /// what makes the list teach which software belongs to which iPod, and an absent row teaches
    /// nothing. It is a machine rule — a 5G's software is not a 5.5G's, ever — so it carries no
    /// escape hatch.
    fn firmware_options(&self) -> Vec<Choice> {
        let mine = self
            .model()
            .map(|m| m.generation.updater_families())
            .unwrap_or(&[]);
        let mut v = Vec::new();
        for (i, r) in video_releases().enumerate() {
            let ok = mine.contains(&(r.updater_family as u32));
            v.push(Choice {
                id: i as u32,
                label: r.file.trim_end_matches(".ipsw").to_string(),
                sub: if r.variant.is_empty() {
                    r.model.to_string()
                } else {
                    format!("{} — {}", r.model, r.variant)
                },
                enabled: ok,
                chosen: matches!(&self.recipe.start, Start::FromIpsw(f) if f == r.file),
                reason: if ok {
                    String::new()
                } else {
                    match self.model() {
                        Some(m) => format!(
                            "the {}'s software; this iPod is a {}",
                            other_label(r.updater_family),
                            m.generation.label()
                        ),
                        None => NO_IPOD.to_string(),
                    }
                },
                escape: String::new(),
                // Only claimed where there is a refusal to classify: a 5G's software is not a
                // 5.5G's, ever, and that is a machine rule rather than something we have not
                // finished.
                machine_rule: !ok,
                presses: 1,
                consequence: String::new(),
            });
        }
        v
    }

    fn loader_options(&self) -> Vec<Choice> {
        Loader::ALL
            .into_iter()
            .enumerate()
            .map(|(i, l)| {
                // Two reasons, and they are not the same kind. *We have not finished this* comes
                // first because it is true whatever the recipe says; the machine rule is about
                // these parts on this drive.
                let (reason, escape, machine_rule) = if !l.offered() {
                    (l.why_not_offered().to_string(), l.escape_hatch().to_string(), false)
                } else if !self.recipe.loader_works(l) {
                    (self.recipe.why_not(l), String::new(), true)
                } else {
                    (String::new(), String::new(), false)
                };
                Choice {
                    id: i as u32,
                    label: l.label().into(),
                    sub: String::new(),
                    enabled: l.offered() && self.recipe.loader_works(l),
                    chosen: self.recipe.loader == l,
                    reason,
                    escape,
                    machine_rule,
                    presses: 1,
                    consequence: String::new(),
                }
            })
            .collect()
    }

    /// Level ①'s rows.
    ///
    /// `clipboard` is the one thing here that is a fact about the *build* rather than about the
    /// iPod, and it is carried in rather than assumed: the copy control is only real where a
    /// pasteboard is.
    pub fn which(&self, s: &Settings, building: bool, clipboard: Clipboard) -> Which {
        let lock = |f: Field| self.lock(f, s, building);
        let serial = self.serial();
        let guid = self.guid();
        Which {
            ipod: self.pick(
                Field::Ipod,
                match &self.rom {
                    None => MAKE_ONE.into(),
                    Some(r) => settings::suggest_ipod_name(r),
                },
                lock(Field::Ipod),
            ),
            model: self.model_row(lock(Field::Model)),
            colour: self.colour_row(lock(Field::Colour)),
            serial: self.field(Field::Serial, &serial, lock(Field::Serial)),
            guid: self.field(Field::Guid, &guid, lock(Field::Guid)),
            title_auth: self.title_auth().map(|(_, l)| l.to_string()).unwrap_or_default(),
            warning: self.oui_warning().unwrap_or_default(),
            copy_command: self.copy_command_row(clipboard),
        }
    }

    fn model_row(&self, lock: Lock) -> Pick {
        let mut p = self.pick(
            Field::Model,
            match self.model() {
                Some(m) => format!("{}, {} GB", m.generation.label(), m.capacity_gb),
                None => String::new(),
            },
            lock,
        );
        // **`!p.locked` and not `note.is_empty()` alone.** A locked picker changes nothing, and
        // `Pressable` draws `consequence` only while enabled — so a sentence written here for a
        // locked row is a string no pixel can read, which is the defect this whole commit is about.
        if !p.locked && p.note.is_empty() {
            p.note = "A different iPod — this changes the serial and the FireWire GUID.".into();
        }
        p
    }

    /// **The colour picker is an identity-changing control**, and it says so before it is pressed.
    ///
    /// `Identity::generate` mixes the model number into the seed, and white / black / U2 are three
    /// model numbers. Choosing one is choosing a different iPod; a cosmetic label over that would be
    /// the surface lying about what a press does.
    fn colour_row(&self, lock: Lock) -> Pick {
        let mut p = self.pick(
            Field::Colour,
            self.model().map(|m| m.colour().label().to_string()).unwrap_or_default(),
            lock,
        );
        // Guarded on `!p.locked` for the same reason [`Composer::model_row`] is.
        if !p.locked && p.note.is_empty() {
            p.note = "A different iPod — this changes the serial and the FireWire GUID.".into();
        }
        p
    }

    /// GUI.md §11.2's clipboard gate. **It copies a recipe, never a value.**
    ///
    /// `nor.rs`'s own doc says the seed *is* the iPod: `Identity::generate` is a pure function of
    /// `(model, seed)`, and the settings file stores that recipe rather than the values. So the
    /// command reproduces the machine exactly while carrying no identifier — and `Show` does not
    /// unlock anything, because a clipboard outlives the screen.
    ///
    /// A **typed** identity has no seed that reproduces it, so the control is disabled and says so
    /// rather than quietly copying a command that would rebuild a different iPod.
    ///
    /// And **the build's own gate comes first.** `main::caps()` reports no clipboard, and
    /// `on_copy_text` under this control can only decline — so a live accent-coloured `Copy the
    /// command line` over it is §14.1's phantom control, which this row shipped. The capability is
    /// asked before the identity because it is the answer for *every* iPod: telling somebody no
    /// seed reproduces theirs implies that some other iPod could be copied, and here none can.
    fn copy_command_row(&self, clipboard: Clipboard) -> FixRow {
        let (enabled, reason) = match &self.rom {
            _ if clipboard == Clipboard::Absent => (false, NO_CLIPBOARD.to_string()),
            Some(nor::Source::Synthetic { guid: Some(_), .. })
            | Some(nor::Source::Synthetic { serial: Some(_), .. }) => (
                false,
                "these values were typed, so no seed reproduces them — the command would have to \
                 carry them in full."
                    .to_string(),
            ),
            Some(nor::Source::Synthetic { .. }) => (true, String::new()),
            Some(nor::Source::File(_)) => (
                false,
                "this identity is read from a dump, so there is no recipe to copy.".to_string(),
            ),
            None => (false, NO_IPOD.to_string()),
        };
        FixRow {
            label: "Copy the command line".into(),
            enabled,
            reason,
            escape: String::new(),
            machine_rule: false,
            presses: 1,
            consequence: String::new(),
        }
    }

    /// The command that reproduces this iPod, **carrying no identifier**. Empty when there is none.
    ///
    /// **Composed by the model, not here.** What this used to write —
    /// `ipod-boot retail --nor-model {model} --nor-seed {seed}` — named two flags that have never
    /// existed in `ipod-boot`, and `retail` hands a flag it does not know to `trace` unchanged, so
    /// the copied line ran, booted somebody else's NOR and exited 0. See
    /// [`settings::reproduce_command`] for the whole of that and for why `make-nor` is the spelling
    /// that moved. Both halves are gated where they can be *run* rather than read: `ipod-boot`'s
    /// own test executes the argv this returns and reads the identity back out of the ROM it wrote.
    pub fn command_line(&self) -> String {
        self.rom
            .as_ref()
            .and_then(settings::reproduce_command)
            .map(|argv| argv.join(" "))
            .unwrap_or_default()
    }

    /// Level ②'s rows.
    pub fn runs(&self, s: &Settings, building: bool) -> Runs {
        let lock = |f: Field| self.lock(f, s, building);
        let mut refusals = Vec::new();
        if let Region::No { why, fix } = &self.region {
            // Nothing-chosen is not a refusal to draw a paragraph about; the picker one row above
            // resolves it, and it is already in the verdict region.
            if !self.recipe.nothing_chosen() {
                refusals.push(Refused {
                    why: why.clone(),
                    fix: fix.as_ref().map(|f| FixRow::of(f, &self.recipe.start)),
                });
            }
        }
        Runs {
            disk: self.pick(
                Field::Disk,
                match &self.recipe.start {
                    Start::FromIpsw(_) => "Build one".into(),
                    Start::FromImage { .. } | Start::FromDisk { .. } => {
                        self.recipe.start.label().to_string()
                    }
                },
                lock(Field::Disk),
            ),
            from: self.pick(
                Field::From,
                match &self.recipe.start {
                    Start::FromIpsw(f) => f.trim_end_matches(".ipsw").to_string(),
                    _ => String::new(),
                },
                lock(Field::From),
            ),
            systems: Os::ALL
                .into_iter()
                .map(|o| Tick {
                    os: o,
                    label: o.label().into(),
                    on: self.recipe.oses.contains(&o),
                    // A tick box is a control like any other, so a running build locks it too —
                    // otherwise `Systems` would be the one field on the page that a build did not
                    // reach, and it is the field that decides what the build is installing.
                    enabled: o.offered() && !building,
                    reason: if building {
                        Lock::Building.reason()
                    } else {
                        o.why_not_offered().into()
                    },
                    escape: if building {
                        String::new()
                    } else {
                        o.escape_hatch().into()
                    },
                })
                .collect(),
            loader: self.pick(Field::Bootloader, self.recipe.loader.label().into(), lock(Field::Bootloader)),
            refusals,
            disabled_reason: if self.rom.is_none() {
                NO_IPOD.into()
            } else {
                String::new()
            },
        }
    }

    /// Level ③'s row.
    pub fn named(&self, s: &Settings, building: bool) -> Named {
        let name = self.name.clone();
        let mine = match &self.mode {
            Mode::Editing { device } => Some(device.as_str()),
            Mode::New => None,
        };
        let taken = if s
            .devices
            .iter()
            .any(|d| d.name == one_line_name(&name) && mine != Some(d.name.as_str()))
        {
            format!("There is already a device called {name}.")
        } else {
            String::new()
        };
        let lock = self.lock(Field::Name, s, building);
        Named {
            name: FieldState {
                field: Field::Name,
                label: Field::Name.label().into(),
                value: name.clone(),
                raw: name,
                masked: false,
                locked: lock.locked(),
                mono: false,
                note: lock.reason(),
                reason: taken.clone(),
                action: String::new(),
            },
            stem: format!("{}.img", self.stem()),
            taken,
        }
    }

    /// The root page's three rows, the verdict, the plan and `Create`.
    pub fn root(&self, s: &Settings, building: bool) -> Root {
        let has_ipod = self.rom.is_some();
        Root {
            which: RowValue {
                label: "Which iPod".into(),
                value: match &self.rom {
                    None => "Make one".into(),
                    Some(r) => settings::suggest_ipod_name(r),
                },
                enabled: !building,
                reason: if building { Lock::Building.reason() } else { String::new() },
            },
            runs: RowValue {
                label: "What it runs".into(),
                // `Os::short`, so `Apple + Rockbox` fits the value column rather than eliding.
                value: {
                    let mut v: Vec<&str> = self.recipe.oses.iter().map(|o| o.short()).collect();
                    if v.is_empty() {
                        v.push("nothing");
                    }
                    v.join(" + ")
                },
                enabled: has_ipod && !building,
                reason: if building {
                    Lock::Building.reason()
                } else if !has_ipod {
                    NO_IPOD.into()
                } else {
                    String::new()
                },
            },
            named: RowValue {
                label: "Name it".into(),
                value: self.name.clone(),
                enabled: has_ipod && !building,
                reason: if building {
                    Lock::Building.reason()
                } else if !has_ipod {
                    NO_IPOD.into()
                } else {
                    String::new()
                },
            },
            region: self.region.clone(),
            plan: self.steps.clone(),
            cost: self.cost,
            create: FixRow {
                label: self.footer_label().into(),
                enabled: !building && self.can_commit_in(s).is_ok(),
                reason: if building {
                    Lock::Building.reason()
                } else {
                    self.can_commit_in(s).err().unwrap_or_default()
                },
                escape: String::new(),
                machine_rule: false,
                presses: 1,
                consequence: String::new(),
            },
        }
    }

    /// **One `Lock`, read three times, in one place.** `locked`, `note` and `presses` are all its
    /// answers — so a shared iPod's row cannot come to say *N devices change with it* while acting
    /// on the first press, which is what it did while [`Lock::presses`] was `#[cfg(test)]`.
    /// **One `Lock`, read four times, in one place.** `locked`, `note`, `reason` and `presses` are
    /// all its answers — so a shared iPod's row cannot come to say *N devices change with it* while
    /// acting on the first press, which is what it did while [`Lock::presses`] was `#[cfg(test)]`.
    ///
    /// **Which of `note` and `reason` the sentence goes in is `Lock::locked`'s own question**, and
    /// getting it wrong was visible in `_out/gui/composer-ipod-dumped.png`. `Pressable` reads
    /// `consequence` while enabled and `reason` while disabled, **with no fallback between them** —
    /// `Field` has one (`primitives.slint:46`), `Pressable` does not — so a `Lock::Dump` sentence
    /// parked in `note` left the locked Model and Colour rows reserving §5's 34 px and saying
    /// nothing at all, two rows above a `Field` drawing that same sentence correctly. That is
    /// `primitives.slint:454`'s own rule — *non-empty whenever `!enabled`* — broken on the page
    /// §9.4 was written for. A refusal is a reason; a shared iPod is a consequence; `locked()` is
    /// exactly that distinction and already knew the answer.
    fn pick(&self, field: Field, value: String, lock: Lock) -> Pick {
        let locked = lock.locked();
        let (note, reason) = if locked {
            (String::new(), lock.reason())
        } else {
            (lock.reason(), String::new())
        };
        Pick {
            field,
            label: field.label().into(),
            value,
            locked,
            note,
            reason,
            escape: String::new(),
            machine_rule: false,
            open: self.open == Some(field),
            presses: lock.presses(),
        }
    }

    fn field(&self, f: Field, secret: &Secret, lock: Lock) -> FieldState {
        FieldState {
            field: f,
            label: f.label().into(),
            value: secret.text(),
            // **`""` while masked**, so the markup holds no identifier at all.
            raw: secret.editable().unwrap_or_default().to_string(),
            masked: !secret.revealed(),
            locked: lock.locked(),
            mono: true,
            note: lock.reason(),
            reason: match f {
                Field::Serial => self.serial_reason(),
                Field::Guid => self.guid_reason(),
                _ => String::new(),
            },
            action: if self.rom.is_some() {
                secret.action().to_string()
            } else {
                String::new()
            },
        }
    }
}

/// The one-line form of a name — the same collapse [`Settings::remember_as`] applies before it
/// files anything.
///
/// **A local copy of a private helper, and it is here rather than borrowed** because the name this
/// checks for a collision and the name the model files under have to be one string: a name that
/// collided in one spelling and not the other would push a duplicate device. It is three
/// replacements and a trim; the alternative is checking against a name that is not the one saved.
fn one_line_name(s: &str) -> String {
    s.replace("\r\n", " ")
        .replace(['\r', '\n'], " ")
        .trim()
        .to_string()
}

/// Every boot ROM in the library, in list order — **one iterator, so the list that is drawn and the
/// list an index resolves against cannot come apart.** A resource list holds four kinds and this is
/// the only one a device can be made of.
fn filed_ipods(s: &Settings) -> impl Iterator<Item = (&str, &nor::Source)> {
    s.resources.iter().filter_map(|it| match &it.what {
        Resource::Firmware(src) => Some((it.name.as_str(), src)),
        _ => None,
    })
}

/// Every model this emulator can be: a PP5021C with a 320x240 panel, which is the two Video
/// generations and nothing else.
fn emulable() -> impl Iterator<Item = &'static Model> {
    MODELS.iter().filter(|m| {
        matches!(
            m.generation,
            identity::Generation::Video1 | identity::Generation::Video2
        )
    })
}

/// The rows that are the same iPod in another colour — same generation, same capacity.
fn siblings(m: &'static Model) -> impl Iterator<Item = &'static Model> {
    let (g, c) = (m.generation, m.capacity_gb);
    emulable().filter(move |r| r.generation == g && r.capacity_gb == c)
}

/// Every firmware release for the two Video generations, in the catalogue's own order.
fn video_releases() -> impl Iterator<Item = &'static firmware::Release> {
    let families: Vec<u32> = identity::Generation::Video1
        .updater_families()
        .iter()
        .chain(identity::Generation::Video2.updater_families())
        .copied()
        .collect();
    firmware::CATALOGUE
        .iter()
        .filter(move |r| families.contains(&(r.updater_family as u32)))
}

/// Which generation a family belongs to, for the sentence on a disabled bundle.
fn other_label(family: u16) -> String {
    for g in [identity::Generation::Video1, identity::Generation::Video2] {
        if g.updater_families().contains(&(family as u32)) {
            return g.label();
        }
    }
    format!("family {family}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use eapp_loader::identity::Colour;

    /// This file, as text, for the sweeps that hold the one-recipe rule.
    ///
    /// `include_str!` resolves against the file it is written in, so this answers the same whether
    /// the module is mounted from `main.rs` or from the test harness beside it.
    fn source() -> &'static str {
        include_str!("composer.rs")
    }

    /// The shipped half of this file — everything above the line that opens the test module.
    ///
    /// **The cut is that line, and it used to be the first `#[cfg(test)]`.** Three sweeps below
    /// read this, and all three split on the attribute — which named the same place for exactly as
    /// long as the test module was the only thing in this file wearing one. Replacing the
    /// module-wide `allow(dead_code)` with per-item gates put eight of them above line 600, and
    /// the old split then handed each sweep a body that stopped short of all of them: one of the
    /// three went red on an arithmetic it could no longer see, and **the other two went green on a
    /// body they had not read** — §6's shape, and the reason this is one function and not three
    /// expressions. `main.rs`'s `rust_sources` makes the same cut for the same reason.
    ///
    /// **How short, measured, because the commit that fixed this guessed it and a commit message
    /// cannot be amended.** That message said the old split handed each sweep *the first two
    /// hundred lines*. It handed them **42**, and the reason is worse than the guess: a `split` on
    /// a literal cannot tell an attribute from prose about one, and the first `#[cfg(test)]` in
    /// this file is not an attribute at all — it is the module header's own bullet counting them,
    /// on line 42, written by that same commit. So the commit that moved the cut off the attribute
    /// also wrote the line that made the attribute worst: the split ran at 2,043 lines before it
    /// and at 42 after, and not one of the eight per-item gates was inside either sweep's body.
    /// Cutting on the module line has no such twin, and the difference is `ends_with` rather than
    /// `contains`: prose may name that line without moving the cut, because a sentence about it
    /// does not end on it. `main.rs`'s `rust_sources` asserts that outright now, per file — a
    /// comment that ever does end there is caught rather than quietly obeyed.
    fn shipped() -> String {
        source()
            .lines()
            .take_while(|l| !l.trim_end().ends_with("mod tests {"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The 30 GB black 5.5G this program makes when nobody has said which.
    fn synthetic() -> nor::Source {
        nor::Source::Synthetic {
            model: nor::DEFAULT_MODEL.into(),
            seed: 7,
            serial: None,
            guid: None,
            splash: None,
        }
    }

    fn with_ipod() -> Composer {
        let mut c = Composer::new();
        c.rom = Some(synthetic());
        c.name = "My 5.5G".into();
        c.recompute();
        c
    }

    /// A settings holding one device made of one synthesised iPod. **Nothing here reaches the data
    /// directory**: `remember_as` files by value and writes no file.
    fn library(device: &str) -> Settings {
        let mut s = Settings {
            nor: synthetic(),
            ..Settings::default()
        };
        s.remember_as(device);
        s
    }

    /// The three derived values are a pure function of the recipe and the read, always.
    fn consistent(c: &Composer) {
        assert_eq!(
            *c.region(),
            region(c.recipe(), c.read()),
            "the verdict is not this recipe's"
        );
        assert_eq!(
            c.plan(),
            c.recipe().steps(Holes::Sparse).as_slice(),
            "the plan is not this recipe's"
        );
        assert_eq!(c.cost(), c.recipe().cost(Holes::Sparse), "the bill is not this plan's");
    }

    // ── one recipe ────────────────────────────────────────────────────────────────────────────

    /// **The fields, the verdict and the plan are one recipe.** Deriving the same fact twice is the
    /// drift this file exists to prevent, and the only way to prove it is to recompute every derived
    /// value from the recipe after every kind of edit and find them equal.
    #[test]
    fn the_verdict_the_plan_and_the_recipe_are_one_recipe() {
        let s = library("My 5.5G");
        let mut c = with_ipod();
        consistent(&c);

        c.set_start(Start::FromIpsw("iPod_25.1.3.ipsw".into()));
        consistent(&c);
        c.set_os(Os::Rockbox, true);
        consistent(&c);
        c.set_loader(Loader::Rockbox);
        consistent(&c);
        c.set_os(Os::Apple, false);
        consistent(&c);
        c.took_reading(Ok(0x0b));
        consistent(&c);
        c.set_start(Start::FromImage {
            path: "/drives/mine.img".into(),
            fat_type: None,
        });
        consistent(&c);
        c.asked_for_reading();
        consistent(&c);
        c.took_reading(Ok(0x0c));
        consistent(&c);
        c.set_name("Another");
        consistent(&c);
        c.set_level(Level::WhatItRuns);
        consistent(&c);

        // And what the root draws is the same three values, not a second derivation of them.
        let root = c.root(&s, false);
        assert_eq!(root.region, *c.region());
        assert_eq!(root.plan, c.plan());
        assert_eq!(root.cost, c.cost());
    }

    /// Every writer ends in the one recompute, and each of them moves the generation — which is what
    /// a two-press control armed against one recipe reads to disarm when another arrives.
    #[test]
    fn the_verdict_recomputes_on_every_edit_and_there_is_one_writer() {
        let mut c = with_ipod();
        let mut last = c.generation();
        let bump = |c: &Composer, last: &mut u32, what: &str| {
            assert!(
                c.generation() != *last,
                "{what} did not recompute: generation stayed at {}",
                *last
            );
            consistent(c);
            *last = c.generation();
        };

        c.set_start(Start::FromIpsw("iPod_25.1.3.ipsw".into()));
        bump(&c, &mut last, "set_start");
        c.set_os(Os::Rockbox, true);
        bump(&c, &mut last, "set_os");
        c.set_loader(Loader::Apple);
        bump(&c, &mut last, "set_loader");
        c.set_name("x");
        bump(&c, &mut last, "set_name");
        c.set_open(Some(Field::From));
        bump(&c, &mut last, "set_open");
        c.set_level(Level::WhichIpod);
        bump(&c, &mut last, "set_level");
        c.set_reveal(Field::Serial);
        bump(&c, &mut last, "set_reveal");
        c.set_serial("").expect("clearing a serial is allowed");
        bump(&c, &mut last, "set_serial");
        c.set_guid("").expect("clearing a guid is allowed");
        bump(&c, &mut last, "set_guid");
        c.took_reading(Err("no such file".into()));
        bump(&c, &mut last, "took_reading");
        c.make_one();
        bump(&c, &mut last, "make_one");
        c.set_model("A444").expect("a white 5.5G");
        bump(&c, &mut last, "set_model");
    }

    /// **The bootloader follows before the verdict is computed**, not after — otherwise the region
    /// draws, for one frame, a refusal the program is about to fix itself.
    #[test]
    fn the_bootloader_follows_before_the_verdict_is_computed() {
        let mut c = with_ipod();
        c.set_start(Start::FromIpsw("iPod_25.1.3.ipsw".into()));
        assert_eq!(c.recipe().loader, Loader::Apple);
        assert!(c.region().claims_a_plan(), "the fixture does not start Ok");

        c.set_os(Os::Rockbox, true);
        assert_eq!(
            c.recipe().loader,
            Loader::Rockbox,
            "ticking Rockbox did not move the bootloader"
        );
        assert!(
            c.region().claims_a_plan(),
            "the region refused a recipe the program had already fixed: {}",
            c.region().text()
        );
        consistent(&c);
    }

    /// **The always-reserved region must never assert a plan for a firmware nobody chose.**
    #[test]
    fn the_opening_state_of_the_composer_asserts_no_plan() {
        let c = Composer::new();
        assert_eq!(*c.region(), Region::Nothing(compose::NOTHING_CHOSEN));
        assert!(!c.region().claims_a_plan());
        assert!(
            !c.region().text().contains("Starts Apple's software"),
            "the opening state describes a plan: {}",
            c.region().text()
        );
        assert!(c.plan().is_empty(), "a plan for nothing: {:?}", c.plan());
        assert_eq!(c.cost(), Cost::NONE);
        assert!(c.can_commit().is_err(), "Create was live on an empty page");
    }

    /// And it stays that way through every state the page can be in before a start is chosen.
    #[test]
    fn the_verdict_region_says_nothing_it_will_have_to_take_back() {
        let mut c = Composer::new();
        for act in 0..4 {
            match act {
                0 => c.make_one(),
                1 => c.set_level(Level::WhatItRuns),
                2 => c.set_os(Os::Rockbox, true),
                _ => c.asked_for_reading(),
            }
            assert!(
                !c.region().claims_a_plan(),
                "step {act} asserted a plan with nothing chosen: {}",
                c.region().text()
            );
            assert_eq!(c.region().text(), compose::NOTHING_CHOSEN, "step {act}");
        }
    }

    /// **The predicate, never the string.** `NOTHING_CHOSEN` is a constant so that the region and
    /// the model cannot drift; comparing against it would put the drift straight back.
    #[test]
    fn the_verdict_region_reads_the_predicate_and_not_the_string() {
        let body = shipped();
        for bad in [
            "== compose::NOTHING_CHOSEN",
            "== NOTHING_CHOSEN",
            "NOTHING_CHOSEN ==",
            "contains(compose::NOTHING_CHOSEN",
            "\"nothing chosen yet\"",
        ] {
            assert!(
                !body.contains(bad),
                "the region is decided by a string comparison: {bad}"
            );
        }
        // The control: the matcher can see one when there is one.
        let planted = "if x == compose::NOTHING_CHOSEN { }";
        assert!(planted.contains("== compose::NOTHING_CHOSEN"));

        // And the predicate really is `nothing_chosen`, over all three starts.
        for start in [
            Start::FromIpsw(String::new()),
            Start::FromImage {
                path: String::new(),
                fat_type: Some(0x0b),
            },
            Start::FromDisk {
                name: String::new(),
                fat_type: None,
            },
        ] {
            let mut c = with_ipod();
            c.set_start(start.clone());
            assert_eq!(
                *c.region(),
                Region::Nothing(compose::NOTHING_CHOSEN),
                "{start:?} was not read as nothing chosen"
            );
        }
    }

    /// **A refusal about the parts is not lost while the drive is being read**: it is put down for
    /// the length of the read and picked up again verbatim, rather than being replaced by a
    /// permanent answer that happens to be a different one.
    ///
    /// This is the shape the correction takes rather than the one first written. *Rockbox on Apple's
    /// bootloader* is refused whatever the drive turns out to be — but a drive with **no** FAT32
    /// partition is refused for a different reason and by a rule that outranks it, so until the read
    /// lands the program does not know which of the two sentences is the true one. Drawing either
    /// would be choosing, and one of the choices is content that moves under the reader.
    #[test]
    fn a_refusal_that_is_not_about_the_volume_survives_the_reading_state() {
        let mut c = with_ipod();
        c.set_start(Start::FromImage {
            path: "/drives/mine.img".into(),
            fat_type: None,
        });
        c.set_loader(Loader::Apple);
        c.recipe.oses = [Os::Apple, Os::Rockbox].into_iter().collect();
        c.recompute();
        let before = c.region().clone();
        match &before {
            Region::No { why, .. } => assert!(why.contains("bootloader of its own"), "{why}"),
            other => panic!("the fixture does not refuse: {other:?}"),
        }

        // Put down for the read…
        c.asked_for_reading();
        assert_eq!(*c.region(), Region::Reading("reading mine.img…".into()));

        // …and picked up again, the same sentence and the same fix.
        c.took_reading(Ok(0x0b));
        assert_eq!(*c.region(), before, "the refusal came back as something else");
        consistent(&c);
    }

    /// **The trial is true for every unread volume today, and that is a fact about the rules rather
    /// than about this function.**
    ///
    /// Rule (2a) refuses a drive with no FAT32 data partition whatever is on it, so the `0x00`
    /// trial never agrees with the other two and `volume_decides` reduces to *there is a volume and
    /// nobody has read it*. This asserts the reduction, so that the day a rule change makes the
    /// trial discriminate again, something goes red and somebody re-reads the reasoning instead of
    /// inheriting it.
    #[test]
    fn the_reading_state_is_shown_for_every_unread_volume_today() {
        let starts = [
            Start::FromImage {
                path: "/drives/mine.img".into(),
                fat_type: None,
            },
            Start::FromDisk {
                name: "off my 5.5G".into(),
                fat_type: None,
            },
        ];
        let mut saw = 0;
        for start in &starts {
            for loader in Loader::ALL {
                for n in 0..8u8 {
                    let mut c = with_ipod();
                    c.set_start(start.clone());
                    c.recipe.loader = loader;
                    c.recipe.oses = Os::ALL
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| n & (1 << i) != 0)
                        .map(|(_, o)| *o)
                        .collect();
                    c.recompute();
                    assert!(
                        c.recipe().volume_decides(),
                        "an unread volume the read cannot decide: {:?} on {loader:?}",
                        c.recipe().oses
                    );
                    c.asked_for_reading();
                    assert!(
                        matches!(c.region(), Region::Reading(_)),
                        "an unread volume drew a verdict: {}",
                        c.region().text()
                    );
                    saw += 1;
                }
            }
        }
        assert_eq!(saw, 48, "the sweep is not every recipe over both volume starts");

        // A volume that has been read never waits, and neither does a bundle.
        let mut read = with_ipod();
        read.set_start(Start::FromImage {
            path: "/drives/mine.img".into(),
            fat_type: Some(0x0b),
        });
        read.asked_for_reading();
        assert!(!matches!(read.region(), Region::Reading(_)));
        let mut bundle = with_ipod();
        bundle.set_start(Start::FromIpsw("iPod_25.1.3.ipsw".into()));
        bundle.asked_for_reading();
        assert!(!matches!(bundle.region(), Region::Reading(_)));
    }

    /// A library with two drives in it, so that a read about one can land onto the other.
    fn two_drives(at: &std::path::Path) -> Settings {
        let mut s = library("My 5.5G");
        for (name, file) in [("off my 5.5G", "a.img"), ("off my 4G", "b.img")] {
            let path = at.join(file);
            std::fs::write(&path, b"a drive, as far as this file is concerned").expect("a drive");
            s.disks.push(settings::Disk {
                name: name.into(),
                path,
                built_from: None,
                installed: Vec::new(),
            });
        }
        s
    }

    /// **The file a read is due on is the one the library says the chosen disk is** — and it is due
    /// exactly once per chosen drive.
    ///
    /// The once matters as much as the file: `main.rs` asks this on every re-push of the page,
    /// which is every keystroke and every tick, so an answer that stayed `Some` while a read was
    /// already going would spawn a thread per frame for the rest of the window's life.
    #[test]
    fn a_read_is_due_once_per_chosen_drive_on_the_file_the_library_names() {
        let at = std::env::temp_dir().join(format!("ipod-composer-due-{}", std::process::id()));
        std::fs::create_dir_all(&at).expect("a scratch directory");
        let s = two_drives(&at);
        let mut c = with_ipod();

        // A bundle has no volume yet, so there is nothing to read about it.
        c.set_start(Start::FromIpsw("iPod_25.1.3.ipsw".into()));
        assert_eq!(c.volume_to_read(&s), None, "a bundle is not a drive");

        // A disk out of the library resolves through the library, not through its label.
        c.choose(&s, Field::Disk, 1).expect("the first disk");
        assert_eq!(c.volume_to_read(&s), Some(at.join("a.img")));

        // Asked for, and no longer due — this is what makes it safe to ask on every frame.
        c.asked_for_reading();
        assert_eq!(c.volume_to_read(&s), None, "a second thread would be spawned for one drive");
        c.took_reading(Ok(0x0b));
        assert_eq!(c.volume_to_read(&s), None, "a drive that has been read is read");

        // A different drive is a different read, and `set_start` is what arms it.
        c.choose(&s, Field::Disk, 2).expect("the second disk");
        assert_eq!(c.volume_to_read(&s), Some(at.join("b.img")));

        // A disk the library no longer holds resolves to nothing rather than to a guess.
        let empty = library("My 5.5G");
        assert_eq!(c.volume_to_read(&empty), None, "a drive nobody can resolve was read anyway");
        let _ = std::fs::remove_dir_all(&at);
    }

    /// **An answer about a drive that is no longer chosen is dropped**, rather than written onto
    /// the drive that is.
    ///
    /// Reachable in one move and not hypothetical: the read is asked for the moment a disk is
    /// picked and answers whenever the filesystem gets round to it, so picking a second disk in
    /// that window is one press. Without the guard the second drive wears the first drive's
    /// partition type — a verdict about a file nobody read, which is the same false `Ok` this
    /// whole path exists to stop, arrived at from the other end.
    #[test]
    fn a_read_that_lands_about_another_drive_is_not_taken() {
        let at = std::env::temp_dir().join(format!("ipod-composer-stale-{}", std::process::id()));
        std::fs::create_dir_all(&at).expect("a scratch directory");
        let s = two_drives(&at);

        let mut c = with_ipod();
        c.set_os(Os::IPodLinux, true);
        c.choose(&s, Field::Disk, 1).expect("the first disk");
        let asked_about = c.volume_to_read(&s).expect("a drive to read");
        c.asked_for_reading();

        // …and the person picks the other one before it answers. `set_start` puts the read back to
        // `Idle` and `main.rs`'s re-push arms the next one, which is the pair of lines below.
        c.choose(&s, Field::Disk, 2).expect("the second disk");
        let mine = c.volume_to_read(&s).expect("the second drive is due");
        c.asked_for_reading();

        assert!(
            !c.took_reading_of(&s, &asked_about, Ok(0x0c)),
            "an answer about {} was taken as an answer about {}",
            asked_about.display(),
            c.recipe().start.label()
        );
        assert_eq!(
            c.recipe().volume_type(),
            None,
            "the second drive is wearing the first drive's partition type"
        );
        assert!(
            matches!(c.region(), Region::Reading(_)),
            "the page stopped waiting on a drive nothing has answered about: {}",
            c.region().text()
        );

        // The drive that IS chosen is answered.
        assert!(c.took_reading_of(&s, &mine, Ok(0x0c)), "the chosen drive's own answer was dropped");
        assert_eq!(c.recipe().volume_type(), Some(0x0c));
        consistent(&c);
        let _ = std::fs::remove_dir_all(&at);
    }

    /// **A drive read as `0x0C` refuses the one bootloader that cannot start it** — and the answer
    /// arrives by the route the window uses, not by writing the field.
    ///
    /// `0x0C` is FAT32's LBA form. Both forms are legitimate; `ipodloader2` 2.9.0d reads only
    /// `0x0B` — its `vfs.c` has `case 0x83` and `case 0xB` and nothing else — and every drive taken
    /// off real hardware is `0x0C`. So this is the drive the reading state exists for: it looks
    /// like every other library disk until somebody reads its MBR, and the plan that installs onto
    /// it produces a drive that cannot boot.
    ///
    /// **Level ②'s tick box for iPodLinux is drawn disabled in this build** (`Os::OFFERED` holds
    /// Apple and Rockbox), so no press a person can make composes this recipe today and the window
    /// cannot show this refusal. That is a fact about what is offered, not about the rule — the
    /// rule is written, tested and correct, and the day `Os::OFFERED` grows this is what it does.
    /// `main.rs`'s `a_library_disk_stops_reading_ok_about_a_drive_nobody_has_read` is the half of
    /// the same defect that IS reachable by presses, and it goes through the window end to end.
    #[test]
    fn a_drive_read_as_0x0c_refuses_the_one_loader_that_cannot_boot_it() {
        let at = std::env::temp_dir().join(format!("ipod-composer-0x0c-{}", std::process::id()));
        std::fs::create_dir_all(&at).expect("a scratch directory");
        let s = two_drives(&at);

        let mut c = with_ipod();
        c.choose(&s, Field::Disk, 1).expect("the first disk");
        c.set_os(Os::IPodLinux, true);
        assert_eq!(c.recipe().loader, Loader::IPodLoader2, "the fixture's bootloader did not follow");

        // Before the read: waiting, and claiming nothing.
        c.asked_for_reading();
        assert!(!c.region().claims_a_plan(), "{}", c.region().text());

        let file = at.join("a.img");
        assert!(c.took_reading_of(&s, &file, Ok(0x0c)), "the chosen drive's own answer was dropped");
        match c.region() {
            Region::No { why, fix } => {
                assert!(why.contains("0x0C"), "the refusal does not name what was read: {why}");
                assert!(why.contains("No valid paritions found!"), "{why}");
                assert_eq!(fix.as_ref(), Some(&Fix::BuildFromIpsw), "no way out was offered");
            }
            other => panic!("a 0x0C drive under ipodloader2 read {other:?}"),
        }

        // The same drive read as `0x0B` is the one `ipodloader2` can start, so it does not refuse —
        // which is what stops the assertion above being satisfied by a page that refuses every
        // drive it manages to read.
        let mut ok = with_ipod();
        ok.choose(&s, Field::Disk, 1).expect("the first disk");
        ok.set_os(Os::IPodLinux, true);
        ok.asked_for_reading();
        assert!(ok.took_reading_of(&s, &file, Ok(0x0b)));
        assert!(ok.region().claims_a_plan(), "a 0x0B drive was refused: {}", ok.region().text());
        consistent(&ok);
        let _ = std::fs::remove_dir_all(&at);
    }

    /// **A spinner nothing stops is the one outcome that is certainly wrong.**
    #[test]
    fn a_failed_volume_read_does_not_leave_the_region_waiting_for_ever() {
        let mut c = with_ipod();
        c.set_start(Start::FromImage {
            path: "/drives/mine.img".into(),
            fat_type: None,
        });
        c.asked_for_reading();
        assert!(matches!(c.region(), Region::Reading(_)), "the fixture is wrong");

        c.took_reading(Err("/drives/mine.img: No such file or directory".into()));
        assert!(
            !matches!(c.region(), Region::Reading(_)),
            "the region is still reading after the read failed"
        );
        // A drive nobody could read is not a drive that fails.
        assert!(
            c.region().claims_a_plan(),
            "a failed read became a refusal: {}",
            c.region().text()
        );
        consistent(&c);
    }

    /// The region always says something, and each of the four is its own rendering.
    #[test]
    fn the_verdict_region_is_reserved_in_all_four_renderings() {
        let mut seen: Vec<Region> = Vec::new();

        let c = Composer::new();
        seen.push(c.region().clone());

        let mut c = with_ipod();
        c.set_start(Start::FromImage {
            path: "/drives/mine.img".into(),
            fat_type: None,
        });
        c.asked_for_reading();
        seen.push(c.region().clone());

        c.took_reading(Ok(0x0b));
        seen.push(c.region().clone());

        c.set_os(Os::IPodLinux, true);
        c.took_reading(Ok(0x0c));
        seen.push(c.region().clone());

        assert!(matches!(seen[0], Region::Nothing(_)), "{:?}", seen[0]);
        assert!(matches!(seen[1], Region::Reading(_)), "{:?}", seen[1]);
        assert!(matches!(seen[2], Region::Ok(_)), "{:?}", seen[2]);
        assert!(matches!(seen[3], Region::No { .. }), "{:?}", seen[3]);
        for r in &seen {
            assert!(!r.text().is_empty(), "an empty reserved region: {r:?}");
        }
        // Only the refusal is emphatic; the other three are `fg-dim`.
        assert_eq!(
            seen.iter().filter(|r| r.emphatic()).count(),
            1,
            "more than one rendering claims the page's attention"
        );
    }

    /// **One `Fix` control per refusal, and it wears the four rules.**
    #[test]
    fn there_is_one_fix_control_per_refusal() {
        let s = library("My 5.5G");
        let mut c = with_ipod();
        c.set_start(Start::FromImage {
            path: "/Volumes/backup/rockbox-test.img".into(),
            fat_type: Some(0x0c),
        });
        c.set_os(Os::IPodLinux, true);
        let runs = c.runs(&s, false);
        assert_eq!(runs.refusals.len(), 1, "{:#?}", runs.refusals);
        let fix = runs.refusals[0].fix.clone().expect("a way out");
        assert_eq!(fix.presses, 2, "the detaching fix is one press");
        assert!(
            fix.consequence.contains("rockbox-test.img"),
            "the consequence does not name what it detaches: {}",
            fix.consequence
        );
        assert!(fix.enabled, "the way out of a 0x0C volume was disabled");

        // Nothing chosen is not drawn as a refusal paragraph: the picker above resolves it, and it
        // is already in the verdict region.
        let empty = Composer::new();
        assert!(empty.runs(&s, false).refusals.is_empty());
    }

    /// **A `Fix` naming a value the picker refuses cannot be applied**, so a disabled control that
    /// somehow reached a press does nothing rather than setting a value the surface refused.
    #[test]
    fn a_fix_that_names_a_value_the_picker_refuses_cannot_be_applied() {
        let mut c = with_ipod();
        c.set_start(Start::FromIpsw("iPod_25.1.3.ipsw".into()));
        // A recipe holding iPodLinux — the shape a hand-edited settings file produces.
        c.recipe.oses = [Os::IPodLinux].into_iter().collect();
        c.recompute();
        let Region::No { fix: Some(f), .. } = c.region().clone() else {
            panic!("the fixture no longer refuses: {:?}", c.region());
        };
        assert_eq!(f, Fix::UseLoader(Loader::IPodLoader2));
        assert!(!f.offered(), "the picker offers ipodloader2 now");

        let before = c.recipe().clone();
        assert!(!c.apply_fix(), "a disabled fix acted");
        assert_eq!(*c.recipe(), before, "a disabled fix changed the recipe");

        // And an offered one does act.
        let mut d = with_ipod();
        d.set_start(Start::FromIpsw("iPod_25.1.3.ipsw".into()));
        d.recipe.oses = [Os::Apple, Os::Rockbox].into_iter().collect();
        d.recipe.loader = Loader::Apple;
        d.recompute();
        assert!(matches!(d.region(), Region::No { .. }), "the fixture is wrong");
        assert!(d.apply_fix(), "an offered fix did nothing");
        assert!(d.region().claims_a_plan(), "{}", d.region().text());
        consistent(&d);
    }

    // ── level ① ───────────────────────────────────────────────────────────────────────────────

    /// **The two look identical.** A retail dump fills and locks; a synthesised one is open. Same
    /// controls, same reserved slots, same masking — the retail case simply carries a reason.
    #[test]
    fn a_dump_and_a_synthesised_rom_draw_the_same_controls_at_the_same_heights() {
        let s = library("My 5.5G");

        let mut synth = with_ipod();
        synth.set_level(Level::WhichIpod);
        let a = synth.which(&s, false, Clipboard::Present);

        let mut dump = with_ipod();
        dump.rom = Some(nor::Source::File("/dumps/mine.rom".into()));
        dump.recompute();
        dump.set_level(Level::WhichIpod);
        let b = dump.which(&s, false, Clipboard::Present);

        // Same rows, same fields, in the same places.
        assert_eq!(a.model.field, b.model.field);
        assert_eq!(a.colour.field, b.colour.field);
        assert_eq!(a.serial.field, b.serial.field);
        assert_eq!(a.guid.field, b.guid.field);
        // Same masking, same trailing control, same mono column — so nothing moves.
        for (x, y) in [(&a.serial, &b.serial), (&a.guid, &b.guid)] {
            assert_eq!(x.masked, y.masked, "one of them is not masked by default");
            assert!(x.masked, "an identifier is drawn in the open by default");
            assert_eq!(x.mono, y.mono);
            assert_eq!(x.action, y.action, "one of them has no Show control");
            assert!(!x.action.is_empty());
        }
        // And the difference is exactly the lock, with a reason attached — **in the slot the row
        // will draw it from**, which is a different slot for the two of them. `Pressable` reads
        // `consequence` (which is `Pick::note`) while enabled and `reason` while disabled, and
        // there is no fallback between them, so a locked row's sentence parked in `note` is a
        // locked row saying nothing. This assertion held the sentence in `note` for both, which is
        // what `_out/gui/composer-ipod-dumped.png` showed the moment level ① was first
        // photographed: Model and Colour greyed, 34 px reserved under each, and nothing in it.
        assert!(!a.model.locked && !a.serial.locked, "a synthesised iPod was locked");
        assert!(b.model.locked && b.serial.locked, "a dump was left editable");
        assert!(
            b.model.reason.contains("Read from the dump"),
            "a locked row with no reason is a wall: reason {:?}, note {:?}",
            b.model.reason,
            b.model.note
        );
        assert!(
            b.model.note.is_empty(),
            "a locked row's `consequence` is drawn by nothing: {}",
            b.model.note
        );
        assert!(!a.model.note.is_empty(), "the open row's slot is not reserved");
        assert!(
            a.model.reason.is_empty(),
            "an enabled row's `reason` is drawn by nothing: {}",
            a.model.reason
        );
        // The heights this test is named for: both rows `tell`, so both are `ROW_H + FIELD_REASON`.
        // Which slot fills is the difference; that one of them fills is the invariant.
        for (which, p) in [("synthesised", &a.model), ("dumped", &b.model)] {
            assert!(
                !p.note.is_empty() || !p.reason.is_empty(),
                "the {which} Model row reserves a slot with nothing to put in it"
            );
        }
    }

    /// **One iPod, one name — the `iPod` row and the `Model` row two below it are about the same
    /// machine.**
    ///
    /// They were not. `suggest_ipod_name` asks `settings::model_of`, which refused to read a file,
    /// so a dump fell through to its **file stem** while the `Model` row read the same bytes
    /// through `nor::Source::model` and answered properly. `_out/gui/composer-ipod-dumped.png` is
    /// the picture of it: `iPod  foreign-oui` over `Model  5G, 30 GB`, and the picker below drawing
    /// `foreign-oui` twice — once as the name the library files it under and once as its own
    /// description of it.
    ///
    /// **The assertion reads the drawn values against each other rather than against a literal**,
    /// which is what makes it a statement about the page instead of about a format string: the
    /// name has to be the `Colour` row's word followed by the `Model` row's generation, whatever
    /// those two say.
    #[test]
    fn one_ipod_has_one_name_on_this_page() {
        use eapp_loader::identity::{Identity, Model, Source as IdSource};

        let at = std::env::temp_dir().join(format!("ipod-composer-onename-{}", std::process::id()));
        std::fs::create_dir_all(&at).expect("a scratch directory");
        let m = Model::lookup("MA146").expect("the reference 5G");
        let path = at.join("foreign-oui.rom");
        std::fs::write(
            &path,
            nor::synthesise(&nor::Spec::new(
                m,
                Identity {
                    serial: Some("AB1234XYZQR".into()),
                    guid: 0x001B_6300_ABCD_EF01,
                    source: IdSource::RealDevice,
                },
            )),
        )
        .expect("a fabricated dump");

        let mut s = Settings::default();
        let dump = nor::Source::File(path);
        let filed = s.file_away(
            Resource::Firmware(dump.clone()),
            &settings::suggest_nor_name(&dump),
            Some(settings::Provenance::Dumped),
        );

        let mut c = Composer::new();
        c.rom = Some(dump);
        c.filed_as = filed.clone();
        c.recompute();
        c.set_level(Level::WhichIpod);
        let w = c.which(&s, false, Clipboard::Present);

        let generation = w
            .model
            .value
            .split(',')
            .next()
            .expect("the Model row names a generation")
            .trim();
        assert!(!generation.is_empty() && !w.colour.value.is_empty());
        assert_eq!(
            w.ipod.value,
            format!("{} {generation}", w.colour.value),
            "the iPod row and the two rows under it are about different iPods: iPod {:?},              Model {:?}, Colour {:?}",
            w.ipod.value,
            w.model.value,
            w.colour.value
        );
        assert_ne!(w.ipod.value, filed, "the row is drawing the file's name, not the iPod's");

        // The root says the same thing as the level it opens onto — one iPod, one name, twice.
        assert_eq!(c.root(&s, false).which.value, w.ipod.value);

        // …and the picker's own row: the label is what the library files it under, the sub is what
        // it is. Two facts, or the row says one thing twice.
        let options = c.options_of(&s, Field::Ipod);
        let row = options.iter().find(|o| o.label == filed).expect("the filed dump is offered");
        assert_eq!(row.sub, w.ipod.value);
        assert_ne!(row.sub, row.label, "the picker row draws its own label back as its description");

        std::fs::remove_dir_all(&at).ok();
    }

    /// **Only iPods this program can be.** libgpod's table is 197 rows and this machine is a
    /// PP5021C with a 320x240 panel; offering a Nano is offering a machine that cannot start.
    #[test]
    fn level_one_offers_only_ipods_this_program_can_be() {
        let s = library("My 5.5G");
        let mut c = with_ipod();
        c.set_open(Some(Field::Model));
        let opts = c.options(&s);
        assert!(opts.len() > 4, "only {} models offered", opts.len());
        assert!(opts.len() < 40, "{} models offered — the whole table", opts.len());
        for o in &opts {
            assert!(o.enabled, "{} is offered and disabled", o.label);
            assert!(
                o.label.starts_with("5G") || o.label.starts_with("5.5G"),
                "a machine this program cannot be: {}",
                o.label
            );
        }
        assert_eq!(opts.iter().filter(|o| o.chosen).count(), 1, "no model is current");
        // Every one of them resolves, so choosing one cannot fail.
        for o in &opts {
            assert!(Model::lookup(&o.sub).is_some(), "{} does not resolve", o.sub);
        }
    }

    /// **One list, drawn and resolved.** Every picker's index means the same thing to the page that
    /// drew it and to the setter that acts on it, and a row the page drew disabled refuses to be
    /// picked rather than acting when somehow pressed.
    #[test]
    fn every_picker_resolves_the_option_it_drew() {
        let mut s = library("My 5.5G");
        s.file_disk("/drives/mine.img".into(), "mine");
        let pickers = [
            Field::Ipod,
            Field::Model,
            Field::Colour,
            Field::Disk,
            Field::From,
            Field::Bootloader,
        ];
        let mut refused = 0;
        for f in pickers {
            let base = with_ipod();
            let opts = base.options_of(&s, f);
            assert!(!opts.is_empty(), "{f:?} draws no options at all");
            for (i, o) in opts.iter().enumerate() {
                assert_eq!(o.id as usize, i, "{f:?}: option {i} carries id {}", o.id);
                let mut c = with_ipod();
                match c.choose(&s, f, i) {
                    Err(e) => {
                        assert!(!o.enabled, "{f:?} refused an enabled option {i}: {e}");
                        assert_eq!(e, o.reason, "{f:?} option {i} refused in other words");
                        refused += 1;
                    }
                    Ok(()) => {
                        assert!(o.enabled, "{f:?} acted on a disabled option {i}");
                        let after = c.options_of(&s, f);
                        assert!(
                            after[i].chosen,
                            "{f:?}: choosing {i} ({}) did not select it",
                            o.label
                        );
                    }
                }
            }
            // And an index the list does not have is refused rather than clamped: clamping picks
            // *something*, and the something is somebody else's iPod.
            let mut c = with_ipod();
            assert!(
                c.choose(&s, f, opts.len()).is_err(),
                "{f:?} accepted an index one past the end"
            );
            assert!(c.choose(&s, f, usize::MAX).is_err(), "{f:?} accepted usize::MAX");
        }
        assert!(
            refused > 0,
            "no picker drew a disabled row, so the refusal half proved nothing"
        );

        // The three that are not pickers say so rather than doing something surprising.
        for f in [Field::Serial, Field::Guid, Field::Name, Field::Systems] {
            let mut c = with_ipod();
            assert!(c.options_of(&s, f).is_empty(), "{f:?} drew options");
            assert!(c.choose(&s, f, 0).is_err(), "{f:?} was picked");
        }
    }

    /// **A colour is a property of the model number.** White, black and U2 are three rows of the
    /// table, so the picker offers the colours that generation and capacity actually shipped in.
    #[test]
    fn the_colour_picker_offers_only_colours_that_generation_shipped_in() {
        let s = library("My 5.5G");
        let mut c = with_ipod();
        c.set_open(Some(Field::Colour));
        let opts = c.options(&s);
        assert!(!opts.is_empty(), "no colours at all");
        let here = c.model().expect("a model");
        for o in &opts {
            let m = Model::lookup(&o.sub).expect("a colour is a model number");
            assert_eq!(m.generation, here.generation, "{} is another generation", o.label);
            assert_eq!(m.capacity_gb, here.capacity_gb, "{} is another capacity", o.label);
        }
        let names: Vec<&str> = opts.iter().map(|o| o.label.as_str()).collect();
        assert!(names.contains(&"Black"), "{names:?}");
        assert_eq!(opts.iter().filter(|o| o.chosen).count(), 1);
    }

    /// **The Colour picker is an identity-changing control wearing a cosmetic label**, and it has to
    /// say so before it is pressed: `Identity::generate` mixes the model number into the seed.
    #[test]
    fn changing_the_colour_says_it_changes_the_ipod() {
        let s = library("My 5.5G");
        let c = with_ipod();
        let row = c.which(&s, false, Clipboard::Present).colour;
        assert!(
            row.note.contains("changes the serial") && row.note.contains("FireWire GUID"),
            "the colour row does not say what pressing it does: {}",
            row.note
        );

        // And it really does. Black `A446` and white `A444` are the same iPod in two colours.
        let before = c.identity().expect("an identity");
        let mut d = with_ipod();
        d.set_model("A444").expect("a white 5.5G");
        let after = d.identity().expect("an identity");
        assert_eq!(d.model().unwrap().colour(), Colour::White);
        assert_ne!(before.guid, after.guid, "a different iPod produced the same GUID");
        assert_ne!(before.serial, after.serial, "a different iPod produced the same serial");
    }

    /// **A typed serial is revalidated against the new iPod, not thrown away.** The model decides
    /// which endings and which years are real, so correcting the model must not cost the typing.
    #[test]
    fn a_model_change_revalidates_a_typed_serial_without_discarding_it() {
        let mut c = with_ipod();
        c.set_reveal(Field::Serial);
        // An original-5G ending, typed onto a 5.5G: refused.
        let e = c.set_serial("4J6011K2TXK").unwrap_err();
        assert!(e.contains("TXK"), "{e}");
        assert!(!c.serial_reason().is_empty());

        // Correct the model rather than the serial, and the same string is now right.
        //
        // **Read through the field and not through `Identity`.** `nor::Source::identity()` ignores a
        // serial that has no GUID beside it and returns the seed's — which is the very thing
        // `a_typed_serial_without_a_guid_is_refused` exists for, and reading the assertion through
        // it would have this test pass while the typing had vanished from the screen.
        c.set_model("A146").expect("a 30 GB 5G");
        assert_eq!(
            c.serial().text(),
            "4J6011K2TXK",
            "the typed serial was discarded"
        );
        assert_eq!(c.serial_reason(), "", "still refused after the model changed");
        consistent(&c);
    }

    /// **The seed is the iPod**, so replacing one is a two-press control that names what is lost.
    #[test]
    fn making_a_second_ipod_takes_two_presses() {
        let mut c = Composer::new();
        assert_eq!(c.make_one_row().presses, 1, "the first one asked for a confirmation");
        assert_eq!(c.make_one_row().consequence, "");

        c.make_one();
        let first = c.identity().expect("an identity");
        let row = c.make_one_row();
        assert_eq!(row.presses, 2, "replacing an iPod took one press");
        assert!(
            row.consequence.contains("seed"),
            "the consequence does not name what is lost: {}",
            row.consequence
        );
        assert!(row.consequence.contains("FireWire GUID"), "{}", row.consequence);

        c.make_one();
        let second = c.identity().expect("an identity");
        assert_ne!(first.guid, second.guid, "the mint produced the same iPod twice");
    }

    /// **GUI.md §11.1, made mechanical**: an iPod states its model, and the model decides which
    /// bundles can follow. The other generation's are drawn and disabled, never absent.
    #[test]
    fn the_offered_firmware_follows_the_chosen_model() {
        let s = library("My 5.5G");
        let mut c = with_ipod(); // a 5.5G
        c.set_open(Some(Field::From));
        let opts = c.options(&s);
        assert!(opts.len() >= 4, "only {} bundles offered", opts.len());
        let (on, off): (Vec<_>, Vec<_>) = opts.iter().partition(|o| o.enabled);
        assert!(!on.is_empty(), "no bundle at all for a 5.5G");
        assert!(!off.is_empty(), "the other generation's bundles are absent, not disabled");
        for o in &off {
            assert!(!o.reason.is_empty(), "a disabled bundle with no reason");
            assert!(
                o.reason.contains("5G") && o.reason.contains("5.5G"),
                "the reason does not name both iPods: {}",
                o.reason
            );
            assert!(o.machine_rule, "a 5G's software is not a 5.5G's, ever");
            assert!(o.escape.is_empty(), "a machine rule carries a command");
        }

        // Choose the other iPod and the two sets swap.
        let mut d = with_ipod();
        d.set_model("A146").expect("a 5G");
        d.set_open(Some(Field::From));
        let theirs = d.options(&s);
        let on_now: Vec<&str> = theirs
            .iter()
            .filter(|o| o.enabled)
            .map(|o| o.label.as_str())
            .collect();
        for o in &on {
            assert!(
                !on_now.contains(&o.label.as_str()),
                "{} is offered to both generations",
                o.label
            );
        }
    }

    /// A bundle that no longer belongs to the chosen iPod is dropped rather than left standing as a
    /// plan that names somebody else's software.
    #[test]
    fn a_model_change_drops_a_firmware_that_is_not_this_ipods() {
        let mut c = with_ipod(); // 5.5G
        c.set_start(Start::FromIpsw("iPod_25.1.3.ipsw".into()));
        assert!(c.region().claims_a_plan(), "{}", c.region().text());

        c.set_model("A146").expect("a 5G");
        assert!(
            c.recipe().nothing_chosen(),
            "a 5.5G bundle survived onto a 5G: {:?}",
            c.recipe().start
        );
        assert!(!c.region().claims_a_plan());
        consistent(&c);
    }

    // ── level ② ───────────────────────────────────────────────────────────────────────────────

    /// **Every box ticks, and the two that are not offered say why and name a command.**
    #[test]
    fn every_system_and_every_bootloader_is_offered_and_the_unoffered_ones_say_why() {
        let s = library("My 5.5G");
        let mut c = with_ipod();
        c.set_start(Start::FromIpsw("iPod_25.1.3.ipsw".into()));

        let runs = c.runs(&s, false);
        assert_eq!(runs.systems.len(), Os::ALL.len(), "a system is absent, not disabled");
        for t in &runs.systems {
            assert!(!t.label.is_empty());
            assert_eq!(t.reason.is_empty(), t.enabled, "{:?}", t.os);
            assert_eq!(t.escape.is_empty(), t.enabled, "{:?} is disabled with no way round it", t.os);
        }

        c.set_open(Some(Field::Bootloader));
        let loaders = c.options(&s);
        assert_eq!(loaders.len(), Loader::ALL.len(), "a bootloader is absent, not disabled");
        for l in &loaders {
            if !l.enabled {
                assert!(!l.reason.is_empty(), "{} is disabled with no reason", l.label);
                // A project state names a command; a machine rule does not.
                assert_eq!(
                    l.escape.is_empty(),
                    l.machine_rule,
                    "{} mixes the two kinds of disabled",
                    l.label
                );
            }
        }
        assert!(
            loaders.iter().any(|l| !l.enabled),
            "the sweep saw no disabled bootloader, so it proved nothing"
        );
    }

    /// Ticking a system moves the bootloader rather than refusing it — and the tick lands even when
    /// the bootloader that was showing could not have carried it.
    #[test]
    fn ticking_a_system_moves_the_bootloader_rather_than_refusing_it() {
        let mut c = with_ipod();
        c.set_start(Start::FromIpsw("iPod_25.1.3.ipsw".into()));
        c.set_loader(Loader::Apple);

        c.set_os(Os::Rockbox, true);
        assert!(c.recipe().oses.contains(&Os::Rockbox), "the tick was refused");
        assert_eq!(c.recipe().loader, Loader::Rockbox);
        assert!(c.region().claims_a_plan(), "{}", c.region().text());

        // And un-ticking moves it back rather than leaving a bootloader with nothing to boot.
        c.set_os(Os::Rockbox, false);
        assert_eq!(c.recipe().loader, Loader::Apple);
        assert!(c.region().claims_a_plan(), "{}", c.region().text());
    }

    // ── locks ─────────────────────────────────────────────────────────────────────────────────

    /// **An iPod two devices share says so before it is edited.** The named resource wins, so
    /// editing one changes every device made of it — which is the point of composing rather than
    /// copying, and is a consequence rather than a wall.
    #[test]
    fn an_ipod_two_devices_share_says_so_before_it_is_edited() {
        let mut s = library("My 5.5G");
        s.remember_as("My other 5.5G");
        assert_eq!(s.devices.len(), 2, "the fixture made one device");
        assert_eq!(
            s.devices[0].firmware, s.devices[1].firmware,
            "the fixture made two iPods"
        );

        let recipe = Recipe::default();
        let c = Composer::editing(&s, "My 5.5G", recipe).expect("the device is there");
        let lock = c.lock(Field::Model, &s, false);
        assert_eq!(lock, Lock::Shared { devices: 2 });
        assert!(!lock.locked(), "a shared iPod refused the edit rather than warning about it");
        assert_eq!(lock.presses(), 2, "a shared edit took one press");
        assert!(
            lock.reason().contains("2 devices"),
            "the row does not say how many change with it: {}",
            lock.reason()
        );

        // A drive is not an identity, so it is not shared by this rule.
        assert_eq!(c.lock(Field::Disk, &s, false), Lock::Open);
    }

    /// **Every field, without exception.** A build is running against this recipe, so a change to
    /// any of it would be a change to something already being written.
    #[test]
    fn a_building_recipe_is_locked_in_every_field() {
        let s = library("My 5.5G");
        let c = with_ipod();
        for f in Field::ALL {
            let l = c.lock(f, &s, true);
            assert_eq!(l, Lock::Building, "{f:?} was left open while a build ran");
            assert!(l.locked());
            assert!(!l.reason().is_empty());
        }
        // Including the tick boxes, which are not a picker and would otherwise be the one control on
        // the page a build did not reach — and they are the ones that decide what it is installing.
        for t in c.runs(&s, true).systems {
            assert!(!t.enabled, "{:?} could be ticked while a build ran", t.os);
            assert!(!t.reason.is_empty(), "{:?} refused with no reason", t.os);
        }

        // And Create is not pressable, with that reason.
        let root = c.root(&s, true);
        assert!(!root.create.enabled);
        assert!(root.create.reason.contains("building"));
        assert!(!root.which.enabled && !root.runs.enabled && !root.named.enabled);
    }

    // ── level ③ and the clipboard ─────────────────────────────────────────────────────────────

    /// The name field states the filename it will produce, because that is the thing somebody has
    /// to find on disk afterwards.
    #[test]
    fn the_name_field_states_the_filename_it_will_produce() {
        let s = library("My 5.5G");
        let mut c = with_ipod();
        c.set_name("My 5.5G");
        assert_eq!(c.named(&s, false).stem, "my-5.5g.img");
        // A name that would produce a hidden file or an illegal one does not.
        c.set_name("../../etc/passwd");
        assert_eq!(c.named(&s, false).stem, "etc-passwd.img");
        c.set_name("   ");
        assert_eq!(c.named(&s, false).stem, "ipod.img");
    }

    /// **The clipboard copies a recipe, never a value.** The seed is the iPod, so the command
    /// reproduces the machine exactly while carrying no identifier — and `Show` does not unlock it,
    /// because a clipboard outlives the screen.
    ///
    /// **The line that asserted `--nor-seed` is deleted rather than corrected.** No such flag has
    /// ever existed in `ipod-boot`, and the subcommand the line named — `retail` — forwards a flag
    /// it does not recognise to `trace` unchanged, so the copied command ran, booted whichever NOR
    /// the setup screen held, and exited 0. **A test over a string can only ever agree with the
    /// string.** What replaces it is here in two halves, in the two places each half can be run:
    ///
    /// - the seed the line carries is read back **through the loader** and has to name the same
    ///   iPod, below;
    /// - and the argv itself is **executed** — `ipod-boot`'s own
    ///   `the_command_the_window_copies_rebuilds_the_ipod_it_names` hands it to `make_nor_cmd` and
    ///   reads the identity out of the ROM it wrote. A flag that does not exist is silently
    ///   defaulted there rather than refused, so that test fails on the iPod and not on the parse,
    ///   which is exactly how this failed in the operator's hands.
    ///
    /// The seed here is `0x123456`, whose hex spelling is made only of digits: a hex rendering
    /// would parse back without complaint, as a different number, naming a different iPod.
    #[test]
    fn the_copied_command_line_carries_no_identifier_and_a_seed_that_reads_back() {
        let s = library("My 5.5G");
        let mut c = with_ipod();
        c.rom = Some(nor::Source::Synthetic {
            model: nor::DEFAULT_MODEL.into(),
            seed: 1_193_046,
            serial: None,
            guid: None,
            splash: None,
        });
        c.recompute();
        let id = c.identity().expect("an identity");
        let serial = id.serial.clone().expect("a generated serial");

        for reveal in [false, true] {
            if reveal {
                c.set_reveal(Field::Serial);
                c.set_reveal(Field::Guid);
            }
            let cmd = c.command_line();
            assert!(!cmd.is_empty(), "there is no command for a generated iPod");
            assert!(!cmd.contains(&serial), "the command carries the serial: {cmd}");
            assert!(!cmd.contains(&id.guid_hex()), "the command carries the GUID: {cmd}");

            let seed = cmd
                .split_whitespace()
                .skip_while(|w| *w != "--seed")
                .nth(1)
                .unwrap_or_else(|| panic!("the command names no seed at all: {cmd}"));
            let back = Settings::parse(&format!(
                "nor_model = {}\nnor_seed = {seed}\n",
                nor::DEFAULT_MODEL
            ));
            assert_eq!(
                back.nor.identity().expect("the parsed recipe has an identity").guid,
                id.guid,
                "the command's `{seed}` rebuilds another iPod: {cmd}"
            );
            assert!(c.which(&s, false, Clipboard::Present).copy_command.enabled, "reveal={reveal}");
        }
    }

    /// A typed identity has no seed that reproduces it, so the control is disabled and says why
    /// rather than copying a command that would rebuild a different iPod.
    #[test]
    fn a_typed_identity_refuses_to_be_copied_and_says_why() {
        let s = library("My 5.5G");
        let mut c = with_ipod();
        c.set_reveal(Field::Guid);
        c.set_guid("000A270011223344").expect("a valid Apple GUID");

        let row = c.which(&s, false, Clipboard::Present).copy_command;
        assert!(!row.enabled, "a typed identity was offered as a command line");
        assert!(
            row.reason.contains("no seed reproduces them"),
            "the refusal does not say why: {}",
            row.reason
        );
        assert_eq!(c.command_line(), "", "a command was produced anyway");
    }

    /// **A build with no clipboard does not draw a live control over one.**
    ///
    /// `main::caps()` reports `clipboard: false` — this program declares no clipboard dependency
    /// and calls no clipboard API — and `main::wire`'s `on_copy_text` declines with that sentence.
    /// This row shipped `enabled: true` regardless, so `Copy the command line` was drawn live, in
    /// accent, over a route that could only refuse. §14.1's phantom control, and the same defect
    /// family as the ordinal the markup was sending one line below the label.
    ///
    /// **That is not what this line used to say.** It said *nothing in this dependency graph
    /// reaches a pasteboard*, and copypasta is in the graph — under `i-slint-backend-winit`, which
    /// is where Slint's own text fields get their copy and paste. `main::caps`'s doc carries the
    /// measurement and the route that does exist; the two claims are different and only one of
    /// them is true.
    ///
    /// The capability is checked **before** the identity's own gate, because it is the one that is
    /// true of every iPod: saying *no seed reproduces them* implies a different iPod could be
    /// copied, and on this build none can.
    #[test]
    fn a_build_with_no_clipboard_does_not_offer_the_copy() {
        let s = library("My 5.5G");
        let c = with_ipod();

        // The control: with a pasteboard this same iPod *is* offered, so the refusal below is the
        // capability talking rather than something about the identity.
        assert!(
            c.which(&s, false, Clipboard::Present).copy_command.enabled,
            "a generated iPod is not offered as a command line even where there is a clipboard"
        );

        let row = c.which(&s, false, Clipboard::Absent).copy_command;
        assert!(!row.enabled, "the copy control is drawn live on a build with no clipboard");
        assert!(!row.reason.is_empty(), "a disabled control with nothing to tell you");
        assert!(!row.machine_rule, "a missing clipboard is a project state, not a machine rule");

        // One fact, worded once: `rail.rs` already says it for `Next::CopyDetails`, and this reads
        // that sentence out of it rather than restating it — two wordings of one absence is how a
        // person comes to believe they have two problems.
        //
        // Anchored on `Next::reason` by name, because `Next::CopyDetails` appears in seven `match`
        // arms in that file and the first one this reached was the control's **label**. The wrong
        // arm is why the sentence below is quoted in the failure rather than merely compared.
        let said = include_str!("rail.rs")
            .split_once("pub fn reason(&self) -> &'static str {")
            .expect("rail.rs declares `Next::reason`")
            .1
            .split_once("Next::CopyDetails => \"")
            .expect("`Next::reason` words a reason for `Next::CopyDetails`")
            .1
            .split_once('"')
            .expect("its closing quote")
            .0;
        assert!(
            row.reason.starts_with(said),
            "the Composer words the missing clipboard as `{}`; rail.rs words it as `{said}`",
            row.reason
        );

        // And the identity's own refusal still stands where a clipboard exists, so the new gate has
        // not swallowed the old one.
        let mut typed = with_ipod();
        typed.set_reveal(Field::Guid);
        typed.set_guid("000A270011223344").expect("a valid Apple GUID");
        let row = typed.which(&s, false, Clipboard::Present).copy_command;
        assert!(!row.enabled, "a typed identity was offered as a command line");
        assert!(row.reason.contains("no seed reproduces them"), "{}", row.reason);
    }

    /// **There is no `raw()` and there must never be one.** While masked, the value that crosses
    /// into the markup is `None`, so a screenshot, a selection and the accessible tree all carry the
    /// mask rather than the identifier.
    #[test]
    fn nothing_outside_the_composer_can_reach_an_unmasked_identifier() {
        let s = library("My 5.5G");
        let mut c = with_ipod();
        let id = c.identity().expect("an identity");
        let serial = id.serial.clone().expect("a serial");

        let w = c.which(&s, false, Clipboard::Present);
        assert!(w.serial.masked && w.guid.masked);
        assert_eq!(w.serial.raw, "", "the markup holds the serial while it is masked");
        assert_eq!(w.guid.raw, "", "the markup holds the GUID while it is masked");
        assert_ne!(w.serial.value, serial, "the mask is not masking");
        assert_ne!(w.guid.value, id.guid_hex());
        assert_eq!(w.serial.value.len(), serial.len(), "the mask changed the width");

        // `Show` reveals and enables in one act, so the drawn text and the editable text are never
        // two different things.
        c.set_reveal(Field::Serial);
        let w = c.which(&s, false, Clipboard::Present);
        assert_eq!(w.serial.value, serial);
        assert_eq!(w.serial.raw, serial);
        assert!(!w.serial.masked);
        // …and the other field is untouched: a reveal is per-field.
        assert!(w.guid.masked && w.guid.raw.is_empty());

        // Leaving level ① re-masks everything.
        c.set_level(Level::WhichIpod);
        c.set_reveal(Field::Serial);
        c.set_level(Level::Root);
        assert!(c.which(&s, false, Clipboard::Present).serial.masked, "a reveal outlived the page");

        // And no accessor hands the full value out while it is masked.
        let secret = Secret::serial(&serial, false);
        assert_eq!(secret.editable(), None);
        assert_ne!(secret.text(), serial);
        let body = shipped();
        assert!(!body.contains("fn raw("), "a raw() reader exists");
    }

    /// **GUI.md §11.2's third behaviour, and the UI must not flatten it.** `nor::Source` would
    /// silently ignore a serial with no GUID and use the seed's identity instead — a typed value
    /// quietly not taking.
    #[test]
    fn a_typed_serial_without_a_guid_is_refused() {
        let mut c = with_ipod();
        c.set_reveal(Field::Serial);
        c.set_serial("4J6011K2V9K").expect("a valid 5.5G serial");
        let e = c.can_commit().unwrap_err();
        assert!(
            e.contains("the GUID is the field with teeth"),
            "a typed serial with no GUID was accepted: {e:?}"
        );

        // With a GUID it goes through.
        c.set_reveal(Field::Guid);
        c.set_guid("000A270011223344").expect("a valid Apple GUID");
        assert!(c.can_commit().is_err(), "the fixture has no firmware yet");
        c.set_start(Start::FromIpsw("iPod_25.1.3.ipsw".into()));
        c.can_commit().expect("a typed identity with both fields is fine");
    }

    // ── Create and Save ───────────────────────────────────────────────────────────────────────

    fn ready() -> Composer {
        let mut c = with_ipod();
        c.set_start(Start::FromIpsw("iPod_25.1.3.ipsw".into()));
        c.set_name("My 5.5G");
        c
    }

    /// **The mint files nothing; `Create` is the filing — and GUI.md §11.2 says that now.**
    ///
    /// §11.2 asked for the opposite for as long as this file has existed — *the iPod becomes a
    /// filed resource the moment it is made, not on `Create`* — and nothing ever built it. Two doc
    /// comments in `eapp_loader::settings` stated the mint-filing as fact, and a comment in
    /// `main.rs`'s `on_composer_act` recorded the gap without closing it. This is the assertion
    /// that keeps the design and the program together from the program's side: whichever way the
    /// decision goes next, it cannot go silently.
    ///
    /// Proved red both ways. Filing the ROM at the mint — one `file_away` between the two halves,
    /// which is exactly what implementing §11.2's old sentence would put there — fails the first:
    /// *the mint filed something: ["A446, seed 6754884334582999373"]*. Deleting `commit`'s
    /// `file_away` fallback fails the second, and **on `filed_as` rather than on the resources
    /// list** — `left: ""`, `right: "A446, seed …"` — which is worth writing down: `remember_as`
    /// files the live ROM by value a few lines later and under the same spelling, so the library
    /// comes out looking right either way. What cannot be faked is the page knowing what its own
    /// iPod is filed under, which is what `Edit` and `restate_firmware` both read.
    #[test]
    fn the_mint_files_nothing_and_create_is_the_filing() {
        let mut s = Settings::default();
        let mut c = Composer::new();
        c.make_one();

        assert!(
            s.resources.is_empty(),
            "the mint filed something: {:?}",
            s.resources.iter().map(|i| i.name.as_str()).collect::<Vec<_>>()
        );
        assert!(s.devices.is_empty(), "the mint made a device");
        assert!(
            c.filed_as().is_empty(),
            "the page thinks its iPod is filed as {:?}",
            c.filed_as()
        );

        // …and `Create` is what files it, once, under §11.2's own spelling.
        c.set_start(Start::FromIpsw("iPod_25.1.3.ipsw".into()));
        c.set_name("My 5.5G");
        let done = c.commit(&mut s).expect("an empty library");
        let filed = settings::suggest_nor_name(c.rom().expect("an iPod"));
        assert_eq!(
            s.resources
                .iter()
                .filter(|i| matches!(i.what, Resource::Firmware(_)))
                .map(|i| i.name.as_str())
                .collect::<Vec<_>>(),
            vec![filed.as_str()],
            "`Create` filed no iPod, or filed more than one"
        );
        assert_eq!(c.filed_as(), filed);
        assert_eq!(
            s.devices.iter().find(|d| d.name == done.device).map(|d| d.firmware.as_str()),
            Some(filed.as_str()),
        );
    }

    /// **`remember_as` replaces a device of the same name outright**, so the refusal has to happen
    /// before it — otherwise `Create` silently destroys a device somebody built by hand.
    #[test]
    fn create_never_overwrites_a_device_somebody_made_by_hand() {
        let mut s = library("My 5.5G");
        s.devices[0].boot_instructions = Some(1_600_000_000);
        let mut c = ready();

        let e = c.commit(&mut s).unwrap_err();
        assert!(e.contains("already a device called My 5.5G"), "{e}");
        assert_eq!(s.devices.len(), 1);
        assert_eq!(
            s.devices[0].boot_instructions,
            Some(1_600_000_000),
            "the hand-made device was written over"
        );

        // Another name is accepted.
        c.set_name("My other 5.5G");
        let done = c.commit(&mut s).expect("a free name");
        assert_eq!(done.device, "My other 5.5G");
        assert_eq!(s.devices.len(), 2);
    }

    /// A device names the iPod it was composed from, resolved through the resources.
    #[test]
    fn create_writes_a_device_that_names_the_ipod_it_was_composed_from() {
        let mut s = Settings::default();
        let mut c = ready();
        let done = c.commit(&mut s).expect("an empty library");

        let d = s.devices.iter().find(|d| d.name == done.device).expect("the device");
        assert!(!d.firmware.is_empty(), "the device names no iPod");
        let filed = s.nor_of(d).expect("the iPod resolves");
        assert_eq!(Some(filed), c.rom(), "the device names some other iPod");
        assert_eq!(c.filed_as(), d.firmware);
        // **And under §11.2's own spelling: `<model>, seed <n>`, not `Black 5.5G`.**
        //
        // `commit` files **before** `remember_as` now, which is what makes the entry's name a
        // question at all: while it filed afterwards, the value was always already in the list and
        // the name it suggested was swallowed whole. `remember_as`, `seed` and the inline-recipe
        // migration all spell a boot ROM through `settings::suggest_nor_name`; a fourth spelling
        // here would file one iPod under two names depending on which route reached it first, and
        // the name is what `ipod_options` draws as the picker's row label.
        assert_eq!(
            d.firmware,
            settings::suggest_nor_name(c.rom().expect("an iPod")),
            "`Create` filed the iPod under a name no other route spells"
        );

        // And the chassis follows the model, so a white iPod does not draw black.
        let mut white = ready();
        white.set_model("A444").expect("a white 5.5G");
        white.set_start(Start::FromIpsw("iPod_25.1.3.ipsw".into()));
        white.set_name("A white one");
        white.commit(&mut s).expect("a free name");
        assert_eq!(s.chassis, Some(Colour::White), "a white iPod was filed as black");
    }

    /// **Changing the iPod re-points the device at it.** `Settings::as_device` deliberately keeps
    /// whatever the stored device already named — which is what stops switching devices cutting one
    /// loose from its parts — so a save that did not state the new reference would leave the device
    /// made of the iPod somebody had just replaced.
    #[test]
    fn a_saved_device_is_made_of_the_ipod_that_is_on_screen() {
        let mut s = library("My 5.5G");
        let was = s.devices[0].firmware.clone();
        let recipe = Recipe {
            start: Start::FromIpsw("iPod_25.1.3.ipsw".into()),
            loader: Loader::Apple,
            oses: [Os::Apple].into_iter().collect(),
        };
        let mut c = Composer::editing(&s, "My 5.5G", recipe).expect("the device");
        assert_eq!(c.filed_as(), was);

        // A different iPod entirely.
        c.make_one();
        c.commit(&mut s).expect("save");
        assert_ne!(s.devices[0].firmware, was, "the device kept the iPod it was replaced from");
        assert_eq!(s.devices[0].firmware, c.filed_as());
        assert_eq!(s.nor_of(&s.devices[0]), c.rom(), "the reference does not resolve to it");
        // And exactly one entry per iPod: filing is by value and idempotent.
        assert_eq!(
            s.resources.iter().filter(|i| i.name == s.devices[0].firmware).count(),
            1,
            "the save made a second entry for one iPod"
        );
    }

    /// **GUI.md §12.3.** A denominator learned on RetailOS reads 6 % at the moment a Rockbox device
    /// finishes, so a shape that moved drops the number rather than keeping one it cannot vouch for.
    #[test]
    fn create_clears_the_boot_denominator_when_the_boot_shape_changed() {
        let mut s = library("My 5.5G");
        s.devices[0].boot_instructions = Some(1_600_000_000);
        let recipe = Recipe {
            start: Start::FromIpsw("iPod_25.1.3.ipsw".into()),
            loader: Loader::Apple,
            oses: [Os::Apple].into_iter().collect(),
        };
        let mut c = Composer::editing(&s, "My 5.5G", recipe).expect("the device");

        // Install Rockbox over Apple's: a different shape.
        c.set_os(Os::Rockbox, true);
        assert!(c.shape_changed(), "the fixture did not change the shape");
        let done = c.commit(&mut s).expect("save");
        assert!(done.shape_changed);
        assert_eq!(
            s.devices[0].boot_instructions, None,
            "the bar kept a denominator from another system"
        );
    }

    /// And a re-save that changed nothing keeps it — a number thrown away for no reason costs a
    /// whole boot without a bar.
    ///
    /// **The fixture records a shape first, and that is the state this property is about.** A
    /// device whose shape is recorded is what every device becomes the first time it is saved
    /// through here; before that recording exists there is nothing the number can be vouched for
    /// by, and `set_boot_shape` drops it once —
    /// `a_device_with_no_recorded_shape_drops_its_denominator_once` is where that half is measured.
    /// Written the other way round, this test asserted *keeps the number* over a device that had
    /// none to compare against and was green only while nothing recorded shapes at all.
    #[test]
    fn saving_an_unchanged_recipe_runs_nothing() {
        let mut s = library("My 5.5G");
        assert!(s.set_boot_shape(
            "My 5.5G",
            &compose::BootShape {
                loader: Loader::Apple,
                oses: [Os::Apple].into_iter().collect(),
            }
        ));
        s.devices[0].boot_instructions = Some(1_600_000_000);
        let recipe = Recipe {
            start: Start::FromIpsw("iPod_25.1.3.ipsw".into()),
            loader: Loader::Apple,
            oses: [Os::Apple].into_iter().collect(),
        };
        let mut c = Composer::editing(&s, "My 5.5G", recipe).expect("the device");
        assert!(!c.shape_changed());

        let done = c.commit(&mut s).expect("save");
        assert!(!done.shape_changed);
        assert_eq!(s.devices.len(), 1, "a save made a second device");
        assert_eq!(
            s.devices[0].boot_instructions,
            Some(1_600_000_000),
            "an unchanged save threw the denominator away"
        );
        assert_eq!(c.footer_label(), "Save");
    }

    /// **`Device::boot_shape` is written, survives the file, and `Settings::recipe_of` takes its
    /// authority branch** — the three halves of GUI.md §12.3 / §20 item 6 that had no production
    /// writer between them.
    ///
    /// `Settings::set_boot_shape` had callers only in test modules, so `commit` did half its job:
    /// it cleared the denominator on a shape change and recorded nothing about the shape it moved
    /// to. `recipe_of`'s first arm — *the recorded shape is the authority on what a device boots* —
    /// was therefore unreachable in production, and **every** Edit re-derived the recipe from the
    /// drive's install list instead.
    ///
    /// **The fixture is built so the two arms cannot agree by accident, which is the whole of what
    /// makes the third assertion mean anything.** The drive records Apple's software and nothing
    /// else, and the device is composed dual-boot on Rockbox's bootloader — so the fallback answers
    /// `apple, apple` and the authority answers `rockbox, apple, rockbox`. The fallback is asserted
    /// outright, off the same device with the shape stripped, rather than left to be assumed.
    #[test]
    fn a_composed_device_records_what_it_boots_and_reopens_on_it() {
        let mut s = Settings::default();
        // A drive in the library whose install list holds Apple's software alone. It is what
        // `recipe_of`'s fallback reads, and it is deliberately not what this device boots.
        let drive = s.file_disk("/drives/mine.img".into(), "mine");
        s.disks[0].installed = vec![Os::Apple.label().to_string()];

        let mut c = with_ipod();
        c.set_start(Start::FromDisk {
            name: drive,
            fat_type: None,
        });
        c.set_os(Os::Apple, true);
        c.set_os(Os::Rockbox, true);
        c.set_name("Both of them");
        let done = c.commit(&mut s).expect("an empty library");
        assert_eq!(
            done.shape.render(),
            "rockbox, apple, rockbox",
            "the fixture does not boot what this test is about"
        );

        // 1. It is recorded on the device. Without this nothing can vouch for the next
        //    denominator, and `recipe_of` has no authority to take.
        let d = s
            .devices
            .iter()
            .find(|d| d.name == done.device)
            .expect("the Composer filed no device");
        assert_eq!(
            d.boot_shape.as_deref(),
            Some("rockbox, apple, rockbox"),
            "the save recorded no shape"
        );

        // 2. It survives the round trip through the settings file.
        let back = Settings::parse(&s.render());
        let d = back
            .devices
            .iter()
            .find(|d| d.name == done.device)
            .expect("the device did not come back");
        assert_eq!(d.boot_shape.as_deref(), Some("rockbox, apple, rockbox"));

        // 3. And `recipe_of` answers with it rather than with the drive-derived guess.
        let reopened = back.recipe_of(d);
        assert_eq!(reopened.shape(), done.shape, "the Edit route re-derived the recipe");
        assert_eq!(reopened.loader, Loader::Rockbox);
        assert_eq!(reopened.oses, [Os::Apple, Os::Rockbox].into_iter().collect());
        // The branch it overtook, spelled out on the same device: an assertion that passed because
        // the two arms happened to agree would prove nothing about which one ran.
        let mut shapeless = d.clone();
        shapeless.boot_shape = None;
        assert_eq!(
            back.recipe_of(&shapeless).shape().render(),
            "apple, apple",
            "the fallback answers what the authority does, so nothing above distinguishes them"
        );

        // And the surface that consumes it opens on that recipe, as `Devices::editor` does.
        let again = Composer::editing(&back, &done.device, reopened.clone()).expect("the device");
        assert_eq!(again.recipe(), &reopened);
        assert_eq!(
            again.mode(),
            &Mode::Editing {
                device: done.device.clone()
            }
        );
    }

    /// **A device that recorded no shape loses its denominator once, and only once.**
    ///
    /// The number is a measurement of one cold boot and a prediction of the next only while the
    /// device goes on booting the same thing. A device written before `Device::boot_shape` had a
    /// writer — which, until this, was every device there has ever been — carries a number measured
    /// on something nobody recorded, so there is nothing for it to be vouched for by.
    /// `Settings::set_boot_shape` drops it, records the shape, and the save after that keeps it.
    ///
    /// **It is the half `Composer::shape_changed` cannot answer**, and asserting both here is what
    /// stops the drop reading as a regression: the Composer's question is *did the shape move while
    /// the page was open*, and it says `false` over exactly this device while the number goes.
    #[test]
    fn a_device_with_no_recorded_shape_drops_its_denominator_once() {
        let mut s = library("My 5.5G");
        s.devices[0].boot_instructions = Some(1_600_000_000);
        assert_eq!(
            s.devices[0].boot_shape, None,
            "the fixture is not the state this is about"
        );
        let recipe = Recipe {
            start: Start::FromIpsw("iPod_25.1.3.ipsw".into()),
            loader: Loader::Apple,
            oses: [Os::Apple].into_iter().collect(),
        };
        let mut c = Composer::editing(&s, "My 5.5G", recipe).expect("the device");
        assert!(!c.shape_changed(), "nothing moved while the page was open");

        let done = c.commit(&mut s).expect("save");
        assert!(!done.shape_changed);
        assert_eq!(s.devices[0].boot_shape.as_deref(), Some("apple, apple"));
        assert_eq!(
            s.devices[0].boot_instructions, None,
            "a number measured on a system nobody recorded was kept and vouched for"
        );

        // Once. The next completed boot's measurement survives the save after it.
        s.devices[0].boot_instructions = Some(1_600_000_000);
        c.commit(&mut s).expect("save");
        assert_eq!(
            s.devices[0].boot_instructions,
            Some(1_600_000_000),
            "the drop repeated, so no device can ever keep a denominator"
        );
    }

    /// **`Lock::Shared` draws *N devices are made of this iPod and will change with it*, and this
    /// is the assertion that the sentence is true.**
    ///
    /// It was not. `commit` filed the edited ROM with `Settings::file_away`, which files by
    /// **value** — an edited identity is a value nobody has filed, so a second entry appeared and
    /// only the device on screen was repointed at it. The other N-1 went on naming the old entry,
    /// holding the old iPod, having been told two rows above the control that they would change.
    ///
    /// The operator settled restate rather than fork, and `Settings::restate_firmware`'s own doc is
    /// the argument: *editing one changes every device made of it, which is the point of composing
    /// rather than copying, so refusing a shared edit would be the window contradicting the model.*
    ///
    /// **The entry count is asserted as well as the two references**, because repointing both
    /// devices at a *copy* would satisfy every `nor_of` below while leaving the library holding two
    /// entries for one iPod — which is the defect `restate_firmware` was written against.
    #[test]
    fn editing_a_shared_ipod_changes_every_device_made_of_it() {
        let mut s = library("My 5.5G");
        // A second device made of the same iPod. `as_device` resolves the reference by value, so
        // this is one entry with two devices naming it rather than two entries.
        s.remember_as("Spare");
        let entry = s.devices[0].firmware.clone();
        assert_eq!(s.devices[1].firmware, entry, "the fixture is not sharing anything");
        let before = s.nor_of(&s.devices[1]).cloned().expect("the iPod resolves");

        let recipe = Recipe {
            start: Start::FromIpsw("iPod_25.1.3.ipsw".into()),
            loader: Loader::Apple,
            oses: [Os::Apple].into_iter().collect(),
        };
        let mut c = Composer::editing(&s, "My 5.5G", recipe).expect("the device");
        // The window says so before the press, and this is the state that sentence describes.
        assert_eq!(c.lock(Field::Colour, &s, false), Lock::Shared { devices: 2 });
        assert_eq!(c.lock(Field::Colour, &s, false).presses(), 2);

        c.set_model("A444").expect("the white 5.5G");
        c.commit(&mut s).expect("save");

        let now = c.rom().cloned().expect("an iPod");
        assert_ne!(now, before, "the fixture edited nothing, so what follows proves nothing");
        // The sentence's own claim first, because it is the one that was false: *every* device
        // made of this iPod resolves to the iPod that was edited.
        for d in &s.devices {
            assert_eq!(
                s.nor_of(d),
                Some(&now),
                "{} was left on the iPod nobody edited",
                d.name
            );
        }
        // And one iPod, not two — repointing both devices at a **copy** would satisfy the loop
        // above while leaving the library holding two entries for one thing.
        assert_eq!(
            s.resources
                .iter()
                .filter(|i| matches!(i.what, Resource::Firmware(_)))
                .count(),
            1,
            "the edit forked the iPod: the shared entry is still there beside a copy"
        );
        assert_eq!(
            s.devices[0].firmware, s.devices[1].firmware,
            "one iPod, filed under two names"
        );
        // And the entry is re-derived through the spelling the mint used, so an iPod that is now a
        // white 5.5G is not still filed — and drawn in the picker — as a black one.
        assert_eq!(s.devices[0].firmware, settings::suggest_nor_name(&now));
    }

    /// A rename moves the device rather than leaving two, and it keeps what the old one knew.
    ///
    /// **`Settings::rename_device` is what moves it**, and *keeps what the old one knew* is the
    /// whole of why it is that function and not `forget` plus `remember_as`. The shape is recorded
    /// for the same reason as in the test above: the denominator this asserts survives a rename is
    /// one the device can vouch for.
    #[test]
    fn save_renames_without_leaving_two_devices() {
        let mut s = library("My 5.5G");
        assert!(s.set_boot_shape(
            "My 5.5G",
            &compose::BootShape {
                loader: Loader::Apple,
                oses: [Os::Apple].into_iter().collect(),
            }
        ));
        s.devices[0].boot_instructions = Some(1_600_000_000);
        s.devices[0].parked_at = Some(1_700_000_000);
        let recipe = Recipe {
            start: Start::FromIpsw("iPod_25.1.3.ipsw".into()),
            loader: Loader::Apple,
            oses: [Os::Apple].into_iter().collect(),
        };
        let mut c = Composer::editing(&s, "My 5.5G", recipe).expect("the device");
        c.set_name("The good one");

        let done = c.commit(&mut s).expect("a free name");
        assert_eq!(done.renamed, Some(("My 5.5G".into(), "The good one".into())));
        assert_eq!(s.devices.len(), 1, "a rename left two devices");
        assert_eq!(s.devices[0].name, "The good one");
        assert_eq!(s.devices[0].boot_instructions, Some(1_600_000_000), "the denominator went");
        assert_eq!(s.devices[0].parked_at, Some(1_700_000_000), "the park time went");
        assert_eq!(s.current.as_deref(), Some("The good one"));

        // And a rename onto a name somebody else holds is refused.
        s.remember_as("Another");
        let mut d = Composer::editing(&s, "The good one", Recipe::default()).expect("the device");
        d.set_name("Another");
        assert!(d.commit(&mut s).is_err(), "a rename collided and went through");
    }

    /// **A device that left underneath the page becomes a new device and says so, once.** Pressing
    /// `Save` against a device that is not there would file one somebody deleted.
    #[test]
    fn a_device_removed_under_the_composer_becomes_a_new_device_and_says_so() {
        let mut s = library("My 5.5G");
        let mut c = Composer::editing(&s, "My 5.5G", Recipe::default()).expect("the device");
        assert_eq!(c.footer_label(), "Save");
        assert_eq!(c.device_vanished(&s), None, "it is still there");

        s.forget("My 5.5G");
        let note = c.device_vanished(&s).expect("it went");
        assert!(note.contains("My 5.5G"), "{note}");
        assert!(note.contains("new device"), "{note}");
        assert_eq!(c.footer_label(), "Create");
        assert_eq!(c.mode(), &Mode::New);
        // Said once.
        assert_eq!(c.device_vanished(&s), None, "the note came back a second time");
    }

    // ── the sweep ─────────────────────────────────────────────────────────────────────────────

    /// Every state of this page a rule could be written into: nine starts, eight sets of systems,
    /// three bootloaders.
    ///
    /// The starts are the three variants times every `fat_type` a read can produce —
    /// `install::data_partition_type` ends `.find(|t| matches!(t, 0x0b | 0x0c)).unwrap_or(0)`, so
    /// an `Ok(_)` is exactly one of `0x00`/`0x0B`/`0x0C` and `None` is *nobody has looked yet*.
    /// **The literals live here in the tests, deliberately**: they are inputs to drive the page
    /// with, which is the opposite of the thing the sweep below refuses.
    fn every_recipe_state() -> Vec<Recipe> {
        let mut starts = vec![Start::FromIpsw("iPod_25.1.3.ipsw".into())];
        for t in [None, Some(0x00), Some(0x0b), Some(0x0c)] {
            starts.push(Start::FromImage {
                path: "/drives/mine.img".into(),
                fat_type: t,
            });
            starts.push(Start::FromDisk {
                name: "The good one".into(),
                fat_type: t,
            });
        }
        let mut out = Vec::new();
        for start in &starts {
            for bits in 0..8u8 {
                let oses: BTreeSet<Os> = Os::ALL
                    .into_iter()
                    .enumerate()
                    .filter(|(i, _)| bits & (1 << i) != 0)
                    .map(|(_, o)| o)
                    .collect();
                for loader in Loader::ALL {
                    out.push(Recipe {
                        start: start.clone(),
                        loader,
                        oses: oses.clone(),
                    });
                }
            }
        }
        out
    }

    /// A page standing in one of those states, with an iPod and a name so that nothing except the
    /// recipe can be what refuses.
    ///
    /// The bootloader is picked **last**, because `set_os` moves it — this is the state the page is
    /// in after somebody chose systems and then chose a bootloader, which is the order §11.3 lays
    /// the rows out in.
    fn page_at(r: &Recipe) -> Composer {
        let mut c = with_ipod();
        c.set_start(r.start.clone());
        for o in Os::ALL {
            c.set_os(o, r.oses.contains(&o));
        }
        c.set_loader(r.loader);
        c.set_name("A name");
        c
    }

    /// **The window computes no compatibility rule of its own.** Every one of them lives in
    /// `compose.rs`, where it is measured and cited; a second copy here is a rule that drifts.
    ///
    /// **This was four greps over this file's own text, and it was blind to the thing it names.**
    /// It refused the literals `0x0b`/`0x0c` and any line pairing `Os::IPodLinux` with `Loader::`.
    /// Measured both ways, 2026-08-22. Planting a real rule in `took_reading` —
    /// `if t == 11u8 { self.recipe.loader = compose::Loader::Apple; }`, a volume type quietly
    /// resetting somebody's bootloader — left **all 342 tests in this crate green**. Adding the
    /// bytes `0x0b` to a *doc comment* saying volume types are not this page's business made it
    /// **panic**. Green through the defect, red on prose; `compose.rs` held one of the same shape
    /// and it was rewritten to call the function, and this is that, here.
    ///
    /// It asks the question by driving the page instead, over [`every_recipe_state`] — 216
    /// recipes — in two halves that catch different mistakes:
    ///
    /// 1. **Every answer the page gives about a recipe is the model's answer for that recipe.** The
    ///    verdict, whether `Create` is live, and each bootloader row's live-or-greyed and its
    ///    sentence. A rule re-derived here shows up the moment it disagrees, which is the moment it
    ///    matters.
    /// 2. **Every recipe the page produces is one the model produced.** For each writer, the recipe
    ///    afterwards is compared against the same edit made to `compose`'s own [`Recipe`] and
    ///    nothing else — so a page that moves a bootloader, a system or a volume type on its own
    ///    behalf fails here **whether or not it happens to agree with a rule**. This is the half the
    ///    `11u8` plant goes red on: `set_volume_type` moves no bootloader, so neither may this.
    ///
    /// [`consistent`] is the third member of the family and is **not** the same check: it holds
    /// that the region, the plan and the bill are derived from *this* recipe, which stays true of a
    /// page that edited the recipe on its own behalf and then recomputed from it.
    #[test]
    fn the_window_computes_no_compatibility_rule_of_its_own() {
        let s = library("Somebody else's");
        for r in every_recipe_state() {
            let c = page_at(&r);
            let recipe = c.recipe().clone();

            // ---- 1. every answer about a recipe is the model's answer for it.
            let verdict = recipe.check();
            assert_eq!(
                c.region().text(),
                verdict.text(),
                "the page worded the verdict for {recipe:?} itself"
            );
            assert_eq!(
                c.region().claims_a_plan(),
                verdict.ok(),
                "the page and the model disagree about whether {recipe:?} works"
            );
            assert_eq!(
                c.can_commit().is_ok(),
                verdict.ok(),
                "`Create` is {} for {recipe:?}; the model calls it {}: {:?}",
                if c.can_commit().is_ok() { "live" } else { "refused" },
                if verdict.ok() { "fine" } else { "impossible" },
                c.can_commit()
            );
            let rows = c.options_of(&s, Field::Bootloader);
            assert_eq!(rows.len(), Loader::ALL.len(), "a bootloader is absent, not disabled");
            for (row, l) in rows.iter().zip(Loader::ALL) {
                assert_eq!(
                    row.enabled,
                    l.offered() && recipe.loader_works(l),
                    "`{}` is {} for {recipe:?} and the model says otherwise",
                    l.label(),
                    if row.enabled { "live" } else { "greyed" }
                );
                // A machine rule is greyed *by the recipe*, and its sentence is the model's own.
                if row.machine_rule {
                    assert_eq!(row.reason, recipe.why_not(l), "the page worded a machine rule");
                }
            }

            // ---- 2. every recipe the page produces is one the model produced. Each writer runs
            // on its own page, from the same state, against the same edit made to a `Recipe`.
            let mut expect = recipe.clone();
            for t in [0x00u8, 0x0b, 0x0c] {
                let mut c = page_at(&r);
                c.took_reading(Ok(t));
                let mut model = recipe.clone();
                model.set_volume_type(t);
                assert_eq!(
                    *c.recipe(),
                    model,
                    "reading a volume as {t:#04x} did more to {recipe:?} than record it"
                );
            }
            // A read that failed records nothing at all.
            let mut c = page_at(&r);
            c.took_reading(Err("no such file".into()));
            assert_eq!(*c.recipe(), recipe, "a volume nobody could read changed the recipe");

            for o in Os::ALL {
                for on in [true, false] {
                    let mut c = page_at(&r);
                    c.set_os(o, on);
                    let mut model = recipe.clone();
                    if on {
                        model.oses.insert(o);
                    } else {
                        model.oses.remove(&o);
                    }
                    // §11.3: the bootloader follows the systems, and `best_loader` **is** the rule.
                    model.loader = model.best_loader();
                    assert_eq!(
                        *c.recipe(),
                        model,
                        "turning {} {} on {recipe:?} left a recipe the model would not have made",
                        o.label(),
                        if on { "on" } else { "off" }
                    );
                }
            }
            for l in Loader::ALL {
                let mut c = page_at(&r);
                c.set_loader(l);
                expect.loader = l;
                assert_eq!(*c.recipe(), expect, "picking `{}` moved something else too", l.label());
            }
            expect.loader = recipe.loader;
            for st in [
                Start::FromIpsw("iPod_25.1.3.ipsw".into()),
                Start::FromImage {
                    path: "/drives/other.img".into(),
                    fat_type: Some(0x0c),
                },
            ] {
                let mut c = page_at(&r);
                c.set_start(st.clone());
                expect.start = st;
                assert_eq!(*c.recipe(), expect, "choosing a drive moved something else too");
            }
        }
    }
}

