// Parts — docs/GUI.md §11.4, §11.5, §9.1.
//
// **This file is a stub, and what is in it is the vocabulary three pages share.** The producer —
// the thing that turns a `Settings` into the six groups, their rows and one expanded row's details
// — is not written yet. What is written is every type that crosses the boundary between Rust and
// `ui/parts.slint`, plus the two that `devices.rs` needs, **defined once here rather than three
// times in three files**. Two spellings of one fact is the defect this program keeps finding in
// itself; three agents each inventing `RowAction` in parallel is that defect by construction.
//
// So: the enums below are the vocabulary, `Detail` is the one line inside an expanded row, and
// `Wrote` is what every producer answers when it is asked to act. `devices.rs` imports two of them
// and defines neither.
//
// ─── Which of these ordinals the markup actually pins ────────────────────────────────────────────
//
// An `int` crossing this boundary is only pinned where the **markup writes the number**. Measured
// against the two files `build.rs` compiles into the binary — `ui/window.slint` imports
// `ui/parts.slint` and `ui/devices.slint`, and nothing imports `ui/preview.slint`, which is a
// slint-viewer root and pins nothing at all:
//
//   - `Kind::Mounted` is **0** — `parts.slint:166`, `inert: root.r.kind == 0 && !root.r.expandable`.
//     §11.4's reserved plugged-in-iPod row is drawn as a line rather than a control, and that
//     comparison is the only place the markup decides anything from a `kind`.
//   - `RowAction::Remove` is **2** — `parts.slint:248`, `root.act(2, root.r.id)`. `Remove` is the
//     row's own control rather than a `Detail`, so it is the one row action the markup fires by
//     number instead of forwarding `DetailRow.action`.
//
// **That is all of it.** Every `Group` and every `Action` travels as `GroupRow.group` /
// `GroupRow.a-action` and comes back through `group-action(int, int)` untouched — `parts.slint:270`
// says so out loud: *in `parts::Group::ALL`'s order — which is written into the Rust type rather
// than into this markup.* `ui/devices.slint` pins nothing whatever: every ordinal it fires is
// `root.d.action`, which Rust put there. The rest of the order below is ours, and it is chosen to
// read in the order a person meets these things rather than to match anything.
//
// Each enum gets `ALL` / `from_i32 -> Option` / `as_i32`, which is `composer::Field`'s trio
// verbatim: an ordinal the markup sends that nothing here knows is a **no-op**, never a wrong
// branch. That is the whole reason the vocabulary is in Rust and not a Slint `enum` — a Slint enum
// cannot be swept, and an `int` with no exhaustive decoder is one renumbering away from firing
// `Remove` for `Reveal`.

use std::path::{Path, PathBuf};

use eapp_loader::settings::{self, Presence, Resource, Settings};
use eapp_loader::{firmware, identity, inspect, nor, si};

use crate::composer::{FixRow, Secret};
use crate::rail::{Caps, Next};

/// The six sections of the Parts page, in the order they are drawn.
///
/// Not pinned by any markup — `parts.slint:270` defers to this type by name. Six, always, and an
/// empty one keeps its heading and its verbs (§9.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Group {
    Ipods,
    Firmware,
    Bootloaders,
    Software,
    Disks,
    Snapshots,
}

impl Group {
    pub const ALL: [Group; 6] = [
        Group::Ipods,
        Group::Firmware,
        Group::Bootloaders,
        Group::Software,
        Group::Disks,
        Group::Snapshots,
    ];

    /// `None` for anything outside the list, so a stray `int` from the markup is a no-op rather
    /// than a panic or, worse, a different group.
    pub fn from_i32(n: i32) -> Option<Group> {
        usize::try_from(n).ok().and_then(|i| Group::ALL.get(i)).copied()
    }

    /// Its index in [`Group::ALL`], which is the number the markup carries.
    pub fn as_i32(self) -> i32 {
        Group::ALL.iter().position(|g| *g == self).expect("ALL holds every variant") as i32
    }
}

/// A group's own verbs — the one or two controls under an empty group that fill it.
///
/// Not pinned: `GroupRow.a-action` and `.b-action` carry the ordinal to the markup and
/// `group-action(g.group, g.a-action)` hands it straight back. Which two a group offers is
/// per-group and is the producer's answer, not this list's order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Action {
    AddDump,
    Synthesise,
    Fetch,
    Provide,
    Build,
    Discard,
}

impl Action {
    pub const ALL: [Action; 6] = [
        Action::AddDump,
        Action::Synthesise,
        Action::Fetch,
        Action::Provide,
        Action::Build,
        Action::Discard,
    ];

    pub fn from_i32(n: i32) -> Option<Action> {
        usize::try_from(n).ok().and_then(|i| Action::ALL.get(i)).copied()
    }

    pub fn as_i32(self) -> i32 {
        Action::ALL.iter().position(|a| *a == self).expect("ALL holds every variant") as i32
    }
}

/// What a part *is*, which decides how its row is drawn.
///
/// **`Mounted` is 0 and the markup depends on it** — `parts.slint:166`. The rest is ours: a `kind`
/// other than 0 reaches the markup only as a value it stores and hands back.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    /// §11.4's reserved row: a real iPod plugged into this machine. Pinned to 0.
    Mounted,
    Rom,
    Installer,
    Bootloader,
    Software,
    Disk,
    Snapshot,
}

impl Kind {
    pub const ALL: [Kind; 7] = [
        Kind::Mounted,
        Kind::Rom,
        Kind::Installer,
        Kind::Bootloader,
        Kind::Software,
        Kind::Disk,
        Kind::Snapshot,
    ];

    /// **`#[cfg(test)]` rather than `#[allow(dead_code)]`**, which is `geometry.rs`'s own answer
    /// to the same shape and the only one available here: the sweep that reads a bare allow decides
    /// a call **by text across files**, and `from_i32(` is called five times in `main.rs` — for
    /// `Group`, `Action`, `RowAction`, `Row` and `Field` — so an allow on this one would be
    /// reported as a retirement condition already met by somebody else's function.
    ///
    /// `Kind` is the one member of this file's vocabulary that travels **one way**. The markup
    /// reads it — `parts.slint:166`'s `r.kind == 0` — and no callback in `ui/` carries one back, so
    /// nothing in the shipped program has a `Kind` ordinal to decode. It is kept rather than
    /// deleted because the round trip is what makes a renumbering a no-op instead of a wrong
    /// branch, and `every_kind_survives_the_boundary` is the test that holds it.
    ///
    /// **Retired when:** a callback sends a `Kind` back and `main.rs` decodes one.
    #[cfg(test)]
    pub fn from_i32(n: i32) -> Option<Kind> {
        usize::try_from(n).ok().and_then(|i| Kind::ALL.get(i)).copied()
    }

    pub fn as_i32(self) -> i32 {
        Kind::ALL.iter().position(|k| *k == self).expect("ALL holds every variant") as i32
    }
}

/// What a control inside a row does. **Shared with `devices.rs`**, which imports it rather than
/// declaring a second one — the two pages draw the same `DetailRow` through the same flattener, so
/// a second copy of this list would be two vocabularies for one `int`.
///
/// **`Remove` is 2 and the markup depends on it** — `parts.slint:248`. Everything else travels as
/// `DetailRow.action`, which Rust wrote, so the rest of the order is ours: the three a part can
/// take, then the three a device can, then the three that need something drawn.
///
/// **`ShowIdentity` is the one variant added after the vocabulary was frozen**, and it is added
/// rather than borrowed because none of the eight meant it. §11.4 asks for the ROM's serial and
/// FireWire GUID to be drawn **masked, with a `Show`** — the same boundary `composer::Secret`
/// already holds for the identity fields — and `Reveal` is a file manager, not a mask. Appending
/// keeps every ordinal below it where the other two producers were written against.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum RowAction {
    Reveal,
    CopyPath,
    /// Pinned to 2 by the markup's own `Remove` control.
    Remove,
    PowerOff,
    Start,
    Edit,
    Rename,
    ShowBootScreen,
    /// §11.4's masked identity. Toggles; the label says what the press will do next.
    ShowIdentity,
}

impl RowAction {
    pub const ALL: [RowAction; 9] = [
        RowAction::Reveal,
        RowAction::CopyPath,
        RowAction::Remove,
        RowAction::PowerOff,
        RowAction::Start,
        RowAction::Edit,
        RowAction::Rename,
        RowAction::ShowBootScreen,
        RowAction::ShowIdentity,
    ];

    pub fn from_i32(n: i32) -> Option<RowAction> {
        usize::try_from(n).ok().and_then(|i| RowAction::ALL.get(i)).copied()
    }

    pub fn as_i32(self) -> i32 {
        RowAction::ALL.iter().position(|a| *a == self).expect("ALL holds every variant") as i32
    }
}

/// One line inside an expanded row. **Shared with `devices.rs`**; `main.rs`'s `to_detail` is the
/// one flattener onto `primitives.slint`'s `DetailRow`, for both pages.
///
/// Four renderings, told apart the way the markup tells them apart: an act when `action` is `Some`,
/// a mono line when `mono`, a labelled fact when there is a label, and a paragraph when there is
/// not.
///
/// **The eight properties an act needs are one field**, so they cannot disagree — `has-action`,
/// `action`, `act-label`, `enabled`, `reason`, `escape-hatch`, `presses` and `consequence` are
/// derived from this one `Option` and from the `FixRow` inside it. A row that is disabled therefore
/// cannot lose its reason on the way across, which `primitives.slint:368` states as the invariant.
///
/// **`machine_rule` is the line's, and the `FixRow`'s copy of it is deliberately not read.**
/// `DetailRow` has exactly one `machine-rule` and the markup binds it twice — to the `Pressable`
/// when there is an act (`parts.slint:63`, `devices.slint:56`) and to the paragraph when there is
/// not. One property, so one producer: this field. Reading the `FixRow`'s as well would be two
/// spellings of one fact arriving at the same pixel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Detail {
    /// `""` for a paragraph or a machine rule.
    pub label: String,
    pub value: String,
    pub mono: bool,
    pub machine_rule: bool,
    /// `FixRow::label` becomes `act-label`; the rest of it becomes the act's own state.
    pub action: Option<(RowAction, crate::composer::FixRow)>,
}

/// Whether the library moved, and therefore whether `main.rs` saves. **Shared by all three
/// producers**, which is why it lives in the shared module rather than three times over.
///
/// `Settings::render` regenerates the file whole and takes any comment the operator added with it,
/// so a save on a callback that mutated nothing is somebody's file rewritten for no reason. A
/// refusal mutates nothing and answers `Nothing`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wrote {
    Nothing,
    Library,
}

/// One line of an expanded row that depends on nothing but the read that opened it.
///
/// **Not a [`Detail`], deliberately.** A `Detail` may carry an act, and an act's `enabled` and
/// `reason` are answers about *this build* and *this moment* — they have to be recomputed on every
/// push, and a line read off a file at the moment the row opened must not be. So the two are
/// separate types and [`Parts::detail`] is the one place they meet.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Line {
    /// A label leading, a value trailing.
    Fact(String, String),
    /// A path, a hash, an address. `Metric.mono-family`.
    Mono(String),
    /// Prose, in `fg-dim`.
    Para(String),
    /// §9.4's machine rule — prose, in `fg`, because its teaching is the point.
    Rule(String),
}

impl Line {
    fn into_detail(self) -> Detail {
        let (label, value, mono, machine_rule) = match self {
            Line::Fact(l, v) => (l, v, false, false),
            Line::Mono(v) => (String::new(), v, true, false),
            Line::Para(v) => (String::new(), v, false, false),
            Line::Rule(v) => (String::new(), v, false, true),
        };
        Detail {
            label,
            value,
            mono,
            machine_rule,
            action: None,
        }
    }
}

/// What was read off the disk at the moment a row was opened.
///
/// **The read happens on the press, not on the push, and that is the whole of this type's
/// reason.** §11.4 wants an `.ipsw` identified *by contents* — `firmware::identify` hashes the
/// file, and the catalogue runs to 121 MB — and a ROM's `SysCfg` read out of the megabyte it sits
/// in. Doing either inside `view` would put a hash of somebody's firmware on the UI thread at
/// every repaint, which is §11.4's own drop rule (*hashing happens after the drop, not during it*)
/// broken one surface along.
///
/// So the cost is paid once, by the press that asked for it, and what a row says is what was read
/// when it was opened. Closing and re-opening the row re-reads. That is a record rather than a
/// live measurement, in exactly the sense [`settings::Provenance::line`] is one, and it is worded
/// as one.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Read {
    /// Nothing had to be read: everything this row's body says is already in the library.
    Plain,
    /// §11.4's ROM body. Boxed because it is much the largest of the three and `Read` is stored
    /// inline in the page's cursor.
    Rom(Box<Rom>),
    /// §11.4's `.ipsw` body.
    Ipsw(Vec<Line>),
}

/// §11.4's ROM body, as read.
///
/// The identity is kept apart from the rest because it is the one part that is re-worded on a
/// press this page owns — `Show` unmasks it — and re-reading a megabyte to change a mask would be
/// a read for a decision that has already been made.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Rom {
    /// The verdict, the image directory, the bootloader's build string.
    head: Vec<Line>,
    serial: Option<String>,
    guid: Option<String>,
    /// The model table's answer, `HwVr`, the record tags, and the raw bytes of everything
    /// `SysCfg` decoded no meaning for.
    tail: Vec<Line>,
    /// §11.4's two machine rules, plus the generation disagreement.
    rules: Vec<String>,
    /// [`identity::TitleAuth::line`] — said where the decision is made.
    title_auth: String,
}

