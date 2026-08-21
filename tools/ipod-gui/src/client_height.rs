//! How much of the display this window may actually use — measured, not predicted.
//!
//! `docs/GUI.md` §9.6, §20 item 8. The previous revision computed the ceiling as
//! `screen − 33 (menu bar) − 32 (title bar)`. That 33 was measured on one machine with the Dock set
//! to auto-hide; with the Dock at its default visible size the client loses a further 70–90 px, so
//! the design's headline answer to *"which displays can show the panel at 1:1"* was true only for
//! the one configuration it was measured on, and the document never said so.
//!
//! **This is a two-platform fact, and it is stated rather than pretended.** winit's `MonitorHandle`
//! exposes only `size()` and `position()` (`winit-0.30.13/src/monitor.rs:118,125`) — there is no
//! work-area API — and Wayland does not publish a work area at all. So [`WorkArea`] names which
//! answer this build can give, and [`WorkArea::describe`] is the sentence Reference prints. It never
//! says "unknown"; it says what was measured.
//!
//! This file and `main.rs` are the only two in the crate that touch the toolkit, and this one only
//! because the question — *which display is this window on* — cannot be asked without a window.

/// What the platform will tell us about usable display height, and why not when it will not.
//
// **Exactly one variant is constructed per target** — [`support`] is a `const fn` of the target,
// not of the display — so the other two are always "never constructed" in any single build. They
// are the other platforms' answers, and the enum is the closed set of them; dropping the two this
// build does not use would mean the honest gap could not be stated at all.
#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WorkArea {
    /// macOS: `NSScreen.visibleFrame` for the screen the window is on. Subtracts the menu bar and
    /// the Dock, including a Dock at its default visible size.
    VisibleFrame,
    /// Windows: `SPI_GETWORKAREA`, which is the **primary** display's work area and not necessarily
    /// the one the window is on.
    PrimaryDisplay,
    /// Everywhere else.
    Unpublished,
}

impl WorkArea {
    /// The one sentence Reference prints. No full stop — Reference adds its own.
    pub fn describe(self) -> &'static str {
        match self {
            WorkArea::VisibleFrame => {
                "the usable height of the display this window is on, from NSScreen.visibleFrame — \
                 the menu bar and the Dock are already taken off"
            }
            WorkArea::PrimaryDisplay => {
                "the primary display's work area, from SPI_GETWORKAREA — not necessarily the \
                 display this window is on"
            }
            WorkArea::Unpublished => {
                "no work area is published on this platform, so the fit is decided from the window \
                 this program actually got"
            }
        }
    }
}

/// Which of the three this build can do. A `const fn` of the target, not of the display.
pub const fn support() -> WorkArea {
    #[cfg(target_os = "macos")]
    {
        WorkArea::VisibleFrame
    }
    #[cfg(target_os = "windows")]
    {
        WorkArea::PrimaryDisplay
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        WorkArea::Unpublished
    }
}

/// The usable height of the display the window is on, in **logical** pixels.
///
/// `None` on every platform where [`support`] is [`WorkArea::Unpublished`], and also on
/// macOS/Windows when the call fails or the window has no winit window yet — before the event loop
/// is running, `WinitWindowAccessor::with_winit_window` returns `None`
/// (`i-slint-backend-winit-1.17.1/lib.rs:967-971`). That is not an error; the caller seeds `k` from
/// `geometry::PREF_HEIGHT` and the first real event corrects it.
///
/// **Never panics. Never blocks. Main thread only** — on macOS it reaches AppKit.
pub fn client_height_logical(window: &slint::Window) -> Option<f64> {
    measure(window)
}

