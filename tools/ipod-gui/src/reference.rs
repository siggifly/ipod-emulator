//! **Every key this program binds, in one place, reachable from inside the window.**
//!
//! `docs/GUI.md` §22.6 and §16.8. The table of bindings has existed in the design since the window
//! did; what has never existed is a way to read it without leaving the program and opening a
//! markdown file. `nav::Page::Reference` carried the comment *"nothing draws a Reference page"*,
//! the menu's own `Reference` row was disabled saying *"The keyboard table and the stated limits
//! have no page yet."*, and `⌘,` — the platform's own chord for exactly this — opened the menu
//! instead. So a person who could not work out how to drive the wheel had no way to be told.
//!
//! **This is the producer and `ui/reference.slint` draws it.** The page words nothing, which is
//! §21.3's rule one page along: a key typed into markup is a second answer to *what does this
//! program bind*, and a page whose entire subject is that question cannot afford two.
//!
//! # A chord is named by the binding that creates it, and the legend is derived
//!
//! [`Legend::Chord`] holds the literal argument of the `@keys(…)` in `ui/*.slint`, and
//! [`Legend::printed`] turns it into what a person reads. **Nothing types `Ctrl-Cmd-P` anywhere**,
//! and that is the point rather than a tidiness: issue #37 is exactly the failure of typing it.
//!
//! `panel.slint` shipped `@keys(Control + "f")` and `window.slint` shipped `@keys(Control + "p")` —
//! one modifier each, which on Apple platforms is ⌘F and ⌘P — while §16.8, §21.7 and
//! `verbs::panel_row`'s value column all printed `⌃⌘F` and `⌃⌘P`. `Keys::matches` ends
//! `key_event.modifiers == expected_modifiers` (`i-slint-core-1.17.1/input.rs:780`), **exact
//! equality**, so neither documented chord fired anything for three weeks and nothing in the tree
//! could tell. A specification that is correct and unimplemented reads exactly like one that is
//! implemented; what told them apart was the operator pressing the key.
//!
//! Deriving the legend from the declaration closes one half of that — the printed form and the
//! declared form cannot disagree, because there is only one of them.
//! `every_chord_the_markup_declares_is_printed_on_the_reference_page` closes the other half by
//! comparing this table's declarations against every `@keys` the markup actually contains, in both
//! directions: a row here that no markup declares fails, and a binding in markup this table does
//! not print fails.
//!
//! # ⌘ and Ctrl are one binding, and only the printing differs
//!
//! §16.8: on Apple platforms Slint's winit backend delivers ⌘ as `Control`
//! (`i-slint-backend-winit-1.17.1/event_loop.rs:258-274`) and the compiler rejects `Cmd` /
//! `Command` / `Win` outright. So `Control` in a declaration means **⌘ here and Ctrl elsewhere**,
//! and `Meta` means ⌃ here and the platform's own Meta elsewhere. [`Legend::printed`] is the only
//! place in this program that knows that, which is §16.8's own instruction — *write the table with
//! one column and use the platform only for the printed hint.*
//!
//! # What is deliberately not here
//!
//! §16.8's table listed `S` · `⇧S` (a screenshot of the panel, and of the window) and `D` (the
//! Readout page), and **nothing in this program binds any of them** — checked 2026-09-07 against
//! every `key-pressed` handler and every `KeyBinding` in `ui/`. `?` was listed too and was bound to
//! nothing; it is bound to this page now, which is the half of that sentence this commit could make
//! true. The other three are documented-but-unimplemented chords, which is the shape issue #37
//! already cost this project three weeks over — so §16.8 is corrected where it stands rather than
//! reprinted here, and this page prints what the program does.
//!
//! # No toolkit
//!
//! Like every file in this crate but `main.rs` (AGENTS.md §9). It produces [`parts::Detail`]s,
//! which is the shape `MadeOfLine` already renders on two other surfaces — one two-column row
//! construction, three pages, and none of them measuring text a second way.

use crate::parts::Detail;

