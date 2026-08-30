//! Everything the window is told about its own size, decided in one place.
//!
//! `docs/GUI.md` §6.6 and §16.1. There is no toolkit in this file and no platform in it, so every
//! rule below is testable on a build machine with no display — which matters, because the two
//! failure modes §16.1 describes (drag the bottom edge up; drag onto a second monitor of the same
//! scale factor) are both invisible until somebody is looking at the wrong window on the wrong
//! screen.
//!
//! **Nothing here is persisted.** `k` is decided from the display, every launch. A remembered `k`
//! would survive a move from a 2× display to a 1× one and put the shelf — the row carrying
//! `write_target()` — below the bottom edge.

use crate::geometry;

/// Everything the window is told about size.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Fit {
    /// Whole-number framebuffer scale. At least 1, at most [`geometry::K_MAX`].
    pub k: i32,
    /// `k × HERO_PHYS_1X / sf` — the length pushed into `MainWindow::set_hero`.
    pub hero_logical: f64,
    /// The panel, logical, walked up by ULPs until the f32 round-trip is exact.
    pub panel_w: f64,
    pub panel_h: f64,
    /// §9.5. Computed from the **measured** client height, with hysteresis.
    pub too_short: bool,
}

/// The four moments that may change a [`Fit`], and they are not interchangeable.
///
/// **Two measurements, not one, and conflating them is a defect this file already had.** `k` comes
/// from *what could this display hold* and the too-short boolean from *how much height is there
/// right now* (§6.6, §9.5, §16.1). A single `client_logical` fed to both meant that after a move,
/// the warning was computed from the display the window is on rather than from the window: drag the
/// bottom edge up on a 923 px display, then move the window, and the shelf — carrying
/// `write_target()` — went back to being drawn past the bottom edge with the boolean reading false.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Moment {
    /// The window is on screen for the first time. Decides `k`.
    Shown {
        display_logical: f64,
        window_logical: f64,
        sf: f64,
    },
    /// A plain resize. Recomputes `too_short` and **nothing else** — §6.6, principle 1: dragging a
    /// window edge is not a request to redraw the iPod at a different size. It carries no display
    /// height because it is not evidence about one.
    Resized { window_logical: f64 },
    /// Possibly a different display. Re-decides `k`.
    Moved {
        display_logical: f64,
        window_logical: f64,
        sf: f64,
    },
    /// Re-decides `k`. Carries the event's own scale factor, because `slint::Window::scale_factor()`
    /// may still report the old one while this is being handled.
    ScaleFactorChanged {
        display_logical: f64,
        window_logical: f64,
        sf: f64,
    },
}

impl Moment {
    /// The display's usable height, when this moment is evidence about one. `None` for a plain
    /// resize, and that `None` is what stops `k` moving under a drag.
    fn display_logical(self) -> Option<f64> {
        match self {
            Moment::Shown {
                display_logical, ..
            }
            | Moment::Moved {
                display_logical, ..
            }
            | Moment::ScaleFactorChanged {
                display_logical, ..
            } => Some(display_logical),
            Moment::Resized { .. } => None,
        }
    }

    /// The window's own height. Every moment carries one, because every moment can change it.
    fn window_logical(self) -> f64 {
        match self {
            Moment::Shown { window_logical, .. }
            | Moment::Moved { window_logical, .. }
            | Moment::ScaleFactorChanged { window_logical, .. }
            | Moment::Resized { window_logical } => window_logical,
        }
    }

    /// The scale factor to use — `None` when the moment does not carry one, in which case the
    /// previous one stands.
    fn sf(self) -> Option<f64> {
        match self {
            Moment::Shown { sf, .. }
            | Moment::Moved { sf, .. }
            | Moment::ScaleFactorChanged { sf, .. } => Some(sf),
            Moment::Resized { .. } => None,
        }
    }
}

/// Holds the previous answer, because hysteresis needs it and `k` is sticky.
#[derive(Clone, Copy, Debug)]
pub struct Fitter {
    fit: Fit,
    sf: f64,
}

impl Fitter {
    /// Before there is a window on a display: `k = 1`, not too short.
    pub fn new(sf: f64) -> Fitter {
        let sf = sane(sf);
        let (panel_w, panel_h) = geometry::panel_logical(1, sf);
        Fitter {
            fit: Fit {
                k: 1,
                hero_logical: geometry::hero_logical(1, sf),
                panel_w,
                panel_h,
                too_short: false,
            },
            sf,
        }
    }

    pub fn fit(&self) -> Fit {
        self.fit
    }