// ── macOS ───────────────────────────────────────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
fn measure(window: &slint::Window) -> Option<f64> {
    use i_slint_backend_winit::winit::platform::macos::MonitorHandleExtMacOS;
    use i_slint_backend_winit::WinitWindowAccessor;
    use objc2_app_kit::NSScreen;

    window.with_winit_window(|w| {
        let ptr = w.current_monitor()?.ns_screen()? as *mut NSScreen;
        // SAFETY: winit hands this pointer over UNRETAINED — `ns_screen` is
        // `Retained::as_ptr(&s)` on a temporary that is dropped as the function returns
        // (`winit-0.30.13/src/platform/macos.rs:472-476`). AppKit's own screen list keeps the
        // object alive in practice, and it would keep appearing to work under a use-after-free.
        // So take a reference of our own before the first message send, and never let it outlive
        // this main-thread turn.
        let screen = unsafe { objc2::rc::Retained::retain(ptr) }?;
        // Cocoa points are Slint's logical pixels on macOS, so there is no division here.
        Some(screen.visibleFrame().size.height)
    })?
}

// ── Windows ─────────────────────────────────────────────────────────────────────────────────────

// The per-monitor-correct call is `GetMonitorInfoW(MonitorFromWindow(hwnd, …))->rcWork`, and it is
// deliberately not taken: winit 0.30 removed `WindowExtWindows::hwnd()`, so reaching an HWND means
// adding `raw-window-handle` and new unsafe to sharpen a value that is a hint rather than the
// mechanism. §9.6's too-short boolean is computed from the window we actually got.
#[cfg(target_os = "windows")]
fn measure(window: &slint::Window) -> Option<f64> {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::UI::WindowsAndMessaging::{SPI_GETWORKAREA, SystemParametersInfoW};

    let mut r = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    // SAFETY: `r` is a live, correctly sized RECT and SPI_GETWORKAREA writes exactly one.
    let ok = unsafe {
        SystemParametersInfoW(SPI_GETWORKAREA, 0, (&mut r as *mut RECT).cast(), 0)
    };
    if ok == 0 {
        return None;
    }
    // winit requests per-monitor-v2 DPI awareness, so this is PHYSICAL pixels — unlike
    // `visibleFrame` above, which is already logical. Forgetting this division gives a value that
    // is right at 100 % and half-size at 200 %.
    Some(f64::from(r.bottom - r.top) / f64::from(window.scale_factor()))
}

// ── Everywhere else ─────────────────────────────────────────────────────────────────────────────

// And there is a sharper reason this matters on Wayland than "no work area": `WindowEvent::Moved`
// is documented Unsupported there (`winit-0.30.13/src/event.rs:163-168`), so the `Moved` recompute
// never fires either. `Resized` is the only signal, and the measured path is the only path.
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn measure(_window: &slint::Window) -> Option<f64> {
    None
}

