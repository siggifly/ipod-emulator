//! The window.
//!
//! [`docs/GUI.md`](../../../docs/GUI.md) is the design this implements; the markup is
//! `ui/window.slint`, compiled to Rust by `build.rs`. This file is the wiring between the model —
//! which lives in `eapp-loader` and knows nothing about any toolkit — and that markup.
//!
//! **The separation is the point.** `settings.rs`, `compose.rs`, `identity.rs` and `nor.rs` hold
//! the device model, the compatibility rules and the identity validation, and none of them has ever
//! imported a UI crate. That is why replacing an 8,039-line window cost one file, and it is worth
//! keeping for whoever replaces this one.

// The window is not a console program; on Windows a console would flash up behind it.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// **These five modules are not dead. They are not yet reconnected.**
//
// The view layer that called them went with `main.rs` when the old window was deleted, and each
// comes back as its surface is rebuilt: `emu` and `wheel` with Running (GUI.md §16), `control` with
// the readout Rail, `png` with the screenshot key, `update` with Reference (§17). Their own tests
// still run — 30 of them pass — so this is unreferenced code, not unverified code.
//
// **The allow is a debt and it has a retirement condition**, in the shape research/04 uses for
// bypasses: it comes off when Running lands, and at that point anything *still* unreferenced is
// genuinely dead and gets deleted rather than allowed. Scoped per module deliberately — a blanket
// `#![allow(dead_code)]` at the crate root would outlive the rebuild and swallow the next real one.
#[allow(dead_code)]
mod control;
#[allow(dead_code)]
mod emu;
#[allow(dead_code)]
mod png;
#[allow(dead_code)]
mod update;
#[allow(dead_code)]
mod wheel;

// These three are wired, and they are the model the window's own size is decided from.
//
// `geometry` is the single source of truth for every ratio and every size constant — `build.rs`
// compiles that same file and renders it into the `.slint` the markup imports, so the tests read
// what the markup reads (GUI.md §16.9). `fit` turns a measured height and a scale factor into the
// one `k` and the one too-short boolean (§6.6, §16.1); it is pure, so all of it is testable with no
// display. `client_height` is the only part that has to ask the platform (§9.6).
mod client_height;
mod fit;
mod geometry;

use eapp_loader::identity::Colour;
use eapp_loader::settings::{Device, Presence, Settings};
// Only for `on_winit_window_event`: `Resized`, `Moved` and `ScaleFactorChanged` are the three
// moments §16.1 says the fit has to be recomputed at, and Slint exposes none of them.
use i_slint_backend_winit::WinitWindowAccessor;
use slint::{ModelRc, SharedPixelBuffer, VecModel};

slint::include_modules!();

fn main() -> Result<(), slint::PlatformError> {
    opaque_window()?;

    // `load_and_seed` rather than `load`: the window is the surface that owns the library, so it is
    // the one entitled to persist what seeding produced — without which the marker that makes a
    // removal stick never reaches the file. `Settings::load` stays a pure read for `ipod-boot`,
    // where a save on a path as incidental as `--print` rewrote the operator's own file.
    let settings = Settings::load_and_seed();
    let window = MainWindow::new()?;

    window.set_devices(device_rows(&settings));
    window.set_resources(resource_rows(&settings));
    window.set_screen_source(dark_screen());

    // ── The fit: what size the iPod is drawn at, and whether it can be drawn at all ──
    //
    // GUI.md §6.6 and §16.1. `hero` is a length pushed IN from Rust and never read back out of the
    // markup — reading a size the layout itself decides is a binding loop, and Slint says so at
    // build time. The whole number `k` cannot be known until there is a window on a display, so the
    // seed uses the preferred height (the reader answers `None` before the event loop runs) and the
    // first real event corrects it.
    let sf = f64::from(window.window().scale_factor());
    let mut fitter = fit::Fitter::new(sf);
    let seed =
        client_height::client_height_logical(window.window()).unwrap_or(geometry::PREF_HEIGHT);
    // Before the event loop runs there is no window size to read — `win.size()` is zero — so the
    // seed's window half is the height the markup asked for. The first real `Resized` replaces it
    // with the height that was actually granted.
    fitter.apply(fit::Moment::Shown {
        display_logical: seed,
        window_logical: geometry::PREF_HEIGHT,
        sf,
    });
    push_fit(&window, &fitter.fit());
    client_height::dump_layout(window.window(), &fitter.fit());

    // **And it is recomputed while you are looking at it**, which is the promise the previous
    // design made and the platform cannot keep. Drag the bottom edge up to make room for a
    // terminal, or drag the window onto a second monitor of the same scale factor: neither fires
    // `ScaleFactorChanged`, every term in §9.6's column except the top margin is a fixed height, so
    // Slint's shrink adjuster can take nothing from any of them, and the trailing children — the
    // shelf, carrying `write_target()` — are positioned past the bottom edge and drawn there. The
    // user is then writing to a disk with the warning off screen.
    let weak = window.as_weak();
    let mut shown = false;
    window.window().on_winit_window_event(move |win, event| {
        use i_slint_backend_winit::winit::event::WindowEvent;

        // The size and the scale factor come from the EVENT rather than from `win`: during
        // `Resized` the window may still report the old size, and during `ScaleFactorChanged` the
        // old scale factor. Taking them from the window instead produces a fit that is one event
        // behind and self-corrects on the next one — invisible except as a single wrong frame
        // after every display change.
        //
        // **Every moment carries both measurements**, because they answer different questions:
        // `k` from the display's usable height, the too-short boolean from the window we actually
        // got (§6.6, §9.5, §16.1). One value fed to both meant that after a move the warning was
        // computed from the display — so a window dragged short and then moved reported that it
        // had room it did not have.
        let moment = match event {
            WindowEvent::Resized(size) => {
                let sf = live_scale(win);
                let own = f64::from(size.height) / sf;
                if shown {
                    // `k` is not re-decided here (§6.6, principle 1), so this brings no display
                    // height: dragging a window edge is not evidence about the screen.
                    fit::Moment::Resized { window_logical: own }
                } else {
                    // There is no "shown" event; the first `Resized` is it — and it decides `k`
                    // from the display, not from `own`.
                    shown = true;
                    fit::Moment::Shown {
                        display_logical: ceiling_logical(win),
                        window_logical: own,
                        sf,
                    }
                }
            }
            WindowEvent::Moved(_) => fit::Moment::Moved {
                display_logical: ceiling_logical(win),
                window_logical: own_height_logical(win),
                sf: live_scale(win),
            },
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                fit::Moment::ScaleFactorChanged {
                    display_logical: ceiling_logical(win),
                    // The event's own scale factor, not the window's: `win.scale_factor()` may
                    // still report the old one here, which would misconvert the physical size.
                    window_logical: f64::from(win.size().height) / sane_scale(*scale_factor),
                    sf: *scale_factor,
                }
            }
            _ => return i_slint_backend_winit::EventResult::Propagate,
        };

        let (fit, changed) = fitter.apply(moment);
        if changed {
            if let Some(w) = weak.upgrade() {
                push_fit(&w, &fit);
            }
            client_height::dump_layout(win, &fit);
        }
        // An observer, never a filter: every arm propagates.
        i_slint_backend_winit::EventResult::Propagate
    });

    // Starting a machine is the next slice; the callback exists so the button is honest about
    // being wired rather than drawn.
    window.on_start_device(|index| {
        eprintln!("start device {index} — not yet reconnected to the emulator");
    });
    window.on_new_device(|| {
        eprintln!("new device — not yet reconnected");
    });

    window.run()
}