    /// Returns the new [`Fit`], and whether it differs from the previous one.
    ///
    /// The caller pushes to Slint only when it does, so a drag does not re-set five properties
    /// sixty times a second.
    pub fn apply(&mut self, moment: Moment) -> (Fit, bool) {
        let sf = sane(moment.sf().unwrap_or(self.sf));

        // From the DISPLAY: what could this screen hold at all. A plain resize is not evidence
        // about that, so it brings none and the previous answer stands.
        let k = match moment.display_logical() {
            Some(display) => geometry::decide_k(display, sf),
            None => self.fit.k,
        };

        let hero_logical = geometry::hero_logical(k, sf);
        let (panel_w, panel_h) = geometry::panel_logical(k, sf);

        // From the WINDOW: how much height there is right now. **The asymmetry is the point.** A
        // symmetric comparison flutters at the boundary under a drag, which is a window that
        // flickers between two layouts while the mouse is down.
        let window = moment.window_logical();
        let threshold = required_client_logical(hero_logical);
        let too_short = if self.fit.too_short {
            window < threshold + geometry::HYSTERESIS
        } else {
            window < threshold
        };

        let fit = Fit {
            k,
            hero_logical,
            panel_w,
            panel_h,
            too_short,
        };
        let changed = fit != self.fit;
        self.fit = fit;
        self.sf = sf;
        (fit, changed)
    }
}

/// A scale factor that cannot produce an infinite `hero`.
///
/// Zero cannot arrive from the platform, but a NaN or a zero pushed into `set_hero` produces a
/// layout with no diagnostic at all — which is worse than a wrong number, because a wrong number is
/// visible.
fn sane(sf: f64) -> f64 {
    if sf.is_finite() { sf.max(0.1) } else { 1.0 }
}

/// The logical height a window needs for a body of `hero_logical`.
pub fn required_client_logical(hero_logical: f64) -> f64 {
    hero_logical + geometry::CHROME_MIN
}

/// §9.6's one line a ruler can check: a display needs `655.751 + 154 × sf` **physical** pixels of
/// window height for `k = 1`. 810 at 100 %, 848 at 125 %, 887 at 150 %, 964 at 200 %.
pub fn required_client_physical(k: i32, sf: f64) -> f64 {
    geometry::hero_phys(k) + geometry::CHROME_MIN * sf
}

#[cfg(test)]
mod tests {
    use super::*;

    /// §16.1: "with hysteresis (drop below the threshold, restore 20 px above it)".
    ///
    /// Both directions matter, so both are asserted: a symmetric comparison in either place makes
    /// one of the four steps wrong.
    #[test]
    fn the_too_short_boolean_has_hysteresis() {
        let mut f = Fitter::new(1.0);
        let t = required_client_logical(geometry::hero_logical(1, 1.0));
        assert!((t - 809.751).abs() < 0.01, "the threshold moved: {t:.3}");

        let (a, _) = f.apply(Moment::Resized { window_logical: 809.0 });
        assert!(a.too_short, "809.0 is below the {t:.1} threshold");

        let (b, _) = f.apply(Moment::Resized { window_logical: 815.0 });
        assert!(
            b.too_short,
            "815.0 is above {t:.1} but below the {:.1} restore point, so it must stay short",
            t + geometry::HYSTERESIS
        );

        let (c, _) = f.apply(Moment::Resized { window_logical: 830.0 });
        assert!(!c.too_short, "830.0 is past the restore point");

        let (d, _) = f.apply(Moment::Resized { window_logical: 820.0 });
        assert!(!d.too_short, "820.0 is above {t:.1} and it was not short, so it stays not short");
    }

    /// **The display decides `k`; the window decides the warning.** Two questions, two
    /// measurements, and the defect this test exists for is what happens when one value answers
    /// both: a window dragged short, then moved to a taller display, comes back reporting that it
    /// has room — because the *display* has room. §9.5's replacement pane goes away and §7.5's
    /// shelf, which carries `write_target()`, is positioned past the bottom edge and drawn there.
    ///
    /// It reads on `Moved`, and it read the same way on `Shown` and `ScaleFactorChanged` — three of
    /// the four moments, against §9.5, §9.6 and §16.1, all three of which say the boolean comes
    /// from the window.
    #[test]
    fn the_display_decides_k_and_the_window_decides_the_warning() {
        let mut f = Fitter::new(2.0);
        let (shown, _) = f.apply(Moment::Shown {
            display_logical: 923.0,
            window_logical: 846.0,
            sf: 2.0,
        });
        assert_eq!(shown.k, 2, "the operator's own display is k = 2");
        assert!(!shown.too_short, "846 logical holds a 655.8 px body");

        let (small, _) = f.apply(Moment::Resized { window_logical: 500.0 });
        assert!(small.too_short, "500 logical cannot hold it");

        // A bigger display, and the window is still 500 px tall.
        let (moved, _) = f.apply(Moment::Moved {
            display_logical: 1200.0,
            window_logical: 500.0,
            sf: 2.0,
        });
        assert_eq!(moved.k, 3, "k did not follow the taller display");
        assert!(
            moved.too_short,
            "the warning was computed from the display: the window is still 500 logical"
        );

        // And the same on the other two k-deciding moments.
        let (sfc, _) = f.apply(Moment::ScaleFactorChanged {
            display_logical: 1200.0,
            window_logical: 500.0,
            sf: 2.0,
        });
        assert!(sfc.too_short, "a scale-factor change read the display instead");

        let mut fresh = Fitter::new(2.0);
        let (first, _) = fresh.apply(Moment::Shown {
            display_logical: 1200.0,
            window_logical: 500.0,
            sf: 2.0,
        });
        assert!(first.too_short, "the first frame read the display instead");
    }