/// The one open row, and what opening it cost.
///
/// **A part ID, never an index.** `parts.slint:355` compares `parts-detail-of` against `r.id`, and
/// `parts-expand(id, on)` and `parts-row-action(a, id)` both carry the id — so a removal that
/// renumbered the rows would leave an Expand open under somebody else's part.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Open {
    id: i32,
    read: Read,
    /// §11.4: the identity is masked, and `Show` reveals it. Per open row, so closing a row
    /// re-masks it — a screenshot of a page nobody is looking at must not carry an identifier.
    identity_shown: bool,
    /// The framebuffer `Show its boot screen` produced, or `None` while it has not been pressed.
    preview: Option<Preview>,
}

/// One group of §11.4's six, as the markup draws it.
///
/// The two `Option`s are `has-a` + `a-action` + `a` as **one field each**, so the three cannot
/// disagree about whether there is a verb — which is the shape `push_composer` lost `make_one` and
/// `warning` through.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroupView {
    pub group: Group,
    pub heading: String,
    pub count: usize,
    /// [`settings::Resource::verb`]'s own words — what this kind is *for*.
    pub verb: String,
    /// §9.1: what belongs here. **Never a bare *nothing here*.**
    pub empty: String,
    pub a: Option<(Action, FixRow)>,
    pub b: Option<(Action, FixRow)>,
}

/// One part, as the markup draws it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PartView {
    pub id: i32,
    pub group: Group,
    pub kind: Kind,
    pub name: String,
    /// The row's second line. **[`settings::Provenance::line`] and the library's own fields**, and
    /// nothing here may construct the word *verified* — a cached file nobody hashed says so.
    pub fact: String,
    /// §11.4's `used by N`, the reference-not-copy property made visible.
    pub used_by: String,
    pub expandable: bool,
    pub selected: bool,
    pub removable: bool,
    /// What goes with it, named **before** the press (`parts.slint:248`).
    pub remove_consequence: String,
    /// §11.4's one machine rule. `""` when nothing is holding it.
    pub locked_by: String,
}

/// A framebuffer, in raw pixels, because `parts.rs` may not name a toolkit type.
///
/// `main.rs`'s `to_image` wraps it. RGB8, three bytes per pixel, `w * h * 3` long.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preview {
    pub w: u32,
    pub h: u32,
    pub rgb: Vec<u8>,
}

/// **One bundle, one field per page property**, so the flattener cannot quietly drop one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct View {
    pub groups: Vec<GroupView>,
    pub rows: Vec<PartView>,
    /// The one open row's lines, and nothing else's.
    pub detail: Vec<Detail>,
    /// The open id, or `-1` — which is the markup's own default.
    pub detail_of: i32,
    pub preview: Option<Preview>,
}

/// The Parts page's whole state: **a cursor, not a copy.** Everything drawn is recomputed from
/// `Settings` on every push, which is what stops it going stale — the same discipline `Composer`
/// holds one `Recipe` under.
///
/// Not an `Option`: this page exists from startup, so there is no absent state to draw.
pub struct Parts {
    open: Option<Open>,
    /// A stable id per `(group, name)`. **Assigned on first sight and never reused**, so removing
    /// a part cannot renumber its neighbours under an open Expand.
    ids: std::collections::BTreeMap<(Group, String), i32>,
    /// Starts at 1. `-1` is the markup's *nothing open*, and `0` is left free so that a defaulted
    /// `int` arriving from anywhere cannot name a row.
    next_id: i32,
}

impl Parts {
    pub fn new() -> Parts {
        Parts {
            open: None,
            ids: std::collections::BTreeMap::new(),
            next_id: 1,
        }
    }

    /// The whole page, recomputed.
    ///
    /// **It does no filesystem work beyond `seen.exists`.** One `Presence` is made per pass and
    /// shared across every row, which is the discipline `device_rows` already holds; sizes, hashes
    /// and `SysCfg` reads belong to the one open row and were paid for by the press that opened it
    /// (see [`Read`]).
    ///
    /// `machine` is the device the emulator is running, by name, or `None`. It is an argument
    /// rather than a question this file asks, for the reason `Composer::lock` states about
    /// `building`: a gate wired to a phase nothing computes must not pretend to fire. `main::phase`
    /// answers `Off` unconditionally today, so today this is always `None` and §11.4's machine rule
    /// is drawn by this file's own tests and by nothing else.
    pub fn view(
        &mut self,
        s: &Settings,
        seen: &mut Presence,
        caps: Caps,
        busy: bool,
        machine: Option<&str>,
    ) -> View {
        let entries = inventory(s, seen, machine);
        let rows: Vec<PartView> = entries
            .iter()
            .map(|e| {
                let id = self.id_of(e.group, &e.key);
                PartView {
                    id,
                    group: e.group,
                    kind: e.kind,
                    name: e.name.clone(),
                    fact: e.fact.clone(),
                    used_by: if e.used_by.is_empty() {
                        String::new()
                    } else {
                        format!("used by {}", e.used_by.len())
                    },
                    expandable: e.expandable,
                    selected: self.open.as_ref().is_some_and(|o| o.id == id),
                    removable: e.removable,
                    remove_consequence: e.consequence.clone(),
                    locked_by: e.locked_by.clone(),
                }
            })
            .collect();

        // A cursor that no longer names a row closes itself. Without this, removing the open part
        // leaves `detail-of` pointing at an id nothing draws and the next part to take that id —
        // there is none, ids are never reused — would inherit an open Expand.
        if self.open.as_ref().is_some_and(|o| !rows.iter().any(|r| r.id == o.id)) {
            self.open = None;
        }

        let groups = Group::ALL
            .iter()
            .map(|g| self.group_view(*g, &rows, caps, busy))
            .collect();

        let detail = self.detail(&entries, caps);
        View {
            groups,
            rows,
            detail,
            detail_of: self.open.as_ref().map_or(-1, |o| o.id),
            preview: self.open.as_ref().and_then(|o| o.preview.clone()),
        }
    }

    /// Open or close one row, **and pay for what the body needs** — see [`Read`].
    ///
    /// `parts-expand(id, on)` is the callback this answers. It is not called `expand`, because
    /// `nav::Stack::expand` already is and `no_dead_code_allow_sits_on_a_function_the_program_
    /// already_calls` matches a call by text — a second `expand(` in this crate makes that sweep
    /// report `nav.rs`'s as reconnected when it is not. Its own rule for the ambiguity list is
    /// *never because a sweep went red*, so the name moved instead.
    ///
    /// `s` is an argument because the read is here: the id has to be resolved to a part before
    /// anything can be read off it, and resolving it needs the library.
    ///
    /// An id nothing answers to closes whatever was open rather than opening nothing, which is the
    /// same no-op an unknown ordinal gets.
    pub fn open_row(&mut self, s: &Settings, id: i32, open: bool) {
        if !open {
            self.open = None;
            return;
        }
        let mut seen = Presence::new();
        let entries = inventory(s, &mut seen, None);
        let Some(e) = entries.iter().find(|e| self.known_id(e.group, &e.key) == Some(id)) else {
            self.open = None;
            return;
        };
        if !e.expandable {
            self.open = None;
            return;
        }
        self.open = Some(Open {
            id,
            read: read_body(s, e),
            identity_shown: false,
            preview: None,
        });
    }

    /// A group's own verb. §11.4's table names which two each group offers.
    pub fn group_action(
        &mut self,
        s: &mut Settings,
        g: Group,
        a: Action,
    ) -> Result<Wrote, String> {
        if !g.offers(a) {
            return Err(format!(
                "{} is not one of {}'s verbs",
                a.label(),
                g.heading()
            ));
        }
        match a {
            // §11.4's Snapshots verb, and it is the only group verb this build can perform. The
            // other five need a file picker, a drop target, a download or the Composer, and each
            // is drawn disabled wearing `rail::Next`'s own sentence about which.
            Action::Discard => {
                let parked: Vec<String> = s
                    .devices
                    .iter()
                    .filter(|d| d.parked_at.is_some())
                    .map(|d| d.name.clone())
                    .collect();
                if parked.is_empty() {
                    return Err("there is no parked machine to discard".into());
                }
                let mut moved = false;
                for name in &parked {
                    moved |= s.discard_park(name);
                }
                Ok(if moved { Wrote::Library } else { Wrote::Nothing })
            }
            // Drawn disabled — there is neither a picker nor a drop target — so a press cannot
            // reach here from the window and the sentence is the one under the greyed control.
            Action::AddDump | Action::Provide => Err(refused_because(&Next::Provide)),
            // **Drawn DISABLED, and the sentence is the one under the greyed control.** It shipped
            // LIVE: [`Action::needs`] answered `Some(Next::Retry)`, which asks *is curl on this
            // computer*, `caps.download` measures that by running `curl --version`, and so on every
            // computer that has curl the control was blue and every press failed. `Fetch…` was a
            // control that only refused — the same defect as a callback nobody registered, one
            // layer quieter.
            //
            // What is true is narrower and is §9.4's second kind: the fetchers exist —
            // `firmware::download` and `rockbox::download` — and nothing on this page reaches
            // either. So [`Action::unwired`] words it once and both sides say it.
            Action::Fetch => Err(refused_because_unwired(a, g)),
            // **`main.rs` routes these two before they arrive**, in the same way it routes
            // `RowAction::Edit`: the Composer is the surface that holds a recipe — which is what
            // [`Action::needs`] says makes both of them live — and opening a page is not something
            // a toolkit-free file can do. So this arm is the one `Devices::row_action` writes for
            // `Edit`: reaching it at all is a defect in the window rather than in the library.
            Action::Synthesise | Action::Build => Err(format!(
                "{} opens the Composer rather than changing the library, so arriving here means \
                 the press is not wired",
                a.label()
            )),
        }
    }

    /// One control inside one row.
    ///
    /// **A refusal mutates nothing and answers `Nothing`**, so `main.rs` does not rewrite the
    /// settings file — which `Settings::render` regenerates whole, taking any comment the operator
    /// added with it.
    pub fn row_action(
        &mut self,
        s: &mut Settings,
        a: RowAction,
        id: i32,
        machine: Option<&str>,
    ) -> Result<Wrote, String> {
        let mut seen = Presence::new();
        let entries = inventory(s, &mut seen, machine);
        let Some(e) = entries
            .iter()
            .find(|e| self.known_id(e.group, &e.key) == Some(id))
        else {
            // An id nothing answers to is a no-op, in the same way an unknown ordinal is: the row
            // it named has left the library between the push and the press.
            return Ok(Wrote::Nothing);
        };
        let (group, key, locked) = (e.group, e.key.clone(), e.locked_by.clone());
        match a {
            RowAction::ShowIdentity => {
                if let Some(o) = self.open.as_mut().filter(|o| o.id == id) {
                    o.identity_shown = !o.identity_shown;
                }
                Ok(Wrote::Nothing)
            }
            RowAction::ShowBootScreen => {
                let source = s
                    .resources
                    .iter()
                    .find(|it| it.name == key)
                    .and_then(|it| match &it.what {
                        Resource::Firmware(src) => Some(src.clone()),
                        _ => None,
                    });
                let Some(source) = source else {
                    return Err("this part is not an iPod, so it has no boot screen".into());
                };
                if let Some(o) = self.open.as_mut().filter(|o| o.id == id) {
                    o.preview = match o.preview {
                        Some(_) => None,
                        None => Some(preview_of(&source)),
                    };
                }
                Ok(Wrote::Nothing)
            }
            RowAction::Remove => {
                // Asked again here rather than trusted from the control, because the control was
                // drawn at the last push and the machine may have started since.
                if !locked.is_empty() {
                    return Err(locked);
                }
                let moved = match group {
                    Group::Snapshots => s.discard_park(&key),
                    Group::Disks => s.remove_disk(&key),
                    _ => s.remove_resource(&key),
                };
                if !moved {
                    return Err(format!("{key} is not in the library"));
                }
                if self.open.as_ref().is_some_and(|o| o.id == id) {
                    self.open = None;
                }
                Ok(Wrote::Library)
            }
            RowAction::Reveal => Err(refused_because(&Next::Reveal)),
            RowAction::CopyPath => Err(refused_because(&Next::CopyDetails)),
            // The four a device's rows fire. They arrive here only if `ui/devices.slint`'s
            // ordinals ever reached this page's callback, which they do not — `devices.rs` owns
            // them — so this is the exhaustive arm rather than a route.
            RowAction::PowerOff | RowAction::Start | RowAction::Edit | RowAction::Rename => {
                Err(format!("{} is a device's control, not a part's", a.name()))
            }
        }
    }

    /// The id this `(group, name)` has, minting one if it has never been seen.
    fn id_of(&mut self, g: Group, name: &str) -> i32 {
        if let Some(id) = self.ids.get(&(g, name.to_string())) {
            return *id;
        }
        let id = self.next_id;
        self.next_id += 1;
        self.ids.insert((g, name.to_string()), id);
        id
    }

    /// The id this `(group, name)` already has. **Mints nothing** — a lookup that minted would
    /// give every unknown press a fresh row.
    fn known_id(&self, g: Group, name: &str) -> Option<i32> {
        self.ids.get(&(g, name.to_string())).copied()
    }

    fn group_view(&self, g: Group, rows: &[PartView], caps: Caps, busy: bool) -> GroupView {
        let (a, b) = g.actions();
        GroupView {
            group: g,
            heading: g.heading().to_string(),
            count: rows.iter().filter(|r| r.group == g && r.kind != Kind::Mounted).count(),
            verb: g.verb().to_string(),
            empty: g.empty().to_string(),
            a: a.map(|a| (a, verb_row(a, rows, g, caps, busy))),
            b: b.map(|b| (b, verb_row(b, rows, g, caps, busy))),
        }
    }

