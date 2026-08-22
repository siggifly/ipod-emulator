// Settings — docs/GUI.md §11.6, §9.1, §9.4.
//
// **One page, three rows, and it is not a settings app.** `ui/settings.slint` draws the three and
// this file answers every question they ask. Until it did, `window.slint`'s nine `setting-*`
// properties were never written by anything: the page drew three rows with empty labels, two of
// them disabled carrying an **empty** `reason` — the construction `primitives.slint:369` declares
// against (*"§9.4 — non-empty whenever `!enabled`"*) — and one live toggle that wrote nothing and
// reflected nothing.
//
// ─── Which of these ordinals the markup actually pins ────────────────────────────────────────────
//
// This is the only one of the three drawer pages whose ordinals the shipping markup states as
// literals: `ui/drawer.slint` has one `setting-toggled(int)` callback for the whole page and fires
// it by number, so a renumbering here silently re-aims a live control — `Copy path` writing the
// update preference, say. Measured in the file `build.rs` compiles:
//
//   - `Row::CheckUpdates` is **1** — `drawer.slint:544`, `root.setting-toggled(1)`.
//   - `Row::CopyPath` is **2** — `drawer.slint:547`, `root.setting-toggled(2)`.
//
// `Row::Theme` is **0** and is ours: the theme row is drawn from `setting-theme-*` and fires
// nothing yet. It is in the list because the page has three rows and a vocabulary with a hole in it
// invites somebody to fill the hole with a different meaning.
//
// **Never `ui/preview.slint`.** It is a slint-viewer-only root that `build.rs` never compiles into
// the binary, so an ordinal pinned against it is pinned against a file the program does not have.
//
// ─── What this page owns, and what it deliberately does not ──────────────────────────────────────
//
// The page's own state is one string: the sentence a failed `Settings::save` leaves behind.
// Everything else it draws is a fact about `Settings` and about what this build can do, recomputed
// on every push. Notably it has **no `busy`** — nothing here is gated on a build, and a gate wired
// to a phase nothing computes must not pretend to fire.
//
// **`chassis` is deliberately not one of the three settings.** The model's own comment says it is
// *"the window's iPod, not the machine's identity"*, so it is an override on the **device's** page;
// a global override for a per-device fact is how one setting comes to mean two things.
//
// **This is the one producer that saves for itself**, and [`Prefs::toggled`] returns `Wrote::Nothing`
// from every arm because of it. `settings.slint:104` binds the failure to the toggle's own
// `consequence` rather than to the Rail, so the page has to *observe* the write to word the
// sentence — and a `Wrote::Library` on top of that would have `main.rs` write the file a second
// time, which `Settings::render` regenerates whole, taking any comment the operator added with it.

use std::path::{Path, PathBuf};

use eapp_loader::settings::Settings;

use crate::parts::Wrote;
use crate::rail::{Caps, Next};

/// Which of the page's three rows a `setting-toggled(int)` is about.
///
/// One handler for three rows, so Rust decides what each one writes — which is why the ordinal has
/// to be exhaustively decoded here and an unknown one has to be a no-op.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Row {
    Theme,
    /// Pinned to 1 by `drawer.slint`'s `toggle-updates`.
    CheckUpdates,
    /// Pinned to 2 by `drawer.slint`'s `copy-path`.
    CopyPath,
}

impl Row {
    pub const ALL: [Row; 3] = [Row::Theme, Row::CheckUpdates, Row::CopyPath];

    /// `None` for anything outside the list, so a stray `int` from the markup is a no-op rather
    /// than a different preference.
    pub fn from_i32(n: i32) -> Option<Row> {
        usize::try_from(n).ok().and_then(|i| Row::ALL.get(i)).copied()
    }

    /// Its index in [`Row::ALL`], which is the number the markup carries.
    ///
    /// **`#[cfg(test)]` rather than `#[allow(dead_code)]`**, and for the reason `parts::Kind`'s
    /// other half now carries: an allow here would sit on a name `main.rs` calls half a dozen times
    /// for other types, and the sweep that reads allows decides a call by text across files. This
    /// vocabulary travels **one way** — `drawer.slint` fires `setting-toggled(1)` and `(2)` as
    /// literals and nothing pushes a `Row` ordinal outward — so the shipped program decodes and
    /// never encodes. What reads it is `the_markup_fires_the_ordinal_this_type_names`, which is the
    /// sweep that would catch a renumbering re-aiming a live control at the wrong preference, and
    /// deleting this would delete that.
    ///
    /// **Retired when:** something pushes a `Row` ordinal to the markup.
    #[cfg(test)]
    pub fn as_i32(self) -> i32 {
        Row::ALL.iter().position(|r| *r == self).expect("ALL holds every variant") as i32
    }
}

