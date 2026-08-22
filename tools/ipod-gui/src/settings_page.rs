// Settings — docs/GUI.md §11.6.
//
// **A stub. The one thing settled here is the row vocabulary, because the markup writes it.**
//
// This is the only one of the three drawer pages whose ordinals the shipping markup states as
// literals: `ui/drawer.slint` has one `setting-toggled(int)` callback for the whole page and fires
// it by number, so a renumbering here silently re-aims a live control — `Copy path` writing the
// update preference, say. Measured in the file `build.rs` compiles:
//
//   - `Row::CheckUpdates` is **1** — `drawer.slint:541`, `root.setting-toggled(1)`.
//   - `Row::CopyPath` is **2** — `drawer.slint:544`, `root.setting-toggled(2)`.
//
// `Row::Theme` is **0** and is ours: the theme row is drawn from `setting-theme-*` and fires
// nothing yet. It is in the list because the page has three rows and a vocabulary with a hole in it
// invites somebody to fill the hole with a different meaning.
//
// The page's own state is one string: the sentence a failed `Settings::save` leaves behind.
// Everything else it draws is a fact about `Settings` and about what this build can do, recomputed
// on every push. Notably it has **no `busy`** — nothing here is gated on a build, and a gate wired
// to a phase nothing computes must not pretend to fire.

/// Which of the page's three rows a `setting-toggled(int)` is about.
///
/// One handler for three rows, so Rust decides what each one writes — which is why the ordinal has
/// to be exhaustively decoded here and an unknown one has to be a no-op.
#[allow(dead_code)] // retired when: `Prefs::toggled` decodes what `setting-toggled` sends — the producer, next wave
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Row {
    Theme,
    /// Pinned to 1 by `drawer.slint`'s `toggle-updates`.
    CheckUpdates,
    /// Pinned to 2 by `drawer.slint`'s `copy-path`.
    CopyPath,
}

#[allow(dead_code)] // retired when: `on_setting_toggled` calls `from_i32` on the row index the markup sends
impl Row {
    pub const ALL: [Row; 3] = [Row::Theme, Row::CheckUpdates, Row::CopyPath];

    /// `None` for anything outside the list, so a stray `int` from the markup is a no-op rather
    /// than a different preference.
    pub fn from_i32(n: i32) -> Option<Row> {
        usize::try_from(n).ok().and_then(|i| Row::ALL.get(i)).copied()
    }

    /// Its index in [`Row::ALL`], which is the number the markup carries.
    pub fn as_i32(self) -> i32 {
        Row::ALL.iter().position(|r| *r == self).expect("ALL holds every variant") as i32
    }
}

/// The Settings page's whole state.
///
/// One transient string, and it is the one thing on this page that is not a fact about `Settings`:
/// a read-only home makes `Settings::save` fail, and that used to be swallowed — the toggle moved
/// on screen and nothing was written. The sentence is drawn as the row's own consequence.
#[allow(dead_code)] // retired when: `Prefs::toggled` writes it after a failed save and `Prefs::view` draws it
pub struct Prefs {
    save_failed: String,
}

#[allow(dead_code)] // retired when: `wire` constructs one beside the Composer's cell — the integrator's step, after the producer lands
impl Prefs {
    pub fn new() -> Prefs {
        Prefs { save_failed: String::new() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