    /// The open row's lines, with its acts.
    fn detail(&self, entries: &[Entry], caps: Caps) -> Vec<Detail> {
        let Some(open) = self.open.as_ref() else {
            return Vec::new();
        };
        let Some(e) = entries
            .iter()
            .find(|e| self.known_id(e.group, &e.key) == Some(open.id))
        else {
            return Vec::new();
        };

        let mut out: Vec<Detail> = Vec::new();
        match &open.read {
            Read::Plain => {}
            Read::Ipsw(lines) => out.extend(lines.iter().cloned().map(Line::into_detail)),
            Read::Rom(rom) => {
                out.extend(rom.head.iter().cloned().map(Line::into_detail));
                // §11.4: masked, with a `Show`. `composer::Secret` is the masking boundary this
                // program already has, and there is deliberately no second one.
                let mut any = false;
                if let Some(v) = &rom.serial {
                    let secret = Secret::serial(v, open.identity_shown);
                    out.push(Line::Fact("Serial".into(), secret.text()).into_detail());
                    any = true;
                }
                if let Some(v) = &rom.guid {
                    let secret = Secret::guid(v, open.identity_shown);
                    out.push(Line::Fact("FireWire GUID".into(), secret.text()).into_detail());
                    any = true;
                }
                if any {
                    let label = Secret::serial("", open.identity_shown).action();
                    out.push(act(
                        RowAction::ShowIdentity,
                        FixRow {
                            label: label.into(),
                            enabled: true,
                            reason: String::new(),
                            escape: String::new(),
                            machine_rule: false,
                            presses: 1,
                            consequence: String::new(),
                        },
                    ));
                }
                out.extend(rom.tail.iter().cloned().map(Line::into_detail));
                out.push(Line::Para(rom.title_auth.clone()).into_detail());
                for r in &rom.rules {
                    out.push(Line::Rule(r.clone()).into_detail());
                }
                out.push(act(
                    RowAction::ShowBootScreen,
                    FixRow {
                        label: if open.preview.is_some() {
                            "Hide its boot screen".into()
                        } else {
                            "Show its boot screen".into()
                        },
                        enabled: true,
                        reason: String::new(),
                        escape: String::new(),
                        machine_rule: false,
                        presses: 1,
                        consequence: String::new(),
                    },
                ));
            }
        }

        // Who is using it, named rather than counted — the count is on the row and this is the
        // page the count sends you to.
        if !e.used_by.is_empty() {
            out.push(Line::Fact("Used by".into(), e.used_by.join(", ")).into_detail());
        }

        // The path, and the two acts over it. Both are drawn disabled in this build wearing
        // `rail::Next`'s own sentence, which is the same sentence the Rail draws for the same
        // absent capability — one refusal, worded once.
        if let Some(p) = &e.path {
            out.push(Line::Mono(p.display().to_string()).into_detail());
            out.push(act(RowAction::Reveal, next_row("Reveal", &Next::Reveal, caps)));
            out.push(act(
                RowAction::CopyPath,
                next_row("Copy path", &Next::CopyDetails, caps),
            ));
        }
        out
    }
}

// ─── The vocabulary's own words ─────────────────────────────────────────────────────────────────

impl Group {
    /// §11.4's own heading for this section.
    pub fn heading(self) -> &'static str {
        match self {
            Group::Ipods => "iPods",
            Group::Firmware => "Apple firmware",
            Group::Bootloaders => "Bootloaders",
            Group::Software => "Software",
            Group::Disks => "Disks",
            Group::Snapshots => "Snapshots",
        }
    }

    /// What this kind is **for**, in the words the model already uses for it.
    ///
    /// The four resource groups read it off [`Resource::verb`] rather than repeating it, which is
    /// why a `Resource` is constructed here only to be asked: the alternative is four string
    /// literals that agree with `settings.rs` on the day they are written. An empty group has no
    /// row to ask, so the sample is what makes the answer available before the first part arrives.
    ///
    /// The last two have no `Resource` — a disk is what ingredients are combined *into*, and a
    /// snapshot is a device's paused state — so §11.4's own table words those two.
    pub fn verb(self) -> &'static str {
        match self {
            Group::Ipods => Resource::Firmware(nor::Source::File(PathBuf::new())).verb(),
            Group::Firmware => Resource::Installer(PathBuf::new()).verb(),
            Group::Bootloaders => Resource::Bootloader(PathBuf::new()).verb(),
            Group::Software => Resource::Software(PathBuf::new()).verb(),
            Group::Disks => "what a device runs",
            Group::Snapshots => "what press the centre button to resume resumes",
        }
    }

    /// §9.1: **never a bare *nothing here***. It says what belongs in this group, and the group's
    /// own verbs below it are the one action that fills it.
    pub fn empty(self) -> &'static str {
        match self {
            Group::Ipods => {
                "A boot ROM — one megabyte read off a real iPod, or one synthesised from a model \
                 number and a seed. A device names one and cannot be made without it."
            }
            Group::Firmware => {
                "An Apple software bundle, an .ipsw. A drive is built from one, and the drive is \
                 what runs."
            }
            Group::Bootloaders => {
                "ipodloader2, or Rockbox's. It goes in the firmware partition, which holds exactly \
                 one thing — so everything called dual or triple boot is one of these offering the \
                 rest."
            }
            Group::Software => {
                "Rockbox, ZeroSlackr, a Linux kernel. It is installed onto a drive rather than run \
                 on its own."
            }
            Group::Disks => {
                "A drive image — what a device runs. One is built from an Apple bundle, or \
                 provided whole."
            }
            Group::Snapshots => {
                "A paused machine: the RAM it stopped in and the drive it was paused against, so \
                 there is something for press the centre button to resume to resume."
            }
        }
    }

    /// The one or two verbs this group offers, in the order §11.4's table names them.
    pub fn actions(self) -> (Option<Action>, Option<Action>) {
        match self {
            Group::Ipods => (Some(Action::AddDump), Some(Action::Synthesise)),
            Group::Firmware | Group::Bootloaders | Group::Software => {
                (Some(Action::Fetch), Some(Action::Provide))
            }
            Group::Disks => (Some(Action::Build), Some(Action::Provide)),
            Group::Snapshots => (Some(Action::Discard), None),
        }
    }

    /// Whether this group offers that verb at all — the guard on `parts-group-action(g, a)`, whose
    /// two ordinals travel independently and can therefore arrive paired with each other wrongly.
    pub fn offers(self, a: Action) -> bool {
        let (x, y) = self.actions();
        x == Some(a) || y == Some(a)
    }

    /// **The command that fetches what belongs in this group, from a terminal.**
    ///
    /// §9.4's rule for a project state: name a route that is real and was run to check it. Both of
    /// these exist in `ipod-boot` today and both **download**:
    ///
    ///   - `firmware get` takes an `UpdaterFamilyID` or a filename and lands the bundle in
    ///     `firmware::cache_dir()` — the same cache this page reads. `firmware list` prints the
    ///     numbers to put in `<family>`.
    ///   - `rockbox-install` downloads Rockbox's bootloader **and** its release, verifies both by
    ///     SHA-256, and installs each into the half of a drive that wants it. It does more than
    ///     fetch, and it is named here rather than a narrower command because there is no narrower
    ///     one: nothing in this program downloads a bootloader on its own.
    ///
    /// `None` for the three groups that do not offer [`Action::Fetch`] at all — an unreachable
    /// route named anyway is the phantom this page is about.
    pub fn fetch_route(self) -> Option<&'static str> {
        match self {
            Group::Firmware => Some("ipod-boot firmware get <family>"),
            Group::Bootloaders | Group::Software => Some("ipod-boot rockbox-install"),
            Group::Ipods | Group::Disks | Group::Snapshots => None,
        }
    }

    /// Whether a part filed in this group can be opened at all.
    ///
    /// **The four resource groups can; disks and snapshots cannot, and that is a scope decision
    /// rather than a fact about the model.** `inspect::disk` and `inspect::drive_facts` both exist
    /// and would fill a drive's body; §11.4's snapshot body wants an instruction count and
    /// `Config::pair_is_whole`, and neither is reachable from `Settings` — nothing records where a
    /// device's snapshot lives. See this file's own note above [`Entry::expandable`].
    pub fn expandable(self) -> bool {
        !matches!(self, Group::Disks | Group::Snapshots)
    }
}

impl Action {
    /// The control's label. §11.4's table words all six.
    pub fn label(self) -> String {
        match self {
            Action::AddDump => "Add a dump…".into(),
            Action::Synthesise => "Synthesise…".into(),
            Action::Fetch => "Fetch…".into(),
            Action::Provide => "Provide…".into(),
            Action::Build => "Build…".into(),
            Action::Discard => "Discard".into(),
        }
    }

    /// **Which capability this verb needs, asked of `rail::Next` rather than answered here.**
    ///
    /// §9.4's refusals belong to one type, so the Rail and this page cannot word one absent
    /// capability two ways. What each verb needs:
    ///
    ///   - a file has to arrive from outside — a picker or a drop — for `Add a dump…` and
    ///     `Provide…`;
    ///   - a surface that holds a recipe for `Synthesise…` and `Build…`, which is the Composer,
    ///     and this build has one — so both are drawn live.
    ///
    /// **Two answer `None`, for two different reasons, and neither is *no capability is needed*
    /// alone.** `Discard` needs nothing: the parked machines are this program's own files on this
    /// computer, in the same way `Next::CancelWrite` needs no capability. `Fetch…` needs nothing
    /// **because no capability is what is missing** — see [`Action::unwired`]. It used to answer
    /// `Some(Next::Retry)`, which asks *is curl on this computer*, and on every computer that has
    /// curl the answer was yes and the control was drawn blue. It was blue and it only ever
    /// refused. A capability question is the wrong question when the mechanism behind the control
    /// does not exist, and asking it draws a live control over a hole.
    pub fn needs(self) -> Option<Next> {
        match self {
            Action::AddDump | Action::Provide => Some(Next::Provide),
            Action::Synthesise | Action::Build => Some(Next::Fix {
                label: self.label(),
                presses: 1,
            }),
            Action::Fetch | Action::Discard => None,
        }
    }

    /// **The verb this build DRAWS and has no mechanism for**, and the sentence that says so.
    ///
    /// §14.1: a control that cannot do the thing is disabled, states why, and names what to do
    /// instead — it is never drawn live so it can apologise on press. `Fetch…` was drawn live on
    /// three groups and every press failed, because there is no per-part fetch behind it: the only
    /// download this build starts is `work::Queue`'s first-run plan, whose releases are fixed at
    /// `compose::FIRST_RUN_FAMILY`.
    ///
    /// **This is §9.4's second kind — a project state, not a machine rule.** Nothing about the
    /// computer refuses. `eapp_loader::firmware::download` and `eapp_loader::rockbox::download`
    /// both exist and both work; what does not exist is a route from a group's verb to either one,
    /// and the group's own [`Group::fetch_route`] is the command that has it today.
    ///
    /// `None` for the other five: their availability is a capability question and [`Action::needs`]
    /// asks `rail::Next` it.
    ///
    /// **One producer for two use sites.** `verb_row` words the disabled control and
    /// `Parts::group_action` words the arm that runs if a press arrives anyway, and a refusal
    /// written twice is a refusal that comes to be worded twice.
    pub fn unwired(self) -> Option<&'static str> {
        match self {
            // **One clause, measured.** It is drawn in §11.4's group-verb pair and, through
            // [`refused_because_unwired`], in a Rail entry as well — and a reason slot elides and
            // cannot wrap. What shipped here — *nothing on this page reaches a fetcher yet — the
            // only download this build starts is the first run's own plan* — measures **558 px**
            // against a 146 px column, so what a person read was
            // *nothing on this page reaches a …*. The half that is gone said which download this
            // build *does* start; `Group::fetch_route`'s command is drawn directly underneath and
            // is the answer a person at this control actually needs.
            Action::Fetch => Some("no fetcher on this page yet"),
            Action::AddDump
            | Action::Synthesise
            | Action::Provide
            | Action::Build
            | Action::Discard => None,
        }
    }
}

impl RowAction {
    /// Whether this control **destroys something** — the one fact about an act the two pages that
    /// draw a [`Detail`] were answering differently.
    ///
    /// `ui/parts.slint` drew every act in `Ink.accent` and `ui/devices.slint` drew every act in
    /// `Ink.danger`: one struct, one flattener, two colours, and the disagreement only became
    /// visible when the devices page gained a line that is not a removal — `Edit…`, drawn in the
    /// destructive colour. The colour is a fact about the act rather than about the page, so the
    /// act answers it here and both files bind the answer.
    ///
    /// **`Remove` is the only one, and `PowerOff` is deliberately not.** §12.4 parks a machine
    /// rather than discarding it, so stopping one destroys nothing; everything else reads,
    /// reveals, or opens a page.
    pub fn destructive(self) -> bool {
        matches!(self, RowAction::Remove)
    }

    /// The control's own word, for a refusal that has to name it.
    pub fn name(self) -> &'static str {
        match self {
            RowAction::Reveal => "Reveal",
            RowAction::CopyPath => "Copy path",
            RowAction::Remove => "Remove",
            RowAction::PowerOff => "Power off",
            RowAction::Start => "Start",
            RowAction::Edit => "Edit",
            RowAction::Rename => "Rename",
            RowAction::ShowBootScreen => "Show its boot screen",
            RowAction::ShowIdentity => "Show",
        }
    }
}

// ─── The library, read once per pass ────────────────────────────────────────────────────────────

/// One part, as the library holds it — before anything about this build is asked.
///
/// It exists so the six groups, the rows, the counts and the open row's body are all built from
/// **one** walk of the library. Two walks is two answers, and the count and the rows it counts
/// disagreeing is what §11.4's own six-group complaint is about one level up.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Entry {
    group: Group,
    /// **The key the model knows it by**, which is what `remove_resource`, `remove_disk` and
    /// `discard_park` are handed. Empty for §11.4's reserved row, which is not in any list.
    key: String,
    /// What is drawn. The same as `key` for everything the library holds.
    name: String,
    kind: Kind,
    fact: String,
    path: Option<PathBuf>,
    /// Whether the row has a body to open.
    ///
    /// **Disks and snapshots ship `false`.** Not a stub and not an oversight: a drive's body wants
    /// the partition table, the FAT type and an in-window FAT32 tree, and a snapshot's wants the
    /// instruction count it was taken at and `Config::pair_is_whole`'s answer — and `Settings`
    /// records **no path to a snapshot at all**, so half of that cannot be produced from this
    /// crate's model however long the body were made. Drawing an Expand that opened onto three
    /// lines of apology is worse than a row that says it does not open.
    expandable: bool,
    removable: bool,
    /// Every device and every drive that names it, by name.
    used_by: Vec<String>,
    /// What the removal costs, named **before** the press.
    consequence: String,
    /// §11.4's one machine rule, or `""`.
    locked_by: String,
}