/// What a person presses, and where the program says so.
pub enum Legend {
    /// A key answered inside a `key-pressed` handler — the arrows, the machine's five letters,
    /// `Esc`, `Tab`, `Enter` / `Space`, `F11` and `?`. There is no declaration to derive from, so
    /// it is printed as written.
    ///
    /// **ASCII, and that is a constraint rather than a style.** §16.6's closed glyph set is `—`,
    /// `…` and `§`; `⌘`, `⌃`, `⇧` and the four arrows are all outside it. Slint takes one
    /// `font-family` per element with no fallback list and nothing in `.slint` can ask whether a
    /// glyph exists, so a legend drawn in `⌘` renders as `.notdef` — an empty square — on any
    /// machine whose UI face lacks it.
    Plain(&'static str),
    /// A `KeyBinding`, named by the exact `@keys(…)` argument that declares it in `ui/*.slint`.
    ///
    /// The printed form is [`Legend::printed`]'s, derived — see this module's header for the three
    /// weeks the alternative cost.
    Chord(&'static str),
    /// A note about the band above, rather than a key. Drawn as prose in §9.4's machine-rule
    /// rendering, because its teaching is the point.
    Note,
}

impl Legend {
    /// What the page draws in the leading column.
    ///
    /// `Control` is ⌘ on Apple platforms and Ctrl everywhere else, and `Meta` is the mirror of
    /// that — §16.8, and the swap is the winit backend's rather than ours. This is the one place in
    /// the program that knows it.
    pub fn printed(&self) -> String {
        match self {
            Legend::Plain(s) => (*s).to_string(),
            Legend::Note => String::new(),
            Legend::Chord(declared) => {
                let mut mods: Vec<&str> = Vec::new();
                let mut key = String::new();
                for part in declared.split('+').map(str::trim) {
                    match part {
                        // Printed leading, so `⌃⌘P` reads `Ctrl-Cmd-P` — modifier order is the
                        // platform's convention and not the declaration's, which lists `Control`
                        // first because the compiler's own enum does.
                        "Meta" => mods.insert(0, if APPLE { "Ctrl" } else { "Meta" }),
                        "Control" => mods.push(if APPLE { "Cmd" } else { "Ctrl" }),
                        "Shift" => mods.insert(0, "Shift"),
                        "Alt" => mods.insert(0, "Alt"),
                        // The key itself: either a quoted character or one of the compiler's named
                        // keys. `BackSlash` has a capital S and `Comma` is `LocalizedShiftable`;
                        // both are spelled the way `@keys` spells them and neither is a modifier.
                        "Comma" => key = ",".into(),
                        "BackSlash" => key = "\\".into(),
                        other => key = other.trim_matches('"').to_ascii_uppercase(),
                    }
                }
                mods.push(&key);
                mods.join("-")
            }
        }
    }
}

/// Whether ⌘ is the modifier `Control` names — §16.8's platform swap, asked once.
///
/// A `const` rather than a `cfg!` at each site so the sweep below can state which platform's
/// legends it is checking, and so the two arms are visibly the same shape.
const APPLE: bool = cfg!(target_os = "macos");

/// One key, or one chord, and what pressing it does.
pub struct Binding {
    pub keys: Legend,
    /// What it does, in the window's own voice.
    pub does: &'static str,
}

/// The bands of the page. A rule is drawn where the band changes, exactly as §21.3's menu does it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Group {
    /// The emulated iPod: the wheel, its buttons and the hold switch.
    Ipod,
    /// The window around it.
    Window,
    /// §21.7's second view of the panel, which has its own keys because it is its own window.
    Panel,
    /// The pointer, which is not a key and is the thing the operator could not work out.
    ///
    /// **On the keyboard's page rather than on one of its own**, because the question a person
    /// arrives with is *how do I drive this*, and answering half of it on a page called Reference
    /// and the other half nowhere is the shape §22.6 exists to fix. The operator drove the shipped
    /// window and could not work out how to use the wheel; every one of these was already built and
    /// none of it was written down anywhere the window could reach.
    Pointer,
}

impl Group {
    /// Every band, in the order the page draws them.
    pub const ALL: [Group; 4] = [Group::Ipod, Group::Window, Group::Panel, Group::Pointer];

    /// The heading over the band.
    pub fn heading(self) -> &'static str {
        match self {
            Group::Ipod => "The iPod",
            Group::Window => "The window",
            Group::Panel => "The screen on its own",
            Group::Pointer => "The pointer",
        }
    }

    pub fn bindings(self) -> &'static [Binding] {
        match self {
            Group::Ipod => IPOD,
            Group::Window => WINDOW,
            Group::Panel => PANEL,
            Group::Pointer => POINTER,
        }
    }
}

/// The wheel, its four buttons and the hold switch.
///
/// **`Up` / `Down` and `Left` / `Right` are two rows because they are two rules.** §16.8: the
/// vertical pair is the wheel always, and the horizontal pair is the wheel *while there is a
/// machine* and the previous / next iPod when there is not — one key with two jobs, and the machine
/// has first claim. A single row covering both would be true of one of them.
const IPOD: &[Binding] = &[
    Binding { keys: Legend::Plain("Up / Down"), does: "turn the wheel" },
    Binding {
        keys: Legend::Plain("Left / Right"),
        does: "turn the wheel while an iPod is running — the previous or next iPod when none is",
    },
    Binding {
        keys: Legend::Plain("Enter / Space"),
        does: "the centre button, in whatever sense the drawn one is right now",
    },
    Binding { keys: Legend::Plain("M"), does: "MENU" },
    Binding { keys: Legend::Plain("P"), does: "Play / Pause" },
    Binding { keys: Legend::Plain("N"), does: "Next" },
    Binding { keys: Legend::Plain("B"), does: "Previous" },
    Binding {
        keys: Legend::Plain("H"),
        does: "the hold switch — throw it and the iPod stops listening",
    },
    // §14.1 on a page whose whole subject is what the keys do: the machine's keys answer with a
    // sentence rather than silently when there is nothing to answer for, and a table that did not
    // say so would leave somebody pressing `M` at an empty bench and concluding it was broken.
    Binding {
        keys: Legend::Note,
        does: "With no iPod running, every key above says so on the fixture rather than doing \
               nothing.",
    },
];

