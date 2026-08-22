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
//   - `Kind::Mounted` is **0** — `parts.slint:160`, `inert: root.r.kind == 0 && !root.r.expandable`.
//     §11.4's reserved plugged-in-iPod row is drawn as a line rather than a control, and that
//     comparison is the only place the markup decides anything from a `kind`.
//   - `RowAction::Remove` is **2** — `parts.slint:242`, `root.act(2, root.r.id)`. `Remove` is the
//     row's own control rather than a `Detail`, so it is the one row action the markup fires by
//     number instead of forwarding `DetailRow.action`.
//
// **That is all of it.** Every `Group` and every `Action` travels as `GroupRow.group` /
// `GroupRow.a-action` and comes back through `group-action(int, int)` untouched — `parts.slint:264`
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

/// The six sections of the Parts page, in the order they are drawn.
///
/// Not pinned by any markup — `parts.slint:264` defers to this type by name. Six, always, and an
/// empty one keeps its heading and its verbs (§9.1).
#[allow(dead_code)] // retired when: `Parts::view` builds a `GroupView` per group — the producer, next wave
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Group {
    Ipods,
    Firmware,
    Bootloaders,
    Software,
    Disks,
    Snapshots,
}

#[allow(dead_code)] // retired when: `Parts::group_action` decodes the ordinal `parts-group-action` sends
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
#[allow(dead_code)] // retired when: `Group::actions` names the pair each group offers — the producer, next wave
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Action {
    AddDump,
    Synthesise,
    Fetch,
    Provide,
    Build,
    Discard,
}

#[allow(dead_code)] // retired when: `Parts::group_action` decodes the ordinal `parts-group-action` sends
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
/// **`Mounted` is 0 and the markup depends on it** — `parts.slint:160`. The rest is ours: a `kind`
/// other than 0 reaches the markup only as a value it stores and hands back.
#[allow(dead_code)] // retired when: `Parts::view` sets `PartView::kind` from the library's resource kinds
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

#[allow(dead_code)] // retired when: `to_part` flattens `PartView::kind` onto `PartRow.kind`
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
/// **`Remove` is 2 and the markup depends on it** — `parts.slint:242`. Everything else travels as
/// `DetailRow.action`, which Rust wrote, so the rest of the order is ours: the three a part can
/// take, then the three a device can, then the two that need something drawn.
#[allow(dead_code)] // retired when: `Parts::row_action` and `Devices::row_action` decode what the two `row-action` callbacks send
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
}

#[allow(dead_code)] // retired when: `to_detail` turns a `Detail`'s action into `DetailRow.action` for a page that draws one
impl RowAction {
    pub const ALL: [RowAction; 8] = [
        RowAction::Reveal,
        RowAction::CopyPath,
        RowAction::Remove,
        RowAction::PowerOff,
        RowAction::Start,
        RowAction::Edit,
        RowAction::Rename,
        RowAction::ShowBootScreen,
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
/// cannot lose its reason on the way across, which `primitives.slint:360` states as the invariant.
///
/// **`machine_rule` is the line's, and the `FixRow`'s copy of it is deliberately not read.**
/// `DetailRow` has exactly one `machine-rule` and the markup binds it twice — to the `Pressable`
/// when there is an act (`parts.slint:63`, `devices.slint:56`) and to the paragraph when there is
/// not. One property, so one producer: this field. Reading the `FixRow`'s as well would be two
/// spellings of one fact arriving at the same pixel.
#[allow(dead_code)] // retired when: `Parts::detail` and `Devices::view` build these — the producers, next wave
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
#[allow(dead_code)] // retired when: a producer's `&mut self` method returns one and `main.rs` matches on it
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wrote {
    Nothing,
    Library,
}

/// The Parts page's whole state: **a cursor, not a copy.** Everything drawn is recomputed from
/// `Settings` on every push, which is what stops it going stale — the same discipline `Composer`
/// holds one `Recipe` under.
///
/// Not an `Option`: this page exists from startup, so there is no absent state to draw.
#[allow(dead_code)] // retired when: `Parts::view` exists and `wire` holds one of these
pub struct Parts;

#[allow(dead_code)] // retired when: `wire` constructs one beside the Composer's cell — the integrator's step, after the producer lands
impl Parts {
    pub fn new() -> Parts {
        Parts
    }
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
}