/// The whole library, in §11.4's six groups and their order.
///
/// **`seen` is the pass's shared stat cache and the only filesystem work this does.** A part whose
/// file has left the disk says so on its own row, which is the state the shipped bench used to
/// draw as fine.
fn inventory(s: &Settings, seen: &mut Presence, machine: Option<&str>) -> Vec<Entry> {
    let running = machine.and_then(|m| s.devices.iter().find(|d| d.name == m));
    // **The one resource a device references directly is its boot ROM.** Its drive is a `Disk`,
    // in its own namespace and with its own row below, and the `.ipsw` that drive was built from
    // is not referenced by the device at all — the drive exists, and removing the bundle's entry
    // takes nothing away from a machine that is running. So the rule is one comparison, not a
    // transitive walk that would refuse removals nothing is holding.
    let holds_resource = |key: &str| running.is_some_and(|d| d.firmware == key);
    let held = |by: bool| -> String {
        match (by, machine) {
            (true, Some(m)) => format!("{m} is running"),
            _ => String::new(),
        }
    };

    let mut out: Vec<Entry> = Vec::new();

    // §11.4: **the plugged-in-iPod row is reserved, always**, and it is a line rather than a
    // control until one appears — the group does not *grow* a row on an event nobody initiated at
    // this surface. `identity::detect_mounted` is what would fill it, and it is deliberately not
    // called: it walks /Volumes, /media and /run/media one and two levels deep, and a machine with
    // an unresponsive SMB mount blocks its caller for as long as the mount takes to time out.
    // §11.4 gives it a 2 s poll **off the UI thread** while Parts is open; there is no such poll,
    // so the row says what is true — that nothing here is looking.
    out.push(Entry {
        group: Group::Ipods,
        key: String::new(),
        name: "No iPod is plugged in".into(),
        kind: Kind::Mounted,
        fact: "nothing is watching yet".into(),
        path: None,
        expandable: false,
        removable: false,
        used_by: Vec::new(),
        consequence: String::new(),
        locked_by: String::new(),
    });

    for it in &s.resources {
        let (group, kind) = match &it.what {
            Resource::Firmware(_) => (Group::Ipods, Kind::Rom),
            Resource::Installer(_) => (Group::Firmware, Kind::Installer),
            // The fourth kind the shipped window dropped on the floor. §3's own named complaint,
            // and a clean-looking Parts page is not evidence that no bootloader is filed.
            Resource::Bootloader(_) => (Group::Bootloaders, Kind::Bootloader),
            Resource::Software(_) => (Group::Software, Kind::Software),
        };
        let path = it.what.path().map(|p| p.to_path_buf());
        let mut used_by = s.devices_using_resource(&it.name);
        used_by.extend(s.disks_recording_resource(&it.name));
        let mut fact = it.from.map(|f| f.line()).unwrap_or_default();
        // A part whose file is gone. `Presence::exists` is `false` only when the path was looked
        // for and was not there, so a permission error does not become a claim about the file.
        if let Some(p) = &path {
            if !seen.exists(p) {
                let gone = "the file is not where it was";
                fact = if fact.is_empty() {
                    gone.into()
                } else {
                    format!("{fact}, {gone}")
                };
            }
        }
        let seed = match &it.what {
            Resource::Firmware(nor::Source::Synthetic { seed, .. }) => Some(*seed),
            _ => None,
        };
        out.push(Entry {
            group,
            key: it.name.clone(),
            name: it.name.clone(),
            kind,
            fact,
            path,
            expandable: group.expandable(),
            removable: true,
            consequence: remove_consequence(&used_by, seed),
            used_by,
            locked_by: held(holds_resource(&it.name)),
        });
    }

    for k in &s.disks {
        let used_by = s.devices_using_disk(&k.name);
        let mut fact = match &k.built_from {
            Some(b) => format!("built from {b}"),
            None => "provided".to_string(),
        };
        if !k.installed.is_empty() {
            fact = format!("{fact}, with {} installed", k.installed.join(", "));
        }
        if !seen.exists(&k.path) {
            fact = format!("{fact}, the image is not where it was");
        }
        let locked = running.is_some_and(|d| {
            d.disk.as_deref() == Some(k.name.as_str()) || d.disk_path.as_ref() == Some(&k.path)
        });
        out.push(Entry {
            group: Group::Disks,
            key: k.name.clone(),
            name: k.name.clone(),
            kind: Kind::Disk,
            fact,
            path: Some(k.path.clone()),
            expandable: Group::Disks.expandable(),
            removable: false,
            used_by,
            consequence: String::new(),
            locked_by: held(locked),
        });
    }

    // §11.4's sixth group. 1.6 GB per park was invisible: close the window four times across four
    // devices and 6.4 GB exists that nobody asked for and nobody could total.
    let now = settings::now_unix();
    for d in &s.devices {
        let Some(secs) = settings::parked_for(d, now) else {
            continue;
        };
        out.push(Entry {
            group: Group::Snapshots,
            key: d.name.clone(),
            name: d.name.clone(),
            kind: Kind::Snapshot,
            // `ago` is the window's one spelling of *time since*, and the shelf already draws it.
            fact: format!("parked {}", crate::ago(secs)),
            path: None,
            expandable: Group::Snapshots.expandable(),
            removable: false,
            used_by: Vec::new(),
            consequence: String::new(),
            locked_by: held(running.is_some_and(|r| r.name == d.name)),
        });
    }
    out
}

/// §11.4: `Remove` **names what goes with it before it acts**.
///
/// Never empty, because the control takes two presses and `primitives.slint` reserves the slot for
/// whatever the second press will do — a blank there is a control that arms and says nothing.
fn remove_consequence(used_by: &[String], synthesised_seed: Option<u64>) -> String {
    let mut s = if used_by.is_empty() {
        "Nothing else names it. The entry goes; the file itself stays where it is.".to_string()
    } else {
        format!(
            "{} still name it — {}. They will say so rather than quietly losing it, and the file \
             itself is not deleted.",
            used_by.len(),
            used_by.join(", ")
        )
    };
    // §11.4: removing a **synthesised** iPod additionally shows the seed, because the identity is
    // regenerable only from it.
    if let Some(seed) = synthesised_seed {
        s.push_str(&format!(
            " This iPod is a recipe: only seed {seed:x} regenerates its identity."
        ));
    }
    s
}

// ─── What a press pays for ──────────────────────────────────────────────────────────────────────

/// Read whatever this row's body needs, once, at the moment it is opened. See [`Read`].
fn read_body(s: &Settings, e: &Entry) -> Read {
    match e.kind {
        Kind::Rom => match s.resources.iter().find(|it| it.name == e.key) {
            Some(it) => match &it.what {
                Resource::Firmware(src) => Read::Rom(Box::new(read_rom(src))),
                _ => Read::Plain,
            },
            None => Read::Plain,
        },
        Kind::Installer => match &e.path {
            Some(p) => Read::Ipsw(read_ipsw(p)),
            None => Read::Plain,
        },
        _ => Read::Plain,
    }
}

/// §11.4's ROM body.
fn read_rom(src: &nor::Source) -> Rom {
    let mut head: Vec<Line> = Vec::new();
    let mut tail: Vec<Line> = Vec::new();
    let mut rules: Vec<String> = Vec::new();

    match src {
        nor::Source::File(p) => {
            // §11.4: the verdict **with its own diagnosis**. A refusal is a machine rule — the
            // file is what it is, and no amount of work on this program changes it — so it is
            // drawn in `fg` rather than `fg-dim`.
            let v = inspect::flash(p);
            head.push(match v.ok() {
                true => Line::Para(drawable(v.text())),
                false => Line::Rule(drawable(v.text())),
            });
            if let Ok(nor) = std::fs::read(p) {
                let images = inspect::nor_images(&nor);
                if !images.is_empty() {
                    let tags: Vec<&str> = images.iter().map(|e| e.tag.as_str()).collect();
                    head.push(Line::Fact("Images".into(), tags.join(", ")));
                }
                if let Some(cfg) = inspect::syscfg(&nor) {
                    match (cfg.model.as_deref(), cfg.model_info()) {
                        (Some(m), Some(info)) => {
                            tail.push(Line::Fact(
                                "Model".into(),
                                format!(
                                    "{m} — {} GB, {}, {}",
                                    info.capacity_gb,
                                    info.colour().label(),
                                    info.generation.label()
                                ),
                            ));
                            // §11.4's second machine rule. The bench draws whatever ROM a device
                            // points at with one drawing, so rendering a nano's dump as a 5.5G
                            // with a 320x240 glass would be inventing a fact about somebody's
                            // hardware.
                            if info.generation.gestalt().is_none() {
                                rules.push(format!(
                                    "this is not a 5th-generation iPod — the table says {}",
                                    info.generation.label()
                                ));
                            }
                        }
                        // §11.4's first machine rule, and it claims nothing it cannot look up.
                        (Some(m), None) => {
                            tail.push(Line::Fact("Model".into(), m.to_string()));
                            rules.push(format!(
                                "Mod# {m} is not in the model table. HwVr says {}. This will not \
                                 claim a generation it cannot look up.",
                                cfg.hw_vr
                                    .map(|v| format!("{v:#010X}"))
                                    .unwrap_or_else(|| "nothing".into())
                            ));
                        }
                        (None, _) => {}
                    }
                    if let Some(v) = cfg.hw_vr {
                        tail.push(Line::Fact("HwVr".into(), format!("{v:#010X}")));
                    }
                    // Two independent statements of the generation. They should agree; that they
                    // do not is worth knowing rather than silently preferring one.
                    if cfg.generation_agrees() == Some(false) {
                        rules.push(
                            "Mod# and HwVr disagree about which generation this is. Both are read \
                             out of the dump, so one of them is not what it looks like."
                                .into(),
                        );
                    }
                    if !cfg.tags.is_empty() {
                        tail.push(Line::Fact("Records".into(), cfg.tags.join(", ")));
                    }
                    // **Every record nothing here decodes, as bytes.** Rockbox names nine tags and
                    // this project's own 5G NOR carries a tenth it does not list, so the next dump
                    // may hold one nobody has written down. Printing the bytes is the difference
                    // between *we do not know what this is* and quietly dropping it.
                    for (tag, raw) in cfg.records.iter().filter(|(t, _)| !DECODED.contains(&t.as_str())) {
                        let hex: Vec<String> = raw.iter().map(|b| format!("{b:02x}")).collect();
                        tail.push(Line::Mono(format!("{tag}  {}", hex.join(" "))));
                    }
                }
            }
        }
        nor::Source::Synthetic { model, seed, .. } => {
            head.push(Line::Para(
                "This iPod is a recipe rather than a dump: a model number, a seed, and whatever \
                 identity was typed over them. Nothing of it is on disk, so there is nothing to go \
                 stale and nothing to clean up."
                    .into(),
            ));
            tail.push(Line::Fact(
                "Model".into(),
                match identity::Model::lookup(model) {
                    Some(info) => format!(
                        "{model} — {} GB, {}, {}",
                        info.capacity_gb,
                        info.colour().label(),
                        info.generation.label()
                    ),
                    None => model.clone(),
                },
            ));
            tail.push(Line::Fact("Seed".into(), format!("{seed:x}")));
        }
    }

    // The identity, from the one resolution point rather than from a second read of `SysCfg`. It
    // is also what answers §11.4's `TitleAuth` line, and it answers it where the decision is made.
    let (mut serial, mut guid, mut title_auth) = (None, None, String::new());
    match src.identity() {
        Ok(id) => {
            serial = id.serial.clone();
            guid = Some(format!("{:016X}", id.guid));
            title_auth = id.title_auth().line().to_string();
            // The strongest evidence there is that a dump did not parse.
            if let Some(w) = id.oui_warning() {
                rules.push(drawable(&w));
            }
        }
        Err(why) => head.push(Line::Rule(drawable(&why))),
    }

    Rom {
        head,
        serial,
        guid,
        tail,
        rules,
        title_auth,
    }
}

/// The four `SysCfg` tags this window words. Everything else is drawn as bytes.
const DECODED: [&str; 4] = ["SrNm", "FwId", "Mod#", "HwVr"];

/// §11.4's `.ipsw` body — versions, checksums, and `firmware::identify`'s answer **by contents**.
fn read_ipsw(path: &Path) -> Vec<Line> {
    let mut out: Vec<Line> = Vec::new();
    if let Ok(m) = std::fs::metadata(path) {
        out.push(Line::Fact("On disk".into(), si(m.len())));
    }
    for (label, value) in inspect::ipsw_facts(path) {
        out.push(Line::Fact(label.to_string(), drawable(&value)));
    }
    // **By contents, never by extension or by name.** People rename downloads and browsers add
    // `(1)`; a file called `iPod_25.1.3.ipsw` is not evidence of anything, and the hash is. This
    // is the expensive half of the read, and it is why the read is on the press.
    match std::fs::read(path) {
        Ok(bytes) => {
            let p = firmware::identify(&bytes);
            out.push(Line::Fact(
                "Identified by contents".into(),
                drawable(&p.line()),
            ));
            // `Unrecognised` is explicitly allowed: modified firmware is a legitimate reason to
            // want an emulator, and it is reported so you know rather than to stop you.
            if let Some(w) = p.warning() {
                out.push(Line::Para(w.into()));
            }
        }
        Err(e) => out.push(Line::Rule(format!("this file could not be read: {e}"))),
    }
    out
}

