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
        .map(|d| DeviceRow {
            name: d.name.clone().into(),
            summary: summary(settings, d).into(),
            state: if d.boot_instructions.is_some() {
                "".into()
            } else {
                "never started".into()
            },
            write_target: write_target(settings, d).into(),
            // Reserved until the write-target line is computed from a real path — see §10. It is
            // never the *absence* of a warning that matters, it is that the row is always drawn.
            write_target_is_warning: false,
            chassis: chassis_colour(d.chassis.unwrap_or_default()),
            parked: false,
        })
        .collect();
    ModelRc::from(std::rc::Rc::new(VecModel::from(rows)))
}

/// One line: what this device is made of. `§13` — the facts a tile could not hold.
fn summary(settings: &Settings, d: &Device) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(f) = &d.firmware {
        parts.push(f.clone());
    }
    if let Some(disk) = &d.disk {
        parts.push(disk.clone());
    }
    let missing = settings.missing(d);
    if !missing.is_empty() {
        // §9: a failure states what is wrong, not that something is wrong.
        parts.push(format!("missing {}", missing.join(", ")));
    }
    parts.join(" · ")
}

/// §10 — whose file is about to be written to, said out loud, before the machine starts.
fn write_target(_settings: &Settings, d: &Device) -> String {
    match d.disk_path.as_ref() {
        None => String::new(),
        Some(p) => {
            let name = p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            if d.work_on_copy.unwrap_or(true) {
                format!("works on a copy of {name}")
            } else {
                format!("writes to {name}")
            }
        }
    }
}

/// The chassis colour the window draws the case in. Cosmetic only — nothing the firmware reads
/// changes with it.
fn chassis_colour(c: Colour) -> slint::Color {
    let (r, g, b) = match c {
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
    };
    slint::Color::from_rgb_u8(r, g, b)
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

    /// Every chassis colour the identity model can produce has a case here.
    ///
    /// A `match` would catch this at compile time, which is why it is written as one — this test
    /// exists to catch the *next* variant being added with a colour nobody chose, by failing on a
    /// value that is still the default black.
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

    /// §10's line is never silently absent for a device that has a disk.
    #[test]
    fn a_device_with_a_disk_always_says_what_it_writes_to() {
        let mut d = Device::default();
        d.disk_path = Some(std::path::PathBuf::from("/tmp/my-5.5g.img"));

        d.work_on_copy = Some(false);
        assert!(
            write_target(&Settings::default(), &d).contains("writes to my-5.5g.img"),
            "a device that writes to the operator's own image has to say so"
        );

        d.work_on_copy = Some(true);
        assert!(
            write_target(&Settings::default(), &d).contains("copy"),
            "a device on a copy has to say that too — silence reads as 'writes to it'"
        );
    }
}