/// Tell the markup what size to draw at. **One direction only** — nothing reads these back.
fn push_fit(window: &MainWindow, fit: &fit::Fit) {
    window.set_hero(fit.hero_logical as f32);
    window.set_screen_w(fit.panel_w as f32);
    window.set_screen_h(fit.panel_h as f32);
    window.set_screen_scale(fit.k);
    window.set_too_short(fit.too_short);
}

/// A scale factor that cannot divide by zero. Slint's is an `f32`.
fn live_scale(win: &slint::Window) -> f64 {
    sane_scale(f64::from(win.scale_factor()))
}

/// The same guard, for a scale factor that arrived on an event rather than from the window.
fn sane_scale(sf: f64) -> f64 {
    if sf.is_finite() && sf > 0.0 { sf } else { 1.0 }
}

/// The window's own height, logical.
fn own_height_logical(win: &slint::Window) -> f64 {
    f64::from(win.size().height) / live_scale(win)
}

/// The usable height of the **display**, falling back to the window's own where no work area is
/// published (§9.6: Wayland, and anywhere winit is not the backend).
///
/// **`k` is decided from this and never from the window.** Measured, not assumed: taking the
/// smaller of the two instead — which reads like the safer rule — makes `k` depend on the size
/// winit happens to report during window creation, and on this machine that produced a visible
/// wrong answer. `IPOD_LAYOUT=1` at startup showed the first `Resized` arriving with a size that is
/// not the final one, `k` settling to 1 and the too-short flag going up, then a later event
/// correcting it to `k = 2`. Reading the display makes the first answer the right one.
///
/// The too-short boolean is the other half and takes the other input — the window we actually got
/// (§9.5, §16.1). Two questions, two measurements: *what could this display hold* and *how much
/// height is there right now*.
fn ceiling_logical(win: &slint::Window) -> f64 {
    client_height::client_height_logical(win).unwrap_or_else(|| own_height_logical(win))
}

/// The list the window shows, built from the model.
///
/// **One [`Presence`] for the whole pass.** `summary` stats every path a device resolves to, and
/// `Presence`'s own rule is that one is made at the top of the pass that rebuilds the rows and
/// shared across every device in it — so N devices on one drive image cost one `stat` rather than
/// N. Calling `Settings::missing` per device instead mints a fresh cache each time and the sharing
/// the design is written around happens nowhere.
///
/// **The blocking half is still open**: a path under a stale network mount blocks until the mount
/// times out, and this runs on the UI thread before `window.run()`. The pass belongs off it,
/// together with §11.4's `detect_mounted()`; until then a share that is not up delays the first
/// window rather than one row of it.
fn device_rows(settings: &Settings) -> ModelRc<DeviceRow> {
    let mut seen = Presence::new();
    let rows: Vec<DeviceRow> = settings
        .devices
        .iter()
        .map(|d| {
            let chassis = d.chassis.unwrap_or_default();
            DeviceRow {
                name: d.name.clone().into(),
                summary: summary(settings, d, &mut seen).into(),
                // **`boot_instructions` is a denominator, not a history.** It is what the progress
                // bar divides by, and §12.3 has `Settings::set_boot_shape` clear it whenever the
                // recipe changes — so a device booted a dozen times renders this the moment its
                // bootloader is changed. "never started" is a claim about the past that the model
                // does not carry; what is missing is the number.
                state: if d.boot_instructions.is_some() {
                    "".into()
                } else {
                    "no boot time learned yet".into()
                },
                write_target: write_target(d).into(),
                write_target_is_warning: writes_to_your_own_image(settings, d),
                chassis: chassis_colour(chassis),
                dark_chassis: is_dark(chassis),
                // **A placeholder, and it is the nearest true statement available.**
                // `parked_at` answers *when*, never *whether*: the authority on whether there is a
                // restore point to resume is `emu::Config::may_restore()`, which stats the snapshot
                // and compares the drive it pairs with. The window holds no `Config` yet, so this
                // is at least no longer the hard-coded `false` it was.
                // Retirement condition, in the shape research/04 uses: when the bench holds a
                // `Config`, ask it, and delete this comment with the expression.
                parked: d.parked_at.is_some(),
            }
        })
        .collect();
    ModelRc::from(std::rc::Rc::new(VecModel::from(rows)))
}