/// The window.
const WINDOW: &[Binding] = &[
    Binding { keys: Legend::Plain("Tab / Shift-Tab"), does: "move the focus, forwards and back" },
    Binding {
        keys: Legend::Plain("Esc"),
        does: "leave, outwards, one step at a time — fullscreen, then an open row, then a level of \
               the menu, then the menu, then park the iPod",
    },
    Binding { keys: Legend::Chord("Control + BackSlash"), does: "the menu" },
    Binding { keys: Legend::Chord("Control + Comma"), does: "this page" },
    Binding { keys: Legend::Plain("?"), does: "this page" },
    Binding {
        keys: Legend::Chord("Control + Meta + \"p\""),
        does: "the screen in a window of its own",
    },
];

/// §21.7's popped-out panel, which is a second window and therefore has its own keys.
const PANEL: &[Binding] = &[
    Binding {
        keys: Legend::Chord("Control + Meta + \"f\""),
        does: "fullscreen. F11 does the same, and is the only one of the two that arrives away \
               from Apple platforms — the shell reserves the chord",
    },
    Binding { keys: Legend::Plain("Esc"), does: "leave fullscreen, and only then close the window" },
    Binding {
        keys: Legend::Note,
        does: "Every key under The iPod works here too. It is a second view of one screen, not a \
               second iPod.",
    },
];

/// The pointer — see [`Group::Pointer`] for why it is on the keyboard's page.
//
// **The legends here are one or two words, and that is a measurement rather than a style.** The
// leading column is `geometry::FIELD_LABEL_W` — 96 px, budgeted for `Bootloader` — and the first
// draft wrote `Scroll on the wheel` and `A MacBook trackpad`, which drew as *Scroll on the w…* and
// *A MacBook tra…*. Read it off `_out/gui/reference.png` before this change.
// `every_legend_the_reference_page_draws_fits_the_column_it_is_drawn_in` is the gate now.
const POINTER: &[Binding] = &[
    Binding {
        keys: Legend::Plain("Drag"),
        does: "the wheel, around the ring, the way a thumb does it",
    },
    Binding {
        keys: Legend::Plain("Scroll"),
        does: "on the wheel — the same turn, two fingers",
    },
    Binding {
        keys: Legend::Plain("Trackpad"),
        does: "a MacBook's is the wheel itself — the pad is a capacitive ring under different \
               plastic",
    },
];

