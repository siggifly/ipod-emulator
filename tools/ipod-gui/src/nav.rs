//! Where you are, and the one way out.
//!
//! `docs/GUI.md` §4 (four entrances to the drawer, and `Esc` is not one of them), §11.2 (three
//! levels, not three groups) and §16.8 (`Esc` has one definition, outwards, in order).
//!
//! **This is the single writer of drawer state.** The markup fires callbacks and reads
//! `drawer-open` / `drawer-depth` / `drawer-page` as `in` properties; nothing in the markup writes
//! them. Four entrances plus `Esc` writing the same state from two sides is how `Esc` came to have
//! three incompatible definitions — §16.8 counted them: a way *into* the drawer in §4, a pure exit
//! here, and the way out of fullscreen in §12.6, so on a 14″ MacBook Pro the same key might instead
//! have initiated a 1.6 GB park.
//!
//! **No toolkit in this file, and no emulator either.** [`Stack::escape`] takes two booleans rather
//! than a phase, so this module does not depend on `emu` and every rule in it is testable with no
//! display and no machine.

use crate::geometry;

/// The drawer's pages. `None` is *no page*, which is what depth 0 shows: the menu.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Page {
    #[default]
    None,
    Devices,
    Parts,
    Games,
    Work,
    Readout,
    Settings,
    Reference,
}

impl Page {
    /// **Which depth slot draws this page — and `None` where nothing does.**
    ///
    /// The drawer's strip has one slot per level and each slot holds the pages that belong at that
    /// level; a page nothing draws would be a bare 420 px rectangle with no header, so
    /// [`Stack::go`] refuses to navigate to one and lands on the menu instead.
    ///
    /// **Exhaustive on purpose.** A page added to this enum stops the crate compiling until somebody
    /// answers *which level draws it*, which is what makes an unbuilt page a stated decision rather
    /// than a blank panel somebody finds by pressing a shortcut. That is exactly how `⌘,` came to
    /// navigate to Reference while `MenuPage`'s own Reference row was disabled and said why.
    ///
    /// **Retirement, one row at a time**: a page gains a slot on the day `ui/drawer.slint` gains a
    /// child for it — not before.
    pub fn slot(self) -> Option<i32> {
        match self {
            // The root page. `MenuPage` is the depth-0 slot's only child and always has been.
            Page::None => Some(0),
            // `WorkPage` is the depth-1 slot's only child.
            Page::Work => Some(1),
            // Not built. Each of these has a `MenuPage` row that is disabled and names its escape
            // hatch, which is where a person is told about them.
            Page::Devices
            | Page::Parts
            | Page::Games
            | Page::Readout
            | Page::Settings
            | Page::Reference => None,
        }
    }
}

/// What one press of `Esc` actually did, so the caller can act on it and a test can name it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Escape {
    LeftFullscreen,
    ClosedExpand,
    WentBack,
    ClosedDrawer,
    /// From `Running`: write the restore point and stop.
    Park,
    /// From `Booting`: parking a boot is a 1.6 GB write of a state nobody wants.
    PowerOff,
    /// Already at the bench, with nothing open.
    Nothing,
}

/// The drawer's position, and the two things that sit in front of it.
///
/// **`open` is a field rather than `!pages.is_empty()`**, and that is the whole of why reopening
/// returns you where you were: closing the drawer must not forget which level you were on, and a
/// derived `open` would have to clear the stack to express "closed".
#[derive(Clone, Debug, Default)]
pub struct Stack {
    open: bool,
    fullscreen: bool,
    /// The id of the one open Expand, if there is one. §8.1: an Expand is a height, not a window.
    expand: Option<u32>,
    /// The pages pushed past the menu. `pages[0]` is depth 1, so `depth() == pages.len()`.
    pages: Vec<Page>,
}

impl Stack {
    pub fn new() -> Stack {
        Stack::default()
    }

    pub fn open(&self) -> bool {
        self.open
    }

    /// §11.2: three numbered levels, each one row deep from the root. 0 is the menu.
    pub fn depth(&self) -> i32 {
        self.pages.len() as i32
    }

    /// The page at the current level. Depth 0 is the menu, which is not a page.
    pub fn page(&self) -> Page {
        self.pages.last().copied().unwrap_or(Page::None)
    }