/// Ask for a window that is not see-through.
///
/// Slint's winit backend creates a transparent window, which on macOS leaves the **system title
/// bar** showing whatever is behind the application. The client area looks right because we paint
/// every pixel of it; the title bar is drawn by the OS over a surface we have declared see-through,
/// so it shows the desktop.
///
/// There is no Slint-level property for this — `Window`'s `background` fills the client area only —
/// so the request has to be made where the window is actually created.
fn opaque_window() -> Result<(), slint::PlatformError> {
    let built = i_slint_backend_winit::Backend::builder()
        .with_window_attributes_hook(|attrs| attrs.with_transparent(false))
        .build();
    match built {
        Ok(backend) => slint::platform::set_platform(Box::new(backend))
            .map_err(|e| slint::PlatformError::Other(format!("{e:?}"))),
        // Not fatal, and not worth refusing to start over: a see-through title bar is ugly, and no
        // window at all is worse. Say so and carry on with the default backend.
        Err(e) => {
            eprintln!("could not ask for an opaque window ({e}); the title bar may be see-through");
            Ok(())
        }
    }
}

/// Everything a device can be made from, as a flat list with headings in it.
///
/// **All four groups are always present, even when empty** (GUI.md §14): a page whose sections come
/// and go is a page you have to re-learn every visit.
fn resource_rows(settings: &Settings) -> ModelRc<ResourceRow> {
    use eapp_loader::settings::{Provenance, Resource};
    let mut rows: Vec<ResourceRow> = Vec::new();

    // **Every trailing column is the model's answer, never a literal.** These four groups used to
    // print `fetched and verified` and `dumped from a real iPod` unconditionally, for files the
    // model knew nothing about. An item nobody recorded a provenance for contributes the empty
    // string — the row says nothing rather than lying (GUI.md §3.2).
    let says = |from: Option<Provenance>| from.map(|p| p.line()).unwrap_or_default();

    let group = |rows: &mut Vec<ResourceRow>, title: &str, empty: &str, items: Vec<(String, String)>| {
        rows.push(ResourceRow {
            heading: true,
            text: title.into(),
            detail: if items.is_empty() { "".into() } else { format!("{}", items.len()).into() },
        });
        if items.is_empty() {
            rows.push(ResourceRow { heading: false, text: empty.into(), detail: "".into() });
        }
        for (text, detail) in items {
            rows.push(ResourceRow { heading: false, text: text.into(), detail: detail.into() });
        }
    };

    // §3.1: a synthesised boot ROM is a resource exactly like a dumped one, listed together in the
    // same row shape. The model already agrees — `Resource::Firmware` carries a `nor::Source`, so
    // both kinds are already the same kind of thing here; only the window was of two minds.
    //
    // The trailing column is provenance, because that is the interesting fact about a boot ROM.
    // Not its size: a synthesised one has none, and printing "recipe" where every other row shows
    // megabytes advertises it as a lesser kind of thing.
    //
    // The model word is the caller's business — §11.4 puts it on the row's first line and the
    // provenance on the second — so only the model word is prepended here.
    let ipods: Vec<(String, String)> = settings
        .resources
        .iter()
        .filter_map(|i| match &i.what {
            Resource::Firmware(eapp_loader::nor::Source::File(_)) => {
                Some((i.name.clone(), says(i.from)))
            }
            Resource::Firmware(eapp_loader::nor::Source::Synthetic { model, .. }) => {
                Some((i.name.clone(), format!("{model} · {}", says(i.from))))
            }
            _ => None,
        })
        .collect();
    group(&mut rows, "iPods", "None yet — one is synthesised for you when you make a device.", ipods);

    group(&mut rows, "Apple firmware", "None yet — fetched from Apple when you make a device.",
        settings.resources.iter().filter(|i| matches!(i.what, Resource::Installer(_)))
            .map(|i| (i.name.clone(), says(i.from))).collect());

    group(&mut rows, "Software", "None yet — Rockbox is fetched and verified on request.",
        settings.resources.iter().filter(|i| matches!(i.what, Resource::Software(_)))
            .map(|i| (i.name.clone(), says(i.from))).collect());

    group(&mut rows, "Disks", "None yet — built from firmware, or provide your own.",
        settings.disks.iter().map(|k| {
            let mut what: Vec<String> = Vec::new();
            if let Some(b) = &k.built_from { what.push(b.clone()); }
            what.extend(k.installed.iter().cloned());
            (k.name.clone(), what.join(" · "))
        }).collect());

    ModelRc::from(std::rc::Rc::new(VecModel::from(rows)))
}

/// One line saying what this device *is* — the facts, not the name again.
///
/// **The first cut of this read "iPod 1 — disk"**, because it joined the device's firmware and disk
/// resource *names*, and a device named "iPod 1" tends to have resources named after it. A caption
/// that repeats the heading is worse than no caption: it takes the space and teaches nothing.
///
/// `seen` is the pass's shared [`Presence`] — see [`device_rows`].
fn summary(settings: &Settings, d: &Device, seen: &mut Presence) -> String {
    let mut parts: Vec<String> = Vec::new();

    // Which iPod. A dump states its own model; a synthesised one was told.
    match settings.nor_of(d) {
        Some(eapp_loader::nor::Source::Synthetic { model, .. }) => parts.push(model.clone()),
        Some(eapp_loader::nor::Source::File(_)) => parts.push("from a dump".into()),
        // Nothing. The `missing` branch below names the iPod that is gone, and saying
        // "from a dump" about a dump that is not there would be the caption inventing a fact.
        None => {}
    }

    // What is on the drive. `built_from` and `installed` are the interesting facts, and they are
    // the disk's, not the device's.
    if let Some(disk) = d.disk.as_ref().and_then(|n| settings.disks.iter().find(|k| &k.name == n)) {
        if let Some(built) = &disk.built_from {
            parts.push(built.clone());
        }
        parts.extend(disk.installed.iter().cloned());
    }

    // §9: a failure names what is wrong, never that something is wrong.
    let missing = settings.missing_with(d, seen);
    if !missing.is_empty() {
        let names: Vec<String> = missing.iter().map(|a| a.label()).collect();
        parts.push(format!("missing {}", names.join(", ")));
    }

    parts.join(" · ")
}