/// The one palette this build has, named for the row's value column.
///
/// **Measured rather than chosen**, which is `tokens.slint`'s own first line, and light: `Ink.bg`
/// is `#ffffff`. Saying `System` here — which §11.6's sketch drew, back when the row was going to
/// offer three — would be the row claiming to follow something it does not read.
const THEME: &str = "Light";

/// Why the Theme row is not a control, in §9.4's second kind: *this is not finished, by us*.
///
/// It names **no escape hatch**, and `settings.slint` binds `machine-rule: false` beside it for
/// the same reason: there is no command that themes this program, and §9.4's rule is to name a
/// real one or none. `Ink` is one measured light palette with no dark values and nothing in the
/// program keys on a scheme, so a Theme control would write a field no pixel reads — which is
/// exactly the landmine §17.Q8 names about `Settings::mode`.
const NO_THEME: &str = "there is one palette in this build and nothing keys on a scheme, so the \
                        control would write a preference no pixel reads";

/// Why there is no path to copy, when this computer has nowhere to keep a settings file.
///
/// **Asked before the clipboard**, because the absence of the thing to copy is a fact about this
/// computer and the clipboard is only the state of this build — the same ordering `main.rs` gives
/// its clipboard gate, where the identifier refusal comes before the missing-pasteboard sentence.
const NO_PATH: &str = "this computer has nowhere to keep a settings file, so there is no path yet";

/// What the page draws — **exactly the nine `setting-*` properties `window.slint:215-223`
/// declares**, and nothing else.
///
/// One bundle rather than nine returns, so a dropped field is a shape mismatch rather than a silent
/// loss. That failure is not hypothetical: `push_composer` reads six of `Which`'s eight fields and
/// drops two, which is precisely how `Which::make_one`'s two-press confirmation and its OUI warning
/// reach no pixel.
///
/// `file_path` is also **the string a `Copy path` would put on the pasteboard**. One producer for
/// the path means the row and the clipboard cannot come to show two different files, and it is why
/// this page needs no tenth field to carry the copy's payload.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct View {
    /// The Row's value column: the palette that exists.
    pub theme_value: String,
    pub theme_enabled: bool,
    /// §9.4 — non-empty whenever `!theme_enabled`.
    pub theme_reason: String,
    /// The ToggleRow's box: `Settings::check_updates_on_start`, and the markup never writes it.
    pub check_updates: bool,
    /// The ToggleRow's `consequence` — **empty unless the last save failed**. See [`Prefs`].
    pub toggle_reason: String,
    /// The Row's value column: `settings.txt`.
    pub file_name: String,
    /// The Row's `sub`, in `mono` (§6.2). Home shortened to `~`; see [`shorten`].
    pub file_path: String,
    pub copy_enabled: bool,
    /// §9.4 — non-empty whenever `!copy_enabled`.
    pub copy_reason: String,
}

/// The Settings page's whole state.
///
/// One transient string, and it is the one thing on this page that is not a fact about `Settings`:
/// a read-only home makes `Settings::save` fail, and that used to be swallowed — the toggle moved
/// on screen and nothing was written. The sentence is drawn as the row's own consequence.
///
/// **It is the last save's outcome and not a log**: a save that succeeds clears it, because a
/// sentence about a failure that has since been repaired is a sentence that lies.
pub struct Prefs {
    save_failed: String,
}

impl Prefs {
    pub fn new() -> Prefs {
        Prefs { save_failed: String::new() }
    }