    // **Not dead — not yet reached.** Fullscreen (§12.6) and the Expand (§8.1) are surfaces this
    // build does not draw, and `close` is the arm `Esc` deliberately does not share (see
    // `escape`). `escape` handles all three today so that the order is written once and tested
    // once; these are the entry points the surfaces themselves will call. **Retirement
    // condition**: the allow comes off when fullscreen and the Expand land.
    #[allow(dead_code)]  // retired when: the bench can enter fullscreen — §12.6
    pub fn fullscreen(&self) -> bool {
        self.fullscreen
    }

    #[allow(dead_code)]  // retired when: an Expand exists — §11.3
    pub fn expand(&self) -> Option<u32> {
        self.expand
    }

    /// §4's four entrances, all of which land here: `⌘\`, the handle, the shelf's row-3 leading
    /// slot, and the menu bar.
    ///
    /// **It never writes `depth`.** Close and reopen and you are where you were.
    pub fn toggle(&mut self) {
        self.open = !self.open;
    }

    /// Go to a page, at a level. Opens the drawer if it was closed.
    ///
    /// `depth` is clamped to `0..=DRAWER_MAX_DEPTH`; §4's *max 3 levels deep* is a geometry
    /// constant and the strip has exactly that many slots, so a fourth would be drawn into a blank.
    ///
    /// **A page with no slot lands on the menu instead of on a blank panel.** `⌘,` was wired to
    /// `Reference` at depth 1 while nothing draws a Reference page, so it navigated to a 420 px
    /// rectangle with no header, no text and no visible way out — while `MenuPage`'s own Reference
    /// row was disabled and said the page was not built. [`Page::slot`] is the exhaustive match that
    /// makes that a *decision* rather than an accident: adding a page is a compile error there until
    /// somebody says which level draws it.
    pub fn go(&mut self, p: Page, depth: i32) {
        let d = match p.slot() {
            Some(_) => depth.clamp(0, max_depth()),
            // Nothing draws it. §9.1 forbids a bare *nothing here* on every surface, and a slot with
            // no page is worse than that — it does not even say so. The menu does.
            None => 0,
        };
        self.open = true;
        if d == 0 {
            self.pages.clear();
            return;
        }
        self.pages.resize(d as usize, Page::None);
        let last = self.pages.len() - 1;
        self.pages[last] = p;
    }

    /// One level out. At depth 0 the way out of the drawer is the same control, so it closes.
    pub fn back(&mut self) {
        if self.pages.pop().is_none() {
            self.open = false;
        }
    }

    /// Close, keeping the level. See [`Stack::toggle`].
    #[allow(dead_code)]  // retired when: a caller closes the drawer without toggling it
    pub fn close(&mut self) {
        self.open = false;
    }

    #[allow(dead_code)]  // retired when: an Expand exists — §11.3
    pub fn expand_opened(&mut self, id: u32) {
        self.expand = Some(id);
    }

    /// Closes the Expand **only if it is the one named**, so a stale close from a subtree that has
    /// already been replaced cannot shut the one that is open now.
    #[allow(dead_code)]  // retired when: an Expand exists — §11.3
    pub fn expand_closed(&mut self, id: u32) {
        if self.expand == Some(id) {
            self.expand = None;
        }
    }

    #[allow(dead_code)]  // retired when: fullscreen exists — §12.6
    pub fn enter_fullscreen(&mut self) {
        self.fullscreen = true;
    }

    /// §16.8's ONE definition of `Esc`, outwards, in this order and no other.
    ///
    /// Leaves fullscreen · then closes an Expand · then goes back a drawer level · then closes the
    /// drawer · then, from `Running`, parks · and **from `Booting`, powers off**, because parking a
    /// boot is a 1.6 GB write of a state nobody wants.
    ///
    /// `booting` and `running` rather than a phase: this module must not depend on `emu`, and in
    /// this build both are always false because nothing starts a machine yet. `main.rs` supplies
    /// them.
    pub fn escape(&mut self, booting: bool, running: bool) -> Escape {
        if self.fullscreen {
            self.fullscreen = false;
            return Escape::LeftFullscreen;
        }
        if self.expand.take().is_some() {
            return Escape::ClosedExpand;
        }
        if self.open && self.depth() > 0 {
            self.pages.pop();
            return Escape::WentBack;
        }
        if self.open {
            // **Not `back()`** — this arm must not touch `depth`, and at depth 0 there is none to
            // touch, so the difference only shows if somebody makes the two share an
            // implementation later.
            self.open = false;
            return Escape::ClosedDrawer;
        }
        if running {
            return Escape::Park;
        }
        if booting {
            return Escape::PowerOff;
        }
        Escape::Nothing
    }
}