    /// §6.6, principle 1: dragging a window edge is not a request to redraw the iPod.
    #[test]
    fn a_plain_resize_never_changes_k() {
        let mut f = Fitter::new(2.0);
        let (shown, _) = f.apply(Moment::Shown {
            display_logical: 891.0,
            window_logical: 891.0,
            sf: 2.0,
        });
        assert_eq!(shown.k, 2, "the operator's own display is k = 2");

        let (small, _) = f.apply(Moment::Resized { window_logical: 400.0 });
        assert_eq!(small.k, 2, "a resize moved k from 2 to {}", small.k);
        assert!(small.too_short, "400 logical cannot hold a 655.8 px body");

        let (big, _) = f.apply(Moment::Resized { window_logical: 3000.0 });
        assert_eq!(big.k, 2, "a resize moved k from 2 to {}", big.k);
        assert!(!big.too_short);
    }

    /// The other half — `Moved` **does** re-decide, which is §16.1's second failure mode: a second
    /// monitor of the same scale factor, where no `ScaleFactorChanged` fires.
    #[test]
    fn a_move_to_a_shorter_display_takes_k_down() {
        let mut f = Fitter::new(2.0);
        let (shown, _) = f.apply(Moment::Shown {
            display_logical: 891.0,
            window_logical: 891.0,
            sf: 2.0,
        });
        assert_eq!(shown.k, 2);

        // A 1280×800 display: 735 usable, and the window is clamped to what is there.
        let (moved, changed) = f.apply(Moment::Moved {
            display_logical: 735.0,
            window_logical: 735.0,
            sf: 1.0,
        });
        assert!(changed, "moving to a different display changed nothing");
        assert_eq!(moved.k, 1, "k stayed {} on a 735 px display", moved.k);
        assert!(moved.too_short, "735 < 809.8, so §9.5's pane is the honest answer");
    }

    /// Every row of §9.6's display table, derived from the constants rather than typed twice.
    #[test]
    fn the_nine_six_table_holds() {
        // display, sf, client (Dock hidden), expected k, expected spare (negative = short)
        let rows: &[(&str, f64, f64, i32, f64)] = &[
            ("1280x800", 1.0, 735.0, 1, -75.0),
            ("1366x768", 1.0, 689.0, 1, -121.0),
            ("1440x900", 1.0, 835.0, 1, 25.0),
            ("1470x956 (the operator's)", 2.0, 891.0, 2, 81.0),
            ("1920x1080 @125%", 1.25, 801.0, 1, 122.0),
            ("1920x1080 @150%", 1.5, 667.0, 1, 76.0),
        ];
        for (name, sf, client, want_k, want_spare) in rows {
            let k = geometry::decide_k(*client, *sf);
            assert_eq!(k, *want_k, "{name}: k is {k}, the table says {want_k}");
            let spare = client - required_client_logical(geometry::hero_logical(k, *sf));
            assert!(
                (spare - want_spare).abs() < 1.0,
                "{name} should be {} and is {}",
                describe(*want_spare),
                describe(spare)
            );
        }
    }

    fn describe(spare: f64) -> String {
        if spare < 0.0 {
            format!("{:.0} short", -spare)
        } else {
            format!("{spare:.0} spare")
        }
    }

    /// §9.6's general rule and §9.6's table are the same arithmetic — the shape `min-width` got
    /// wrong when its parenthetical summed to 449 beside a declared 880.
    #[test]
    fn the_general_rule_is_the_same_arithmetic_as_the_table() {
        for (sf, want) in [(1.0, 810.0), (1.25, 848.0), (1.5, 887.0), (2.0, 964.0)] {
            let got = required_client_physical(1, sf);
            assert!(
                (got - want).abs() < 1.0,
                "at {sf}× a display needs {got:.0} physical px; §9.6 says {want:.0}"
            );
        }
        for sf in geometry::SF_SWEEP {
            let from_logical = required_client_logical(geometry::hero_logical(1, sf)) * sf;
            let from_rule = required_client_physical(1, sf);
            assert!(
                (from_logical - from_rule).abs() < 1e-6,
                "at {sf}× the two derivations give {from_logical:.4} and {from_rule:.4}"
            );
        }
    }

    /// A hostile or transitional scale factor cannot push an infinity into `set_hero`, where it
    /// would produce a layout with no diagnostic at all.
    #[test]
    fn a_zero_scale_factor_does_not_produce_a_nan_hero() {
        let mut f = Fitter::new(0.0);
        assert!(f.fit().hero_logical.is_finite(), "the seed fit is not finite");

        let (fit, _) = f.apply(Moment::ScaleFactorChanged {
            display_logical: 900.0,
            window_logical: 900.0,
            sf: 0.0,
        });
        assert!(
            fit.hero_logical.is_finite() && fit.panel_w.is_finite() && fit.panel_h.is_finite(),
            "a zero scale factor produced {fit:?}"
        );
        assert!(fit.k >= 1 && fit.k <= geometry::K_MAX);
    }
}