/// §11.4's boot-screen preview, widened to the eight-bit channels the window draws.
///
/// The panel's own size, from `emu::FB_W` / `FB_H` — not a number typed here — because §6.1's rule
/// is an exact integer scale and nearest neighbour, and a preview at any other size is a resampled
/// framebuffer presented as a screenshot.
fn preview_of(src: &nor::Source) -> Preview {
    let (w, h) = (crate::emu::FB_W, crate::emu::FB_H);
    let fb = src.boot_screen(w, h);
    let mut rgb = vec![0u8; w * h * 3];
    for (i, px) in fb.iter().enumerate() {
        // 5- and 6-bit channels widened by bit replication, the same expansion
        // `emu::read_framebuffer` uses — so a pixel here and a pixel in the panel are the same
        // number.
        let (r, g, b) = ((px >> 11) & 0x1f, (px >> 5) & 0x3f, px & 0x1f);
        rgb[i * 3] = ((r << 3) | (r >> 2)) as u8;
        rgb[i * 3 + 1] = ((g << 2) | (g >> 4)) as u8;
        rgb[i * 3 + 2] = ((b << 3) | (b >> 2)) as u8;
    }
    Preview {
        w: w as u32,
        h: h as u32,
        rgb,
    }
}

// ─── Controls ───────────────────────────────────────────────────────────────────────────────────

/// A `Detail` that is nothing but an act.
///
/// `DetailView` draws `act-label` for a line with an action and reads neither `label` nor `value`,
/// so both are empty rather than carrying a second copy of the label.
fn act(a: RowAction, mut fix: FixRow) -> Detail {
    // **One property, one producer, and the value is MOVED rather than copied.** `DetailRow` has
    // exactly one `machine-rule`; the markup binds it to the `Pressable` when there is an act
    // (`parts.slint:63`) and to the paragraph when there is not, and `main.rs`'s `to_detail` reads
    // it off the `Detail`. Leaving the `FixRow`'s copy set as well would be two fields holding one
    // fact on their way to one pixel, which is how they come to disagree.
    let machine_rule = std::mem::take(&mut fix.machine_rule);
    Detail {
        label: String::new(),
        value: String::new(),
        mono: false,
        machine_rule,
        action: Some((a, fix)),
    }
}

/// A control whose availability is a question about **this build**, asked of `rail::Next`.
///
/// **Never a literal `true`**, which is the defect `copy_command_row` shipped. The reason is
/// `Next`'s own sentence, so the Rail and this page cannot word one absent capability two ways —
/// and it is blanked when the control is live, because a reason under a pressable control is a
/// refusal for something that is not being refused.
///
/// **The escape hatch is `Next`'s only where `Next`'s is about this act.** `Next::Reveal` and
/// `Next::CopyDetails` are worded for the Rail's failure flow — `IPOD_EMULATOR_DATA=<path>` is how
/// you move this program's files, and `ipod-boot firmware cache --verify` prints a bundle's
/// hashes — and neither reveals or copies the path under the finger. §9.4's rule is that a project
/// state names a command that exists and was run to check it; naming one that does something else
/// is worse than naming none, so these two carry none. What they have instead is the `mono` path
/// drawn immediately above them, which is the value the act would have produced.
fn next_row(label: &str, n: &Next, caps: Caps) -> FixRow {
    let enabled = n.available(caps);
    FixRow {
        label: label.to_string(),
        enabled,
        reason: if enabled {
            String::new()
        } else {
            drawable(n.reason())
        },
        escape: String::new(),
        // §9.4's two kinds: `Next::Retry` is the one sentence in that function that says *your
        // computer cannot do this*; the rest say *we have not finished this*.
        machine_rule: matches!(n, Next::Retry),
        presses: 1,
        consequence: String::new(),
    }
}

/// One of a group's verbs.
fn verb_row(a: Action, rows: &[PartView], g: Group, caps: Caps, busy: bool) -> FixRow {
    let mut row = match a.needs() {
        Some(n) => next_row(&a.label(), &n, caps),
        // The one verb that needs no capability. It is still not unconditionally enabled: there
        // has to be something to discard.
        None => FixRow {
            label: a.label(),
            enabled: true,
            reason: String::new(),
            escape: String::new(),
            machine_rule: false,
            presses: 1,
            consequence: String::new(),
        },
    };
    // **§14.1 for the one verb with no mechanism behind it.** It is disabled here rather than left
    // blue to refuse on press, and the sentence is [`Action::unwired`]'s so the drawn refusal and
    // the pressed one are the same words. The escape hatch is the group's, because what you would
    // fetch differs per group and a command that fetches the wrong thing is worse than none.
    if let Some(why) = a.unwired() {
        row.enabled = false;
        row.reason = drawable(why);
        row.escape = g.fetch_route().unwrap_or_default().into();
        // Not a machine rule: nothing about this computer refuses. See [`Action::unwired`].
        row.machine_rule = false;
    }
    if a == Action::Discard {
        let parked: Vec<&PartView> = rows.iter().filter(|r| r.group == g).collect();
        if parked.is_empty() {
            row.enabled = false;
            row.reason = "nothing is parked".into();
        } else {
            // §11.3: anything that detaches a reference arms first, and the consequence is drawn
            // before the press rather than after it.
            row.presses = 2;
            row.consequence = format!(
                "{} parked {} forgotten. The RAM and the frozen drive behind them are not deleted \
                 by this — nothing in the library records where they are.",
                parked.len(),
                if parked.len() == 1 {
                    "machine is"
                } else {
                    "machines are"
                }
            );
        }
    }
    // A build owns the drive it is writing and the bundle it is reading. A verb that would start a
    // second one waits — **and only those**: `Discard` needs no capability and starts nothing, and
    // `Remove` is not gated at all, because `remove_resource` and `remove_disk` touch no file.
    if busy && row.enabled && a.needs().is_some() {
        row.enabled = false;
        row.reason = "a build is already running".into();
        row.escape = String::new();
    }
    row
}

/// The sentence under a control this build draws **disabled**, for the arm that would run if one
/// were pressed anyway.
///
/// **Only for the disabled ones**, and the doc used to say the opposite — *a control this build
/// cannot honour is pressed anyway*, which describes a control that cannot be pressed. [`Next::
/// reason`] is non-empty *exactly for the steps [`Next::available`] can refuse*, so reusing it for
/// a **live** control puts a sentence about an absent capability under a press that only happened
/// because the capability is present. That shipped: `Synthesise…` was blue because the Composer
/// exists and refused with *there is no Composer in this build yet*, one row from the page that
/// opens it. Every caller left is a control the window greys out.
fn refused_because(n: &Next) -> String {
    drawable(n.reason())
}

/// The sentence under a control this build draws **disabled because it has no mechanism**, for the
/// arm that would run if a press arrived anyway.
///
/// [`refused_because`]'s sibling, and the difference is which question was refused: that one is for
/// a control an absent **capability** greys out, this one for a control an absent **route** greys
/// out. Both are §9.4; only the second names a command, because only the second has one.
///
/// **What a press produces is the drawn sentence plus the route**, joined rather than re-worded, so
/// `every_verb_with_no_mechanism_is_drawn_disabled_with_a_route` can hold the two to each other with
/// a `starts_with`.
fn refused_because_unwired(a: Action, g: Group) -> String {
    let why = drawable(a.unwired().expect("the caller is an unwired verb"));
    match g.fetch_route() {
        Some(cmd) => format!("{why}. `{cmd}` does it from a terminal."),
        None => why,
    }
}