/// §4's *max 3 levels deep*, read from the one place the number lives.
fn max_depth() -> i32 {
    geometry::DRAWER_MAX_DEPTH.round() as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    const EVERY_PAGE: [Page; 8] = [
        Page::None,
        Page::Devices,
        Page::Parts,
        Page::Games,
        Page::Work,
        Page::Readout,
        Page::Settings,
        Page::Reference,
    ];

    /// The worst case, **derived rather than typed**: one press for fullscreen, one for an Expand,
    /// one per drawer level, and one to close the drawer.
    fn worst_case() -> usize {
        2 + max_depth() as usize + 1
    }

    /// A page the drawer actually draws, for the tests that are about **depth** rather than about
    /// which page. `go` refuses a page nothing draws and lands on the menu, so a test that navigated
    /// to `Settings` at depth 2 would be testing the refusal by accident and asserting nothing about
    /// depth at all.
    const A_DRAWN_PAGE: Page = Page::Work;

    /// **T-15.** Every surface can be left, and the bound is the stack's own arithmetic.
    ///
    /// The build contract asked for *≤ 3 `escape()` calls*, which is arithmetically impossible
    /// under the order it specifies in the same paragraph — fullscreen and an Expand are two
    /// presses before the drawer is even reached, and §4 allows three levels inside it. So the
    /// bound is derived from the terms, exactly as `the_column_terms_sum_to_the_declared_chrome`
    /// does with §9.6's column, and the number it produces is asserted.
    #[test]
    fn every_surface_can_be_left() {
        for page in EVERY_PAGE {
            for depth in 0..=max_depth() {
                for expand in [false, true] {
                    for fullscreen in [false, true] {
                        let mut s = Stack::new();
                        s.go(page, depth);
                        if expand {
                            s.expand_opened(7);
                        }
                        if fullscreen {
                            s.enter_fullscreen();
                        }

                        let mut presses = 0;
                        loop {
                            let what = s.escape(false, false);
                            if what == Escape::Nothing {
                                break;
                            }
                            presses += 1;
                            assert!(
                                presses <= worst_case(),
                                "{page:?} at depth {depth} (expand {expand}, fullscreen \
                                 {fullscreen}) needs more than {} presses to leave",
                                worst_case()
                            );
                        }
                        assert!(!s.open(), "the drawer is still open after {presses} presses");
                        assert!(!s.fullscreen());
                        assert!(s.expand().is_none());
                    }
                }
            }
        }

        // And the worst case really is the worst case, rather than a bound nothing reaches.
        let mut s = Stack::new();
        s.go(A_DRAWN_PAGE, max_depth());
        s.expand_opened(1);
        s.enter_fullscreen();
        let mut presses = 0;
        while s.escape(false, false) != Escape::Nothing {
            presses += 1;
        }
        assert_eq!(
            presses,
            worst_case(),
            "the deepest surface takes {presses} presses and the derivation says {}",
            worst_case()
        );
        assert_eq!(worst_case(), 6, "the terms moved: 1 fullscreen + 1 expand + 3 levels + 1 close");
    }

    /// **T-16.** `Esc` from `Booting` powers off and never parks, and neither `Esc` nor `⌘\` writes
    /// `depth`.
    #[test]
    fn escape_from_booting_powers_off_and_never_parks() {
        let mut s = Stack::new();
        assert_eq!(
            s.escape(true, false),
            Escape::PowerOff,
            "escaping a boot parked it — that is a 1.6 GB write of a state nobody wants"
        );
        assert_eq!(s.escape(false, true), Escape::Park);
        assert_eq!(s.escape(false, false), Escape::Nothing);

        // The drawer comes first either way: a boot is not interrupted by a key that had somewhere
        // nearer to go.
        let mut s = Stack::new();
        s.go(Page::Work, 1);
        assert_eq!(s.escape(true, false), Escape::WentBack);
        assert_eq!(s.escape(true, false), Escape::ClosedDrawer);
        assert_eq!(s.escape(true, false), Escape::PowerOff);

        // **Neither closing key writes `depth`.** Close and reopen and you are where you were.
        for depth in 0..=max_depth() {
            let mut s = Stack::new();
            s.go(A_DRAWN_PAGE, depth);

            s.toggle();
            assert!(!s.open());
            assert_eq!(s.depth(), depth, "⌘\\ moved the depth at level {depth}");
            s.toggle();
            assert!(s.open());
            assert_eq!(s.depth(), depth, "reopening did not return to level {depth}");
            assert_eq!(s.page(), if depth == 0 { Page::None } else { A_DRAWN_PAGE });

            s.close();
            assert_eq!(s.depth(), depth, "close() moved the depth at level {depth}");
        }

        // And `Esc` on a closed drawer touches nothing.
        let mut s = Stack::new();
        s.go(A_DRAWN_PAGE, 2);
        s.close();
        assert_eq!(s.escape(false, false), Escape::Nothing);
        assert_eq!(s.depth(), 2, "Esc moved the depth of a drawer that was not even open");
    }

    /// §4: three levels, and a fourth is clamped rather than drawn into a blank slot.
    #[test]
    fn the_drawer_is_never_deeper_than_the_strip_has_slots() {
        let mut s = Stack::new();
        s.go(A_DRAWN_PAGE, 9);
        assert_eq!(s.depth(), max_depth());
        assert_eq!(max_depth(), 3, "§4 says max 3 levels deep");
        // The strip is one slot per level plus a blank at each end, because `lively` overshoots.
        assert!(
            (geometry::DRAWER_STRIP_SLOTS - (geometry::DRAWER_MAX_DEPTH + 3.0)).abs() < 1e-9,
            "the strip has {} slots for {} levels",
            geometry::DRAWER_STRIP_SLOTS,
            geometry::DRAWER_MAX_DEPTH
        );
    }

    /// `Esc` is not a way *into* the drawer — §4 already has four.
    #[test]
    fn escape_never_opens_the_drawer() {
        let mut s = Stack::new();
        for _ in 0..5 {
            s.escape(false, false);
            assert!(!s.open(), "Esc opened the drawer");
        }
    }

    /// A stale close from a subtree that has been replaced cannot shut the Expand that is open now.
    #[test]
    fn only_the_named_expand_closes() {
        let mut s = Stack::new();
        s.expand_opened(3);
        s.expand_closed(4);
        assert_eq!(s.expand(), Some(3), "a different Expand's close shut this one");
        s.expand_closed(3);
        assert!(s.expand().is_none());
    }

    /// **A page nothing draws is not somewhere you can be sent.**
    ///
    /// `⌘,` was wired to `Reference` at depth 1 while the depth-1 slot has exactly one child, the
    /// Work page. What it produced was a 420 px `bg-raised` panel with no header, no text and no
    /// visible way out — `Esc` and the handle worked, silently — and it bypassed the very row that
    /// states the gap: `MenuPage` draws Reference disabled with *"The keyboard table and the stated
    /// limits have no page yet."* Every other unbuilt page in this program is unreachable and says
    /// why; that was the one hole in the policy.
    #[test]
    fn a_page_nothing_draws_lands_on_the_menu() {
        for p in [
            Page::Devices,
            Page::Parts,
            Page::Games,
            Page::Readout,
            Page::Settings,
            Page::Reference,
        ] {
            assert_eq!(p.slot(), None, "{p:?} claims a depth slot; which child draws it?");
            for depth in 0..=4 {
                let mut s = Stack::new();
                s.go(p, depth);
                assert!(s.open(), "{p:?} at {depth} did not open the drawer at all");
                assert_eq!(
                    s.depth(),
                    0,
                    "{p:?} at depth {depth} navigated to a slot with no page in it — a blank panel \
                     with no header and therefore no visible way out"
                );
                assert_eq!(s.page(), Page::None, "{p:?} at {depth} is drawn by nothing");
            }
        }

        // The control: the two pages that ARE drawn still go where they are sent.
        assert_eq!(Page::None.slot(), Some(0));
        assert_eq!(Page::Work.slot(), Some(1));
        let mut s = Stack::new();
        s.go(Page::Work, 1);
        assert_eq!((s.depth(), s.page()), (1, Page::Work), "the one built page stopped working");
    }
}
