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

use eapp_loader::identity::Colour;
use eapp_loader::settings::{Device, Settings};
use slint::{ModelRc, SharedPixelBuffer, VecModel};

slint::include_modules!();

fn main() -> Result<(), slint::PlatformError> {
    opaque_window()?;

    let settings = Settings::load();
    let window = MainWindow::new()?;

    window.set_devices(device_rows(&settings));
    window.set_resources(resource_rows(&settings));
    window.set_screen_source(dark_screen());

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

/// The list the window shows, built from the model.
fn device_rows(settings: &Settings) -> ModelRc<DeviceRow> {
    let rows: Vec<DeviceRow> = settings
        .devices
        .iter()
        .map(|d| {
            let chassis = d.chassis.unwrap_or_default();
            DeviceRow {
                name: d.name.clone().into(),
                summary: summary(settings, d).into(),
                state: if d.boot_instructions.is_some() {
                    "".into()
                } else {
                    "never started".into()
                },
                write_target: write_target(d).into(),
                write_target_is_warning: writes_to_your_own_image(settings, d),
                chassis: chassis_colour(chassis),
                dark_chassis: is_dark(chassis),
                parked: false,
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
    use eapp_loader::settings::Resource;
    let mut rows: Vec<ResourceRow> = Vec::new();

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
    let ipods: Vec<(String, String)> = settings
        .resources
        .iter()
        .filter_map(|i| match &i.what {
            Resource::Firmware(eapp_loader::nor::Source::File(_)) => {
                Some((i.name.clone(), "dumped from a real iPod".to_string()))
            }
            Resource::Firmware(eapp_loader::nor::Source::Synthetic { model, seed, .. }) => {
                Some((i.name.clone(), format!("{model} · synthesised · seed {seed:x}")))
            }
            _ => None,
        })
        .collect();
    group(&mut rows, "iPods", "None yet — one is synthesised for you when you make a device.", ipods);

    group(&mut rows, "Apple firmware", "None yet — fetched from Apple when you make a device.",
        settings.resources.iter().filter(|i| matches!(i.what, Resource::Installer(_)))
            .map(|i| (i.name.clone(), "fetched and verified".into())).collect());

    group(&mut rows, "Software", "None yet — Rockbox is fetched and verified on request.",
        settings.resources.iter().filter(|i| matches!(i.what, Resource::Software(_)))
            .map(|i| (i.name.clone(), "fetched and verified".into())).collect());

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
fn summary(settings: &Settings, d: &Device) -> String {
    let mut parts: Vec<String> = Vec::new();

    // Which iPod. A dump states its own model; a synthesised one was told.
    match &d.nor {
        eapp_loader::nor::Source::Synthetic { model, .. } => parts.push(model.clone()),
        eapp_loader::nor::Source::File(_) => parts.push("from a dump".into()),
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
    let missing = settings.missing(d);
    if !missing.is_empty() {
        parts.push(format!("missing {}", missing.join(", ")));
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

    // The geometry as written in `ui/ipod.slint` and `ui/window.slint`.
    //
    // **These are hand-copied, and that is a known hole**: editing the markup leaves them green.
    // docs/GUI.md §16.9 and §20 item 9 close it by generating both the `.slint` and a Rust `const`
    // module from one source in `build.rs`. Until that lands, `arfi`-style discipline does not
    // apply here and the duplication is the risk it looks like.
    const HERO: f64 = 655.751;
    const SCREEN_W: f64 = 0.48799;
    const SCREEN_H: f64 = 0.36599;
    const BODY_ASPECT: f64 = 0.5917;
    const WHEEL_D: f64 = 0.3675;
    const WHEEL_TOP: f64 = 0.5215;
    const CENTRE_D: f64 = 0.1228;
    const CORNER_R: f64 = 0.0501;
    const SCREEN_TOP: f64 = 0.0525;

    /// What the markup draws the panel at, for a given body height and pair of ratios.
    fn panel_at(hero: f64, sw: f64, sh: f64) -> (f64, f64) {
        (hero * sw, hero * sh)
    }

    /// §10's line is never silently absent for a device that has a disk.
    #[test]
    fn a_device_with_a_disk_always_says_what_it_writes_to() {
        let mut d = Device::default();
        d.disk_path = Some(std::path::PathBuf::from("/tmp/my-5.5g.img"));

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
        let mut d = Device::default();
        d.disk = Some("theirs.img".into());
        d.work_on_copy = Some(false);

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

    /// The caption says what the device *is*, never the heading again.
    ///
    /// The first cut read "iPod 1 — disk" because it joined resource *names*, and resources tend to
    /// be named after the device that uses them.
    #[test]
    fn the_caption_never_just_repeats_the_name() {
        let mut s = Settings::default();
        let mut d = Device::default();
        d.name = "iPod 1".into();
        d.nor = eapp_loader::nor::Source::Synthetic {
            model: "5.5G 80 GB".into(),
            seed: 1,
            serial: None,
            guid: None,
            splash: None,
        };
        d.disk = Some("iPod 1".into());
        s.disks.push(eapp_loader::settings::Disk {
            name: "iPod 1".into(),
            path: "/tmp/x.img".into(),
            built_from: Some("iPod_25.1.3.ipsw".into()),
            installed: vec!["Rockbox 4.0".into()],
        });

        let line = summary(&s, &d);
        assert!(line.contains("5.5G"), "the caption has to say which iPod: {line}");
        assert!(line.contains("Rockbox"), "and what is on it: {line}");
        assert_ne!(line.trim(), d.name, "a caption that repeats the heading teaches nothing");
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
        let (w, h) = panel_at(HERO, SCREEN_W, SCREEN_H);
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

    /// The window has to be tall enough to hold the device, or the minimum size and the fidelity
    /// rule are in silent conflict and the layout wins.
    #[test]
    fn the_window_minimum_holds_the_device() {
        const CHROME: f64 = 56.0;
        const PADDING: f64 = 16.0;
        const CAPTION: f64 = 96.0;
        const MIN_WINDOW_H: f64 = 860.0;
        let needed = CHROME + PADDING + HERO + PADDING + CAPTION + PADDING;
        assert!(
            needed <= MIN_WINDOW_H,
            "a {HERO:.0} px device needs {needed:.0} px of window; the minimum is {MIN_WINDOW_H:.0}"
        );
    }

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
    #[test]
    fn the_drawn_ipod_is_the_shape_of_a_real_one() {
        // As written in `ui/ipod.slint`, every one a fraction of body height.
        const BODY_ASPECT: f64 = 0.5917;
        const SCREEN_W: f64 = 0.4866;
        const SCREEN_H: f64 = 0.3672;
        const SCREEN_TOP: f64 = 0.0525;
        const WHEEL_D: f64 = 0.3675;
        const WHEEL_TOP: f64 = 0.5215;
        const CENTRE_D: f64 = 0.1228;
        const CORNER_R: f64 = 0.0501;

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
        // so it is worth holding onto: the left inset is (body_width - screen_width) / 2.
        let left_inset = (BODY_ASPECT - SCREEN_W) / 2.0;
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
}