/// `IPOD_LAYOUT=1` — print the measurements the size constants derive from, and the fit they
/// produced.
///
/// `docs/DEVELOPING.md` has documented this flag for some time — with an example output in a format
/// nothing ever printed — and `grep -rn IPOD_LAYOUT tools/` returned nothing (§16.10: *either build
/// it or delete the claim*). This is the build half. It is the only thing in the program that prints
/// a measured display height, which is what makes the reader observable at all — and it is what
/// caught `k` being decided from the window rather than from the display.
pub fn dump_layout(window: &slint::Window, fit: &crate::fit::Fit) {
    if std::env::var_os("IPOD_LAYOUT").is_none_or(|v| v.is_empty() || v == "0") {
        return;
    }
    let sf = f64::from(window.scale_factor());
    let size = window.size();
    eprintln!("── IPOD_LAYOUT ────────────────────────────────────────────");
    eprintln!("  work area   {:?} — {}", support(), support().describe());
    // Two different `None`s, and printing one sentence for both would be an instrument lying: on
    // macOS the reader answers `None` at the seed call because there is no winit window yet, which
    // is not the same statement as "this platform publishes no work area".
    eprintln!(
        "  display     {}",
        match (support(), client_height_logical(window)) {
            (_, Some(h)) => format!("{h:.1} logical px usable"),
            (WorkArea::Unpublished, None) => "not published on this platform".to_string(),
            (_, None) => "no answer yet — the window is not on a display".to_string(),
        }
    );
    eprintln!(
        "  window      {} x {} physical, {:.1} x {:.1} logical, scale {sf}",
        size.width,
        size.height,
        f64::from(size.width) / sf,
        f64::from(size.height) / sf
    );
    eprintln!(
        "  fit         k = {}, body {:.3} logical ({:.3} physical), panel {:.4} x {:.4}{}",
        fit.k,
        fit.hero_logical,
        crate::geometry::hero_phys(fit.k),
        fit.panel_w,
        fit.panel_h,
        if fit.too_short { ", too short for 1:1" } else { "" }
    );
    eprintln!(
        "  needs       {:.1} logical / {:.1} physical for k = {}",
        crate::fit::required_client_logical(fit.hero_logical),
        crate::fit::required_client_physical(fit.k, sf),
        fit.k
    );
    let (glass_w, glass_h) = crate::geometry::glass_phys(fit.k);
    eprintln!(
        "  glass       {glass_w:.1} x {glass_h:.1} physical, {:.2} px surround on all four sides",
        crate::geometry::bezel_phys()
    );
    eprintln!(
        "  inset       {:.5} of body height at the sides, {:.5} at the top",
        crate::geometry::left_inset(),
        crate::geometry::SCREEN_TOP
    );
    // The constants once, not on every event: this is called on each change to the fit, and a
    // startup burst or a drag across a display boundary would otherwise bury the four lines that
    // moved under two hundred that did not.
    static CONSTANTS: std::sync::Once = std::sync::Once::new();
    CONSTANTS.call_once(|| {
        eprintln!("  ── the constants, from src/geometry.rs ──");
        for (name, unit, value) in crate::geometry::ALL {
            eprintln!(
                "  {:<24} {value}{}",
                crate::geometry::slint_name(name),
                match unit {
                    crate::geometry::Unit::Px => "px",
                    crate::geometry::Unit::Ratio => "",
                }
            );
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// [`support`] and the compiled `measure` cannot disagree — the honest-gap claim Reference
    /// prints is the same fact as the code path taken.
    #[test]
    fn the_support_answer_matches_the_target() {
        let want = if cfg!(target_os = "macos") {
            WorkArea::VisibleFrame
        } else if cfg!(target_os = "windows") {
            WorkArea::PrimaryDisplay
        } else {
            WorkArea::Unpublished
        };
        assert_eq!(
            support(),
            want,
            "support() says {:?} but this build measures {want:?}",
            support()
        );
    }

    /// A window with no winit window behind it answers `None` rather than panicking.
    ///
    /// This is the one branch that cannot be reasoned about from the signature: before the event
    /// loop is running, and under any backend that is not winit, `with_winit_window` returns `None`
    /// — and the `?` that turns that into `None` here is the whole of what stops the reader
    /// panicking at startup, which is the moment it is first called.
    ///
    /// The headless testing backend is exactly that case: its adapter is not a `WinitWindowAdapter`,
    /// the downcast fails, and the closure never runs. `Once` because `set_platform` is
    /// process-global and panics on a second call.
    #[test]
    fn a_missing_winit_window_is_none_not_a_panic() {
        use i_slint_backend_winit::WinitWindowAccessor;
        use slint::ComponentHandle;
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(i_slint_backend_testing::init_no_event_loop);

        let window = crate::MainWindow::new().expect("the headless backend makes a window");
        assert!(
            !window.window().has_winit_window(),
            "the testing backend produced a winit window; this test is measuring the wrong thing"
        );
        assert_eq!(
            client_height_logical(window.window()),
            None,
            "a window with no winit window behind it has to answer None"
        );
    }

    /// §9.6's "stated in Reference rather than pretended".
    #[test]
    fn every_work_area_answer_has_a_sentence() {
        for w in [WorkArea::VisibleFrame, WorkArea::PrimaryDisplay, WorkArea::Unpublished] {
            let s = w.describe();
            assert!(!s.is_empty(), "{w:?} has no sentence");
            assert!(
                !s.ends_with('.'),
                "{w:?} ends with a full stop; Reference adds its own: {s}"
            );
            assert!(
                !s.to_ascii_lowercase().contains("unknown"),
                "{w:?} says 'unknown'; the whole point is that each answer names what it \
                 measured: {s}"
            );
        }
    }
}