    /// The whole page, recomputed.
    ///
    /// **Nothing here is remembered between calls except [`Prefs::save_failed`]**, which is the
    /// discipline that stops a settings page going stale: the toggle is `Settings`' own field read
    /// afresh, and both refusals are re-derived from `caps` rather than from what the build could
    /// do when the page was opened.
    pub fn view(&self, s: &Settings, caps: Caps) -> View {
        let at = Settings::path();
        let clipboard = Next::CopyDetails.available(caps);
        View {
            theme_value: THEME.into(),
            // **A constant `false`, and it is a fact about this build rather than a placeholder.**
            // See [`NO_THEME`]: there is one palette. The day there are two, this reads whichever
            // field says which — and the row stops being disabled in the same edit.
            theme_enabled: false,
            theme_reason: NO_THEME.into(),
            check_updates: s.check_updates_on_start,
            toggle_reason: self.save_failed.clone(),
            file_name: at
                .as_deref()
                .and_then(Path::file_name)
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            file_path: at.as_deref().map(|p| shorten(p, home().as_deref())).unwrap_or_default(),
            copy_enabled: at.is_some() && clipboard,
            copy_reason: match &at {
                None => NO_PATH.into(),
                // **`rail.rs`'s own words for the same fact**, and then what it costs here. One
                // fact worded twice on two surfaces is how a person comes to believe they are two
                // different problems — `composer::NO_CLIPBOARD` is built the same way, from the
                // same `Next`, and `the_copy_row_wears_rails_own_sentence_for_an_absent_clipboard`
                // pins the shared half by reading it out of `rail.rs` rather than repeating it.
                Some(_) if !clipboard => {
                    format!("{}, so there is nowhere for the path to go", Next::CopyDetails.reason())
                }
                Some(_) => String::new(),
            },
        }
    }

    /// One row pressed, decoded from the `int` `drawer.slint` fires.
    ///
    /// **An ordinal nothing here knows is a no-op**, never a wrong branch — one callback serves all
    /// three rows, so a stray number that fell through to a `match` arm would write a preference
    /// nobody asked for.
    ///
    /// **Every arm answers [`Wrote::Nothing`], and that is not the type going unused.** `Wrote` is
    /// a *save instruction* — its own words are "whether the library moved, and therefore whether
    /// `main.rs` saves" — and this page has already saved by the time it returns, because it is the
    /// page that draws the failure. A `Library` here would rewrite the operator's file a second
    /// time for no change.
    pub fn toggled(&mut self, s: &mut Settings, n: i32, caps: Caps) -> Result<Wrote, String> {
        let Some(row) = Row::from_i32(n) else {
            return Ok(Wrote::Nothing);
        };
        match row {
            // The shipping markup fires nothing for this row — `settings.slint` gives Theme no
            // `activated` handler at all — so this arm is what a press would meet if one were ever
            // wired. It answers in **the row's own words**, read back out of the view, so the
            // refusal and the reason under the row cannot come to disagree.
            Row::Theme => Err(self.view(s, caps).theme_reason),
            Row::CheckUpdates => {
                s.check_updates_on_start = !s.check_updates_on_start;
                // **The toggle moves either way, and the sentence is what says the disk did not.**
                // Reverting the field on a failed write would be the other design and is worse: the
                // control would spring back with no explanation, which is the silent-non-stick this
                // page exists to cure.
                self.save_failed = match s.save() {
                    Ok(()) => String::new(),
                    Err(e) => {
                        format!("this is set for now, but the settings file was not written — {e}")
                    }
                };
                Ok(Wrote::Nothing)
            }
            // The copy itself belongs to `main.rs`, which is the only file that may name a
            // toolkit: what this answers is whether it may happen at all, and the text is
            // [`View::file_path`].
            Row::CopyPath => {
                let v = self.view(s, caps);
                if v.copy_enabled {
                    Ok(Wrote::Nothing)
                } else {
                    Err(v.copy_reason)
                }
            }
        }
    }
}

/// The home directory, for [`shorten`] and for nothing else.
///
/// **`eapp_loader::settings` has this function and it is private**, so this is a second reader of
/// one environment variable rather than a second spelling of a decision. It is deliberately not
/// worth widening that module's surface for: this is *presentation*, and the worst a disagreement
/// can do is draw a path in full.
fn home() -> Option<PathBuf> {
    ["HOME", "USERPROFILE"]
        .into_iter()
        .filter_map(std::env::var_os)
        .find(|v| !v.is_empty())
        .map(PathBuf::from)
}