/// One line of the Reference page.
pub enum Row {
    Heading(&'static str),
    Binding(Detail),
}

/// The page, in the order it is drawn.
pub fn page() -> Vec<Row> {
    let mut out: Vec<Row> = Vec::new();
    for g in Group::ALL {
        out.push(Row::Heading(g.heading()));
        for b in g.bindings() {
            out.push(Row::Binding(binding_detail(b)));
        }
    }
    out
}

/// **A [`Legend::Note`] is a sentence about the band above it**, drawn as prose rather than as a
/// two-column fact — which is what `MadeOfLine` already does with an empty label, so there is no
/// second rendering to write.
fn binding_detail(b: &Binding) -> Detail {
    Detail {
        label: b.keys.printed(),
        value: b.does.to_string(),
        // §6.2 gives `mono` to paths, hashes, serials and addresses. A key legend is none of those,
        // and the label column is already `weight-label`, which is what sets it apart.
        mono: false,
        // §9.4's machine rule is prose in `fg` rather than `fg-dim`, because its teaching is the
        // point — which is exactly what a note row is.
        machine_rule: matches!(b.keys, Legend::Note),
        action: None,
    }
}

/// Every `@keys(…)` this table says the markup declares.
///
/// Used by `every_chord_the_markup_declares_is_printed_on_the_reference_page`, which compares it
/// against what `ui/*.slint` actually contains. `pub(crate)` because that sweep lives in `main.rs`
/// with the rest of the markup sweeps, beside the file reader they all share.
///
/// **`#[cfg(test)]` rather than an allow**, which is this crate's own rule for a thing only a sweep
/// reads — `geometry::SF_SWEEP` states it: *a constant kept alive by an allow is the shape §16.9
/// deletes*. The shipped page reads `Legend::printed`; nothing shipped needs the declaration back.
#[cfg(test)]
pub(crate) fn declared_chords() -> Vec<&'static str> {
    Group::ALL
        .iter()
        .flat_map(|g| g.bindings())
        .filter_map(|b| match b.keys {
            Legend::Chord(d) => Some(d),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **The control every sweep below needs**: the bands have rows and the page carries all of
    /// them. A producer that quietly returned an empty list would make every gate here green.
    #[test]
    fn every_band_has_bindings_and_the_page_carries_all_of_them() {
        let mut bindings = 0;
        for g in Group::ALL {
            assert!(!g.bindings().is_empty(), "{g:?} is a band with nothing in it");
            assert!(!g.heading().is_empty(), "{g:?} has no heading");
            bindings += g.bindings().len();
        }

        let page = page();
        let headings = page.iter().filter(|r| matches!(r, Row::Heading(_))).count();
        let rows = page.iter().filter(|r| matches!(r, Row::Binding(_))).count();
        assert_eq!(headings, Group::ALL.len(), "the page draws {headings} headings for four bands");
        assert_eq!(rows, bindings, "the page drops {} of its own rows", bindings - rows);
        assert!(rows > 15, "only {rows} bindings; this program binds more than that");
    }

    /// **The derivation, on the four chords this program declares and on the platform swap.**
    ///
    /// This is the test that would have caught issue #37 had the legend been derived then: the
    /// printed form comes out of the declaration, so a declaration carrying one modifier can only
    /// ever print one.
    #[test]
    fn a_chord_prints_the_modifiers_its_binding_declares_and_no_others() {
        let printed = |d: &'static str| Legend::Chord(d).printed();
        if APPLE {
            assert_eq!(printed("Control + Meta + \"p\""), "Ctrl-Cmd-P");
            assert_eq!(printed("Control + Meta + \"f\""), "Ctrl-Cmd-F");
            assert_eq!(printed("Control + Comma"), "Cmd-,");
            assert_eq!(printed("Control + BackSlash"), "Cmd-\\");
            // **The defect itself, as an assertion.** `@keys(Control + "p")` is what shipped, and
            // on this platform it is ⌘P — a key the program never meant to claim. Printed, it says
            // so, which is the whole of what deriving buys.
            assert_eq!(
                printed("Control + \"p\""),
                "Cmd-P",
                "the one-modifier form that shipped as issue #37 must print as the key it actually \
                 fires, or deriving the legend buys nothing"
            );
        } else {
            assert_eq!(printed("Control + Meta + \"p\""), "Meta-Ctrl-P");
            assert_eq!(printed("Control + Comma"), "Ctrl-,");
        }
    }

    /// **Every legend is ASCII**, which is §16.6's closed glyph set doing its job one file early.
    ///
    /// `geometry::no_ui_string_carries_a_glyph_outside_the_closed_set` sweeps this file too and is
    /// the real gate; this exists to name *why* for the next person reaching for a `⌘`. The set is
    /// **The set is `—`, `…` and `§`, and that is all of it.** This test permitted `·` when it was
    /// written, and the real gate rejected it on the first run: U+00B7 is §9.2's separator in prose
    /// and is **not** in the closed set — which `main.rs`'s own note about preferring the em dash
    /// already records. So a pair of keys is separated by `/`, which is ASCII and reads as *or*.
    /// Widening the list to make a legend pass is the one thing the gate's own message forbids.
    #[test]
    fn no_legend_reaches_for_a_glyph_the_font_may_not_have() {
        for g in Group::ALL {
            for b in g.bindings() {
                let printed = b.keys.printed();
                for c in printed.chars().chain(b.does.chars()) {
                    assert!(
                        c.is_ascii() || matches!(c, '—' | '…' | '§'),
                        "{printed:?} carries `{c}` (U+{:04X}), which this program's font is not \
                         trusted for — §16.6",
                        c as u32
                    );
                }
            }
        }
    }

    /// **The declaration list is not empty and is not the whole table.**
    ///
    /// A sweep driven off a list that collected nothing reads exactly like a program that declares
    /// no chords (AGENTS.md §6), and the markup comparison in `main.rs` is driven off this.
    #[test]
    fn the_declared_chords_are_the_keybindings_and_only_those() {
        let chords = declared_chords();
        assert_eq!(
            chords.len(),
            4,
            "this program declares four `KeyBinding`s and this table names {}: {chords:?}",
            chords.len()
        );
        for c in &chords {
            assert!(c.contains('+'), "`{c}` declares no modifier, so it is not a chord");
        }
    }
}