/// §10 — whose file is about to be written to, said out loud, before the machine starts.
fn write_target(d: &Device) -> String {
    let Some(p) = d.disk_path.as_ref() else {
        return String::new();
    };
    let name = p
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    // `None` is not "no". With nobody having said, a drive this program built is regenerable and a
    // drive the operator supplied might be their only copy — so the honest default is a copy.
    if d.work_on_copy.unwrap_or(true) {
        format!("works on a copy of {name}")
    } else {
        format!("writes to {name}")
    }
}

/// True when the machine will write to an image the operator supplied rather than one we built.
///
/// This is the only routine use of the warn colour in the program, and it is the line standing
/// between an afternoon and somebody's only image of an iPod they own.
fn writes_to_your_own_image(settings: &Settings, d: &Device) -> bool {
    if d.work_on_copy.unwrap_or(true) {
        return false;
    }
    // Built by us means regenerable byte for byte; anything else is theirs until proven otherwise.
    let built_here = d
        .disk
        .as_ref()
        .and_then(|n| settings.disks.iter().find(|k| &k.name == n))
        .is_some_and(|k| k.built_from.is_some());
    !built_here
}

/// Whether the case is dark enough that its markings and highlights have to invert.
///
/// Passed to the drawing rather than computed there: Slint has no luminance function, and the
/// alternative is the markup guessing with arithmetic that would be wrong for gold and green.
fn is_dark(c: Colour) -> bool {
    let (r, g, b) = rgb(c);
    // Rec. 709 luma. The threshold is set between Purple (the darkest light case) and Blue.
    let luma = 0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32;
    luma < 128.0
}

/// The chassis colour the window draws the case in. Cosmetic only — nothing the firmware reads
/// changes with it.
fn chassis_colour(c: Colour) -> slint::Color {
    let (r, g, b) = rgb(c);
    slint::Color::from_rgb_u8(r, g, b)
}

/// The case colours, as bytes, so both the drawing and [`is_dark`] read the same table.
fn rgb(c: Colour) -> (u8, u8, u8) {
    match c {
        Colour::White => (0xF2, 0xF2, 0xF0),
        Colour::Black => (0x2B, 0x2B, 0x2D),
        Colour::U2 => (0x2B, 0x2B, 0x2D),
        Colour::Silver => (0xC8, 0xCA, 0xCC),
        Colour::Blue => (0x4A, 0x7F, 0xC1),
        Colour::Gold => (0xC9, 0xAE, 0x74),
        Colour::Green => (0x7A, 0xB5, 0x5C),
        Colour::Pink => (0xE0, 0x8F, 0xAE),
        Colour::Orange => (0xE0, 0x8B, 0x45),
        Colour::Purple => (0x8B, 0x77, 0xB5),
        Colour::Red => (0xC4, 0x3B, 0x3B),
        Colour::Yellow => (0xE0, 0xCB, 0x55),
        Colour::Stainless => (0xD2, 0xD4, 0xD6),
        // **Not a colour, and deliberately not black.** `Unspecified` is a ROM that did not say,
        // and drawing it as Black would be the window inventing a fact about somebody's iPod. A
        // neutral case reads as "unknown", which is what it is.
        Colour::Unspecified => (0xE4, 0xE4, 0xE2),
    }
}

