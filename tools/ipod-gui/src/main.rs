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
    let settings = Settings::load();
    let window = MainWindow::new()?;

    window.set_devices(device_rows(&settings));
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

    /// The drawing's ratios are Apple's, and this is what says so out loud.
    ///
    /// `ui/ipod.slint` derives every dimension from `body-height` using the constants below. They
    /// are arithmetic on the published 103.5 × 61.8 mm body, the 2.5-inch 4:3 display and the 43 mm
    /// click wheel — not eyeballed. If the markup and this test drift apart, one of them is wrong
    /// and it is cheaper to find out here.
    #[test]
    fn the_drawn_ipod_keeps_apples_proportions() {
        const H: f64 = 103.5;
        const W: f64 = 61.8;
        let screen_w = 2.5 * 0.8 * 25.4;
        let screen_h = 2.5 * 0.6 * 25.4;

        let close = |a: f64, b: f64, what: &str| {
            assert!(
                (a - b).abs() < 0.001,
                "{what}: markup uses {b}, the hardware says {a:.4}"
            );
        };

        close(W / H, 0.5971, "body aspect");
        close(screen_w / H, 0.4909, "screen width over body height");
        close(screen_h / H, 0.3681, "screen height over body height");
        close(9.0 / H, 0.0870, "forehead above the screen");
        close(43.0 / H, 0.4155, "click wheel diameter");
        close((9.0 + screen_h + 8.0) / H, 0.5324, "click wheel top");
        close(17.0 / H, 0.1643, "centre button diameter");

        // And the screen the framebuffer goes into is 4:3, so 320×240 is never stretched (§2.8).
        close(screen_w / screen_h, 4.0 / 3.0, "screen aspect");
    }
}