/// The window's font draws ASCII, an em dash, an ellipsis and a section mark, and nothing else
/// (§6.7) — so `·` is `.notdef`, an empty square, and **nothing in `.slint` can ask whether a
/// glyph exists**.
///
/// The model words several sentences this page draws verbatim and joins lists in three of them
/// with `·`: `inspect::flash`'s Good verdict, `inspect::rom_facts`, `inspect::ipsw_facts` and
/// `nor::Source::describe`. §6.7's sweep reads `tools/ipod-gui/src` only, so none of those is
/// covered by it, and every one of them would have drawn empty squares here.
///
/// A comma is this program's own answer for a joined list — `rail::Tool::remedy` says so, `parts
/// .slint:155` joins with one, and `ipod-boot syscfg` prints its record tags with one. So the
/// substitution is that answer applied at the boundary, in one place, rather than a widened glyph
/// set. Everything else passes through untouched, which is what leaves
/// `no_line_carries_a_glyph_the_window_cannot_draw` able to go red on the next one.
fn drawable(s: &str) -> String {
    s.replace(" \u{b7} ", ", ").replace('\u{b7}', ",")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The markup this crate actually compiles, read from disk.
    ///
    /// `build.rs` compiles `ui/window.slint` and only what it imports. `ui/preview.slint` is
    /// imported by nothing, so a test that asserted against it would be measuring a file the
    /// program does not carry.
    fn markup(name: &str) -> String {
        let path =
            std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/ui")).join(name);
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
    }

    /// Whitespace removed, so a reflow of the markup is not a failure and a change of *number* is.
    fn dense(text: &str) -> String {
        text.chars().filter(|c| !c.is_whitespace()).collect()
    }

    /// **`ALL` holds each variant exactly once**, which is the thing the round trip below cannot
    /// tell you.
    ///
    /// Measured, by mutation, before this existed: replace `Group::Firmware` in `ALL` with a
    /// second `Group::Ipods` and the round trip stays **green** — it walks `ALL`, so it walks
    /// `Ipods` twice and never walks `Firmware` at all. The population and the thing under test
    /// were the same list. `[Group; 6]` is fixed-length, so a variant that goes missing must leave
    /// a duplicate behind, and that is what this counts.
    ///
    /// The one shape neither this nor the round trip can see is a variant **added** to the enum and
    /// not to `ALL`: the array is still full and still distinct. `as_i32`'s `expect` is what
    /// catches that, loudly, the first time anything asks for its ordinal.
    fn each_variant_once<T: Ord + Copy + std::fmt::Debug>(all: &[T]) {
        let mut seen: Vec<T> = all.to_vec();
        seen.sort();
        seen.dedup();
        assert_eq!(
            seen.len(),
            all.len(),
            "{all:?} does not hold each variant exactly once, so one ordinal answers for two \
             controls and one variant has no ordinal at all"
        );
    }

    /// **Every ordinal survives the round trip, and nothing outside the list decodes.**
    ///
    /// This is the whole reason the vocabulary is a Rust enum rather than a Slint one: an `int`
    /// the markup sends that no variant answers to must be a no-op. `-1` is the markup's own
    /// "nothing" for `parts-detail-of`, and one past the end is what a renumbered markup sends.
    #[test]
    fn every_ordinal_round_trips_and_an_unknown_one_decodes_to_nothing() {
        each_variant_once(&Group::ALL);
        each_variant_once(&Action::ALL);
        each_variant_once(&Kind::ALL);
        each_variant_once(&RowAction::ALL);
        for g in Group::ALL {
            assert_eq!(Group::from_i32(g.as_i32()), Some(g), "{g:?}");
        }
        for a in Action::ALL {
            assert_eq!(Action::from_i32(a.as_i32()), Some(a), "{a:?}");
        }
        for k in Kind::ALL {
            assert_eq!(Kind::from_i32(k.as_i32()), Some(k), "{k:?}");
        }
        for a in RowAction::ALL {
            assert_eq!(RowAction::from_i32(a.as_i32()), Some(a), "{a:?}");
        }
        assert_eq!(Group::from_i32(-1), None);
        assert_eq!(Group::from_i32(Group::ALL.len() as i32), None);
        assert_eq!(Action::from_i32(-1), None);
        assert_eq!(Action::from_i32(Action::ALL.len() as i32), None);
        assert_eq!(Kind::from_i32(-1), None);
        assert_eq!(Kind::from_i32(Kind::ALL.len() as i32), None);
        assert_eq!(RowAction::from_i32(-1), None);
        assert_eq!(RowAction::from_i32(RowAction::ALL.len() as i32), None);
    }

    /// **The two ordinals the shipping markup writes are where it thinks they are.**
    ///
    /// Both halves, because either alone is half an instrument. The `assert_eq!` pins the Rust
    /// order; the markup search proves the number in this file is the number the markup sends,
    /// rather than a number somebody wrote down once. `ui/devices.slint` is deliberately not
    /// searched: it writes no ordinal at all, every act it fires being `root.d.action`.
    #[test]
    fn the_two_pinned_ordinals_are_where_the_shipping_markup_writes_them() {
        assert_eq!(Kind::Mounted.as_i32(), 0, "the reserved plugged-in-iPod row");
        assert_eq!(RowAction::Remove.as_i32(), 2, "the row's own `Remove` control");

        let parts = dense(&markup("parts.slint"));
        assert!(
            parts.contains(&format!("root.r.kind=={}&&!root.r.expandable", Kind::Mounted.as_i32())),
            "`parts.slint` no longer tests `kind` against `Kind::Mounted`'s ordinal, so either the \
             enum was renumbered under it or the reserved row is decided somewhere else now"
        );
        assert!(
            parts.contains(&format!("root.act({},root.r.id)", RowAction::Remove.as_i32())),
            "`parts.slint`'s `Remove` control no longer fires `RowAction::Remove`'s ordinal; a \
             renumbering here re-aims a live control at a different act"
        );

        let devices = dense(&markup("devices.slint"));
        assert!(
            !devices.contains("root.act(") || devices.contains("root.act(root.d.action,"),
            "`devices.slint` has begun writing an ordinal of its own; it pinned none, and a second \
             place that writes one is a second vocabulary"
        );
    }

    // ─── Fixtures ───────────────────────────────────────────────────────────────────────────────

    use eapp_loader::settings::{Device, Disk, Item, Provenance, Verification};

    /// Every capability on. The running build has **none** of the first four — see `main::caps` —
    /// so this is the arm that proves a control goes live rather than being disabled for ever.
    fn all_on() -> Caps {
        Caps {
            file_picker: true,
            drop_target: true,
            clipboard: true,
            reveal: true,
            devices_page: true,
            download: true,
            composer: true,
        }
    }

    fn rom(name: &str, path: &str) -> Item {
        Item {
            name: name.into(),
            what: Resource::Firmware(nor::Source::File(PathBuf::from(path))),
            from: Some(Provenance::Dumped),
        }
    }

    fn synthetic(name: &str, seed: u64) -> Item {
        Item {
            name: name.into(),
            what: Resource::Firmware(nor::Source::Synthetic {
                model: "MA146".into(),
                seed,
                serial: None,
                guid: None,
                splash: None,
            }),
            from: None,
        }
    }

    fn filed(name: &str, what: Resource, from: Provenance) -> Item {
        Item {
            name: name.into(),
            what,
            from: Some(from),
        }
    }

    fn device(name: &str, firmware: &str, disk: Option<&str>) -> Device {
        Device {
            name: name.into(),
            firmware: firmware.into(),
            disk: disk.map(str::to_string),
            ..Device::default()
        }
    }

    /// One of each kind, so a sweep over "everything this producer can emit" is over something.
    fn library() -> Settings {
        Settings {
            resources: vec![
                rom("Black 5.5G", "/tmp/ipod-parts-nowhere/rom.bin"),
                synthetic("From my 30 GB", 0x4f2a),
                filed(
                    "iPod_25.1.3.ipsw",
                    Resource::Installer(PathBuf::from("/tmp/ipod-parts-nowhere/a.ipsw")),
                    Provenance::Fetched {
                        verified: Verification::Sha256,
                    },
                ),
                filed(
                    "iPod_20.1.3.ipsw",
                    Resource::Installer(PathBuf::from("/tmp/ipod-parts-nowhere/b.ipsw")),
                    Provenance::Fetched {
                        verified: Verification::SizeOnly,
                    },
                ),
                filed(
                    "Rockbox's bootloader",
                    Resource::Bootloader(PathBuf::from("/tmp/ipod-parts-nowhere/loader.bin")),
                    Provenance::Fetched {
                        verified: Verification::Sha256,
                    },
                ),
                filed(
                    "Rockbox 4.0",
                    Resource::Software(PathBuf::from("/tmp/ipod-parts-nowhere/rockbox.zip")),
                    Provenance::Built,
                ),
            ],
            disks: vec![
                Disk {
                    name: "my-5.5g.img".into(),
                    path: PathBuf::from("/tmp/ipod-parts-nowhere/my-5.5g.img"),
                    built_from: Some("iPod_25.1.3.ipsw".into()),
                    installed: vec!["Rockbox 4.0".into()],
                },
                Disk {
                    name: "rockbox-test.img".into(),
                    path: PathBuf::from("/tmp/ipod-parts-nowhere/rockbox-test.img"),
                    built_from: None,
                    installed: Vec::new(),
                },
            ],
            devices: vec![
                device("My 5.5G", "Black 5.5G", Some("my-5.5g.img")),
                Device {
                    parked_at: Some(settings::now_unix().saturating_sub(240)),
                    ..device("Second", "From my 30 GB", Some("rockbox-test.img"))
                },
            ],
            ..Settings::default()
        }
    }

    /// A scratch path of this test's own. Never inside the operator's data directory.
    fn scratch(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ipod-parts-{}-{tag}", std::process::id()));
        let _ = std::fs::create_dir_all(&d);
        d
    }

    /// A 1 MiB NOR that `inspect::flash` passes, written where a `Source::File` can find it.
    ///
    /// **`nor::synthesise` writes no `flsh` image directory**, so `inspect::flash` calls this
    /// program's own generated ROM `Wrong` — *1 MiB and a plausible reset vector, but no `flsh`
    /// image directory at 0xffe00*. Measured here rather than assumed, and it is not this file's
    /// to fix; what it means for the fixture is that a synthesised NOR cannot stand in for a dump.
    ///
    /// So the directory is written in. Four images at the 5G's own load address is what a retail
    /// dump carries, and the Good verdict it produces is also the one sentence this page renders
    /// verbatim that carries U+00B7 — which is what gives
    /// `no_line_carries_a_glyph_the_window_cannot_draw` something to measure.
    fn write_nor(at: &Path) {
        let src = nor::Source::Synthetic {
            model: "MA146".into(),
            seed: 0x4f2a,
            serial: None,
            guid: None,
            splash: None,
        };
        let mut nor_bytes = src.bytes().expect("a synthesised NOR");
        assert_eq!(nor_bytes.len() as u64, inspect::NOR_LEN, "a 5G/5.5G NOR is exactly 1 MiB");
        // The record layout `inspect::nor_images` reads: `hslf`, then the tag as a little-endian
        // u32 of four characters — so the bytes go in backwards — then `addr` at +0x14.
        const DIRECTORY: usize = 0x000f_fe00;
        for (i, tag) in ["disk", "diag", "logo", "vmcs"].iter().enumerate() {
            let rec = DIRECTORY + i * 40;
            nor_bytes[rec..rec + 4].copy_from_slice(b"hslf");
            let backwards: Vec<u8> = tag.bytes().rev().collect();
            nor_bytes[rec + 4..rec + 8].copy_from_slice(&backwards);
            nor_bytes[rec + 0x14..rec + 0x18]
                .copy_from_slice(&inspect::LOAD_ADDR_5G.to_le_bytes());
        }
        std::fs::write(at, nor_bytes).expect("writing the NOR");
        assert!(
            inspect::flash(at).ok(),
            "the fixture does not pass the check the body is built out of: {}",
            inspect::flash(at).text()
        );
    }

    fn view_of(p: &mut Parts, s: &Settings, caps: Caps) -> View {
        let mut seen = Presence::new();
        p.view(s, &mut seen, caps, false, None)
    }

    fn row_named<'a>(v: &'a View, name: &str) -> &'a PartView {
        v.rows
            .iter()
            .find(|r| r.name == name)
            .unwrap_or_else(|| panic!("no row named {name}; the page drew {:?}", names(v)))
    }

    fn names(v: &View) -> Vec<&str> {
        v.rows.iter().map(|r| r.name.as_str()).collect()
    }

    fn group(v: &View, g: Group) -> &GroupView {
        v.groups.iter().find(|x| x.group == g).expect("six groups")
    }

    // ─── The page ───────────────────────────────────────────────────────────────────────────────

    /// **Six groups, always, and an empty one keeps everything but its rows.**
    ///
    /// The shipped window rendered four and dropped `Resource::Bootloader` on the floor, which is
    /// §3's own named complaint: a clean-looking Parts page is not evidence that no bootloader is
    /// filed. It asserts the six by *drawing* them out of an empty library rather than by reading
    /// `Group::ALL`, because `ALL` is the population the view is built from and a test over it
    /// would be the list checking itself.
    #[test]
    fn all_six_groups_are_drawn_out_of_an_empty_library_and_each_says_what_belongs_in_it() {
        let s = Settings::default();
        let v = view_of(&mut Parts::new(), &s, Caps::default());
        assert_eq!(v.groups.len(), 6, "{:?}", v.groups.iter().map(|g| g.group).collect::<Vec<_>>());
        let mut headings: Vec<&str> = v.groups.iter().map(|g| g.heading.as_str()).collect();
        let drawn = headings.len();
        headings.sort_unstable();
        headings.dedup();
        assert_eq!(headings.len(), drawn, "two groups share a heading: {headings:?}");
        for g in &v.groups {
            assert_eq!(g.count, 0, "{} counted rows in an empty library", g.heading);
            assert!(!g.heading.is_empty(), "{:?} has no heading", g.group);
            assert!(!g.verb.is_empty(), "{} does not say what its kind is for", g.heading);
            assert!(
                g.empty.len() > 40,
                "{}'s empty line is `{}` — §9.1 asks what belongs here, never a bare nothing-here",
                g.heading,
                g.empty
            );
            assert!(g.a.is_some(), "{} offers no verb at all", g.heading);
        }
        // The one group that offers a single verb, and the five that offer two. §11.4's table.
        assert!(group(&v, Group::Snapshots).b.is_none());
        for g in [Group::Ipods, Group::Firmware, Group::Bootloaders, Group::Software, Group::Disks] {
            assert!(group(&v, g).b.is_some(), "{g:?} lost its second verb");
        }
    }

    /// **A bootloader is filed in a group of its own**, which is the thing the shipped window did
    /// not do.
    #[test]
    fn a_bootloader_is_its_own_group_and_is_not_swept_in_with_the_software() {
        let v = view_of(&mut Parts::new(), &library(), Caps::default());
        assert_eq!(row_named(&v, "Rockbox's bootloader").group, Group::Bootloaders);
        assert_eq!(row_named(&v, "Rockbox's bootloader").kind, Kind::Bootloader);
        assert_eq!(group(&v, Group::Bootloaders).count, 1);
        assert_eq!(group(&v, Group::Software).count, 1, "the bootloader was counted as software");
        // And each of the six holds what §11.4's table says it holds.
        for (g, n) in [
            (Group::Ipods, 2),
            (Group::Firmware, 2),
            (Group::Bootloaders, 1),
            (Group::Software, 1),
            (Group::Disks, 2),
            (Group::Snapshots, 1),
        ] {
            assert_eq!(group(&v, g).count, n, "{g:?}");
        }
    }

    /// §11.4: **the plugged-in-iPod row is reserved, always** — a line rather than a control, and
    /// the group does not *grow* a row when one appears.
    #[test]
    fn the_reserved_ipod_row_is_a_line_that_no_count_includes() {
        let v = view_of(&mut Parts::new(), &library(), Caps::default());
        let reserved = v
            .rows
            .iter()
            .find(|r| r.kind == Kind::Mounted)
            .expect("the reserved row is drawn out of every library");
        // `parts.slint:166` draws it inert on exactly this pair.
        assert_eq!(reserved.kind.as_i32(), 0);
        assert!(!reserved.expandable, "the reserved row would be drawn as a control");
        assert!(!reserved.removable);
        assert!(!reserved.fact.is_empty(), "it claims to be looking and is not");
        assert_eq!(group(&v, Group::Ipods).count, 2, "the reserved row was counted as an iPod");
    }

    // ─── §9.4, the invariant `primitives.slint` states ──────────────────────────────────────────

    /// Every control this producer can emit, in every combination of build and phase it can be
    /// drawn in, and one rule over all of them.
    ///
    /// `primitives.slint:368` declares a non-empty `reason` as the invariant on a disabled
    /// control, and the shipped Settings page draws three rows two of which are disabled with an
    /// **empty** reason. The sweep runs over both `Caps` arms because a rule checked in one is a
    /// rule checked where nothing is refused.
    #[test]
    fn every_disabled_control_states_its_reason() {
        let s = library();
        for caps in [Caps::default(), all_on()] {
            for busy in [false, true] {
                let mut p = Parts::new();
                let mut seen = Presence::new();
                // Every expandable row opened in turn, so the sweep sees every act as well as
                // every verb.
                let ids: Vec<i32> = {
                    let v = p.view(&s, &mut seen, caps, busy, None);
                    v.rows.iter().filter(|r| r.expandable).map(|r| r.id).collect()
                };
                assert!(!ids.is_empty(), "no row opens, so no act was swept");
                for id in ids {
                    p.open_row(&s, id, true);
                    let v = p.view(&s, &mut seen, caps, busy, None);
                    let mut checked = 0usize;
                    for g in &v.groups {
                        for (_, f) in [&g.a, &g.b].into_iter().flatten() {
                            checked += 1;
                            assert!(!f.label.is_empty(), "{}: a verb with no label", g.heading);
                            assert!(
                                f.enabled || !f.reason.is_empty(),
                                "{}'s `{}` is disabled and says nothing",
                                g.heading,
                                f.label
                            );
                            assert!(
                                !f.enabled || f.reason.is_empty(),
                                "{}'s `{}` is pressable and still wears a refusal: {}",
                                g.heading,
                                f.label,
                                f.reason
                            );
                        }
                    }
                    for d in &v.detail {
                        let Some((_, f)) = &d.action else { continue };
                        checked += 1;
                        assert!(!f.label.is_empty(), "an act with no label");
                        assert!(
                            f.enabled || !f.reason.is_empty(),
                            "the act `{}` is disabled and says nothing",
                            f.label
                        );
                    }
                    for r in v.rows.iter().filter(|r| r.removable) {
                        // `parts.slint:238` binds `enabled` to `locked-by == ""` and `reason` to
                        // `locked-by`, so the invariant is the same one asked of one field.
                        checked += 1;
                        assert!(
                            !r.remove_consequence.is_empty(),
                            "{}'s Remove arms and says nothing about what the second press does",
                            r.name
                        );
                    }
                    assert!(checked > 10, "only {checked} controls were swept at id {id}");
                }
            }
        }
    }

    /// **`DetailRow.machine-rule` has one producer**, and it is the `Detail`'s own field.
    ///
    /// `to_detail` reads it there; a `FixRow` that still carried its own copy would be a second
    /// field holding one fact on its way to one pixel. So `act` moves it rather than copying it,
    /// and this is the assertion that the move happened.
    #[test]
    fn the_machine_rule_on_a_line_has_exactly_one_producer() {
        let dir = scratch("one-producer");
        let at = dir.join("rom.bin");
        write_nor(&at);
        let mut s = library();
        s.resources.push(Item {
            name: "the dump".into(),
            what: Resource::Firmware(nor::Source::File(at.clone())),
            from: Some(Provenance::Dumped),
        });
        let mut p = Parts::new();
        let mut seen = Presence::new();
        let ids: Vec<i32> = p
            .view(&s, &mut seen, Caps::default(), false, None)
            .rows
            .iter()
            .filter(|r| r.expandable)
            .map(|r| r.id)
            .collect();
        let (mut acts, mut rules) = (0usize, 0usize);
        for id in ids {
            p.open_row(&s, id, true);
            for d in p.view(&s, &mut seen, Caps::default(), false, None).detail {
                if let Some((_, f)) = &d.action {
                    acts += 1;
                    assert!(
                        !f.machine_rule,
                        "an act's `FixRow` still carries a `machine_rule` the `Detail` also \
                         carries: {f:?}"
                    );
                }
                rules += usize::from(d.machine_rule);
            }
        }
        assert!(acts > 5, "only {acts} acts were swept");
        // The control: the field reaches the view at all, so `false` above means something.
        assert!(rules > 0, "no line is a machine rule, so nothing produced the field");

        // **And the half that can actually fail.** Measured: replacing the move in `act` with a
        // copy leaves the sweep above green, because nothing on this page hands `act` a `FixRow`
        // carrying the flag — every row act asks a capability whose refusal is a project state,
        // and `Next::Retry`, the one machine rule in that function, is a group verb. A sweep that
        // cannot go red is not an instrument, so the move is asserted where it happens.
        let moved = act(
            RowAction::Reveal,
            FixRow {
                machine_rule: true,
                ..FixRow::default()
            },
        );
        assert!(moved.machine_rule, "the line lost the fact on the way in");
        assert!(
            !moved.action.expect("an act").1.machine_rule,
            "the `FixRow` kept a second copy of `machine-rule`, so two fields reach one pixel"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The verbs an absent **capability** refuses are refused **in `rail::Next`'s own words**, so
    /// the Rail and this page cannot word one absent capability two ways.
    ///
    /// `Fetch…` is deliberately absent from this test now: no capability refuses it, and the
    /// capability that used to be asked said yes.
    /// `every_verb_with_no_mechanism_is_drawn_disabled_with_a_route` is where it went.
    #[test]
    fn a_verb_this_build_cannot_perform_wears_rails_own_sentence() {
        let s = library();
        let v = view_of(&mut Parts::new(), &s, Caps::default());
        let provide = &group(&v, Group::Firmware).b.as_ref().expect("Provide…").1;
        assert!(!provide.enabled, "this build has neither a picker nor a drop target");
        assert_eq!(provide.reason, Next::Provide.reason());
        // And the Composer exists, so the two verbs that need one are drawn live.
        let synth = &group(&v, Group::Ipods).b.as_ref().expect("Synthesise…").1;
        assert!(!synth.enabled, "the all-off fixture has no Composer either");
        let live = view_of(&mut Parts::new(), &s, all_on());
        let synth = &group(&live, Group::Ipods).b.as_ref().expect("Synthesise…").1;
        assert!(synth.enabled, "a build with a Composer still refuses Synthesise…");
        assert!(synth.reason.is_empty());
    }

    /// **A verb with no mechanism behind it is drawn DISABLED, says why, and names a route.**
    ///
    /// §14.1, and `Fetch…` was the counter-example: [`Action::needs`] answered `Some(Next::Retry)`,
    /// which asks *is curl on this computer*, `caps.download` answers that by running
    /// `curl --version`, and so on every computer that has curl the control was **blue on all three
    /// groups that offer it** and every press failed. A live control that only refuses is the same
    /// defect as a callback nobody registered.
    ///
    /// **Three claims, and the third is the one worth having.** It is drawn disabled in the fixture
    /// where every capability — `download` included — answers yes, so curl cannot be what greys it
    /// out; its sentence is a project state naming a real command rather than a machine rule blaming
    /// the computer; and **the drawn refusal and the pressed one are the same words**, which is what
    /// stops one refusal from coming to be worded twice.
    #[test]
    fn every_verb_with_no_mechanism_is_drawn_disabled_with_a_route() {
        let s = library();
        // `all_on()` is the fixture where every capability answers yes — including `download`, so
        // curl cannot be what greys this out.
        assert!(all_on().download, "the fixture that proves the point has no curl in it");
        let v = view_of(&mut Parts::new(), &s, all_on());
        let mut checked = 0;
        for g in Group::ALL {
            if !g.offers(Action::Fetch) {
                assert_eq!(g.fetch_route(), None, "{g:?} names a route to a verb it does not offer");
                continue;
            }
            let row = &group(&v, g).a.as_ref().expect("Fetch… is the first of the pair").1;
            assert_eq!(row.label, Action::Fetch.label());
            assert!(!row.enabled, "`Fetch…` is drawn live on {g:?} and pressing it only refuses");
            assert!(!row.reason.is_empty(), "a disabled control with nothing to say (§9.4)");
            assert!(
                !row.machine_rule,
                "`Fetch…` on {g:?} blames the computer; nothing about this computer refuses"
            );
            let cmd = g.fetch_route().expect("a group that offers Fetch… has a route");
            assert_eq!(row.escape, cmd, "{g:?} greys a verb out and offers no way round it");
            assert!(
                !row.reason.contains("curl"),
                "{g:?} still names the capability that made it live: {}",
                row.reason
            );

            // …and the arm that runs if a press arrives anyway says the same thing.
            let mut s2 = s.clone();
            let said = Parts::new()
                .group_action(&mut s2, g, Action::Fetch)
                .expect_err("nothing here fetches yet");
            assert!(
                said.starts_with(&row.reason),
                "{g:?} draws `{}` and refuses with `{said}`",
                row.reason
            );
            assert!(said.contains(cmd), "the press refused without naming the route: {said}");
            assert_eq!(s2, s, "a refusal mutated the library");
            checked += 1;
        }
        assert_eq!(checked, 3, "only {checked} groups offer `Fetch…`; §11.4's table says three");
    }

    /// A build owns the drive it is writing. A verb that would start a second one waits — and
    /// `Remove`, which touches no file at all, does not.
    #[test]
    fn a_running_build_stops_a_verb_that_would_start_a_second_one() {
        let s = library();
        let mut p = Parts::new();
        let mut seen = Presence::new();
        let idle = p.view(&s, &mut seen, all_on(), false, None);
        let busy = p.view(&s, &mut seen, all_on(), true, None);
        let verb = |v: &View, g: Group| v.groups.iter().find(|x| x.group == g).unwrap().a.clone().unwrap().1;
        assert!(verb(&idle, Group::Disks).enabled);
        assert!(!verb(&busy, Group::Disks).enabled);
        assert!(!verb(&busy, Group::Disks).reason.is_empty());
        assert!(
            busy.rows.iter().filter(|r| r.removable).all(|r| r.locked_by.is_empty()),
            "a build blocked a Remove, which deletes no file and takes nothing the build is using"
        );
    }

    // ─── §11.4's machine rule ───────────────────────────────────────────────────────────────────

    /// **Nothing the machine is using can be removed while it is running.**
    ///
    /// One rule covering the resource case and the device case, and it is asked again inside
    /// `row_action` rather than trusted from the control — the control was drawn at the last push
    /// and the machine may have started since.
    #[test]
    fn nothing_the_machine_is_using_can_be_removed_while_it_is_running() {
        let mut s = library();
        let mut p = Parts::new();
        let mut seen = Presence::new();

        let free = p.view(&s, &mut seen, Caps::default(), false, None);
        assert!(
            row_named(&free, "Black 5.5G").locked_by.is_empty(),
            "a device that is not running held its ROM"
        );

        let held = p.view(&s, &mut seen, Caps::default(), false, Some("My 5.5G"));
        let rom = row_named(&held, "Black 5.5G");
        assert!(
            rom.locked_by == "My 5.5G is running",
            "the ROM the machine boots is removable while it runs: {:?}",
            rom.locked_by
        );
        assert!(
            row_named(&held, "From my 30 GB").locked_by.is_empty(),
            "a ROM the machine does not name was held too"
        );

        let id = rom.id;
        let before = s.resources.len();
        let refused = p.row_action(&mut s, RowAction::Remove, id, Some("My 5.5G"));
        assert!(refused.is_err(), "the machine's own ROM was removed out from under it");
        assert_eq!(s.resources.len(), before, "a refusal mutated the library");

        let done = p.row_action(&mut s, RowAction::Remove, id, None);
        assert_eq!(done, Ok(Wrote::Library));
        assert_eq!(s.resources.len(), before - 1);
    }

    // ─── The cursor ─────────────────────────────────────────────────────────────────────────────

    /// **Ids are minted per `(group, name)` and never reused**, so a removal cannot renumber its
    /// neighbours under an open Expand.
    #[test]
    fn an_open_row_survives_its_neighbour_being_removed() {
        let mut s = library();
        let mut p = Parts::new();
        let mut seen = Presence::new();

        let before = p.view(&s, &mut seen, Caps::default(), false, None);
        let watched = row_named(&before, "Rockbox 4.0").id;
        p.open_row(&s, watched, true);
        assert_eq!(p.view(&s, &mut seen, Caps::default(), false, None).detail_of, watched);

        let first = row_named(&before, "Black 5.5G").id;
        assert!(s.remove_resource("Black 5.5G"));
        assert!(p.row_action(&mut s, RowAction::Remove, first, None).is_ok());

        let after = p.view(&s, &mut seen, Caps::default(), false, None);
        assert_eq!(
            row_named(&after, "Rockbox 4.0").id,
            watched,
            "removing a part above renumbered the one below it"
        );
        assert_eq!(after.detail_of, watched, "the open Expand moved to another row");
        assert!(!after.rows.iter().any(|r| r.id == first), "a removed id came back");
    }

    /// The open row leaving the library closes the cursor rather than leaving `detail-of` pointing
    /// at nothing.
    #[test]
    fn removing_the_open_row_closes_it() {
        let mut s = library();
        let mut p = Parts::new();
        let mut seen = Presence::new();
        let id = row_named(&p.view(&s, &mut seen, Caps::default(), false, None), "Rockbox 4.0").id;
        p.open_row(&s, id, true);
        s.resources.retain(|it| it.name != "Rockbox 4.0");
        let v = p.view(&s, &mut seen, Caps::default(), false, None);
        assert_eq!(v.detail_of, -1);
        assert!(v.detail.is_empty());
    }

    /// An id nothing answers to is a no-op — never a panic and never a different row's act.
    #[test]
    fn an_id_nothing_answers_to_acts_on_nothing() {
        let mut s = library();
        let mut p = Parts::new();
        let mut seen = Presence::new();
        let before = s.clone();
        assert_eq!(p.row_action(&mut s, RowAction::Remove, 9_999, None), Ok(Wrote::Nothing));
        assert_eq!(s, before);
        p.open_row(&s, 9_999, true);
        assert_eq!(p.view(&s, &mut seen, Caps::default(), false, None).detail_of, -1);
        // …and a row with no body does not open one.
        let disk = row_named(&p.view(&s, &mut seen, Caps::default(), false, None), "my-5.5g.img").id;
        assert!(!row_named(&p.view(&s, &mut seen, Caps::default(), false, None), "my-5.5g.img").expandable);
        p.open_row(&s, disk, true);
        assert_eq!(p.view(&s, &mut seen, Caps::default(), false, None).detail_of, -1);
    }

    // ─── `used by N`, and what a removal costs ──────────────────────────────────────────────────

    /// §11.4's `used by N` counts **the drives that recorded it as well as the devices that name
    /// it** — an `.ipsw` is named by no device and by every drive built from it, so counting
    /// devices alone reports *used by 0* about the bundle a drive came out of.
    #[test]
    fn used_by_counts_the_drives_that_recorded_it_and_not_only_the_devices() {
        let v = view_of(&mut Parts::new(), &library(), Caps::default());
        assert_eq!(row_named(&v, "iPod_25.1.3.ipsw").used_by, "used by 1");
        assert_eq!(row_named(&v, "Rockbox 4.0").used_by, "used by 1");
        assert_eq!(row_named(&v, "Black 5.5G").used_by, "used by 1");
        assert_eq!(row_named(&v, "iPod_20.1.3.ipsw").used_by, "", "nothing names it");
        assert_eq!(row_named(&v, "my-5.5g.img").used_by, "used by 1");
    }

    /// §11.4: `Remove` **names them before it acts**, and a synthesised iPod additionally shows
    /// the seed, because its identity is regenerable only from it.
    #[test]
    fn the_removal_consequence_names_the_dependents_and_the_seed_before_the_press() {
        let v = view_of(&mut Parts::new(), &library(), Caps::default());
        let used = &row_named(&v, "Black 5.5G").remove_consequence;
        assert!(used.contains("My 5.5G"), "the consequence does not name the device: {used}");
        assert!(used.contains("not deleted"), "it does not say the file survives: {used}");
        let free = &row_named(&v, "iPod_20.1.3.ipsw").remove_consequence;
        assert!(free.contains("Nothing else names it"), "{free}");
        let synth = &row_named(&v, "From my 30 GB").remove_consequence;
        assert!(synth.contains("4f2a"), "a synthesised iPod's seed is not named: {synth}");
    }

    // ─── What a row is allowed to claim ─────────────────────────────────────────────────────────

    /// **Nothing here may construct the word *verified***. A cached file nobody hashed says so.
    #[test]
    fn no_row_claims_a_verification_the_model_did_not_record() {
        let v = view_of(&mut Parts::new(), &library(), Caps::default());
        for r in &v.rows {
            let claims = r.fact.contains("verified");
            let recorded = library()
                .resources
                .iter()
                .find(|it| it.name == r.name)
                .and_then(|it| it.from)
                .is_some_and(|f| f.is_verified());
            assert_eq!(
                claims, recorded,
                "{}'s line is `{}` and the model recorded {recorded}",
                r.name, r.fact
            );
        }
        // The control: the sweep can see the claim, so its silence would mean something.
        assert!(row_named(&v, "iPod_25.1.3.ipsw").fact.contains("verified"));
        assert!(!row_named(&v, "iPod_20.1.3.ipsw").fact.contains("verified"));
    }

    /// A part whose file has left the disk says so on its own row. Nothing else in this window
    /// re-stats the library, and the shipped bench drew a deleted drive as fine for an hour.
    #[test]
    fn a_part_whose_file_has_left_says_so_on_its_own_row() {
        let dir = scratch("present");
        let here = dir.join("rockbox.zip");
        std::fs::write(&here, b"a file").unwrap();
        let s = Settings {
            resources: vec![
                filed("here", Resource::Software(here.clone()), Provenance::Built),
                filed(
                    "gone",
                    Resource::Software(dir.join("not-here.zip")),
                    Provenance::Built,
                ),
            ],
            ..Settings::default()
        };
        let v = view_of(&mut Parts::new(), &s, Caps::default());
        assert!(!row_named(&v, "here").fact.contains("not where it was"), "{:?}", row_named(&v, "here").fact);
        assert!(
            row_named(&v, "gone").fact.contains("not where it was"),
            "a part whose file is gone reads as fine: {:?}",
            row_named(&v, "gone").fact
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ─── The bodies, and what opening one costs ─────────────────────────────────────────────────

    /// **The ROM is read by the press that opens the row, not by the push that draws it.**
    ///
    /// Deleting the file under an open row leaves the body standing, which is what proves the read
    /// is not happening on every push. Re-opening re-reads, and then the verdict changes.
    #[test]
    fn a_rom_is_read_once_by_the_press_that_opens_it() {
        let dir = scratch("rom-read");
        let at = dir.join("rom.bin");
        write_nor(&at);
        let s = Settings {
            resources: vec![Item {
                name: "Black 5.5G".into(),
                what: Resource::Firmware(nor::Source::File(at.clone())),
                from: Some(Provenance::Dumped),
            }],
            ..Settings::default()
        };
        let mut p = Parts::new();
        let mut seen = Presence::new();
        let id = row_named(&p.view(&s, &mut seen, Caps::default(), false, None), "Black 5.5G").id;
        p.open_row(&s, id, true);
        let opened = p.view(&s, &mut seen, Caps::default(), false, None);
        let says = |v: &View, what: &str| v.detail.iter().any(|d| d.value.contains(what) || d.label.contains(what));
        assert!(says(&opened, "Images"), "the image directory was not read: {:?}", opened.detail);
        assert!(says(&opened, "Serial"), "the identity was not read");
        assert!(says(&opened, "Model"), "the model table was not consulted");

        std::fs::remove_file(&at).unwrap();
        let still = p.view(&s, &mut seen, Caps::default(), false, None);
        assert_eq!(
            still.detail, opened.detail,
            "the body changed when the file went away, so it is being re-read on every push"
        );

        // Presence is a per-pass cache, so a fresh one is what a fresh pass would hold.
        let mut fresh = Presence::new();
        p.open_row(&s, id, true);
        let reread = p.view(&s, &mut fresh, Caps::default(), false, None);
        assert!(
            reread.detail.iter().any(|d| d.machine_rule && d.value.contains("cannot read")),
            "re-opening did not re-read: {:?}",
            reread.detail
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// §11.4: the identity is **masked**, and `Show` reveals it. Per open row, so closing it
    /// re-masks — a screenshot of a page nobody is looking at must not carry an identifier.
    #[test]
    fn the_roms_identity_is_masked_until_show_is_pressed_and_re_masks_when_the_row_closes() {
        let dir = scratch("mask");
        let at = dir.join("rom.bin");
        write_nor(&at);
        let mut s = Settings {
            resources: vec![Item {
                name: "Black 5.5G".into(),
                what: Resource::Firmware(nor::Source::File(at.clone())),
                from: Some(Provenance::Dumped),
            }],
            ..Settings::default()
        };
        let mut p = Parts::new();
        let mut seen = Presence::new();
        let id = row_named(&p.view(&s, &mut seen, Caps::default(), false, None), "Black 5.5G").id;
        p.open_row(&s, id, true);

        let guid = |v: &View| {
            v.detail
                .iter()
                .find(|d| d.label == "FireWire GUID")
                .map(|d| d.value.clone())
                .expect("the GUID line")
        };
        let masked = guid(&p.view(&s, &mut seen, Caps::default(), false, None));
        assert!(masked.contains('•') || masked.contains('*') || masked.len() < 16, "{masked}");

        assert_eq!(p.row_action(&mut s, RowAction::ShowIdentity, id, None), Ok(Wrote::Nothing));
        let shown = guid(&p.view(&s, &mut seen, Caps::default(), false, None));
        assert_ne!(shown, masked, "Show revealed nothing");
        assert_eq!(shown.len(), 16, "a FireWire GUID is sixteen hex digits: {shown}");

        p.open_row(&s, id, false);
        p.open_row(&s, id, true);
        assert_eq!(
            guid(&p.view(&s, &mut seen, Caps::default(), false, None)),
            masked,
            "closing and re-opening the row left the identity revealed"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **By contents, never by name.** People rename downloads and browsers add `(1)`, so a file
    /// called `iPod_25.1.3.ipsw` is not evidence of anything.
    #[test]
    fn an_ipsw_is_identified_by_its_contents_and_never_by_what_it_is_called() {
        let dir = scratch("ipsw");
        let at = dir.join("iPod_25.1.3.ipsw");
        std::fs::write(&at, b"not an Apple bundle").unwrap();
        let s = Settings {
            resources: vec![filed(
                "iPod_25.1.3.ipsw",
                Resource::Installer(at.clone()),
                Provenance::Fetched {
                    verified: Verification::None,
                },
            )],
            ..Settings::default()
        };
        let mut p = Parts::new();
        let mut seen = Presence::new();
        let id = row_named(&p.view(&s, &mut seen, Caps::default(), false, None), "iPod_25.1.3.ipsw").id;
        p.open_row(&s, id, true);
        let v = p.view(&s, &mut seen, Caps::default(), false, None);
        let line = v
            .detail
            .iter()
            .find(|d| d.label == "Identified by contents")
            .expect("§11.4's by-contents line");
        assert_eq!(line.value, firmware::Provenance::Unrecognised.line());
        assert!(
            v.detail.iter().any(|d| d.value.contains("does not match any firmware Apple published")),
            "an unrecognised bundle is allowed and says so; it did not"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// §11.4's `Show its boot screen ›`, at the panel's own size.
    #[test]
    fn the_boot_screen_preview_is_the_panels_own_size_and_toggles() {
        let s = {
            let mut s = Settings::default();
            s.resources.push(synthetic("From my 30 GB", 0x4f2a));
            s
        };
        let mut owned = s.clone();
        let mut p = Parts::new();
        let mut seen = Presence::new();
        let id = row_named(&p.view(&s, &mut seen, Caps::default(), false, None), "From my 30 GB").id;
        p.open_row(&s, id, true);
        assert!(p.view(&s, &mut seen, Caps::default(), false, None).preview.is_none());

        assert_eq!(p.row_action(&mut owned, RowAction::ShowBootScreen, id, None), Ok(Wrote::Nothing));
        let v = p.view(&s, &mut seen, Caps::default(), false, None);
        let shot = v.preview.as_ref().expect("a framebuffer");
        assert_eq!((shot.w as usize, shot.h as usize), (crate::emu::FB_W, crate::emu::FB_H));
        assert_eq!(shot.rgb.len(), shot.w as usize * shot.h as usize * 3);
        assert!(shot.rgb.iter().any(|b| *b != 0), "the mark was not drawn");
        assert!(shot.rgb.contains(&0), "every pixel is lit, so nothing was drawn on it");

        assert_eq!(p.row_action(&mut owned, RowAction::ShowBootScreen, id, None), Ok(Wrote::Nothing));
        assert!(p.view(&s, &mut seen, Caps::default(), false, None).preview.is_none());
        assert_eq!(owned, s, "showing a picture wrote to the library");
    }

    // ─── The group verbs that act ───────────────────────────────────────────────────────────────

    /// §11.4's Snapshots verb: **it forgets the time and deletes nothing.**
    #[test]
    fn discarding_the_parked_machines_forgets_the_time_and_deletes_nothing() {
        let mut s = library();
        s.devices[0].parked_at = Some(settings::now_unix());
        let mut p = Parts::new();
        assert_eq!(s.devices.iter().filter(|d| d.parked_at.is_some()).count(), 2);

        assert_eq!(
            p.group_action(&mut s, Group::Snapshots, Action::Discard),
            Ok(Wrote::Library)
        );
        assert!(s.devices.iter().all(|d| d.parked_at.is_none()));
        assert_eq!(s.devices.len(), 2, "discarding a park removed a device");
        // Nothing left to do, and a second press says so rather than reporting a write.
        assert!(p.group_action(&mut s, Group::Snapshots, Action::Discard).is_err());
    }

    /// The two ordinals of `parts-group-action(g, a)` travel independently, so they can arrive
    /// paired with each other wrongly. A pair the table does not name acts on nothing.
    #[test]
    fn a_verb_a_group_does_not_offer_acts_on_nothing() {
        let mut s = library();
        let mut p = Parts::new();
        let before = s.clone();
        assert!(p.group_action(&mut s, Group::Snapshots, Action::Fetch).is_err());
        assert!(p.group_action(&mut s, Group::Ipods, Action::Discard).is_err());
        assert_eq!(s, before, "a mismatched pair mutated the library");
        // …and a verb drawn DISABLED wears the Rail's own sentence for the missing capability.
        assert_eq!(
            p.group_action(&mut s, Group::Firmware, Action::Provide),
            Err(Next::Provide.reason().to_string())
        );
        // **A verb drawn LIVE must not.** `Fetch…` is blue exactly when `caps.download` is true,
        // and `Next::Retry`'s sentence is *curl is not on this computer* — so the press that only
        // happened because curl is there used to answer that it is not.
        let fetch = p
            .group_action(&mut s, Group::Firmware, Action::Fetch)
            .expect_err("nothing here fetches yet");
        assert!(
            !fetch.contains("not on this computer"),
            "a live control refuses by naming the capability that made it live: {fetch}"
        );
        assert!(fetch.contains("ipod-boot firmware get"), "§9.4 wants a real route: {fetch}");
        // …and the two the Composer answers say so, rather than claiming it does not exist.
        for (g, a) in [(Group::Ipods, Action::Synthesise), (Group::Disks, Action::Build)] {
            let why = p.group_action(&mut s, g, a).expect_err("the Composer's, not the library's");
            assert!(
                why.contains("opens the Composer"),
                "{a:?} refuses with something other than its route: {why}"
            );
            assert!(
                !why.contains("no Composer in this build"),
                "{a:?} still denies the page it is about to open: {why}"
            );
        }
        assert_eq!(s, before);
    }

    // ─── §6.7, over the strings that never pass through a source file ───────────────────────────

    /// **The closed glyph set, applied to what the window actually draws.**
    ///
    /// `geometry.rs`'s sweep reads *string literals in source* and cannot see this: the ROM body's
    /// longest lines are `inspect::flash`'s verdict and `inspect::ipsw_facts`' joined lists, which
    /// are built in `eapp-loader` — a crate that sweep does not read — and joined with `·`, which
    /// is not in the set and renders as an empty square. So this asks the same question of the
    /// finished `View`.
    ///
    /// The three permitted non-ASCII characters are the same three `geometry.rs` permits; they are
    /// written out rather than imported because that module's list is private to its own test
    /// module, and a second copy that disagreed would be caught by the assertion below it.
    #[test]
    fn no_line_carries_a_glyph_the_window_cannot_draw() {
        const DRAWN: [char; 3] = ['—', '…', '§'];
        let dir = scratch("glyphs");
        let at = dir.join("rom.bin");
        write_nor(&at);

        // **The control, and it is the whole reason this test is not decoration**: the model's own
        // verdict for this very file carries the character, so a `drawable` that did nothing would
        // fail below rather than pass silently.
        let raw = inspect::flash(&at);
        assert!(
            raw.text().contains('\u{b7}'),
            "the model's verdict no longer carries the middle dot, so this test is measuring a \
             substitution that has nothing to substitute: {}",
            raw.text()
        );

        let mut s = library();
        s.resources.push(Item {
            name: "the dump".into(),
            what: Resource::Firmware(nor::Source::File(at.clone())),
            from: Some(Provenance::Dumped),
        });
        let mut p = Parts::new();
        let mut seen = Presence::new();
        let ids: Vec<i32> = p
            .view(&s, &mut seen, Caps::default(), false, Some("My 5.5G"))
            .rows
            .iter()
            .filter(|r| r.expandable)
            .map(|r| r.id)
            .collect();
        let mut swept = 0usize;
        for id in ids {
            p.open_row(&s, id, true);
            let v = p.view(&s, &mut seen, Caps::default(), false, Some("My 5.5G"));
            for text in every_string(&v) {
                swept += 1;
                for c in text.chars() {
                    assert!(
                        c.is_ascii() || DRAWN.contains(&c),
                        "a line carries `{c}` (U+{:04X}), which the window's font draws as an \
                         empty square. §6.7's answer for a symbol is a drawn Path, and this string \
                         never passes through a source file so no source sweep can see it:\n  \
                         {text}",
                        c as u32
                    );
                }
            }
        }
        assert!(swept > 100, "only {swept} strings were swept, so the reach is not the page's");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Every string a `View` puts on screen, in one list.
    fn every_string(v: &View) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for g in &v.groups {
            out.extend([g.heading.clone(), g.verb.clone(), g.empty.clone()]);
            for (_, f) in [&g.a, &g.b].into_iter().flatten() {
                out.extend([
                    f.label.clone(),
                    f.reason.clone(),
                    f.escape.clone(),
                    f.consequence.clone(),
                ]);
            }
        }
        for r in &v.rows {
            out.extend([
                r.name.clone(),
                r.fact.clone(),
                r.used_by.clone(),
                r.remove_consequence.clone(),
                r.locked_by.clone(),
            ]);
        }
        for d in &v.detail {
            out.extend([d.label.clone(), d.value.clone()]);
            if let Some((_, f)) = &d.action {
                out.extend([
                    f.label.clone(),
                    f.reason.clone(),
                    f.escape.clone(),
                    f.consequence.clone(),
                ]);
            }
        }
        out.retain(|s| !s.is_empty());
        out
    }

}