/// An iPod at rest: the screen is off, not blank-white. 320×240 because that is the panel, and the
/// window scales it by whole numbers only (§2.8).
fn dark_screen() -> slint::Image {
    let buf = SharedPixelBuffer::new(emu::FB_W as u32, emu::FB_H as u32);
    slint::Image::from_rgb8(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    // The geometry, read from the one place it is written.
    //
    // **These used to be hand-copied**, with a comment admitting it was a known hole — and the hole
    // had already swallowed something: `the_drawn_ipod_is_the_shape_of_a_real_one` carried its own
    // second copy of `SCREEN_W` and `SCREEN_H` at the pre-§6.6 values, shadowing these, inside the
    // test that claims to verify the drawing. docs/GUI.md §16.9 and §20 item 9 closed it: one Rust
    // source, `build.rs` renders it into the `.slint` the markup imports, and the tests read the
    // same module. Editing a ratio now moves the markup and the tests together or it moves neither.
    use crate::geometry::{
        BODY_ASPECT, CENTRE_D, CORNER_R, HERO_PHYS_1X, SCREEN_H, SCREEN_TOP, SCREEN_W, WHEEL_D,
        WHEEL_TOP,
    };

    /// What the markup draws the panel at, for a given body height and pair of ratios.
    ///
    /// **Deliberately not `geometry::panel_logical`**, and the difference is the point of having
    /// both. This is `Geometry.screen-w * body-height` — the expression `ui/ipod.slint` actually
    /// evaluates, which is how the well is sized inside the drawing. `panel_logical` answers a
    /// different question: what length to push into `screen-w` so that the *framebuffer* lands on
    /// a whole number of device pixels, which is why it walks the value up by an f32 ULP.
    /// `geometry::the_panel_is_an_exact_integer_number_of_device_pixels` is the test for that one.
    fn panel_at(hero: f64, sw: f64, sh: f64) -> (f64, f64) {
        (hero * sw, hero * sh)
    }

    /// §10's line is never silently absent for a device that has a disk.
    #[test]
    fn a_device_with_a_disk_always_says_what_it_writes_to() {
        let mut d = Device {
            disk_path: Some(std::path::PathBuf::from("/tmp/my-5.5g.img")),
            ..Device::default()
        };

        d.work_on_copy = Some(false);
        assert!(
            write_target(&d).contains("writes to my-5.5g.img"),
            "a device that writes to the operator's own image has to say so"
        );

        d.work_on_copy = Some(true);
        assert!(
            write_target(&d).contains("copy"),
            "a device on a copy has to say that too — silence reads as 'writes to it'"
        );
    }

    /// **Nobody having said is not the same as having said no.**
    ///
    /// A drive this program built is regenerable byte for byte, so writing to it costs nothing; one
    /// the operator supplied might be the only image of an iPod they own, and defaulting to writing
    /// on it is how an afternoon disappears.
    #[test]
    fn an_unanswered_device_works_on_a_copy() {
        let d = Device::default();
        assert!(
            write_target(&Device {
                disk_path: Some(std::path::PathBuf::from("/tmp/x.img")),
                ..d
            })
            .contains("copy"),
            "with nobody having said, the default has to be the safe one"
        );
    }

    /// The warn colour appears when — and only when — the machine will write to an image we did
    /// not build.
    #[test]
    fn the_warning_is_for_images_we_did_not_build() {
        let mut s = Settings::default();
        let mut d = Device {
            disk: Some("theirs.img".into()),
            work_on_copy: Some(false),
            ..Device::default()
        };

        s.disks.push(eapp_loader::settings::Disk {
            name: "theirs.img".into(),
            path: "/tmp/theirs.img".into(),
            built_from: None,
            installed: vec![],
        });
        assert!(
            writes_to_your_own_image(&s, &d),
            "an image with no `built_from` is the operator's until proven otherwise"
        );

        s.disks[0].built_from = Some("iPod_25.1.3.ipsw".into());
        assert!(
            !writes_to_your_own_image(&s, &d),
            "one we built from a bundle is regenerable, so writing to it warrants no warning"
        );

        d.work_on_copy = Some(true);
        s.disks[0].built_from = None;
        assert!(
            !writes_to_your_own_image(&s, &d),
            "on a copy nothing of theirs is touched, whoever made the original"
        );
    }

    /// A device made of a synthesised iPod and a drive image that is really there.
    ///
    /// The image has to exist: `Settings::missing` stats every resolved path now, so a fixture
    /// pointing at a fictional `/tmp/x.img` would put `missing x.img` in the caption and the line
    /// under test would no longer be the intended one.
    fn a_composed_device(dir: &std::path::Path) -> (Settings, Device) {
        use eapp_loader::settings::Resource;
        let img = dir.join("x.img");
        std::fs::write(&img, b"not really a drive").unwrap();

        let mut s = Settings::default();
        let rom = s.file_away(
            Resource::Firmware(eapp_loader::nor::Source::Synthetic {
                model: "5.5G 80 GB".into(),
                seed: 1,
                serial: None,
                guid: None,
                splash: None,
            }),
            "an iPod",
            None,
        );
        s.disks.push(eapp_loader::settings::Disk {
            name: "iPod 1".into(),
            path: img,
            built_from: Some("iPod_25.1.3.ipsw".into()),
            installed: vec!["Rockbox 4.0".into()],
        });
        let d = Device {
            name: "iPod 1".into(),
            firmware: rom,
            disk: Some("iPod 1".into()),
            ..Device::default()
        };
        (s, d)
    }

    /// A directory of our own, named after the test, so two running at once cannot collide.
    fn temp_dir(what: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("ipod-gui-test-{what}-{}", std::process::id()));
        std::fs::create_dir_all(&d).expect("a temp directory");
        d
    }

    /// The caption says what the device *is*, never the heading again.
    ///
    /// The first cut read "iPod 1 — disk" because it joined resource *names*, and resources tend to
    /// be named after the device that uses them.
    #[test]
    fn the_caption_never_just_repeats_the_name() {
        let dir = temp_dir("caption");
        let (s, d) = a_composed_device(&dir);

        let line = summary(&s, &d, &mut Presence::new());
        assert!(line.contains("5.5G"), "the caption has to say which iPod: {line}");
        assert!(line.contains("Rockbox"), "and what is on it: {line}");
        assert_ne!(line.trim(), d.name, "a caption that repeats the heading teaches nothing");
        assert!(!line.contains("missing"), "nothing is missing: {line}");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// **A device whose iPod is gone says so rather than guessing.**
    ///
    /// The caption used to read the device's own inline copy of the recipe, so it could describe an
    /// iPod that no longer existed anywhere. With the copy gone there is nothing to guess from, and
    /// saying `from a dump` about a dump that is not there would be inventing a fact.
    #[test]
    fn a_device_whose_ipod_is_gone_says_so_instead_of_guessing() {
        let dir = temp_dir("gone-ipod");
        let (mut s, d) = a_composed_device(&dir);
        s.resources.clear();

        let line = summary(&s, &d, &mut Presence::new());
        assert!(line.contains("missing"), "the caption said nothing: {line}");
        assert!(line.contains("an iPod"), "it did not name what is gone: {line}");
        assert!(!line.contains("from a dump"), "it guessed: {line}");
        assert!(!line.contains("5.5G"), "it described an iPod it cannot reach: {line}");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// **One `Presence` for the whole row pass**, which is the sharing `Presence` exists for.
    ///
    /// `summary` called `Settings::missing`, which mints a fresh cache per call — so N devices on
    /// one drive image cost N `stat`s, and the doc comment describing a shared pass described a
    /// pass that happened nowhere. Observable because a cache that is shared answers from memory:
    /// delete the image between the two devices and the second still reads it as present.
    #[test]
    fn the_row_pass_shares_one_look_at_the_filesystem() {
        let dir = temp_dir("shared-presence");
        let (s, d) = a_composed_device(&dir);
        let mut seen = Presence::new();

        let first = summary(&s, &d, &mut seen);
        assert!(!first.contains("missing"), "the fixture is not intact: {first}");

        std::fs::remove_file(dir.join("x.img")).unwrap();
        let second = summary(&s, &d, &mut seen);
        assert_eq!(
            first, second,
            "the second device in the pass re-stat'ed a path the pass had already answered"
        );

        // And the sharing is a cache, not a blindfold: a new pass sees the world as it is.
        let fresh = summary(&s, &d, &mut Presence::new());
        assert!(
            fresh.contains("missing x.img"),
            "a new pass did not notice the deleted image: {fresh}"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// **`never started` was a claim about history from a field that is a denominator.**
    ///
    /// `boot_instructions` is what the progress bar divides by, and §12.3 has
    /// `Settings::set_boot_shape` clear it whenever the recipe changes — so a device booted a dozen
    /// times renders this row the moment its bootloader is changed. What is missing is the number,
    /// and that is what the row says.
    ///
    /// Asserted against `device_rows`' output rather than through the markup, because
    /// `DeviceRow.state` is declared at `window.slint:18` and drawn nowhere yet.
    #[test]
    fn a_device_whose_recipe_changed_does_not_claim_it_never_started() {
        use slint::Model;

        let mut s = Settings::default();
        s.remember_as("mine");
        // A device that HAS booted, whose denominator was then dropped — which is exactly what
        // §12.3's rule does to a device whose bootloader changed.
        s.devices[0].boot_instructions = None;

        let rows = device_rows(&s);
        let state = rows.row_data(0).unwrap().state.to_string();
        assert!(
            !state.contains("never started"),
            "the row claims history the model does not carry: {state:?}"
        );
        assert_eq!(state, "no boot time learned yet");

        s.devices[0].boot_instructions = Some(3_000_000);
        let rows = device_rows(&s);
        assert_eq!(
            rows.row_data(0).unwrap().state.to_string(),
            "",
            "a device with a denominator says nothing here"
        );
    }

    /// **No row claims a verification the model did not record.**
    ///
    /// The four groups printed `fetched and verified` and `dumped from a real iPod` as string
    /// literals, for files `Resource` carried nothing but a path for.
    #[test]
    fn no_row_claims_a_verification_the_model_did_not_record() {
        use eapp_loader::settings::{Provenance, Resource, Verification};
        use slint::Model;

        let mut s = Settings::default();
        let stated = [
            (
                "verified",
                Some(Provenance::Fetched {
                    verified: Verification::Sha256,
                }),
            ),
            (
                "size only",
                Some(Provenance::Fetched {
                    verified: Verification::SizeOnly,
                }),
            ),
            ("provided", Some(Provenance::Provided)),
            ("nobody said", None),
        ];
        for (i, (name, from)) in stated.iter().enumerate() {
            s.file_away(
                Resource::Installer(std::path::PathBuf::from(format!("/fw/{i}.ipsw"))),
                name,
                *from,
            );
            s.file_away(
                Resource::Software(std::path::PathBuf::from(format!("/sw/{i}.ipod"))),
                &format!("sw {name}"),
                *from,
            );
        }

        let rows = resource_rows(&s);
        let mut claims = 0;
        for i in 0..rows.row_count() {
            let row = rows.row_data(i).unwrap();
            if row.heading {
                continue;
            }
            if row.detail.contains("verified") {
                claims += 1;
                assert!(
                    row.text.contains("verified"),
                    "`{}` claims a verification the model did not record: {}",
                    row.text,
                    row.detail
                );
            }
        }
        assert_eq!(claims, 2, "the verified rows were not found at all");
        // And the item nobody recorded a provenance for says nothing, rather than picking one.
        let nothing = (0..rows.row_count())
            .filter_map(|i| rows.row_data(i))
            .find(|r| !r.heading && r.text == "nobody said")
            .expect("the row");
        assert_eq!(nothing.detail.as_str(), "", "a row nobody recorded invented a claim");
        // `parked` is the model's answer now, not a hard-coded `false`.
        let mut with_park = Settings::default();
        let rom = with_park.file_away(
            Resource::Firmware(eapp_loader::nor::Source::default()),
            "an iPod",
            None,
        );
        with_park.devices.push(Device {
            name: "mine".into(),
            firmware: rom,
            parked_at: Some(1_755_738_000),
            ..Device::default()
        });
        let devices = device_rows(&with_park);
        assert!(devices.row_data(0).unwrap().parked, "the park flag is still a literal");
    }

    /// Every chassis colour the identity model can produce has a case here.
    #[test]
    fn every_chassis_colour_is_distinct_from_the_default() {
        let black = chassis_colour(Colour::Black);
        for c in [
            Colour::White,
            Colour::Silver,
            Colour::Blue,
            Colour::Gold,
            Colour::Green,
            Colour::Pink,
            Colour::Orange,
            Colour::Purple,
            Colour::Red,
            Colour::Yellow,
        ] {
            assert_ne!(
                chassis_colour(c),
                black,
                "{c:?} draws the same case as Black — a colour was added without one"
            );
        }
    }

    /// The markings on a case have to contrast with it, which is the whole reason `is_dark` exists.
    #[test]
    fn dark_cases_are_dark_and_light_cases_are_not() {
        for c in [Colour::Black, Colour::U2, Colour::Purple, Colour::Red] {
            assert!(is_dark(c), "{c:?} needs light markings");
        }
        for c in [
            Colour::White,
            Colour::Silver,
            Colour::Stainless,
            Colour::Gold,
            Colour::Green,
            Colour::Pink,
            Colour::Yellow,
            Colour::Unspecified,
        ] {
            assert!(!is_dark(c), "{c:?} needs dark markings");
        }
    }

    /// The panel is an exact whole number of device pixels — the check the last one could not make.
    ///
    /// **The test this replaces asserted `drawn_w >= 320` and `drawn_h >= 240`.** That is a
    /// *downscale* detector. It passes on a stretch, and it was passing on one: at the 658 px body
    /// and the drawing's own `0.4866 / 0.3672` pair, the panel was drawn **320.18 × 241.62** for a
    /// 320 × 240 buffer — 1.00057 across, 1.00674 down, non-uniform and non-integer — while its own
    /// doc comment claimed "never smoothed and never stretched". Under nearest-neighbour that
    /// duplicates about 1.6 rows out of 240 at fixed positions, and those land on RetailOS's own
    /// 1 px separator rules.
    ///
    /// That is the shape `AGENTS.md` calls an instrument that lies, and the reason the rule is now
    /// *no prose claim about the window without a check that can fail*. This one fails against the
    /// constants that shipped; `the_old_ratios_would_fail_this_test` proves it rather than asserting
    /// it.
    ///
    /// The fix inverts the dependency, which is how MAME solves the same problem: **the drawing
    /// governs where the screen sits, the hardware governs how big it is.** 50.8 / 104.1 and
    /// 38.1 / 104.1 are 4:3 by construction, so the well cannot be off-ratio.
    #[test]
    fn the_panel_is_an_exact_whole_number_of_pixels() {
        let (w, h) = panel_at(HERO_PHYS_1X, SCREEN_W, SCREEN_H);
        assert!(
            (w - 320.0).abs() < 0.01,
            "the panel is drawn {w:.4} px wide for a 320 px framebuffer"
        );
        assert!(
            (h - 240.0).abs() < 0.01,
            "the panel is drawn {h:.4} px tall for a 240 px framebuffer"
        );
        // Uniform, not merely close: a scale that differs between the axes is a stretch, and a
        // stretch is what the old pair actually did.
        assert!(
            ((w / 320.0) - (h / 240.0)).abs() < 1e-4,
            "the scale is {:.5} across and {:.5} down — that is a stretch, not a scale",
            w / 320.0,
            h / 240.0
        );
    }

    /// The well is 4:3 by construction, not by luck.
    #[test]
    fn the_screen_well_is_exactly_four_thirds() {
        let r = SCREEN_W / SCREEN_H;
        assert!(
            (r - 4.0 / 3.0).abs() < 1e-5,
            "the well is {r:.5}, not 4:3 — a 320×240 buffer would be stretched in one axis only"
        );
        // And the ratio is the hardware's, so say so in a way that breaks if someone edits it.
        assert!((SCREEN_W - 50.8 / 104.1).abs() < 1e-5, "SCREEN_W is not 50.8 mm / 104.1 mm");
        assert!((SCREEN_H - 38.1 / 104.1).abs() < 1e-5, "SCREEN_H is not 38.1 mm / 104.1 mm");
    }

    /// **Proof that the test above can fail**, which is the only thing that makes it worth having.
    ///
    /// `AGENTS.md`: before believing a zero, run the control that makes the instrument produce a
    /// non-zero. This is that control — it feeds the constants that shipped into the same arithmetic
    /// and asserts they are caught. If this test ever passes trivially, the check above has stopped
    /// checking.
    #[test]
    fn the_old_ratios_would_fail_this_test() {
        const OLD_HERO: f64 = 658.0;
        const OLD_W: f64 = 0.4866;
        const OLD_H: f64 = 0.3672;

        let (w, h) = panel_at(OLD_HERO, OLD_W, OLD_H);
        assert!(
            (w - 320.0).abs() >= 0.01 || (h - 240.0).abs() >= 0.01,
            "the constants that shipped drew {w:.2} × {h:.2}; if that now passes, the check is dead"
        );
        assert!(
            ((w / 320.0) - (h / 240.0)).abs() >= 1e-4,
            "the shipped pair scaled {:.5} across and {:.5} down; a stretch has to be detectable",
            w / 320.0,
            h / 240.0
        );
        assert!(
            (OLD_W / OLD_H - 4.0 / 3.0).abs() >= 1e-5,
            "the drawing's own pair was 1.32516, not 4:3 — that has to be detectable too"
        );
    }

    // `the_window_minimum_holds_the_device` used to sit here, asserting that
    // `56 (chrome) + 16 + 655.751 + 16 + 96 (caption) + 16 <= 860 (min-height)`.
    //
    // **It encoded the pre-§9.6 model and every number in it was wrong**: the chrome bar is
    // deleted, the caption is an 88 px shelf rather than a 96 px stack, and the column it summed
    // came to 894 against the operator's own 891 px of client — three pixels short of its own
    // fidelity rule, on the machine this program is developed on, with this test unable to see it
    // because it hard-coded the caption at 96 while the window drew ~132.
    //
    // More fundamentally, the thing it asserted as a REQUIREMENT is the thing §9.6 says must not be
    // true: a minimum tall enough to guarantee the 1:1 panel would have to vary with `k` and `sf`,
    // and a window minimum is a constant. `min-height` is a floor and the too-short boolean is the
    // mechanism. `geometry::the_minimum_height_is_a_floor_not_a_fit` asserts that instead, and
    // `fit::the_nine_six_table_holds` covers the displays this one thought it was covering.

    /// The drawing's proportions are **measured**, and this is the check that says so.
    ///
    /// They come from Rockbox's own scale drawing of this device —
    /// `manual/rockbox_interface/images/ipodvideo-front.svg`, a vector front elevation shipped with
    /// the Rockbox manual. That file lives under `resources/`, which is gitignored, so this test
    /// cannot read it; instead it checks the constants against the thing the drawing was checked
    /// against, which is the hardware.
    ///
    /// **The tell that the parse was right**: on a 104.1 mm body these ratios give a display of
    /// 50.7 × 38.2 mm, and a 2.5-inch 4:3 panel is 50.8 × 38.1 mm. Getting the right rectangle out
    /// of a drawing full of rectangles is only provable that way.
    ///
    /// The version before this one had none of it. Body and screen *sizes* were roughly right
    /// because they came from published specs, but the *placement* was arithmetic chosen to make
    /// the column add up — wrong by 66 % on where the screen sits, 34 % on the centre button, 13 %
    /// on the wheel. An invented proportion is very hard to see; an iPod that does not look like an
    /// iPod is easy to see.
    //
    // The overlap and physical-sanity assertions below relate declared constants to each other, so
    // clippy's "this assertion has a constant value" is describing the point rather than a defect.
    // Left as run-time assertions rather than `const` blocks so a failure names both numbers at
    // test time instead of stopping the build with a const-eval panic.
    #[allow(clippy::assertions_on_constants)]
    #[test]
    fn the_drawn_ipod_is_the_shape_of_a_real_one() {
        // **This block used to redeclare eight constants here**, and two of them —
        // `SCREEN_W: 0.4866` and `SCREEN_H: 0.3672` — were the pre-§6.6 pair, shadowing the
        // corrected ones at the top of the module. So the test that claims to verify the drawing
        // was verifying a drawing nothing drew, and would have gone on passing after the ratios
        // were corrected in the markup. They are gone; the module's `use crate::geometry::…` is
        // what is in scope now, and it is the same file `build.rs` renders into the markup.

        // Apple's published body height for the 5G: 4.1 inches.
        const BODY_MM: f64 = 104.1;
        let mm = |f: f64| f * BODY_MM;

        // A 2.5-inch display at 4:3.
        let want_w = 2.5 * 0.8 * 25.4;
        let want_h = 2.5 * 0.6 * 25.4;
        assert!(
            (mm(SCREEN_W) - want_w).abs() < 0.5,
            "screen is {:.1} mm wide; a 2.5-inch 4:3 panel is {want_w:.1}",
            mm(SCREEN_W)
        );
        assert!(
            (mm(SCREEN_H) - want_h).abs() < 0.5,
            "screen is {:.1} mm tall; a 2.5-inch 4:3 panel is {want_h:.1}",
            mm(SCREEN_H)
        );

        // 320×240 is 4:3, so the well it goes into has to be, or it is stretched (§2.8).
        assert!(
            (SCREEN_W / SCREEN_H - 4.0 / 3.0).abs() < 0.01,
            "the screen well is {:.4}, not 4:3 — the framebuffer would be stretched",
            SCREEN_W / SCREEN_H
        );

        // The drawing insets the screen by the same amount left, top and right. That symmetry is
        // how the parse was confirmed to have found the display rather than some other rectangle,
        // so it is worth holding onto: the left inset is (body_width - screen_width) / 2. It is an
        // OUTPUT since §6.6 took the screen's size off the drawing, so it is computed in one place
        // and read here rather than recomputed.
        let left_inset = crate::geometry::left_inset();
        assert!(
            (left_inset - SCREEN_TOP).abs() < 0.002,
            "left inset {left_inset:.4} and top inset {SCREEN_TOP:.4} disagree — one of them is wrong"
        );

        // Nothing may overlap or leave the body.
        assert!(SCREEN_TOP + SCREEN_H < WHEEL_TOP, "the screen and the wheel overlap");
        assert!(WHEEL_TOP + WHEEL_D < 1.0, "the wheel hangs off the bottom of the body");
        assert!(WHEEL_D < BODY_ASPECT, "the wheel is wider than the iPod");
        assert!(CENTRE_D < WHEEL_D * 0.5, "the centre button is more than half the wheel");
        assert!(CORNER_R * 2.0 < BODY_ASPECT, "the corner radius is wider than half the body");

        // And the physical sanity of the two controls, which is what caught the first version.
        assert!(
            (mm(WHEEL_D) - 38.3).abs() < 1.0,
            "the click wheel is {:.1} mm; the drawing says 38.3",
            mm(WHEEL_D)
        );
        assert!(
            (mm(CENTRE_D) - 12.8).abs() < 1.0,
            "the centre button is {:.1} mm; the drawing says 12.8",
            mm(CENTRE_D)
        );
    }

    /// `MainWindow::set_hero` exists — which is the compile-time fact §6.6's code block assumes and
    /// the markup did not provide.
    ///
    /// `hero` was `property <length> hero: 655.751px` **inside the pane**, private and unreachable
    /// from Rust: there was no `set_hero` on `MainWindow` at all, so the design's own
    /// `window.set_hero(hero as f32)` described a call that could not compile. This test does not
    /// assert a value; it fails to BUILD if the property goes back to being private or is renamed,
    /// which is the only failure mode that matters here.
    #[test]
    fn the_hero_is_an_input_the_window_can_be_told() {
        let _: fn(&MainWindow, f32) = MainWindow::set_hero;
        let _: fn(&MainWindow, f32) = MainWindow::set_screen_w;
        let _: fn(&MainWindow, f32) = MainWindow::set_screen_h;
        let _: fn(&MainWindow, i32) = MainWindow::set_screen_scale;
        let _: fn(&MainWindow, bool) = MainWindow::set_too_short;
    }

    /// **The non-circular half of §20 item 11.**
    ///
    /// It does not ask whether a feature flag is set — it asks whether the crate is actually
    /// resolved, which is the thing every ARIA claim in the previous revision was false about.
    /// §16.7's own sentence was `grep -c accesskit Cargo.lock` returns **0**; this is that command,
    /// run in the test binary.
    #[test]
    fn the_lockfile_carries_accesskit() {
        let lock = include_str!("../../../Cargo.lock");
        assert!(
            lock.contains("name = \"accesskit\""),
            "accesskit is not in the lockfile; `accessible-*` in the markup is decorative"
        );
    }

    /// The feature is on in a default build, so nobody can quietly drop it back the way
    /// `default-features = false` dropped it the first time.
    ///
    /// Not the same test as the one above, and the control proves it: under
    /// `cargo test -p ipod-gui --no-default-features` the lockfile still contains accesskit — a
    /// lock records the union of all features — so that one stays green while this one goes red.
    //
    // `cfg!()` is a compile-time constant, and its VALUE is the whole of what is under test, so
    // clippy's "this assertion has a constant value" is the point. Not a `const` block: that would
    // fail the build instead of the test, taking `the_lockfile_carries_accesskit` down with it and
    // destroying the control that proves the two are different checks.
    #[allow(clippy::assertions_on_constants)]
    #[test]
    fn accessibility_is_compiled_in() {
        assert!(
            cfg!(feature = "accessibility"),
            "the accessibility feature is off; every accessible-* property in the markup is \
             decorative"
        );
    }
}