/// `~` for the home directory, which is what §11.6 draws.
///
/// **The separator check is the whole of it.** A prefix match alone shortens `/Users/siggifly` to
/// `~fly` under a home of `/Users/siggi` — a different person's directory, drawn as this one's —
/// so the remainder has to start at a component boundary or nothing happens. `home` is a parameter
/// rather than read inside, which is what lets that case be measured without an environment.
fn shorten(p: &Path, home: Option<&Path>) -> String {
    let full = p.display().to_string();
    let Some(home) = home else {
        return full;
    };
    let at = home.display().to_string();
    let at = at.trim_end_matches(['/', '\\']);
    if at.is_empty() {
        return full;
    }
    match full.strip_prefix(at) {
        Some("") => "~".into(),
        Some(rest) if rest.starts_with(['/', '\\']) => format!("~{rest}"),
        _ => full,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every capability on, so the two refusals below can be measured against a build that has
    /// what they ask for. Copied from `devices.rs`'s fixture of the same name.
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

    /// Point the data directory somewhere disposable for the length of one test.
    ///
    /// **`Settings::save` writes wherever `IPOD_EMULATOR_DATA` points**, and with nothing holding
    /// it that is the operator's real library. `crate::data_dir_lock` redirects it away as a
    /// precondition of holding the lock; the override below only narrows it further. Same shape as
    /// `work.rs`'s `DataDir`, and the same lock, because two locks over one variable is the same as
    /// no lock.
    struct DataDir {
        _guard: crate::DataDirLock,
        at: PathBuf,
    }

    impl DataDir {
        fn new(tag: &str) -> DataDir {
            let guard = crate::data_dir_lock();
            let at = std::env::temp_dir().join(format!("ipod-settings-{}-{tag}", std::process::id()));
            let _ = std::fs::remove_dir_all(&at);
            std::fs::create_dir_all(&at).expect("a scratch data directory");
            // SAFETY: `data_dir_lock` serialises every test in this binary that touches this
            // variable, and it is put back by `DataDirLock`'s own `Drop`, which runs after this.
            unsafe { std::env::set_var("IPOD_EMULATOR_DATA", &at) };
            DataDir { _guard: guard, at }
        }

        /// Point it at somewhere `create_dir_all` cannot make: a path **under a regular file**.
        ///
        /// A read-only home is what §20 item 13 is about and is what this stands in for; a file
        /// where a directory has to be is the same `io::Error` shape and is the one a test can
        /// arrange on any platform without asking for privileges.
        fn make_saving_fail(&self) {
            let blocker = self.at.join("not-a-directory");
            std::fs::write(&blocker, b"").expect("a regular file");
            // SAFETY: as above — the lock is held for the life of this `DataDir`.
            unsafe { std::env::set_var("IPOD_EMULATOR_DATA", blocker.join("below")) };
        }
    }

    impl Drop for DataDir {
        fn drop(&mut self) {
            // The variable is put back by `DataDirLock`'s own `Drop`, which runs after this — a
            // restore here would unset it on the run where this was the first test to touch it.
            let _ = std::fs::remove_dir_all(&self.at);
        }
    }

    // ── the vocabulary ──────────────────────────────────────────────────────────────────────────

    /// **The two ordinals `ui/drawer.slint` writes are where it thinks they are.**
    ///
    /// Both halves: the `assert_eq!` pins the Rust order, and the markup search proves the numbers
    /// in this file are the numbers the markup sends. `drawer.slint` is read rather than
    /// `preview.slint` because `build.rs` compiles `ui/window.slint`, which imports the first and
    /// not the second.
    #[test]
    fn the_two_pinned_setting_rows_are_where_the_shipping_markup_writes_them() {
        assert_eq!(Row::CheckUpdates.as_i32(), 1, "`drawer.slint`'s `toggle-updates`");
        assert_eq!(Row::CopyPath.as_i32(), 2, "`drawer.slint`'s `copy-path`");

        let path = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/ui/drawer.slint"));
        let text = std::fs::read_to_string(path).expect("ui/drawer.slint");
        let dense: String = text.chars().filter(|c| !c.is_whitespace()).collect();
        for row in [Row::CheckUpdates, Row::CopyPath] {
            let fires = format!("root.setting-toggled({})", row.as_i32());
            assert!(
                dense.contains(&fires),
                "`drawer.slint` no longer fires `{fires}` for {row:?}; one handler serves all \
                 three rows, so a renumbering here writes a different preference"
            );
        }
        assert_eq!(
            dense.matches("root.setting-toggled(").count(),
            2,
            "`drawer.slint` fires a number of `setting-toggled` calls this vocabulary has not been \
             measured against"
        );
    }

    /// Every ordinal survives the round trip, and nothing outside the list decodes.
    ///
    /// **`ALL` is checked for duplicates first**, and that is not tidiness: the round trip walks
    /// `ALL`, so a variant swapped out of it for a second copy of its neighbour leaves the loop
    /// green while one row has no ordinal and another answers for two. `[Row; 3]` is fixed-length,
    /// so a variant that goes missing leaves a duplicate behind. Measured by mutation, in
    /// `parts.rs`'s twin of this test, before either of them said so.
    #[test]
    fn every_setting_row_round_trips_and_an_unknown_one_decodes_to_nothing() {
        let mut seen = Row::ALL.to_vec();
        seen.sort();
        seen.dedup();
        assert_eq!(
            seen.len(),
            Row::ALL.len(),
            "{:?} does not hold each row exactly once, so one `setting-toggled` ordinal writes two \
             preferences and one preference cannot be written at all",
            Row::ALL
        );
        for r in Row::ALL {
            assert_eq!(Row::from_i32(r.as_i32()), Some(r), "{r:?}");
        }
        assert_eq!(Row::from_i32(-1), None);
        assert_eq!(Row::from_i32(Row::ALL.len() as i32), None);
    }

    // ── the shape of what crosses the boundary ──────────────────────────────────────────────────

    /// **`View` carries one field per `setting-*` property, and both sides are pinned.**
    ///
    /// The exhaustive `let View { .. }` below is one half: adding a field to `View` stops this
    /// compiling, so nothing can be added without a use here. Counting the markup's declarations is
    /// the other: adding a `setting-*` property to `window.slint` turns it red. Between them a
    /// property cannot appear on one side alone — which is the defect this bundle exists to
    /// prevent, and which `push_composer` has today, reading six of `Which`'s eight fields.
    ///
    /// `window.slint` and not `preview.slint`: `build.rs` compiles the first.
    #[test]
    fn the_view_carries_one_field_for_every_setting_property_the_window_declares() {
        let path = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/ui/window.slint"));
        let text = std::fs::read_to_string(path).expect("ui/window.slint");
        let declared: Vec<&str> = text
            .lines()
            .map(str::trim)
            .filter(|l| l.starts_with("in property <"))
            .filter_map(|l| l.split_once('>').map(|(_, r)| r.trim()))
            .filter(|r| r.starts_with("setting-"))
            .collect();
        assert_eq!(
            declared.len(),
            9,
            "`window.slint` declares {} `setting-*` properties and `View` has nine fields: {declared:?}",
            declared.len()
        );

        let _guard = DataDir::new("shape");
        let s = Settings::default();
        // **Exhaustive on purpose.** A `..` here would let a tenth field be added and drawn by
        // nothing, which is the silent loss this whole bundle is for.
        let View {
            theme_value,
            theme_enabled,
            theme_reason,
            check_updates,
            toggle_reason,
            file_name,
            file_path,
            copy_enabled,
            copy_reason,
        } = Prefs::new().view(&s, all_on());
        assert_eq!(theme_value, THEME);
        assert!(!theme_enabled);
        assert!(!theme_reason.is_empty());
        assert!(!check_updates, "`Settings::default` does not check for updates");
        assert!(toggle_reason.is_empty(), "nothing has failed to save yet");
        assert_eq!(file_name, "settings.txt");
        assert!(file_path.ends_with("settings.txt"), "{file_path}");
        assert!(copy_enabled, "every capability is on and there is a path");
        assert!(copy_reason.is_empty());
    }

    // ── §9.4, the invariant the page was breaking ───────────────────────────────────────────────

    /// **No disabled row carries an empty reason** — `primitives.slint:369`'s own words.
    ///
    /// This is the state the page shipped in and the worst-looking thing in the window: two rows
    /// drawn `fg-disabled` with a reason slot reserved and nothing in it, which is a control
    /// refusing to say why. Swept over both capability arms and both toggle states, and the
    /// converse is asserted too — a row that *is* pressable must not wear a refusal, which is how
    /// `copy_reason` going stale after the cap turns on would be caught.
    ///
    /// Proved red by returning `String::new()` for `theme_reason`.
    #[test]
    fn no_disabled_setting_row_carries_an_empty_reason() {
        let _guard = DataDir::new("reasons");
        let mut checked = 0usize;
        let mut refused = 0usize;
        for caps in [Caps::default(), all_on()] {
            for checking in [false, true] {
                let s = Settings {
                    check_updates_on_start: checking,
                    ..Settings::default()
                };
                let v = Prefs::new().view(&s, caps);
                // The ToggleRow is not in this sweep and that is not an omission: it has no
                // `enabled` binding in `settings.slint` at all, so it is never disabled, and its
                // `consequence` is a different slot from `reason` — see the failed-save test.
                for (row, enabled, reason) in [
                    ("Theme", v.theme_enabled, &v.theme_reason),
                    ("Settings file", v.copy_enabled, &v.copy_reason),
                ] {
                    checked += 1;
                    assert!(
                        enabled || !reason.is_empty(),
                        "the `{row}` row is disabled and says nothing"
                    );
                    assert!(
                        !enabled || reason.is_empty(),
                        "the `{row}` row is pressable and still wears a refusal: {reason}"
                    );
                    refused += usize::from(!enabled);
                }
            }
        }
        // Two capability arms, two toggle states, two rows that can be disabled. The floor sits
        // **on** the population rather than under it, so a row that stops being emitted turns this
        // red instead of quietly shrinking what the sweep reads.
        assert_eq!(checked, 8, "the sweep read {checked} rows");
        assert!(
            refused > 0,
            "nothing was ever refused, so the disabled half of the sweep read nothing"
        );
    }

    /// **The copy row is refused in the words `rail::Next` already uses for the same fact.**
    ///
    /// One absent capability, worded once: the Rail draws `this build has no clipboard` for
    /// `Next::CopyDetails` and so does this page, and `composer::NO_CLIPBOARD` is built from the
    /// same `Next` for the same reason. Proved red by writing the sentence out here instead.
    #[test]
    fn the_copy_row_wears_rails_own_sentence_for_an_absent_clipboard() {
        let _guard = DataDir::new("clipboard");
        let s = Settings::default();

        let off = Prefs::new().view(&s, Caps::default());
        assert!(!off.copy_enabled, "this build has no clipboard and the row is live");
        assert!(
            off.copy_reason.starts_with(Next::CopyDetails.reason()),
            "the page words the absent clipboard differently from the Rail:\n  page: {}\n  rail: {}",
            off.copy_reason,
            Next::CopyDetails.reason()
        );

        let on = Prefs::new().view(&s, all_on());
        assert!(on.copy_enabled, "the clipboard is there and the control is still dead");
        assert!(on.copy_reason.is_empty(), "{}", on.copy_reason);
    }

    // ── the toggle, and the sentence a failed save leaves behind ────────────────────────────────

    /// **The toggle writes the model and the file, and reflects the model on the way back.**
    ///
    /// The markup never writes the box — `ToggleRow`'s own comment says so — so this is the only
    /// writer, and a `view` that did not read `Settings` afresh would leave the box where it was.
    #[test]
    fn toggling_updates_flips_the_setting_saves_it_and_shows_it() {
        let guard = DataDir::new("toggle");
        let mut s = Settings::default();
        let mut p = Prefs::new();
        assert!(!p.view(&s, all_on()).check_updates);

        assert_eq!(p.toggled(&mut s, Row::CheckUpdates.as_i32(), all_on()), Ok(Wrote::Nothing));
        assert!(s.check_updates_on_start, "the model did not move");
        assert!(p.view(&s, all_on()).check_updates, "the model moved and the box did not");
        assert!(p.view(&s, all_on()).toggle_reason.is_empty(), "a good save left a sentence");

        // **On disk, not merely in memory.** A preference that does not survive the launch it was
        // set in is not a preference, and `Wrote::Nothing` says `main.rs` need not save — so if
        // this page did not save for itself, nothing would have.
        let written = std::fs::read_to_string(guard.at.join("settings.txt")).expect("a saved file");
        assert!(
            written.contains("check_updates_on_start"),
            "the settings file does not carry the preference:\n{written}"
        );

        assert_eq!(p.toggled(&mut s, Row::CheckUpdates.as_i32(), all_on()), Ok(Wrote::Nothing));
        assert!(!s.check_updates_on_start, "the second press did not flip it back");
    }

    /// **A failed save is a sentence on the toggle's own row, and it clears when the save works.**
    ///
    /// §20 item 13: a read-only home, a full disk or a second process holding the file used to be
    /// swallowed, and the control moved on screen with nothing written. `settings.slint:104` binds
    /// this to the ToggleRow's `consequence` for exactly that.
    ///
    /// Proved red twice: by discarding the `io::Error` (the sentence goes empty) and by leaving
    /// `save_failed` alone on a good save (the sentence outlives the failure it describes).
    #[test]
    fn a_failed_save_becomes_a_sentence_and_a_good_one_takes_it_away() {
        let guard = DataDir::new("failed-save");
        guard.make_saving_fail();
        let mut s = Settings::default();
        let mut p = Prefs::new();

        assert_eq!(p.toggled(&mut s, Row::CheckUpdates.as_i32(), all_on()), Ok(Wrote::Nothing));
        let said = p.view(&s, all_on()).toggle_reason;
        assert!(
            !said.is_empty(),
            "the settings file could not be written and the toggle said nothing about it"
        );
        // The control still moved, and the sentence is what says the disk did not — springing the
        // box back with no explanation is the other design and is the one this page cures.
        assert!(s.check_updates_on_start, "the toggle did not move");
        assert!(
            p.view(&s, all_on()).check_updates,
            "the box sprang back instead of standing beside its sentence"
        );

        // **The OS's own words reach the row.** A sentence that only said *it failed* would leave
        // a person with nowhere to go; `Failure::said` is the same rule one surface over.
        let bare = "this is set for now, but the settings file was not written";
        assert!(said.starts_with(bare), "{said}");
        assert!(
            said.len() > bare.len() + 3,
            "the row carries no word of the `io::Error` that caused it: {said}"
        );

        // Somewhere writable again, and the sentence goes. **Bound, not `let _`**, which would
        // drop it on the spot and take the directory with it before the save ran.
        let _repaired = DataDir::new("failed-save-repaired");
        assert_eq!(p.toggled(&mut s, Row::CheckUpdates.as_i32(), all_on()), Ok(Wrote::Nothing));
        assert!(
            p.view(&s, all_on()).toggle_reason.is_empty(),
            "the save worked and the row still carries the old failure: {}",
            p.view(&s, all_on()).toggle_reason
        );
    }

    /// **A press this page cannot perform refuses in the row's own words and writes nothing.**
    ///
    /// One callback serves three rows, so a refusal that mutated on its way out would be the
    /// wrong preference written — and the wording is read back out of the view so the refusal and
    /// the reason drawn under the row cannot come to disagree.
    #[test]
    fn a_refused_press_says_what_the_row_says_and_changes_nothing() {
        let _guard = DataDir::new("refusals");
        let before = Settings::default();

        for (row, reason) in [
            (Row::Theme, Prefs::new().view(&before, all_on()).theme_reason),
            (Row::CopyPath, Prefs::new().view(&before, Caps::default()).copy_reason),
        ] {
            // Theme is refused on every build; Copy path only where there is no clipboard.
            let caps = if row == Row::Theme { all_on() } else { Caps::default() };
            let mut s = before.clone();
            let mut p = Prefs::new();
            assert_eq!(p.toggled(&mut s, row.as_i32(), caps), Err(reason), "{row:?}");
            assert_eq!(s, before, "{row:?} was refused and the library moved anyway");
            assert!(
                p.view(&s, caps).toggle_reason.is_empty(),
                "{row:?} was refused and left a save failure behind"
            );
        }

        // And the one press this build can perform is not refused.
        let mut s = before.clone();
        assert!(Prefs::new().toggled(&mut s, Row::CopyPath.as_i32(), all_on()).is_ok());
        assert_eq!(s, before, "copying a path wrote to the library");
    }

    /// **An ordinal this vocabulary does not know writes nothing at all.**
    ///
    /// `setting-toggled(int)` is one callback for three rows, so an unknown number falling through
    /// to a `match` arm is a different preference written. Proved red by decoding out of range.
    #[test]
    fn an_unknown_setting_ordinal_is_a_no_op() {
        let _guard = DataDir::new("stray");
        let before = Settings::default();
        for stray in [-1, 3, 99, i32::MIN, i32::MAX] {
            let mut s = before.clone();
            let mut p = Prefs::new();
            assert_eq!(p.toggled(&mut s, stray, all_on()), Ok(Wrote::Nothing), "{stray}");
            assert_eq!(s, before, "the ordinal {stray} wrote a preference");
            assert!(p.view(&s, all_on()).toggle_reason.is_empty(), "{stray}");
        }
    }

    // ── the path on the row ─────────────────────────────────────────────────────────────────────

    /// **`~` only at a component boundary**, which is the difference between shortening a path and
    /// drawing somebody else's directory as this one's.
    #[test]
    fn the_home_directory_shortens_to_a_tilde_and_only_at_a_boundary() {
        let home = PathBuf::from("/Users/siggi");
        assert_eq!(shorten(Path::new("/Users/siggi/Library/x.txt"), Some(&home)), "~/Library/x.txt");
        assert_eq!(shorten(Path::new("/Users/siggi"), Some(&home)), "~");
        // The trap: a prefix match alone answers `~fly` here, which names a directory that is not
        // this person's.
        assert_eq!(
            shorten(Path::new("/Users/siggifly/x.txt"), Some(&home)),
            "/Users/siggifly/x.txt"
        );
        assert_eq!(shorten(Path::new("/opt/x.txt"), Some(&home)), "/opt/x.txt");
        // A trailing separator on the home directory is the same home directory.
        let slashed = PathBuf::from("/Users/siggi/");
        assert_eq!(shorten(Path::new("/Users/siggi/x.txt"), Some(&slashed)), "~/x.txt");
        // No home, and `/` as one, both leave the path alone — `~` for the root would be a lie.
        assert_eq!(shorten(Path::new("/Users/siggi/x.txt"), None), "/Users/siggi/x.txt");
        let root = PathBuf::from("/");
        assert_eq!(shorten(Path::new("/Users/siggi/x.txt"), Some(&root)), "/Users/siggi/x.txt");
    }

    /// **The row names the file, and the fact line under it is the path a copy would carry.**
    ///
    /// §11.6: *a preference nobody can find is one nobody can reset.* `file_path` is also the
    /// clipboard's payload, which is why there is no tenth field for it — and why the two cannot
    /// come to name different files.
    #[test]
    fn the_settings_row_names_the_file_and_the_path_under_it() {
        let guard = DataDir::new("path");
        let v = Prefs::new().view(&Settings::default(), all_on());
        assert_eq!(v.file_name, "settings.txt");
        assert!(
            v.file_path.ends_with("settings.txt"),
            "the fact line is not the settings file: {}",
            v.file_path
        );
        assert!(
            v.file_path.contains(&guard.at.display().to_string())
                || v.file_path.starts_with('~'),
            "the fact line names neither the data directory nor a shortened home: {}",
            v.file_path
        );
        // **A path the clipboard gate would let through.** `main.rs` refuses text carrying a serial
        // or a FireWire GUID, and a copy control that offered a string that gate then rejected
        // would be a control that visibly does nothing.
        assert!(
            crate::clipboard_refusal(&v.file_path).is_none(),
            "the path the copy would carry is refused by the clipboard gate: {}",
            v.file_path
        );
    }

    // ── the boundary ────────────────────────────────────────────────────────────────────────────

    /// **No toolkit in this file.** The window is replaceable exactly as long as this holds, and
    /// `main.rs` is the only file that may name one.
    ///
    /// **Comments are excluded, and that is not cosmetic**: this file's prose cites `.slint` paths
    /// and property names by design, and a sweep that counted those returns thirteen hits on
    /// `parts.rs` and misled the agent who wrote it. The control below is what proves the scanner
    /// still sees a real one.
    #[test]
    fn nothing_in_the_settings_page_names_a_toolkit_type() {
        const BANNED: [&str; 8] = [
            "slint::",
            "MainWindow",
            "GroupRow",
            "PartRow",
            "DetailRow",
            "ComponentHandle",
            "SharedString",
            "ModelRc",
        ];
        for banned in BANNED {
            for (n, line) in code_lines() {
                assert!(!line.contains(banned), "line {n}: the settings page names `{banned}`");
            }
        }
        // **The control**: a sweep that read nothing, or that skipped every line as a comment,
        // would pass the loop above in silence. This proves it reads code and would catch one.
        let seen = code_lines();
        assert!(seen.len() > 50, "the sweep read {} lines of code", seen.len());
        assert!(
            seen.iter().any(|(_, l)| l.contains("pub fn view")),
            "the sweep did not reach the producer, so it swept the header and stopped"
        );
        let offender = "    let label: SharedString = row.label.clone().into();";
        assert!(
            BANNED.iter().any(|b| offender.contains(b)),
            "the banned list no longer matches a line that plainly names a toolkit type"
        );
    }

    /// This file's shipped half: its own source, comments dropped, cut at the test module.
    fn code_lines() -> Vec<(usize, &'static str)> {
        const SOURCE: &str = include_str!("settings_page.rs");
        let end = SOURCE.lines().position(|l| l.trim() == "mod tests {").unwrap_or(usize::MAX);
        SOURCE
            .lines()
            .enumerate()
            .take(end)
            .filter(|(_, l)| !l.trim_start().starts_with("//"))
            .map(|(i, l)| (i + 1, l))
            .collect()
    }
}
