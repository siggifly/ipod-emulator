//! An interactive iPod 5G: the emulator's framebuffer on a drawn device whose click wheel, five
//! buttons and hold switch reach the machine.
//!
//! ```text
//! ipod-emulator [--user | --debug] [--cold] [--clock=N] [--snapshot=FILE] [--snap-at=N]
//!          [--flash=FILE] [--disk=FILE] [--workdisk=FILE] [--wheel-click-instr=N]
//!          [--headless=N | --selftest | --selftest-control]
//!          [--check-images] [--check-update] [--make-app DIR [ICON.png]]
//! ```
//!
//! See `tools/ipod-emulator/README.md` for what it is for, what it measures, and the two speed ratios it
//! reports — which are different numbers and get confused if only one of them is shown.
//!
//! # Two modes, one toggle, and one number that appears in both
//!
//! **User mode** is the iPod and nothing else: no counters, no addresses, no instrument panel. It
//! is the default, because it is what somebody who just cloned this should meet. **Debug mode** is
//! everything — instruction counts, both clocks, the task-entry watches, the ATA census, the
//! framebuffer inspector, the screenshot button.
//!
//! One thing is in **both**: the emulator runs at roughly 30 % of the real hardware's instruction
//! rate. A clean UI that hid that would teach people something false about how responsive a 5G is,
//! and timing is exactly where this emulator's remaining unknowns live. So user mode carries a
//! badge and debug mode carries the full readout with both ratios.
//!
//! # The device is drawn, not photographed
//!
//! Every part of the iPod here is vector geometry: rounded rectangles, discs and text. That is not
//! an aesthetic preference. Ninety-six hit regions want real angles rather than tuned pixel offsets;
//! white and black are a fill swap rather than two assets; it stays crisp at any window size and on
//! a HiDPI panel; and it means no Apple product photograph is committed to a repository heading
//! toward publication. The *proportions* are the real device's (61.8 x 103.5 mm case, 50.8 x 38.1 mm
//! active area, ~28 mm wheel) and those are facts about the object, not expression.
//!
//! **One front view, and only one.** The hold switch used to be drawn on a sliver of the *top* face
//! laid above the front — a top-down view pasted onto a front view, two viewpoints in one drawing.
//! It is now a control protruding from the top edge of the body, the way it physically sits proud
//! of the case, and it slides.
//!
//! # The panel is the one surface that must not be prettified
//!
//! `App::device` draws the framebuffer at an **integer** physical-pixel scale with
//! nearest-neighbour sampling, on a rect snapped to the physical pixel grid. Bilinear filtering, a
//! fractional scale, or a half-pixel offset would each blur an emulator artefact into a rendering
//! artefact, and this project has retired nine published conclusions to instruments that lied. The
//! scale in use is printed under the device in debug mode so it can be checked rather than trusted.

mod control;
mod emu;
use eapp_loader::inspect;
use eapp_loader::ipsw;
mod png;
use eapp_loader::settings;
mod update;
mod wheel;

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eapp_loader::WheelEvent;
use eapp_loader::identity::Colour;
use eframe::egui;
use egui::{Align2, Color32, CornerRadius, FontId, Pos2, Rect, Stroke, StrokeKind, Vec2};

use emu::{Link, Phase, FB_BACK, FB_FRONT, FB_H, FB_W};
use inspect::Verdict;
use eapp_loader::settings::{Mode, Settings};
use wheel::{Button, Hit, WheelRing};

// ---------------------------------------------------------------- the device's proportions
//
// Millimetres, from Apple's published dimensions and the panel's own size. Held as millimetres
// rather than as ratios so a reader can check them against a tape measure.
//
// **A struct rather than constants**, because this emulator is named for the line and not for one
// model. Everything that draws a device reads these, so adding a model is a row here rather than a
// change to the painter. Only the 5.5G is emulated today and only the 5.5G is listed: a device
// drawn in the picker is a promise, and an unkeepable one is worse than an absent one.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Device {
    /// The full name, for a heading or a place that is naming one device.
    pub name: &'static str,
    /// The short name, for anywhere several devices sit next to each other — a tile, a row, or a
    /// description with a serial after it. `iPod Video (5G / 5.5G) · Y7TXK` is a sentence;
    /// `iPod Video · Y7TXK` is a label.
    pub short: &'static str,
    /// Case outline.
    pub case_w: f32,
    pub case_h: f32,
    /// Active area of the panel.
    pub screen_w: f32,
    pub screen_h: f32,
    /// Top of the case to the top of the active area.
    pub screen_top: f32,
    /// The wheel: diameter, and centre measured from the top of the case.
    pub wheel_d: f32,
    pub wheel_cy: f32,
    /// The framebuffer this model's panel actually holds.
    pub fb: (usize, usize),
    /// Apple's model number, which is how these dumps are filed and mis-filed. The Video is
    /// `A1136`; collections put its ROM under the Classic's `A1238`, which is the single most
    /// expensive piece of misinformation this project has met.
    pub model_no: &'static str,
    /// Exactly how long this model's boot ROM is. The 5G/5.5G is 1 MiB; a 2 MiB dump is a later
    /// model, and telling somebody *which* is the difference between a dead end and a next step.
    pub rom_len: u64,
    /// The updater family Apple ships this model's software under — the `20` in `iPod_20.1.3.ipsw`
    /// and in the `Firmware-20.6.3` inside it.
    ///
    /// Here so that a mismatched pair can be caught **before** the boot rather than seventy-five
    /// seconds into one. It is a property of the model, which is why it lives beside the model's
    /// millimetres rather than being asked of the user.
    pub ipsw_family: u32,
    /// Does this model boot here **today**?
    ///
    /// The field exists so that the set of models the emulator knows about and the set it can run
    /// are two different sets, held in one place, rather than one set with an unwritten caveat.
    /// Nothing that draws a device may draw an unsupported one — see [`MODELS`].
    pub boots: bool,
}

/// The iPod Video, 5th and 5.5 generation — A1136. The only model this emulator boots.
pub const IPOD_VIDEO: Device = Device {
    name: "iPod Video (5G / 5.5G)",
    short: "iPod Video",
    case_w: 61.8,
    case_h: 103.5,
    screen_w: 50.8,
    screen_h: 38.1,
    screen_top: 9.5,
    wheel_d: 28.0,
    wheel_cy: 75.5,
    fb: (FB_W, FB_H),
    model_no: "A1136",
    ipsw_family: 20,
    rom_len: 1024 * 1024,
    boots: true,
};

/// Every model this program knows about, whether or not it can run one.
///
/// **A table, so that supporting another iPod is a row.** The painter, the identification of a
/// dump, and anything that lists devices all read this — none of them names a model.
///
/// It holds exactly one entry today, and that is the point rather than an omission:
/// [`ROADMAP.md`](../../../ROADMAP.md) Ⅳ says *a device drawn in the picker is a promise, and each
/// one appears when it boots, not before*. So the structure carries every clickwheel iPod from the
/// day it is written, and the list carries the ones that work. Adding the Classic 6G means adding
/// its millimetres and flipping [`Device::boots`] — not a refactor, and not a claim made early.
pub const MODELS: &[Device] = &[IPOD_VIDEO];

/// The models that can actually be run, which is the only set any picker may draw from.
pub fn bootable() -> impl Iterator<Item = &'static Device> {
    MODELS.iter().filter(|d| d.boots)
}

const CASE_W: f32 = IPOD_VIDEO.case_w;
const CASE_H: f32 = IPOD_VIDEO.case_h;
const SCREEN_W: f32 = IPOD_VIDEO.screen_w;
const SCREEN_H: f32 = IPOD_VIDEO.screen_h;
const SCREEN_TOP: f32 = IPOD_VIDEO.screen_top;
const WHEEL_D: f32 = IPOD_VIDEO.wheel_d;
const WHEEL_CY: f32 = IPOD_VIDEO.wheel_cy;

/// How far the hold switch stands proud of the top edge, and how much room is reserved above the
/// case for it. It is a control on the top face seen from the front, so what is visible is the part
/// sticking up — not the face it sits on.
const SWITCH_PROUD: f32 = 2.0;
/// The switch's own width and where it sits along the top edge, from the left of the case.
const SWITCH_W: f32 = 12.0;
const SWITCH_X: f32 = CASE_W - 17.0;

/// The interpreter's measured throughput, and a PP5021C's. The ratio of the two is the one number
/// that has to be visible in every mode.
///
/// ~21.5 M instructions/second headless and ~19 M with the window drawing, against ~72 MIPS. Both
/// measured; see `tools/ipod-emulator/README.md` §"The two speed ratios".
const HARDWARE_MIPS: f64 = 72e6;

// ---------------------------------------------------------------- one window, one shape
//
// **The window never changes size, and nothing inside it ever scrolls.** Those two rules are the
// same rule: a program that resizes itself between screens is several windows wearing one title
// bar, and a screen that scrolls is a screen that did not fit and said nothing. Scrolling in this
// program belongs to the click wheel and to nothing else — an earlier version took scroll from
// anywhere in the window, so scrolling a panel turned the wheel and RetailOS's menu moved with it.
//
// The minimum is not a guess. `every_screen_fits_the_smallest_window` lays every screen out at
// exactly this size with no window and no GPU, and fails if any of them needs more room — so the
// number below and the content are checked against each other on every `cargo test` rather than
// on somebody's laptop.

/// The narrowest and shortest the window may be made.
///
/// **`MIN_H` is derived from the pages, not chosen for them.** Measured 2026-08-18 by the test
/// below, at `MIN_W`, in pixels of content:
///
/// ```text
///   first run,  nothing chosen        512
///   first run,  two files chosen      512
///   first run,  two files refused     667
///   settings,   two files chosen      523
///   settings,   two files refused     678   <- the tallest page this program has
///   help                              470
///   details                           230
/// ```
///
/// The tallest is the one nobody designs for and everybody eventually sees: the settings, with a
/// restart to offer, both files printing the sentence explaining why they were turned down, *and*
/// the warning that the two are for different iPods. 700 leaves that case a little over 20 px,
/// which is the room a different font or a translated string would want, and still fits a 1366x768
/// laptop once the menu bar and the title bar are counted. The previous minimum was 520 — under
/// every one of these numbers.
const MIN_W: f32 = 720.0;
const MIN_H: f32 = 700.0;
/// What the window opens at. Comfortably above the minimum, and the device gets the difference.
const DEFAULT_W: f32 = 980.0;
const DEFAULT_H: f32 = 800.0;
/// The reading column. Capped so prose does not run the full width of a maximised window.
const COLUMN_W: f32 = 620.0;
/// The space above a page's first line and below its last.
const PAGE_MARGIN: f32 = 20.0;

/// Wrap this binary in a macOS `.app` bundle.
///
/// **It bundles itself.** The shell script this replaced took the binary as an argument, which is
/// one more thing a release step can get wrong — pointing it at yesterday's build produces an app
/// that looks right and is stale, and RELEASING already carries a check for exactly that class of
/// mistake. `current_exe()` cannot be the wrong binary.
///
/// The icon work shells out to `sips` and `iconutil`, which are macOS's own and have no Rust
/// equivalent worth carrying. Missing either is not fatal: an app with no icon still runs, and a
/// release that stopped because of a picture would be worse.
#[cfg(target_os = "macos")]
fn make_app(out: &str, icon: Option<&str>) -> Result<String, String> {
    use std::path::PathBuf;
    let me = std::env::current_exe().map_err(|e| e.to_string())?;
    let app = PathBuf::from(out).join("ipod-emulator.app");
    let _ = std::fs::remove_dir_all(&app);
    let macos = app.join("Contents/MacOS");
    let res = app.join("Contents/Resources");
    std::fs::create_dir_all(&macos).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&res).map_err(|e| e.to_string())?;
    std::fs::copy(&me, macos.join("ipod-emulator")).map_err(|e| format!("copying self: {e}"))?;

    if let Some(icon) = icon.filter(|p| std::path::Path::new(p).is_file()) {
        let set = std::env::temp_dir().join(format!("ipod-iconset-{}", std::process::id())).join("icon.iconset");
        if std::fs::create_dir_all(&set).is_ok() {
            for s in [16u32, 32, 128, 256, 512] {
                for (px, name) in [(s, format!("icon_{s}x{s}.png")), (s * 2, format!("icon_{s}x{s}@2x.png"))] {
                    let _ = std::process::Command::new("sips")
                        .args(["-z", &px.to_string(), &px.to_string(), icon, "--out"])
                        .arg(set.join(&name))
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .status();
                }
            }
            let _ = std::process::Command::new("iconutil")
                .arg("-c").arg("icns").arg(&set).arg("-o").arg(res.join("icon.icns"))
                .stderr(std::process::Stdio::null())
                .status();
            let _ = std::fs::remove_dir_all(set.parent().unwrap());
        }
    }

    // One version, from the workspace, so the bundle cannot report a different number from the
    // program inside it.
    let v = env!("CARGO_PKG_VERSION");
    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key>                 <string>ipod-emulator</string>
  <key>CFBundleDisplayName</key>          <string>ipod-emulator</string>
  <key>CFBundleExecutable</key>           <string>ipod-emulator</string>
  <key>CFBundleIdentifier</key>           <string>net.siggifly.ipod-emulator</string>
  <key>CFBundleVersion</key>              <string>{v}</string>
  <key>CFBundleShortVersionString</key>   <string>{v}</string>
  <key>CFBundlePackageType</key>          <string>APPL</string>
  <key>CFBundleIconFile</key>             <string>icon</string>
  <key>LSMinimumSystemVersion</key>       <string>11.0</string>
  <!-- The panel is 320x240 upscaled; without this it renders at 1x and looks soft on Retina. -->
  <key>NSHighResolutionCapable</key>      <true/>
</dict>
</plist>
"#
    );
    std::fs::write(app.join("Contents/Info.plist"), plist).map_err(|e| e.to_string())?;
    Ok(app.display().to_string())
}

/// The bundle is a macOS format; everywhere else this is an error rather than a silent no-op, so a
/// release script cannot "succeed" at producing nothing.
#[cfg(not(target_os = "macos"))]
fn make_app(_out: &str, _icon: Option<&str>) -> Result<String, String> {
    Err("a .app bundle is a macOS format; there is nothing to build here".into())
}

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "-h" || a == "--help") {
        print_help();
        return Ok(());
    }
    // Two answers that need no machine, no window and no images. Both print and exit, so they work
    // over SSH and in CI.
    if let Some(i) = args.iter().position(|a| a == "--make-app") {
        let out = args.get(i + 1).cloned().unwrap_or_else(|| ".".into());
        let icon = args.get(i + 2).cloned();
        match make_app(&out, icon.as_deref()) {
            Ok(p) => {
                println!("{p}");
                std::process::exit(0);
            }
            Err(e) => {
                eprintln!("--make-app: {e}");
                std::process::exit(1);
            }
        }
    }
    if args.iter().any(|a| a == "--check-update") {
        match update::check() {
            Some(f) => println!("{}", f.line()),
            None => println!(
                "No answer from GitHub. That is the expected result offline, and this is the only \
                 place it is ever mentioned — the window says nothing when a check fails."
            ),
        }
        return Ok(());
    }

    settings::migrate_legacy();

    let mut settings = Settings::load();
    let cfg = match config(&args, &settings) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };
    if args.iter().any(|a| a == "--check-images") {
        std::process::exit(inspect::report(&cfg.flash, &cfg.disk));
    }

    // The command line wins over the remembered mode, and does not overwrite it: `--debug` for one
    // run should not silently change what the next launch opens in.
    let mode = if args.iter().any(|a| a == "--debug") {
        Mode::Debug
    } else if args.iter().any(|a| a == "--user") {
        Mode::User
    } else {
        settings.mode
    };
    settings.mode = mode;

    let headless =
        cfg.headless.is_some() || cfg.selftest || cfg.probe.is_some() || cfg.power_cycle_at.is_some();

    let missing = missing_images(&cfg);
    if headless {
        // No window means nowhere to ask, so a missing image is fatal here and says which one.
        if let Some(e) = missing {
            eprintln!("{e}");
            std::process::exit(2);
        }
        let link = Link::new();
        let worker = spawn_worker(cfg, Arc::clone(&link));
        let _ = worker.join();
        return Ok(());
    }

    // The icon, as raw RGBA rather than a PNG, because decoding one would mean a decoder: this
    // crate's only dependency is `eframe` and `eapp_loader::png` encodes but does not read.
    // 512x512x4 = 1 MiB, generated from `docs/media/icon-1024.png`.
    //
    // 512 and not 64: winit documents the macOS window icon as unsupported, and it is — but eframe
    // sets the application icon there anyway, so Cmd-Tab does show this. Cmd-Tab draws at 256
    // physical pixels on a Retina display, and a 64-pixel source upscaled four times is what
    // "low-res icon" looks like. Measured, after shipping exactly that mistake.
    const ICON_RGBA: &[u8] = include_bytes!("../../../docs/media/icon-512.rgba");
    let icon = egui::IconData { rgba: ICON_RGBA.to_vec(), width: 512, height: 512 };

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([DEFAULT_W, DEFAULT_H])
            .with_min_inner_size([MIN_W, MIN_H])
            .with_icon(icon)
            // The machine, not the binary. A window called `ipod-emulator` says what ran it; one
            // called `iPod Video (5G / 5.5G)` says what is in it, which is the thing a second
            // model would change. Deliberately no `— RetailOS`: the OS is whatever the drive
            // holds, and this window will one day boot a drive that holds something else.
            .with_title(IPOD_VIDEO.name),
        ..Default::default()
    };
    // `--ipsw=` hands a bundle to the window at launch, which builds a drive from it exactly as
    // dropping one does. `ipod-boot make-disk` is the way to do it without a window at all.
    let ipsw = args
        .iter()
        .find_map(|a| a.strip_prefix("--ipsw="))
        .unwrap_or_default()
        .to_string();
    eframe::run_native(
        "ipod-emulator",
        options,
        Box::new(move |cc| Ok(Box::new(App::new(&cc.egui_ctx, cfg, settings, ipsw)))),
    )
}

/// Set the window's colours and text sizes explicitly, instead of inheriting them.
///
/// **This shipped broken and it is worth saying why.** Nothing here called `set_visuals`, so egui
/// used its default, which follows the operating system — and the device is drawn on a black
/// background regardless. On the wrong system that is dark grey text on black: the first-run screen's
/// heading was barely legible and its body text was not legible at all. A user reported it as "5px,
/// dark-grey text on a black background: nothing is readable", and he was right.
///
/// It survived because a developer never sees that screen. Run the binary from inside the
/// repository and `repo_root()` resolves the default image paths, `both_good()` passes, and it
/// boots straight past setup. The screen is only reachable from a machine that does not have the
/// files — which is every user and no author.
///
/// So: one theme, chosen here, dark to match the device's surround, with text light enough to read
/// against it and large enough to read at all.
fn theme(ctx: &egui::Context) {
    // Force it. egui keeps a style per theme and `set_visuals` only touches the CURRENT one, so
    // leaving the preference on System means the window's readability depends on a setting in the
    // user's operating system. That is precisely how this shipped unreadable.
    ctx.set_theme(egui::ThemePreference::Dark);
    let mut v = egui::Visuals::dark();
    v.override_text_color = Some(Color32::from_gray(0xE6));
    v.panel_fill = Color32::from_gray(0x12);
    v.window_fill = Color32::from_gray(0x12);
    v.extreme_bg_color = Color32::from_gray(0x08);
    // Controls need to be visible as controls, not guessed at.
    v.widgets.inactive.bg_fill = Color32::from_gray(0x2A);
    v.widgets.inactive.weak_bg_fill = Color32::from_gray(0x2A);
    v.widgets.hovered.bg_fill = Color32::from_gray(0x3A);
    v.widgets.active.bg_fill = Color32::from_gray(0x45);
    v.widgets.noninteractive.fg_stroke.color = Color32::from_gray(0xC8);
    v.widgets.inactive.fg_stroke.color = Color32::from_gray(0xE6);
    ctx.set_visuals(v);

    // egui's defaults are sized for a dense tool panel. This screen is prose somebody reads once,
    // on a machine where nothing works yet, so it gets ordinary reading sizes.
    use egui::{FontFamily::Proportional, FontId, TextStyle::*};
    ctx.all_styles_mut(|st| {
        st.text_styles = [
            (Heading, FontId::new(22.0, Proportional)),
            (Body, FontId::new(14.5, Proportional)),
            (Button, FontId::new(14.0, Proportional)),
            (Small, FontId::new(12.5, Proportional)),
            (Monospace, FontId::new(13.0, egui::FontFamily::Monospace)),
        ]
        .into();
    });
}

fn spawn_worker(cfg: emu::Config, link: Arc<Link>) -> std::thread::JoinHandle<()> {
    std::thread::Builder::new()
        .name("ipod".into())
        // The interpreter recurses through nothing deep, but a restore allocates a 64 MB region
        // list on this thread and the default 2 MB stack is uncomfortably close to some of it.
        .stack_size(8 << 20)
        .spawn(move || emu::run(cfg, link))
        .expect("cannot start the emulator thread")
}

fn print_help() {
    println!(
        "\
ipod-emulator — an interactive iPod over the eapp-loader emulator

  --user | --debug        which mode to open in, this run only. The default is whatever you
                          last switched to in the window; a fresh install is user mode
  --copy | --no-copy      run on a COPY of the drive image, leaving the original untouched, or
                          insist on running directly. The default is direct, which is what the
                          hardware does -- the iPod writes its settings to its own disk, and
                          closing the window parks the machine so the next launch resumes. A copy
                          costs 8 GB on any filesystem without reflinks, and forgets what the iPod
                          wrote. Neither flag means whatever the window last used
  --cold                  boot from the reset vector even if a snapshot exists (~75 s)
  --snapshot=FILE         where the idle snapshot lives (default: a per-user cache directory)
  --snap-at=N             instruction count the snapshot is taken at (default 1600000000)
  --clock=N               interpreter instructions per simulated microsecond (default 75, the
                          real part). Lowering it skips the bootloader's timeout-polled delay
                          loops, which is why measurement used 5 -- but it makes the machine's
                          own clock outrun its code, and timing-sensitive code notices
  --wheel-click-instr=N   instructions between the frames of a rotation (default 300000, which
                          is 4 ms at --clock=75; it was 20000, the same 4 ms at --clock=5)
  --flash=FILE            the NOR image (default: what the window was last pointed at,
                          else the retail ROM under resources/)
  --disk=FILE             the drive image (default: as above, else resources/drives/)
  --workdisk=FILE         the writable per-run clone. Naming one implies --copy
  --ipsw=FILE             build a drive from this bundle at launch, exactly as dropping it on
                          the window would; `ipod-boot make-disk IPSW OUT.img` needs no window
  --check-images          no window: parse both images, say what they are, exit 0 if usable
  --check-update          no window: ask GitHub for the latest release. Silent when offline
  --headless=N            no window: run N instructions and print the boot fingerprint
  --selftest              no window: push a scripted gesture through the GUI's own input path
                          and print what reached RetailOS
  --selftest-control      the matched control: the same run with no input at all
  --cop-awake             stop forcing COP_STATUS to say the second core is asleep (ledger #7).
                          Does NOT emulate the core -- it stops lying about it, so the
                          dependency can be measured for the first time.
  --no-ide-irq-latch      stop reporting the ATA controller's interrupt in IDE0_CFG bit 3
                          (ledger #9). The arm that says what the latch is worth.
  --input-regs=BASE:SIZE  enumerate the addresses read before they were ever written -- the
                          hardware inputs we answer with whatever the region holds. The honest
                          list of where this emulator invents.
  --read-count=A[,A]      count reads of these addresses, with the PC that made each. Answers
                          whether the firmware ever looks at it -- which --input-regs cannot
                          answer once the address has been seeded.
  --watch-writes=BASE:LEN log every write into a range with the PC that made it. A buffer's
                          first writer names where its contents came from.
  --regs-at=ADDR:N        dump the register file the first N times ADDR executes. At a bignum
                          loop head the registers are the operands.
  --profile               count executed instructions per 64-byte bucket and print the
                          hottest. Finds a loop that a call graph cannot see.
  --no-idle-stop          do not stop a headless run when no NEW code has executed. A long
                          computation through known code looks idle to that heuristic.
  --save-region=NAME:FILE write a memory region out at the end of a headless run, so
                          `tcb` can read the RTXC scheduler off a real boot.
  --trace-calls-from=N    record every `bl` taken after N instructions. For flattened code the
                          calls are the shape the obfuscation cannot hide.
  --trace-pc=LO:HI        record every address executed in this range. For code that is
                          control-flow flattened, watching beats reading.
  --control=PATH          open a control socket: wheel / press / shot / peek / state.
                          Lets something other than a person drive the machine.
  --watch=ADDR[,ADDR]     report a word whenever it changes. `--watch=14937194` is the DRM
                          context pointer; it has been 0 in every arm measured so far.
  --probe=WHICH           no window: act at --probe-at and watch the panel for 800 M instructions.
                          menu | menu-control | combo | combo-control
  --probe-at=N            instruction anchor the probe acts at (default 1500000000)
  --clock-v3              ablation: zero `slept_usec` after a restore, reproducing the snapshot
                          format from before 2026-08-14. For the A/B in research/10 Addendum 31.
  --charger               plug the mains charger in (GPIOL bit 3 low). The default is a bare iPod;
                          with a charger RetailOS returns to its charging screen when idle.
  --samples=A,B,C         where --probe samples the panel, in millions of instructions past the
                          moment it acts (default 8,40,100,200,400,800)
  --ablate=pmu            at the moment --probe acts, replace the PMU with a factory-fresh one —
                          the state a restored machine runs with, since a snapshot omits the chip
  --power-cycle-at=N      cut power at instruction N and boot again, with no window. The self-check
                          for the power controls: the second session prints its own fingerprint

Keys: arrows scroll the wheel · Enter/Space select · M menu · P play · , / . prev / next
      H toggles hold · D switches user/debug · S writes a .png and a .ppm into _out/ (debug)"
    );
}

/// Where the snapshot and the writable clone live for a given machine.
///
/// Keyed on the emulator's own source, the boot configuration and the snapshot point, for the
/// reason `from-idle.sh` spells out at length: a snapshot restored under a different build is a
/// hybrid machine, and it is the most convincing silent failure this project has available.
///
/// Three files, and the third is load-bearing: `.frozen` is the drive as it stood when `.snap` was
/// taken, and `.img` is the throwaway the machine actually writes to. See `emu::Config::frozen` for
/// why a snapshot without its drive is not restorable.
fn cache_paths(flash: &Path, disk: &Path, clock: usize, snap_at: u64) -> Cache {
    let key = cache_key(flash, disk, clock, snap_at);
    let cache = settings::data_dir();
    let _ = std::fs::create_dir_all(&cache);
    Cache {
        snap: cache.join(format!("idle-{key}.snap")),
        frozen: cache.join(format!("idle-{key}.frozen")),
        work: cache.join(format!("idle-{key}.img")),
        stamp: cache.join(format!("idle-{key}.drive")),
    }
}

/// The cached files belonging to one boot configuration.
///
/// Four names, of which any one launch writes the snapshot and **one** of the other three: the
/// frozen drive and the working copy belong to copy mode, the stamp to working directly. They share
/// a stem so that `reclaimable` can recognise a whole set, and so that switching modes leaves the
/// unused ones visibly reclaimable rather than invisibly resident.
struct Cache {
    snap: PathBuf,
    frozen: PathBuf,
    work: PathBuf,
    /// A few bytes naming the drive the snapshot was taken against. See [`emu::Config::stamp`].
    stamp: PathBuf,
}

/// Delete every cached drive and snapshot that does not belong to the configuration now loaded.
///
/// **This is why the cache is keyed and not accumulated.** A working disk is 8 GB sparse and a
/// snapshot is about 1.6 GB, and the key includes both image paths — so trying four firmware
/// versions used to leave four of each, silently, in a directory the user never opened, on whatever
/// volume the program happened to resolve. Somebody lost 50 GB that way and was right to be angry
/// about it. One set is kept: the one belonging to the images now loaded.
///
/// The frozen drive added a third file per key without materially changing the bill — it is made
/// with `cp -c`, so on APFS it shares its blocks with the working drive and only costs what the two
/// have written differently.
/// What is in the cache that this launch will not use, and how much it comes to.
///
/// **This only looks.** It used to delete, at startup, with no prompt and no way to decline — and
/// the cache key includes a hash of the emulator's own source, so *any* change to the model mints a
/// new key and orphans the previous 8 GB frozen drive and its snapshot. That made "reclaimed 17.3
/// GB from images no longer in use" a message on almost every launch during development, and each
/// one threw away a 75-second cold boot somebody had waited for, plus whatever state they had
/// parked with `snapshot`.
///
/// Disk is the user's, and 17 GB is not a rounding error. The size is stated and the button is
/// theirs to press.
///
/// **What counts as "in use" depends on the mode**, and that is the point rather than a detail.
/// Working directly, the frozen drive and the working copy for these very images are not going to
/// be written and not going to be read — so they are exactly what somebody switching to direct mode
/// wants back, and keeping them because their name matches would hold 8 GB against a mode that
/// exists to stop holding 8 GB.
///
/// The directory is a parameter rather than `settings::data_dir()` read from inside, because that
/// reads an environment variable — and two tests pointing it at their own directories are two
/// tests racing over one process-wide value. Passing it in is also the honest signature: this
/// function's answer is entirely about one directory.
fn reclaimable(dir: &Path, keep: &Cache, work_on_copy: bool) -> (u64, Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return (0, Vec::new()) };
    let mut total = 0;
    let mut paths = Vec::new();
    for e in rd.flatten() {
        let p = e.path();
        let name = e.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with("idle-") {
            continue;
        }
        let in_use = p == keep.snap
            || (work_on_copy && (p == keep.frozen || p == keep.work))
            || (!work_on_copy && p == keep.stamp);
        if in_use {
            continue;
        }
        // Real blocks, not the logical length: these are sparse clones, and their length is a
        // number about the emulated drive rather than about this disk.
        total += e.metadata().map(|m| settings::on_disk_size(&m)).unwrap_or(0);
        paths.push(p);
    }
    (total, paths)
}

/// Delete what `reclaimable` found, once somebody has asked for it.
fn reclaim(paths: &[PathBuf]) -> u64 {
    let mut freed = 0;
    for p in paths {
        let n = std::fs::metadata(p).map(|m| settings::on_disk_size(&m)).unwrap_or(0);
        if std::fs::remove_file(p).is_ok() {
            freed += n;
        }
    }
    freed
}

impl App {
    /// The cache, stated and reclaimable, from wherever the user happens to be.
    ///
    /// This lived on the last step of the setup wizard, which is unreachable once a machine is
    /// running and unreachable *before* it if an earlier step is not satisfied. So the operator
    /// watched the folder grow to 19 GB with the only control behind a door they could not open —
    /// which is worse than the automatic deletion it replaced, because at least that one worked.
    fn cache_controls(&mut self, ui: &mut egui::Ui) {
        let (stale, paths) = (self.stale_cache.0, self.stale_cache.1.clone());
        ui.horizontal_wrapped(|ui| {
            // The figure and the folder on one line, the folder on hover. A path nobody can find
            // is a preference nobody can reset — but a path on its own line, above the number that
            // is the actual answer, was costing more height than it was worth.
            ui.label(
                egui::RichText::new(format!("{} in ", human(cache_size())))
                    .small()
                    .color(Color32::from_gray(0x9A)),
            )
            .on_hover_text(settings::data_dir().display().to_string());
            ui.label(
                egui::RichText::new(settings::data_dir().display().to_string())
                    .small()
                    .monospace()
                    .color(Color32::from_gray(0x78)),
            );
            if stale > 0 {
                if ui
                    .button(format!("reclaim {}", human(stale)))
                    .on_hover_text(
                        "Snapshots and working drives from earlier builds of this emulator, from \
                         other image pairs, or from working on a copy when you now work on the \
                         drive itself. A new set is minted whenever the emulator's own model \
                         changes, because a snapshot must never restore a machine the model no \
                         longer describes. Deleting them costs those images a cold boot, and \
                         nothing else.",
                    )
                    .clicked()
                {
                    let freed = reclaim(&paths);
                    self.stale_cache = (0, Vec::new());
                    self.say(format!("reclaimed {}", human(freed)));
                }
            } else {
                ui.label(
                    egui::RichText::new("nothing to reclaim")
                        .small()
                        .color(Color32::from_gray(0x78)),
                );
            }
        });
    }
}

/// Why this drive is being written to, or not.
///
/// The question the window answers out loud, because "your image will be modified" is not something
/// to discover afterwards.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum WriteTo {
    /// The drive itself. It is one this program built and can rebuild byte for byte.
    OursDirect,
    /// The drive itself, because a person asked for that.
    YoursDirect,
    /// A copy, because a person asked for that.
    ChosenCopy,
    /// A copy, because the drive came from the user and nothing said otherwise.
    TheirsByDefault,
    /// A copy, because the file cannot be opened for writing at all.
    ReadOnly,
}

impl WriteTo {
    fn copies(self) -> bool {
        !matches!(self, WriteTo::OursDirect | WriteTo::YoursDirect)
    }

    /// One line for the row, in the user's terms.
    fn line(self) -> &'static str {
        match self {
            WriteTo::OursDirect => "The iPod writes to this drive. It was built here and can be rebuilt.",
            WriteTo::YoursDirect => "The iPod writes to this drive — your file will change.",
            WriteTo::ChosenCopy => "The iPod writes to a copy. Your file is untouched.",
            WriteTo::TheirsByDefault => "The iPod writes to a copy, so your file is untouched. It is yours, not ours.",
            WriteTo::ReadOnly => "This file is read-only, so the iPod writes to a copy.",
        }
    }
}

/// **Whose file gets written to, and why.**
///
/// A drive this program built from a bundle is named for the bundle's version and CRC, so building
/// it again produces the same bytes at the same path — writing to it costs nothing that cannot be
/// got back. A drive somebody supplied might be the only image of an iPod they own, and one of
/// those took twelve iTunes sync rounds to make.
///
/// So the *default* follows provenance, and an explicit answer overrides it in either direction.
/// A read-only file overrides everything, because direct is not merely unwise there, it does not
/// work: `Ata::open` asks for write access and the operating system refuses, which used to surface
/// as `disk: Permission denied (os error 13)` after the window had already committed to booting.
fn write_target(disk: &Path, chosen: Option<bool>) -> WriteTo {
    let readonly = std::fs::metadata(disk)
        .map(|m| m.permissions().readonly())
        .unwrap_or(false);
    let ours = disk.parent().is_some_and(|p| p == drives_dir());
    match (readonly, chosen) {
        (true, _) => WriteTo::ReadOnly,
        (false, Some(true)) => WriteTo::ChosenCopy,
        (false, Some(false)) => {
            if ours { WriteTo::OursDirect } else { WriteTo::YoursDirect }
        }
        (false, None) if ours => WriteTo::OursDirect,
        (false, None) => WriteTo::TheirsByDefault,
    }
}

/// Where drives built from Apple's bundles are kept.
///
/// A folder of its own, under the one data directory, because these are the only files here that
/// are **derived and durable**: a snapshot is regenerable in seventy-five seconds and a working
/// copy is a throwaway, but a built drive is what an iPod has been writing its settings and its
/// music to, and it is named for the software it holds rather than for when it was made. When
/// machines are a first-class thing they will point at these; nothing else in the data directory
/// is something another file may reference.
fn drives_dir() -> PathBuf {
    let d = settings::data_dir().join("drives");
    let _ = std::fs::create_dir_all(&d);
    d
}

/// What the cache currently holds, for the settings to state rather than hide.
fn cache_size() -> u64 {
    settings::dir_size(&settings::data_dir())
}

/// `12.4 GB`, `840 MB`, `0 bytes` — sized for a sentence, not a table.
fn human(n: u64) -> String {
    const K: u64 = 1000;
    match n {
        0 => "nothing".into(),
        n if n < K * K => format!("{:.0} kB", n as f64 / K as f64),
        n if n < K * K * K => format!("{:.0} MB", n as f64 / (K * K) as f64),
        n => format!("{:.1} GB", n as f64 / (K * K * K) as f64),
    }
}

fn config(args: &[String], saved: &Settings) -> Result<emu::Config, String> {
    let get = |k: &str| args.iter().find_map(|a| a.strip_prefix(k)).map(|s| s.to_string());
    let num = |k: &str, d: u64| -> u64 {
        get(k).and_then(|v| v.replace('_', "").parse().ok()).unwrap_or(d)
    };

    // Three sources, in order: the command line, what the window last recorded, and the
    // gitignored `resources/` tree the recipes use. The last one is `retail-boot.sh`'s default
    // verbatim, because a GUI that quietly booted the *prototype* NOR would produce a different
    // machine from every number in research/ with nothing saying so.
    let root = settings::repo_root();
    let res = root.join("resources");
    let flash = get("--flash=")
        .map(PathBuf::from)
        .or_else(|| saved.flash.clone())
        .unwrap_or_else(|| {
            res.join("roms/retail_5g_MA146_HwVr000B0005_internal_rom_000000-0FFFFF.bin")
        });
    let disk = get("--disk=")
        .map(PathBuf::from)
        .or_else(|| saved.disk.clone())
        .unwrap_or_else(|| res.join("drives/ipod8g-retail.img"));

    // 75 instructions per simulated microsecond is the real PP5021C. It was 5 for most of this
    // project -- a research accelerant, adopted in research/03 because the bootloader polls with
    // timeouts and a low clock skips those delay loops, reaching 5 ATA commands in a 600 M budget
    // where real time reached 2. It was never turned back, and it is wrong for anything that is
    // being *used* rather than measured: the machine's own sense of time runs 15x fast, so every
    // wait a game asks for expires almost immediately. Brick's ball was unplayable for this
    // reason. Measurement can still ask for the accelerant by name.
    let clock = num("--clock=", 75) as usize;
    let snap_at = num("--snap-at=", 1_600_000_000);
    let cache = cache_paths(&flash, &disk, clock, snap_at);
    // A hand-given snapshot brings its own frozen drive, sitting beside it under the same stem.
    // Letting it fall back to the keyed one would pair a snapshot chosen by the user with a drive
    // chosen by the cache — which is the stale pair again, arrived at from a different direction.
    let (snapshot, frozen) = match get("--snapshot=").map(PathBuf::from) {
        Some(s) => {
            let f = s.with_extension("frozen");
            (s, f)
        }
        None => (cache.snap, cache.frozen),
    };

    // `--copy` for one run, `--no-copy` for one run, and neither means whatever was last chosen in
    // the window. The pair exists because `--debug` has a `--user`: a flag that can only turn a
    // remembered setting *on* leaves somebody who ticked the box once with no way to try the other
    // way round without editing a file.
    //
    // `--workdisk=` naming a file is itself a request for a working copy — it is a *separate* drive
    // for the machine to write to, which is the whole of what copy mode means.
    let explicit_workdisk = get("--workdisk=").map(PathBuf::from);
    // The flags are an answer for this run; `saved` is the remembered answer; absent from both, the
    // drive's own provenance decides. See `write_target`.
    let chosen = if args.iter().any(|a| a == "--no-copy") {
        Some(false)
    } else if args.iter().any(|a| a == "--copy") || explicit_workdisk.is_some() {
        Some(true)
    } else {
        saved.work_on_copy
    };
    let work_on_copy = write_target(&disk, chosen).copies();

    // **Which drive the machine writes to, decided here and only here.** It used to be decided in
    // the window's `start`, which meant every path that does not go through a window — `--headless`,
    // `--selftest`, `--probe`, `--power-cycle-at` — kept pointing at a cached working copy that
    // nothing had made, and opened it, and failed. `Ata::open` does not create files.
    let workdisk = match (&explicit_workdisk, work_on_copy) {
        (Some(p), _) => p.clone(),
        (None, true) => cache.work,
        (None, false) => disk.clone(),
    };

    Ok(emu::Config {
        flash,
        disk,
        workdisk,
        frozen,
        clock,
        snapshot: Some(snapshot),
        snap_at,
        work_on_copy,
        cold: args.iter().any(|a| a == "--cold"),
        control: get("--control=").map(PathBuf::from),
        cop_awake: args.iter().any(|a| a == "--cop-awake"),
        ide_irq_latch_off: args.iter().any(|a| a == "--no-ide-irq-latch"),
        read_count: get("--read-count=")
            .map(|s| {
                s.split(',')
                    .filter_map(|a| u32::from_str_radix(a.trim().trim_start_matches("0x"), 16).ok())
                    .collect()
            })
            .unwrap_or_default(),
        input_regs: get("--input-regs=").and_then(|s| {
            let (b, n) = s.split_once(':')?;
            Some((
                u32::from_str_radix(b.trim_start_matches("0x"), 16).ok()?,
                u32::from_str_radix(n.trim_start_matches("0x"), 16).ok()?,
            ))
        }),
        // `--trace-pc=LO:HI`, hex, for watching a flattened function execute.
        watch_writes: get("--watch-writes=").and_then(|s| {
            let (b, n) = s.split_once(':')?;
            Some((
                u32::from_str_radix(b.trim_start_matches("0x"), 16).ok()?,
                u32::from_str_radix(n.trim_start_matches("0x"), 16).ok()?,
            ))
        }),
        regs_at: get("--regs-at=").and_then(|s| {
            let (a, n) = s.split_once(':').unwrap_or((s.as_str(), "1"));
            Some((u32::from_str_radix(a.trim_start_matches("0x"), 16).ok()?, n.parse().ok()?))
        }),
        profile: args.iter().any(|a| a == "--profile"),
        no_idle_stop: args.iter().any(|a| a == "--no-idle-stop"),
        save_region: get("--save-region=").and_then(|s| {
            let (n, f) = s.split_once(':')?;
            Some((n.to_string(), PathBuf::from(f)))
        }),
        trace_calls_from: get("--trace-calls-from=").and_then(|s| s.parse().ok()),
        trace_pc: get("--trace-pc=").and_then(|s| {
            let (a, b) = s.split_once(':')?;
            Some((
                u32::from_str_radix(a.trim_start_matches("0x"), 16).ok()?,
                u32::from_str_radix(b.trim_start_matches("0x"), 16).ok()?,
            ))
        }),
        // Comma-separated hex, with or without 0x, because both spellings turn up in the research
        // notes these addresses are copied out of.
        watch: get("--watch=")
            .map(|s| {
                s.split(',')
                    .filter_map(|v| u32::from_str_radix(v.trim().trim_start_matches("0x"), 16).ok())
                    .collect()
            })
            .unwrap_or_default(),
        // Held at 4 ms of *simulated* time, which is what the firmware's wheel poll sees. The
        // figure is 15x the old one because the clock it is divided by went up by 15.
        click_gap: num("--wheel-click-instr=", 300_000).max(1),
        headless: get("--headless=").and_then(|v| v.replace('_', "").parse().ok()),
        selftest: args.iter().any(|a| a == "--selftest" || a == "--selftest-control"),
        selftest_control: args.iter().any(|a| a == "--selftest-control"),
        shots: root.join("_out"),
        probe: match get("--probe=").as_deref() {
            None => None,
            Some("menu") => Some(emu::Probe::Menu),
            Some("menu-control") => Some(emu::Probe::MenuControl),
            Some("combo") => Some(emu::Probe::Combo),
            Some("combo-control") => Some(emu::Probe::ComboControl),
            Some(other) => {
                return Err(format!(
                    "--probe={other}: expected menu, menu-control, combo or combo-control"
                ))
            }
        },
        probe_at: num("--probe-at=", 1_500_000_000),
        clock_v3: args.iter().any(|a| a == "--clock-v3"),
        charger: args.iter().any(|a| a == "--charger"),
        samples: get("--samples=")
            .map(|v| {
                v.split(',')
                    .filter_map(|s| s.trim().parse::<u64>().ok())
                    .map(|m| m * 1_000_000)
                    .collect()
            })
            .unwrap_or_default(),
        ablate_pmu: get("--ablate=").as_deref() == Some("pmu"),
        power_cycle_at: get("--power-cycle-at=").and_then(|v| v.replace('_', "").parse().ok()),
    })
}

/// Which of the two images is not there, phrased for somebody who has just cloned this and does not
/// yet know that `resources/` is deliberately absent.
fn missing_images(cfg: &emu::Config) -> Option<String> {
    let mut out = String::new();
    for (what, p) in [("NOR dump", &cfg.flash), ("disk image", &cfg.disk)] {
        if !p.exists() {
            out.push_str(&format!("no {what} at {}\n", p.display()));
        }
    }
    if out.is_empty() {
        return None;
    }
    out.push_str(
        "\nThis emulator needs two files it does not ship: a 1 MB dump of your iPod's NOR flash, \
         and an image of its drive. Both are Apple's and neither is in this repository. \
         Run the window with no arguments and drop them in, `--check-images` to test a pair you \
         already have, or see the README's \"What you have to supply\".",
    );
    Some(out)
}


/// A short hash over everything that decides what the snapshot *is*. Not cryptographic — this is a
/// cache key, and its only job is to change whenever the machine would.
fn cache_key(flash: &std::path::Path, disk: &std::path::Path, clock: usize, snap_at: u64) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut eat = |b: &[u8]| {
        for x in b {
            h ^= *x as u64;
            h = h.wrapping_mul(0x100_0000_01b3);
        }
    };
    eat(flash.to_string_lossy().as_bytes());
    eat(disk.to_string_lossy().as_bytes());
    eat(&clock.to_le_bytes());
    eat(&snap_at.to_le_bytes());
    // The emulator's own source. `include_str!` puts the bytes in this binary, so a change to the
    // model changes the key without anything having to remember to bump it.
    eat(include_str!("../../eapp-loader/src/lib.rs").as_bytes());
    eat(include_str!("emu.rs").as_bytes());
    format!("{h:016x}")
}

// ---------------------------------------------------------------- the app

struct App {
    /// `None` until there is a machine to talk to — the state a fresh clone opens in.
    link: Option<Arc<Link>>,
    /// The emulator thread, kept so that closing the window can wait for it to park the machine.
    worker: Option<std::thread::JoinHandle<()>>,
    /// The machine to build once the images are satisfied. Cloned into the worker thread.
    cfg: emu::Config,
    /// What the window is showing. Independent of whether a machine exists — see [`Screen`].
    screen: Screen,
    images: Images,
    settings: Settings,
    /// The restart-needing settings as they stood when the settings screen was opened, so that
    /// "does this owe anyone a restart" is answered by comparison rather than by bookkeeping.
    /// `None` whenever the settings screen is closed.
    cold_at_open: Option<Cold>,
    /// Which screen a full-page detour was opened from, so leaving it returns you to where you
    /// were rather than to wherever the program thinks you ought to be. Used by the help and by
    /// the details pages alike — there is one rule, not one per page.
    back_to: Screen,
    /// Which file the details page is about: 0 the boot ROM, 1 the drive.
    details_row: usize,
    tex: egui::TextureHandle,
    seen_seq: u64,
    /// The dimmer level the texture on screen was built at, so a brightness change repaints even
    /// when no pixel moved.
    seen_backlight: u8,
    /// Where the drag started and the position it last reported, so a move is a *delta* in clicks.
    drag: Option<Drag>,
    hold: bool,
    /// Buttons the UI believes are down, so a release is only sent for a press that was sent.
    down: Vec<Button>,
    /// A keyboard scroll asserts touch; this is when it lapses if no further key arrives.
    kbd_touch_until: Option<Instant>,
    touching: bool,
    show_back_buffer: bool,
    /// The last few things the UI did, so a person can see their input was accepted.
    log: VecDeque<String>,
    shot_dir: PathBuf,
    last_shot: Option<String>,
    scale: u32,
    /// Where `hold_switch` drew the switch this frame, so the pointer test is against the thing on
    /// screen rather than against an offset guessed from the wheel's radius. The first version
    /// guessed, and the guess covered the whole upper half — clicking the screen threw the switch.
    hold_slot: Rect,
    /// The region the device is drawn in, remembered from the last frame so that input handling —
    /// which runs before the device is painted — can ask whether the pointer is over it.
    dev_area: Rect,
    /// `None` = no check has finished. `Some(None)` = a check ran and found nothing, which is what
    /// offline looks like and is never shown.
    update_slot: Arc<Mutex<Option<Option<update::Found>>>>,
    update_line: Option<String>,
    update_asked: bool,
    /// Leftover scroll that has not yet added up to a detent. See [`SCROLL_UNITS_PER_DETENT`].
    wheel_units: f32,
    /// Cache files this launch will not use: their total size, and where they are. Measured at
    /// `start`, deleted only when the button below is pressed.
    stale_cache: (u64, Vec<PathBuf>),
    /// A fault the emulator found in the drive before booting it, drawn under the case.
    ///
    /// RetailOS answers several unrelated faults with one picture — the plug-into-a-computer glyph
    /// — so a user who lands on it cannot tell which they hit, and neither could we. Anything we
    /// can determine ourselves is better said in our own words, next to the screen showing it.
    notice: Option<String>,
}

struct Drag {
    last: u8,
    /// A press inside a label zone is a button until the pointer leaves it; a press that starts on
    /// bare ring is a scroll and never becomes a button.
    button: Option<Button>,
    /// This gesture has already done whatever it was going to do — a hold-switch throw, or a press
    /// that began off the device. Held open so the pointer-up still has a gesture to close.
    consumed: bool,
}

/// Which of the three things the window is showing.
///
/// **This used to be inferred from whether a machine existed**, and that one fact standing in for
/// two is the whole of why opening the settings rebooted the iPod: the only way to see that screen
/// was to destroy the machine, because "no machine" was how the window knew to draw it. They are
/// separate now, and a machine runs behind the settings the entire time it is open.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Screen {
    /// Nothing is configured yet. One screen, no way out but forward — an emulator with no images
    /// is not a thing anyone can be left holding.
    FirstRun,
    /// The iPod.
    Device,
    /// Everything about the program, in sections, with the machine still running behind it.
    Settings,
    /// Where the two files come from. A page of its own, reachable from either of the others and
    /// returning to whichever asked for it — see [`App::back_to`].
    Help,
    /// Everything found inside one of the two chosen files. Which one is [`App::details_row`].
    Details,
}

/// The two files the emulator runs on, and what they turn out to be.
///
/// **No slots.** A file is identified by its contents ([`inspect::classify`]) and routed, so the
/// question "which box does this go in" is never asked. The previous version put both files, every
/// byte count, two model numbers and a paragraph about first boot onto a three-step wizard, and
/// then hid the program's settings on its last step where a running machine could not reach them.
struct Images {
    flash: String,
    disk: String,
    /// An IPSW to build a drive image *from*, which is the path most people should take: about
    /// 14 MB of Apple's firmware rather than 8 GB of somebody else's iPod.
    ipsw: String,
    /// An OS image waiting for a drive to be installed onto, and files waiting for a volume.
    /// Held rather than acted on immediately, because installing needs a target and the user may
    /// drop them in either order — the same reason nothing else here has slots.
    pending_os: Option<String>,
    pending_bundle: Option<String>,
    flash_verdict: Option<Verdict>,
    disk_verdict: Option<Verdict>,
    ipsw_verdict: Option<Verdict>,
    /// What each file *is*, when that can be said — the model and the device a ROM came off, the
    /// software version a drive holds. `None` falls back to the filename. See
    /// [`inspect::describe_rom`].
    flash_name: Option<String>,
    disk_name: Option<String>,
    /// What was found inside each, for the row to show rather than discard. See
    /// [`inspect::rom_facts`].
    flash_facts: Vec<inspect::Fact>,
    disk_facts: Vec<inspect::Fact>,
    /// Set when the ROM and the software are for **different iPods** — a fault of the pair that
    /// neither file's own verdict can see, and the one that fails silently seventy-five seconds
    /// into a boot. See [`inspect::family_mismatch`].
    mismatch: Option<String>,
    /// What the last build said, good or bad.
    built: Option<Result<String, String>>,
    /// What the last dropped or chosen file was, when it was not any of the three. Named rather
    /// than ignored: a drop that appears to do nothing is indistinguishable from a drop the window
    /// did not receive.
    rejected: Option<String>,
}

/// The settings that cannot be changed under a running machine.
///
/// **Everything else applies live.** Comparing this against itself is how the settings screen knows
/// whether it owes anyone a restart, and it is a struct rather than a set of `changed` flags
/// scattered through the widgets because those rot: a field added to the screen and forgotten in
/// the flags is a change that silently does not take effect, which is worse than one that asks.
#[derive(Clone, PartialEq, Eq)]
struct Cold {
    flash: String,
    disk: String,
    work_on_copy: bool,
}

impl Cold {
    /// What changed, in the user's words, for the banner to name.
    fn differences(&self, other: &Cold) -> Vec<&'static str> {
        let mut v = Vec::new();
        if self.flash != other.flash {
            v.push("the boot ROM");
        }
        if self.disk != other.disk {
            v.push("the drive");
        }
        if self.work_on_copy != other.work_on_copy {
            v.push("where the iPod writes");
        }
        v
    }
}

/// egui units of scroll per click-wheel detent.
///
/// egui reports one mouse-wheel line as `line_scroll_speed`, which it defaults to **40.0** on
/// native — so one notch of a physical wheel is one detent here. A trackpad sends a continuous
/// stream of much smaller values, which accumulate into the same thing, so the wheel glides under
/// a finger and steps under a notch. Taken from egui's default rather than tuned by feel, so it
/// stays right if that default changes and wrong visibly if it does not.
const SCROLL_UNITS_PER_DETENT: f32 = 40.0;

/// A verdict's first line — the Ready page summarises, it does not recite.
fn first_line(s: &str) -> String {
    s.lines().next().unwrap_or("").trim().to_string()
}

/// The path as a string, or empty if nothing is there.
fn existing(p: &Path) -> String {
    if p.is_file() {
        p.to_string_lossy().into_owned()
    } else {
        String::new()
    }
}

impl Images {
    fn new(cfg: &emu::Config) -> Images {
        let mut s = Images {
            // Only prefill a path that is actually there. The defaults are this repository's
            // layout, so a released binary would open on two paths that do not exist and an error
            // under each — telling a first-time user their files are wrong before they have chosen
            // any. Empty and asking is better than full and wrong.
            flash: existing(&cfg.flash),
            disk: existing(&cfg.disk),
            ipsw: String::new(),
            pending_os: None,
            pending_bundle: None,
            flash_verdict: None,
            disk_verdict: None,
            ipsw_verdict: None,
            flash_name: None,
            disk_name: None,
            flash_facts: Vec::new(),
            disk_facts: Vec::new(),
            mismatch: None,
            built: None,
            rejected: None,
        };
        s.revalidate();
        s
    }

    /// Parse whatever is in the two image fields. Cheap — both read a few hundred bytes at fixed
    /// offsets — except the NOR's build string, which reads 1 MB and only when the rest passed.
    ///
    /// The descriptions are computed here and cached for the same reason: naming a ROM means
    /// reading its `SysCfg`, which means reading the megabyte, which is not a thing to do sixty
    /// times a second because a window is open.
    fn revalidate(&mut self) {
        let f = PathBuf::from(self.flash.trim());
        let d = PathBuf::from(self.disk.trim());
        self.flash_verdict = f.is_file().then(|| inspect::flash(&f));
        self.disk_verdict = d.is_file().then(|| inspect::disk(&d));
        // Only for a ROM that passed. Describing a file that was just refused would put a
        // confident model name next to the sentence explaining why it cannot be used.
        self.flash_name = match &self.flash_verdict {
            Some(v) if v.ok() => inspect::describe_rom(&f, IPOD_VIDEO.short),
            _ => None,
        };
        self.disk_name = match &self.disk_verdict {
            Some(v) if v.ok() => inspect::describe_drive(&d),
            _ => None,
        };
        // What was found, for the rows to show instead of throwing it away. Computed here for the
        // same reason the names are: the ROM's facts mean reading its megabyte.
        self.flash_facts = match &self.flash_verdict {
            Some(v) if v.ok() => inspect::rom_facts(&f),
            _ => Vec::new(),
        };
        self.disk_facts = match &self.disk_verdict {
            Some(v) if v.ok() => inspect::drive_facts(&d),
            _ => Vec::new(),
        };
        self.recheck_pair();
    }

    /// Do the two chosen things belong to the same iPod?
    ///
    /// Asked whenever either changes, because the answer is about the pair and not about either
    /// one — which is exactly why neither file's own verdict could ever have caught it.
    fn recheck_pair(&mut self) {
        let rom_ok = matches!(&self.flash_verdict, Some(v) if v.ok());
        // The software's family, from whichever of the two things is actually there.
        let family = if !self.ipsw.trim().is_empty() {
            inspect::ipsw_family(Path::new(self.ipsw.trim()))
        } else {
            inspect::drive_family(Path::new(self.disk.trim()))
        };
        self.mismatch = rom_ok
            .then(|| inspect::family_mismatch(IPOD_VIDEO.short, IPOD_VIDEO.ipsw_family, family))
            .flatten();
    }

    /// The IPSW is checked separately because checking it means inflating 13.9 MB and hashing it,
    /// which is not something to do on every repaint.
    fn revalidate_ipsw(&mut self) {
        let p = PathBuf::from(self.ipsw.trim());
        self.ipsw_verdict = p.is_file().then(|| inspect::ipsw(&p));
        self.built = None;
        self.recheck_pair();
    }

    fn both_good(&self) -> bool {
        matches!(&self.flash_verdict, Some(v) if v.ok())
            && matches!(&self.disk_verdict, Some(v) if v.ok())
    }

    /// Take a file the user gave us — dropped anywhere on the window, or chosen from a dialog — and
    /// put it where it belongs.
    ///
    /// Returns what to say about it, which is never nothing: a file that vanishes without comment
    /// is the same experience as a window that did not receive the drop.
    ///
    /// An `.ipsw` is *built* here rather than parked in a third field waiting for a button. It
    /// takes about a second — inflate 13.9 MB, check its CRC-32, write an 8 GiB sparse file of
    /// which about 20 MB is real — and the only reason it was ever a separate step is that the
    /// screen had steps.
    fn accept(&mut self, path: &Path, into_dir: &Path) -> String {
        self.rejected = None;
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        match inspect::classify(path) {
            inspect::Kind::Rom => {
                self.flash = path.to_string_lossy().into_owned();
                self.revalidate();
                format!("boot ROM: {name}")
            }
            inspect::Kind::Disk => {
                self.disk = path.to_string_lossy().into_owned();
                self.revalidate();
                format!("drive: {name}")
            }
            // An operating system or bootloader for the firmware partition. Identified by its own
            // checksum, so this is a verified claim rather than a size guess — which matters,
            // because before this arm existed `rockbox.ipod` was 7.5 MB and fell through to the
            // size test, and the window called an operating system a drive.
            inspect::Kind::Os => {
                let (model, len) = inspect::os_checksum(path)
                    .unwrap_or_else(|| ("????".into(), 0));
                self.pending_os = Some(path.to_string_lossy().into_owned());
                if self.disk.is_empty() {
                    format!(
                        "{name}: an OS image for the firmware partition — `{model}`, {len} bytes, \
                         checksum OK. Drop a drive too and it can be installed onto a copy of it."
                    )
                } else {
                    format!("{name}: `{model}`, {len} bytes, checksum OK — ready to install")
                }
            }
            // Files for the data volume rather than the firmware partition — a Rockbox release is
            // the case this exists for, and it is a zip like an `.ipsw`, so "is a zip" was never
            // the answer on its own.
            inspect::Kind::OsBundle => {
                self.pending_bundle = Some(path.to_string_lossy().into_owned());
                format!("{name}: files for the volume — ready to copy onto a drive")
            }
            inspect::Kind::Ipsw => {
                self.ipsw = path.to_string_lossy().into_owned();
                self.revalidate_ipsw();
                if !matches!(&self.ipsw_verdict, Some(v) if v.ok()) {
                    let why = self
                        .ipsw_verdict
                        .as_ref()
                        .map(|v| first_line(v.text()))
                        .unwrap_or_else(|| "could not be read".into());
                    return format!("{name}: {why}");
                }
                // Named for the bundle it comes from, so a second build cannot land on the first.
                let out = match inspect::ipsw_identity(path) {
                    Some((v, crc)) => into_dir.join(inspect::built_drive_name(&v, crc)),
                    // Unreachable in practice — the verdict above already parsed this archive — but
                    // a fallback that overwrites is exactly the bug being fixed, so this one is
                    // distinct per source instead.
                    None => into_dir.join(format!("ipod-from-{name}.img")),
                };
                // The same bundle resolves to the same path, so a rebuild is work already done.
                // Anything else here would either redo eight gigabytes or overwrite a drive an
                // iPod is using.
                if out.is_file() {
                    self.disk = out.to_string_lossy().into_owned();
                    self.revalidate();
                    self.built = Some(Ok(format!("{} was already built", out.display())));
                    return format!("{name}: that drive is already built");
                }
                let built = inspect::build_from_ipsw(path, &out);
                let line = match &built {
                    Ok(what) => {
                        self.disk = out.to_string_lossy().into_owned();
                        self.revalidate();
                        format!("built a drive from {name} — {}", first_line(what))
                    }
                    Err(e) => format!("{name}: {e}"),
                };
                self.built = Some(built);
                line
            }
            inspect::Kind::Unknown => {
                let size = std::fs::metadata(path).map(|m| human(m.len())).unwrap_or_default();
                let line = format!(
                    "{name} is not a boot ROM, an iPod software update or a drive image ({size})."
                );
                self.rejected = Some(line.clone());
                line
            }
        }
    }
}

impl App {
    /// Built from a bare [`egui::Context`], not from an [`eframe::CreationContext`].
    ///
    /// The two are the same thing here — everything below wanted `cc.egui_ctx` and nothing else —
    /// and taking the smaller one is what lets the screens be laid out with no window at all. That
    /// is what `every_screen_fits_the_smallest_window` needs, and a rule about the size of the UI
    /// that could not be checked without a person looking at it is a rule that would rot.
    fn new(
        ctx: &egui::Context,
        cfg: emu::Config,
        settings: Settings,
        ipsw: String,
    ) -> Self {
        theme(ctx);
        let tex = ctx.load_texture(
            "panel",
            egui::ColorImage::from_rgb([FB_W, FB_H], &vec![0u8; FB_W * FB_H * 3]),
            egui::TextureOptions::NEAREST,
        );
        let shot_dir = settings::repo_root().join("_out");
        let mut images = Images::new(&cfg);
        // `--ipsw=` used to fill a field and wait for a Build button that no longer exists — the
        // build happens when a bundle arrives, whichever way it arrives. So the flag goes through
        // the same door a dropped file does, and means what its name says.
        if !ipsw.is_empty() {
            images.accept(Path::new(&ipsw), &drives_dir());
        }
        let update_slot = Arc::new(Mutex::new(None));
        // Opt-in, and only opt-in. Off by default; the button in the panel works regardless.
        if settings.check_updates_on_start {
            update::spawn(Arc::clone(&update_slot));
        }
        let mut app = App {
            link: None,
            worker: None,
            cfg,
            screen: Screen::FirstRun,
            images,
            cold_at_open: None,
            back_to: Screen::FirstRun,
            details_row: 0,
            settings,
            tex,
            seen_seq: 0,
            seen_backlight: 16,
            drag: None,
            hold: false,
            down: Vec::new(),
            kbd_touch_until: None,
            touching: false,
            show_back_buffer: false,
            log: VecDeque::new(),
            shot_dir,
            last_shot: None,
            scale: 1,
            hold_slot: Rect::NOTHING,
            dev_area: Rect::NOTHING,
            notice: None,
            update_slot,
            update_line: None,
            update_asked: false,
            wheel_units: 0.0,
            stale_cache: (0, Vec::new()),
        };
        // Nothing to set up: the images are there and they parse. Skip straight to the iPod.
        if app.images.both_good() {
            app.start();
        }
        app
    }

    /// Build the machine and start it. Idempotent — a second press does nothing.
    fn start(&mut self) {
        if self.link.is_some() {
            return;
        }
        self.cfg.flash = PathBuf::from(self.images.flash.trim());
        self.cfg.disk = PathBuf::from(self.images.disk.trim());
        // The cache key includes both paths, so a different pair of images gets a different
        // snapshot rather than restoring one taken on the other machine.
        let cache = cache_paths(&self.cfg.flash, &self.cfg.disk, self.cfg.clock, self.cfg.snap_at);
        // Measured, not deleted. Anything cached for a different pair of images -- or for an
        // older build of this emulator, which is the common case -- is reported and left alone.
        let (stale, paths) = reclaimable(&settings::data_dir(), &cache, self.cfg.work_on_copy);
        self.stale_cache = (stale, paths);
        if stale > 0 {
            self.say(format!(
                "{} in the cache is from earlier builds or other images — reclaim it in Setup",
                human(stale)
            ));
        }
        self.cfg.snapshot = Some(cache.snap);
        // Direct by default: the machine runs on the image the user chose, and the iPod's writes
        // land in it. In copy mode the working drive and its frozen twin live in the cache instead,
        // at 8 GB apiece on any filesystem without reflinks.
        //
        // `frozen` keeps its cache path either way. Pointing it at the user's own image in direct
        // mode would name a file this mode never writes and never reads, and name it as the thing
        // `build` clones *from* — a value that is inert only for as long as nobody adds a caller.
        self.cfg.frozen = cache.frozen;
        self.cfg.workdisk =
            if self.cfg.work_on_copy { cache.work } else { self.cfg.disk.clone() };
        self.notice = self.inspect_drive();

        // Remember what worked, so the next launch opens straight into the iPod.
        self.adopt_chassis_from_nor(&self.cfg.flash.clone());
        self.settings.flash = Some(self.cfg.flash.clone());
        self.settings.disk = Some(self.cfg.disk.clone());
        self.settings.save();

        let link = Link::new();
        // Opt-in: without --control there is no socket and nothing can drive this but a person.
        if let Some(path) = &self.cfg.control {
            if let Err(e) = control::serve(path, Arc::clone(&link)) {
                self.say(format!("control socket: {e}"));
            }
        }
        // The handle is kept, not dropped: closing the window has to *wait* for this thread now,
        // because the restore point is written on its way out. See `App::on_exit`.
        self.worker = Some(spawn_worker(self.cfg.clone(), Arc::clone(&link)));
        self.link = Some(link);
        // There is a machine now, so there is an iPod to look at. The screen is set here rather
        // than by each caller: `start` is reached from the first run, from a restart in the
        // settings, and from launch with images already saved, and all three want the same thing.
        self.screen = Screen::Device;
    }

    /// End the machine, parking it first, and wait for the thread to finish doing so.
    ///
    /// **The wait is the point.** The restore point is written by the emulator thread on its way
    /// out — it is the only thread that owns the machine — so a caller that signals and walks away
    /// gets a half-written snapshot or none at all, depending on how quickly the process ends after
    /// it. Two seconds is generous for a 1.6 GB write to a file that is already open, and the
    /// timeout exists so that a thread wedged somewhere unexpected cannot hold the window open for
    /// ever: a lost restore point costs a cold boot, and a window that will not close costs trust.
    fn stop_machine(&mut self) {
        if let Some(l) = &self.link {
            l.save_on_quit.store(true, Ordering::Relaxed);
            l.quit.store(true, Ordering::Relaxed);
        }
        if let Some(w) = self.worker.take() {
            let deadline = Instant::now() + Duration::from_secs(2);
            while !w.is_finished() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(20));
            }
            if w.is_finished() {
                let _ = w.join();
            }
        }
        self.link = None;
    }

    /// Read the drive before booting it, and say what is wrong in our words rather than leaving
    /// RetailOS to say it in a picture.
    ///
    /// **Only faults this can actually determine are reported.** Two of the three known causes of
    /// the plug-into-a-computer screen are readable straight out of the firmware partition; the
    /// third — a `SysCfg FwId` family that disagrees with the drive's firmware — is not, because
    /// nothing here parses the NOR's `SysCfg` yet. Reporting a guess would be worse than reporting
    /// nothing, so an absent notice means "nothing found", never "nothing wrong".
    ///
    /// Everything read goes to the log whether or not it is a fault, because the question this
    /// answers most often is "what is on the drive I just pointed it at".
    fn inspect_drive(&mut self) -> Option<String> {
        let state = match ipsw::firmware_state(&self.cfg.disk) {
            Ok(s) => s,
            Err(e) => {
                self.say(format!("drive: {e}"));
                return Some("This file does not look like an iPod drive.".into());
            }
        };
        self.say(format!(
            "drive: firmware images [{}]{}",
            state.tags.join(", "),
            if state.aupd_armed { ", updater armed" } else { "" }
        ));
        if !state.has_os {
            return Some("There is no operating system on this drive.".into());
        }
        if state.aupd_armed {
            // Real hardware runs the updater, reboots itself, and runs the OS on the second boot.
            // Nothing here power-cycles the machine, so this drive stops at the updater.
            return Some("The flash updater is armed, so this drive boots the updater instead of the OS.".into());
        }
        None
    }

    fn say(&mut self, s: impl Into<String>) {
        self.log.push_front(s.into());
        self.log.truncate(8);
    }

    fn push(&self, ev: WheelEvent) {
        if let Some(l) = &self.link {
            l.push(ev);
        }
    }

    fn press(&mut self, b: Button) {
        if self.down.contains(&b) {
            return;
        }
        self.down.push(b);
        self.push(WheelEvent::Button(b.mask(), true));
        self.say(format!("{} pressed", b.label()));
    }

    fn release(&mut self, b: Button) {
        if let Some(i) = self.down.iter().position(|x| *x == b) {
            self.down.remove(i);
            self.push(WheelEvent::Button(b.mask(), false));
        }
    }

    fn touch(&mut self) {
        if !self.touching {
            self.touching = true;
            self.push(WheelEvent::Touch);
        }
    }

    fn untouch(&mut self) {
        if self.touching {
            self.touching = false;
            self.push(WheelEvent::Release);
        }
    }

    /// Deliver `d` clicks, one `Step` each, in order. The model advances its own position by
    /// exactly these, which is why the UI never writes `position` itself.
    fn rotate(&mut self, d: i32) {
        let step = if d > 0 { 1i8 } else { -1 };
        for _ in 0..d.abs() {
            self.push(WheelEvent::Step(step));
        }
    }

    fn set_hold(&mut self, on: bool) {
        if self.hold != on {
            self.hold = on;
            self.push(WheelEvent::Hold(on));
            // Said in as many words, because the switch moving on screen is otherwise an implicit
            // claim that something happened. Nothing in RetailOS has been measured to act on it:
            // the panel is byte-identical with the switch thrown and without, and RetailOS never
            // reads the GPIOA line our model drives. See the README's "What is not here".
            self.say(if on {
                "hold ENGAGED — the wheel reports it; RetailOS is not measured to act on it"
            } else {
                "hold released"
            });
        }
    }

    fn set_mode(&mut self, m: Mode) {
        if self.settings.mode != m {
            self.settings.mode = m;
            self.settings.save();
        }
    }

    fn screenshot(&mut self, rgb: &[u8], addr: u32) {
        let _ = std::fs::create_dir_all(&self.shot_dir);
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let stem = format!("ipod-{stamp}-{addr:06x}");
        let p = self.shot_dir.join(format!("{stem}.png"));
        let q = self.shot_dir.join(format!("{stem}.ppm"));
        let ok = std::fs::write(&p, png::encode(rgb, FB_W, FB_H)).is_ok()
            && std::fs::write(&q, png::encode_ppm(rgb, FB_W, FB_H)).is_ok();
        self.last_shot = Some(if ok {
            format!("{}", p.display())
        } else {
            format!("could not write {}", p.display())
        });
        let msg = self.last_shot.clone().unwrap_or_default();
        self.say(msg);
    }

    /// Collect a finished update check. Silent on failure, by contract — see `update.rs`.
    fn poll_update(&mut self) {
        let taken = self.update_slot.lock().unwrap().take();
        if let Some(found) = taken {
            self.update_line = match found {
                Some(update::Found::Newer { tag, url }) => {
                    Some(format!("A newer release is out: {tag} — {url}"))
                }
                // A manual check says so; a launch check that found nothing says nothing.
                Some(f) if self.update_asked => Some(f.line()),
                _ => None,
            };
            self.update_asked = false;
        }
    }
}

/// Draw a device at rest — case, wheel and a dark panel — in whatever room it is given.
///
/// The running window's `device()` derives its size from the framebuffer, because there the panel
/// must land on exact pixel boundaries. Nothing is running here, so this one simply fits the height
/// it is offered. It exists so the first-run screen can show the thing being set up.
fn device_at_rest(
    p: &egui::Painter,
    d: &Device,
    centre: Pos2,
    height: f32,
    chassis: Colour,
) -> Rect {
    let k = height / (d.case_h + SWITCH_PROUD);
    let (w, h) = (d.case_w * k, (d.case_h + SWITCH_PROUD) * k);
    let o = Pos2::new(centre.x - w / 2.0, centre.y - h / 2.0);
    let at = |x: f32, y: f32| Pos2::new(o.x + x * k, o.y + (y + SWITCH_PROUD) * k);

    let (body, wheel, ink, _glass) = palette_for(chassis);

    // The hold switch first, so the case's rounded corner covers the half that is inside it.
    let sw = Rect::from_min_max(
        Pos2::new(o.x + SWITCH_X * k, o.y),
        Pos2::new(o.x + (SWITCH_X + SWITCH_W) * k, o.y + SWITCH_PROUD * 2.0 * k),
    );
    // Two tones, because the switch is a slider: the pale half is the travel it sits in.
    p.rect_filled(sw, CornerRadius::from((1.0 * k) as u8), Color32::from_gray(0x9C));
    let half = Rect::from_min_max(sw.min, Pos2::new(sw.min.x + sw.width() * 0.5, sw.max.y));
    p.rect_filled(half, CornerRadius::from((1.0 * k) as u8), Color32::from_gray(0xD8));

    let face = Rect::from_min_max(at(0.0, 0.0), at(d.case_w, d.case_h));
    p.rect_filled(face, CornerRadius::from((5.5 * k) as u8), body);

    // The panel, dark: there is no machine yet, and a blank white rectangle reads as a fault.
    let sx = o.x + (d.case_w - d.screen_w) / 2.0 * k;
    let sy = o.y + (SWITCH_PROUD + d.screen_top) * k;
    let glass = Rect::from_min_size(Pos2::new(sx, sy), Vec2::new(d.screen_w * k, d.screen_h * k));
    p.rect_filled(glass.expand(1.2 * k), CornerRadius::from((0.8 * k) as u8), Color32::from_gray(0x24));
    p.rect_filled(glass, CornerRadius::ZERO, Color32::from_gray(0x0C));

    let c = Pos2::new(o.x + d.case_w / 2.0 * k, o.y + (SWITCH_PROUD + d.wheel_cy) * k);
    let r = d.wheel_d / 2.0 * k;
    p.circle_filled(c, r, wheel);
    p.circle_filled(c, r * 0.34, body);

    // The four labels. Without them a light circle on a light case reads as a blemish rather than
    // as the control the whole device is known for.
    //
    // The transport marks are DRAWN, via the same `transport` the live device uses, not typed. An
    // earlier version set them as text and they came out as empty boxes: the default font has no
    // glyph for U+25C2, and a missing glyph is a rectangle rather than nothing, so the wheel
    // acquired three small squares.
    p.text(
        Pos2::new(c.x, c.y - r * 0.64),
        egui::Align2::CENTER_CENTER,
        "MENU",
        egui::FontId::proportional((r * 0.26).max(5.0)),
        ink,
    );
    let g = (r * 0.30).max(4.0);
    transport(p, Button::Prev, Pos2::new(c.x - r * 0.64, c.y), g, ink);
    transport(p, Button::Next, Pos2::new(c.x + r * 0.64, c.y), g, ink);
    transport(p, Button::Play, Pos2::new(c.x, c.y + r * 0.64), g, ink);
    face
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.poll_update();
        // Anywhere on the window, on every screen. A drop target that is a rectangle inside the
        // window is a rectangle somebody has to find; the window itself is the thing they are
        // already aiming at. Read once here, because `dropped_files` is reported to every widget
        // that asks and an earlier version delivered one dropped file into every slot at once.
        let dropped: Vec<PathBuf> = ui
            .ctx()
            .input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect());
        if !dropped.is_empty() {
            let before = self.cold();
            for p in dropped {
                let line = self.images.accept(&p, &drives_dir());
                self.say(line);
            }
            // A file dropped on a *running* iPod cannot be applied to it, and applying it silently
            // at the next launch would be a change nobody agreed to. So it opens the settings on
            // the row it landed in, with the restart banner already showing what it did — the
            // state the drop actually put the program in, rather than a log line under the device.
            if self.screen == Screen::Device {
                self.open_settings();
                self.cold_at_open = Some(before);
            }
        }

        match self.screen {
            Screen::FirstRun => return self.first_run(ui),
            Screen::Settings => return self.settings_screen(ui),
            Screen::Help => return self.help_screen(ui),
            Screen::Details => return self.details_screen(ui),
            Screen::Device => {}
        }
        let ctx = ui.ctx().clone();
        // The machine is running on another thread whether or not anything moved, so the window has
        // to keep asking for repaints or it would freeze on a still screen.
        ctx.request_repaint_after(Duration::from_millis(16));

        let out = self.link.as_ref().unwrap().out.lock().unwrap().clone();
        if out.fb_seq != self.seen_seq || out.backlight != self.seen_backlight {
            self.seen_seq = out.fb_seq;
            self.seen_backlight = out.backlight;
            // The dimmer is a property of the LAMP, not of the pixels: the LCD holds the same
            // bytes at every brightness and the backlight decides how much of it you see. So the
            // scaling happens here, on the way to the screen, and never touches `out.fb` -- which
            // is what `shot` writes and what every measurement in research/ counts.
            //
            // The 1..32 level is measured. The curve from it to a screen is not: a panel at its
            // minimum is dim, not off, so level 1 lands at 12 % rather than at 3 %. That is a
            // rendering choice and is the only invented number here.
            let lit = if out.backlight >= 32 {
                None
            } else {
                Some(0.12 + 0.88 * (out.backlight.max(1) - 1) as f32 / 31.0)
            };
            let img = match lit {
                None => egui::ColorImage::from_rgb([FB_W, FB_H], &out.fb),
                Some(f) => {
                    let dimmed: Vec<u8> =
                        out.fb.iter().map(|v| (*v as f32 * f).round() as u8).collect();
                    egui::ColorImage::from_rgb([FB_W, FB_H], &dimmed)
                }
            };
            self.tex.set(img, egui::TextureOptions::NEAREST);
        }

        self.keyboard(&ctx);

        self.footer(ui, &out);
        self.device_controls(ui, &out);

        let rect = ui.available_rect_before_wrap();
        self.device(ui, rect, &out);
        // Over the device, not beside it. See `readout`.
        if self.settings.mode == Mode::Debug {
            self.readout(ui, rect, &out);
        }
    }

    /// Closing the window parks the machine rather than dropping it.
    ///
    /// This is where "your iPod remembers" is actually kept. Working directly on the drive, the
    /// instant the window closes is the only one at which RAM and that drive provably agree —
    /// nothing runs after it, so nothing can write after it. Both halves go down together and the
    /// next launch resumes in seconds instead of cold-booting for seventy-five.
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.stop_machine();
    }
}

impl App {
    // ------------------------------------------------------------ the two screens that are prose

    /// A centred column with margins, at a width prose can be read at.
    ///
    /// Every screen that is words rather than an iPod is this shape, and that is the whole of "it
    /// feels like one window": the column is the same width, starts at the same place and ends at
    /// the same place whichever screen you are on, so moving between them moves the content and
    /// nothing else. A 1400-pixel line of prose is unreadable at any contrast, which is why the
    /// column is capped rather than filling whatever the window happens to be.
    ///
    /// **There is no scroll area.** Scrolling belongs to the click wheel. Any screen that does not
    /// fit the smallest window the program will open is a bug, and
    /// `every_screen_fits_the_smallest_window` fails on it — which is a different and much better
    /// outcome than a scrollbar quietly appearing on somebody's laptop.
    fn column(&mut self, ui: &mut egui::Ui, f: impl FnOnce(&mut Self, &mut egui::Ui)) {
        let avail = ui.available_width();
        let col = avail.min(COLUMN_W);
        let side = ((avail - col) / 2.0).max(0.0);
        ui.add_space(PAGE_MARGIN);
        ui.horizontal(|ui| {
            ui.add_space(side);
            ui.vertical(|ui| {
                ui.set_width(col);
                f(self, ui);
            });
        });
    }

    // ------------------------------------------------------------ the first run

    /// What a stranger meets: one screen, two files, and no way to leave without them.
    ///
    /// **There are no steps and no slots.** A dropped file says what it is ([`inspect::classify`]),
    /// so the only instruction is "give me your two files" and the window does the sorting. The
    /// previous version asked for them one at a time, in an order the user had no reason to know,
    /// with a text field for a path nobody types, and then hid the program's settings on its third
    /// page — where a running machine could never reach them.
    ///
    /// Everything this project knows about where the files come from is still here, behind one
    /// disclosure. It is reference material: right when you are stuck, noise when you are not.
    fn first_run(&mut self, ui: &mut egui::Ui) {
        self.column(ui, |app, ui| {
            let dev = IPOD_VIDEO;
            let (rect, _) =
                ui.allocate_exact_size(Vec2::new(ui.available_width(), 130.0), egui::Sense::hover());
            device_at_rest(ui.painter(), &dev, rect.center(), 120.0, app.settings.chassis);
            ui.add_space(10.0);

            ui.vertical_centered(|ui| {
                ui.label(egui::RichText::new("ipod-emulator").heading());
                ui.label(
                    egui::RichText::new("Apple's own iPod software, on a machine that is not one.")
                        .color(Color32::from_gray(0x9A)),
                );
            });
            ui.add_space(20.0);

            ui.vertical_centered(|ui| {
                ui.label("Drop your iPod's boot ROM and its software anywhere on this window.");
                ui.add_space(2.0);
                ui.label(
                    egui::RichText::new(
                        "The software can be Apple's .ipsw, or a drive image you already have.",
                    )
                    .small()
                    .color(Color32::from_gray(0x9A)),
                );
                ui.add_space(8.0);
                if ui.button("  Choose…  ").clicked() {
                    app.choose_files();
                }
            });
            ui.add_space(18.0);

            app.file_rows(ui);
            ui.add_space(16.0);

            ui.vertical_centered(|ui| {
                if ui.link("What are these, and where do I get them?").clicked() {
                    app.open_help();
                }
            });

            ui.add_space(22.0);
            ui.separator();
            ui.add_space(10.0);
            ui.vertical_centered(|ui| {
                let ready = app.images.both_good();
                if ui.add_enabled(ready, egui::Button::new("      Start      ")).clicked() {
                    app.start();
                }
                if !ready {
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new("both files are needed")
                            .small()
                            .color(Color32::from_gray(0x78)),
                    );
                }
            });
        });
    }

    /// The two rows, and what the window makes of what is in them.
    ///
    /// A filename and a mark, not a path and a paragraph. The verdict's own words appear when it is
    /// *not* good, because that is when they are the answer to a question rather than reassurance
    /// nobody asked for — and those sentences are the ones that save an evening: an `iPod_24`
    /// bundle against a family 20 ROM, a 2 MiB dump that is somebody else's iPod, an Apple
    /// Partition Map where an MBR was expected.
    fn file_rows(&mut self, ui: &mut egui::Ui) {
        let rows: [(&str, &str, &[&str]); 2] = [
            ("Boot ROM", "flash", &["bin"]),
            ("Software", "disk", &["img", "bin", "dmg", "iso", "ipsw", "zip"]),
        ];
        let mut pick: Option<(&str, &[&str])> = None;
        let mut show_details: Option<usize> = None;
        ui.group(|ui| {
            ui.set_width(ui.available_width());
            for (i, (title, which, exts)) in rows.iter().enumerate() {
                if i > 0 {
                    ui.add_space(6.0);
                    ui.separator();
                    ui.add_space(6.0);
                }
                let (path, verdict, described) = match *which {
                    "flash" => (
                        &self.images.flash,
                        self.images.flash_verdict.as_ref(),
                        self.images.flash_name.as_ref(),
                    ),
                    _ => (
                        &self.images.disk,
                        self.images.disk_verdict.as_ref(),
                        self.images.disk_name.as_ref(),
                    ),
                };
                // What it is, if we can say; otherwise what it is called. A user's own drive image
                // gets its filename, which is their word for it and the right thing to show.
                let name = described.cloned().unwrap_or_else(|| {
                    Path::new(path.trim())
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default()
                });
                let (mark, colour) = match verdict {
                    Some(v) if v.ok() => ("✓", Color32::from_rgb(0x6C, 0xC6, 0x88)),
                    Some(_) => ("!", Color32::from_rgb(0xE0, 0xA0, 0x40)),
                    None => ("○", Color32::from_gray(0x70)),
                };
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(mark).color(colour).monospace());
                    ui.label(egui::RichText::new(*title).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if !name.is_empty() && ui.small_button("replace").clicked() {
                            pick = Some((*title, *exts));
                        }
                        if name.is_empty() {
                            ui.label(
                                egui::RichText::new("waiting")
                                    .small()
                                    .color(Color32::from_gray(0x70)),
                            );
                        } else {
                            ui.label(egui::RichText::new(name).small())
                                .on_hover_text(path.as_str());
                        }
                    });
                });
                // Only when it is not simply fine. A green tick with a sentence under it is a
                // sentence nobody reads, and it is in the way of the ones that matter.
                if let Some(v) = verdict {
                    if !v.ok() {
                        ui.label(
                            egui::RichText::new(v.text())
                                .small()
                                .color(Color32::from_rgb(0xE0, 0xA0, 0x40)),
                        );
                    }
                }
                // What was found in it. **Folded, and folded by default**: on the day it matters —
                // three dumps, or a pair that will not boot — it is the whole answer, and on every
                // other day it is eight lines of furniture above the button somebody came here to
                // press. The parse already happened either way.
                let facts = match i {
                    0 => &self.images.flash_facts,
                    _ => &self.images.disk_facts,
                };
                //
                // **A page, not a fold.** Measured: opening both rows' facts inline, on a page
                // that also had two rejection verdicts and the pair warning, came to 914 px
                // against a 680 px window — 234 over, and only because somebody clicked. A
                // disclosure that can overflow the window is a scrollbar waiting for a Tuesday, so
                // the facts get somewhere with room, the same way the help did.
                if !facts.is_empty() && ui.link(egui::RichText::new("what's in it").small()).clicked()
                {
                    show_details = Some(i);
                }
            }
        });

        // **A fault of the pair, so it sits under the pair.** Neither file's own verdict can see
        // this one: each is a perfectly good file, and they are for different iPods. Loud, because
        // the alternative is finding out from a picture of a cable seventy-five seconds from now.
        if let Some(m) = self.images.mismatch.clone() {
            ui.add_space(8.0);
            egui::Frame::NONE
                .fill(Color32::from_rgb(0x3A, 0x2A, 0x10))
                .inner_margin(8.0)
                .corner_radius(4.0)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            egui::RichText::new("These are not the same iPod —")
                                .strong()
                                .color(Color32::from_rgb(0xE0, 0xA0, 0x40)),
                        );
                        ui.label(egui::RichText::new(m).color(Color32::from_gray(0xC8)));
                    })
                    .response
                    .on_hover_text(inspect::WHY_FAMILY_MATTERS);
                });
        }
        if let Some((title, exts)) = pick {
            self.take(&pick_files(title, exts));
        }
        if let Some(row) = show_details {
            self.open_details(row);
        }
        if let Some(r) = self.images.rejected.clone() {
            ui.add_space(6.0);
            ui.label(egui::RichText::new(r).small().color(Color32::from_rgb(0xD0, 0x6C, 0x6C)));
        }
        if let Some(Err(e)) = self.images.built.clone() {
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new(format!("Could not build a drive from it: {e}"))
                    .small()
                    .color(Color32::from_rgb(0xD0, 0x6C, 0x6C)),
            );
        }
    }

    /// Take however many files arrived — dropped, or chosen from a dialog — and route each one.
    ///
    /// The one door. Both ways of handing the window a file end up here, so there is no second
    /// place where "what is this file" could be answered differently.
    fn take(&mut self, paths: &[String]) {
        for p in paths {
            let line =
                self.images.accept(Path::new(p), &drives_dir());
            self.say(line);
        }
    }

    /// Everything the project knows about obtaining the two files.
    ///
    /// A page, not a fold. These paragraphs are why several people got a working emulator instead
    /// of a `Bootloader could not execute target image!` an hour in — the Video's ROM being filed
    /// under the Classic's model number is not something anyone would guess — and somebody reading
    /// them is going to be switching to a browser and back. A disclosure that pushes the rest of
    /// the screen down while they do that is the wrong shape for reading; a page they leave when
    /// they are done is the right one.
    ///
    /// It returns to whichever screen opened it, which is why [`App::help_from`] exists rather than
    /// a rule about where help "goes back to".
    fn help_screen(&mut self, ui: &mut egui::Ui) {
        if ui.ctx().input(|i| i.key_pressed(egui::Key::Escape)) {
            self.screen = self.back_to;
            return;
        }
        self.column(ui, |app, ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("What this needs").heading());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("  Back  ").clicked() {
                        app.screen = app.back_to;
                    }
                });
            });
            ui.add_space(6.0);
            // **Two things, and only one of them is a particular file.** The second is a role with
            // three ways to fill it, and saying "two files" made one of the three sound like the
            // only one — which is how a drive image somebody already had stopped looking allowed.
            //
            // The ownership line is worded carefully and deliberately. Apple wrote this software
            // and still holds the copyright in it; owning an iPod is owning the device and the copy
            // of the firmware inside it, which is not the same thing and does not become it. What
            // is flatly true, and is what a person here needs to know, is that this project ships
            // neither and that the place to get both is the iPod on your desk. Anything past that
            // is a question about your jurisdiction, and this window is not the place it gets
            // answered.
            ui.label(
                egui::RichText::new(
                    "Two things: the boot ROM, and something to make a drive from. Apple wrote \
                     both, this project ships neither, and an iPod you own has both on it.",
                )
                .color(Color32::from_gray(0x9A)),
            );
            ui.add_space(18.0);
            app.where_from(ui);
        });
    }

    /// Open the help, remembering where from.
    fn open_help(&mut self) {
        self.back_to = self.screen;
        self.screen = Screen::Help;
    }

    /// Open the details of one of the two chosen files, remembering where from.
    fn open_details(&mut self, row: usize) {
        self.back_to = self.screen;
        self.details_row = row;
        self.screen = Screen::Details;
    }

    /// Everything found inside one file, on a page with room for it.
    ///
    /// **The parse already happened.** Every one of these lines was read to decide whether the file
    /// could be used at all, and until now all of it was discarded unless the answer was no. It is
    /// the whole answer on the two days it matters — telling three dumps apart, and working out why
    /// a pair will not boot — and it is furniture on every other day, which is what a page you
    /// choose to open is for.
    fn details_screen(&mut self, ui: &mut egui::Ui) {
        if ui.ctx().input(|i| i.key_pressed(egui::Key::Escape)) {
            self.screen = self.back_to;
            return;
        }
        let row = self.details_row;
        let (heading, path, facts) = if row == 0 {
            ("Boot ROM", self.images.flash.clone(), self.images.flash_facts.clone())
        } else {
            ("Software", self.images.disk.clone(), self.images.disk_facts.clone())
        };
        let described = if row == 0 { self.images.flash_name.clone() } else { self.images.disk_name.clone() };
        self.column(ui, |app, ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(heading).heading());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("  Back  ").clicked() {
                        app.screen = app.back_to;
                    }
                });
            });
            if let Some(d) = described {
                ui.label(egui::RichText::new(d).color(Color32::from_gray(0x9A)));
            }
            ui.add_space(14.0);
            for (k, v) in &facts {
                ui.horizontal_wrapped(|ui| {
                    ui.label(egui::RichText::new(*k).color(Color32::from_gray(0x88)));
                    ui.label(egui::RichText::new(v).monospace());
                });
                ui.add_space(2.0);
            }
            ui.add_space(14.0);
            ui.separator();
            ui.add_space(6.0);
            // Last, small, and selectable: the path is the least interesting true thing about a
            // file and the one people occasionally need to copy.
            ui.label(
                egui::RichText::new("Where it is").small().color(Color32::from_gray(0x88)),
            );
            ui.label(egui::RichText::new(path).small().monospace());
        });
    }

    /// The prose itself, so both the help page and anything later that wants it read one copy.
    fn where_from(&mut self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new("The boot ROM").strong());
        ui.add_space(2.0);
        ui.label(
            "A 1 MB dump of the chip your iPod starts from. Rockbox can read it off your own \
             device in about five minutes and be uninstalled straight afterwards: install with \
             Rockbox Utility (only \"bootloader\" and \"rockbox\"), then on the iPod go to",
        );
        ui.label(
            egui::RichText::new("System \u{2192} Debug (Keep Out!) \u{2192} Dump ROM contents")
                .monospace(),
        );
        ui.label("and copy the internal_rom_… file off when you plug it in.");
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.hyperlink_to("Rockbox Utility", "https://www.rockbox.org/wiki/RockboxUtility");
            ui.label("·");
            ui.hyperlink_to("the flash guide", "https://www.rockbox.org/wiki/IpodFlash.html");
        });
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new(
                "Archived dumps file the iPod VIDEO's ROM under the iPod CLASSIC, in a folder \
                 named A1238. That is the Classic's model number; the Video is A1136 — so \
                 searching for \"iPod Video\" or \"5.5G\" finds nothing and searching for the \
                 Classic finds it. The one you want reads HwVr 0x000b0005 and Mod# MA146; a \
                 prototype dump also circulates, reads 0x000b0011 and M8976, and will not boot \
                 here.",
            )
            .color(Color32::from_gray(0xB4)),
        );

        ui.add_space(12.0);
        ui.label(egui::RichText::new("The software").strong());
        ui.add_space(2.0);
        ui.label(
            "Apple's iPod software update — for the iPod Video, iPod_20.1.3.ipsw. Drop it in and \
             the emulator builds a drive from it. A whole-drive image of an iPod you already have \
             works too.",
        );
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new(
                "The 20 is the updater family and it must match the iPod your ROM came from: \
                 iPod_24 and iPod_26 files are other devices and will not boot here. Apple no \
                 longer serves these, so there is no official source to try. Neither file is \
                 distributed with this project.",
            )
            .color(Color32::from_gray(0xB4)),
        );
    }

    /// Ask the platform for files, and take however many come back.
    ///
    /// **Plural, and it has to be**: the window wants two, it sorts them itself, and a dialog that
    /// takes one selection makes somebody open it twice for a job that is one job. Select both and
    /// press Open.
    fn choose_files(&mut self) {
        let exts = ["bin", "ipsw", "zip", "img", "dmg", "iso"];
        let picked = pick_files("iPod boot ROM, software update, or drive image", &exts);
        self.take(&picked);
    }

    // ------------------------------------------------------------ settings

    /// Open the settings without touching the machine.
    ///
    /// The whole point of the screen split: this used to end the worker, because the settings
    /// screen *was* the no-machine screen. The iPod keeps running behind it, and only a change
    /// that cannot be applied to a running machine costs anything — see [`Cold`].
    fn open_settings(&mut self) {
        self.images.revalidate();
        self.images.revalidate_ipsw();
        self.cold_at_open = Some(self.cold());
        self.screen = Screen::Settings;
    }

    /// The restart-needing settings, as they stand right now.
    fn cold(&self) -> Cold {
        Cold {
            flash: self.images.flash.trim().to_string(),
            disk: self.images.disk.trim().to_string(),
            work_on_copy: self.cfg.work_on_copy,
        }
    }

    /// Everything about the program, on one page, in four sections.
    ///
    /// **Done is always there.** It is disabled only while the images do not validate, which is the
    /// same predicate the first run uses — one rule, so there is no second one to drift out of step
    /// with it. Somebody who opened this to change the case colour can close it again immediately;
    /// somebody who broke their drive path cannot leave the emulator pointed at nothing.
    fn settings_screen(&mut self, ui: &mut egui::Ui) {
        // Esc is what people press, and it obeys the same gate the button does — a first run cannot
        // be escaped, and a settings visit that broke nothing costs one key.
        if ui.ctx().input(|i| i.key_pressed(egui::Key::Escape)) && self.images.both_good() {
            self.close_settings();
            return;
        }
        self.column(ui, |app, ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Settings").heading());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let ready = app.images.both_good();
                    if ui.add_enabled(ready, egui::Button::new("  Done  ")).clicked() {
                        app.close_settings();
                    }
                    if !ready {
                        ui.label(
                            egui::RichText::new("both files are needed")
                                .small()
                                .color(Color32::from_gray(0x78)),
                        );
                    }
                });
            });
            ui.add_space(18.0);

            // No model name here. The boot ROM's row already says which iPod this is, and says it
            // with the serial of the one the dump came off — a heading repeating the model above it
            // is a line that costs height and tells you less.
            app.section(ui, "DEVICE");
            app.file_rows(ui);
            // Named before it happens, and only when it is true.
            if let Some(before) = &app.cold_at_open {
                let changed = before.differences(&app.cold());
                if !changed.is_empty() {
                    ui.add_space(8.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            egui::RichText::new(format!(
                                "⟳ {} changed. Restart to apply now, or Done to apply next launch.",
                                changed.join(" and ")
                            ))
                            .small()
                            .color(Color32::from_rgb(0xE0, 0xA0, 0x40)),
                        );
                    });
                    ui.add_space(4.0);
                    // Gated on the same predicate as Done. A restart into images that do not
                    // validate is a machine that stops during `build` and a window showing a
                    // stopped iPod nobody asked for.
                    if ui
                        .add_enabled(app.images.both_good(), egui::Button::new("Restart the iPod"))
                        .on_hover_text(
                            "A booted RetailOS read its partition table at startup and has been \
                             writing to that drive since, so there is no honest way to hand it \
                             another one. The machine is parked first, so nothing is lost that \
                             these files can get back.",
                        )
                        .clicked()
                    {
                        app.restart();
                    }
                }
            }

            ui.add_space(20.0);
            app.section(ui, "APPEARANCE");
            ui.horizontal(|ui| {
                ui.label("Case");
                // Three, not two: the U2 Special Edition is a black case with a RED wheel, and
                // Apple's own asset names for this family are `iPod6-White`, `iPod6-Black` and
                // `iPod6-BlackRed`. Offering only two was leaving out a device that shipped.
                let mut c = app.settings.chassis;
                let mut hit = false;
                for opt in [Colour::White, Colour::Black, Colour::U2] {
                    hit |= ui.selectable_value(&mut c, opt, opt.label()).clicked();
                }
                if hit {
                    app.settings.chassis = c;
                    app.settings.save();
                }
            });
            ui.add_space(4.0);
            let mut debug = app.settings.mode == Mode::Debug;
            if ui
                .checkbox(&mut debug, "Show the readout over the device")
                .on_hover_text("Instruction counts, the clocks, the wheel and the panel. `D` toggles it.")
                .changed()
            {
                app.set_mode(if debug { Mode::Debug } else { Mode::User });
            }

            ui.add_space(20.0);
            app.section(ui, "STORAGE");
            app.cache_controls(ui);
            ui.add_space(8.0);
            // **Whose file gets written to, said out loud.** Both modes remember across launches
            // -- copy mode re-freezes its working drive on the way out -- so the only difference
            // that was ever real is this one, and it is the one a person needs to see before the
            // machine starts rather than after.
            let target = write_target(Path::new(app.images.disk.trim()), app.settings.work_on_copy);
            ui.label(egui::RichText::new(target.line()).small().color(Color32::from_gray(0xC8)));
            ui.add_space(4.0);
            if target == WriteTo::ReadOnly {
                ui.label(
                    egui::RichText::new(
                        "There is no choice to make while it stays read-only: opening it for \
                         writing is refused by the operating system.",
                    )
                    .small()
                    .color(Color32::from_gray(0x9A)),
                );
            } else {
                let mut copy = target.copies();
                if ui
                    .checkbox(&mut copy, "Work on a copy, leaving my image untouched")
                    .on_hover_text(
                        "A copy costs a second drive -- up to 8 GB where the filesystem cannot \
                         share blocks, which is most of Linux and all of NTFS.\n\nUntouched \
                         until you set it, this follows where the drive came from: one built here \
                         from an .ipsw is written to directly, because building it again produces \
                         the same bytes; one you supplied is copied, because it might be the only \
                         image of an iPod you own.",
                    )
                    .changed()
                {
                    app.cfg.work_on_copy = copy;
                    app.settings.work_on_copy = Some(copy);
                    app.settings.save();
                }
            }

            ui.add_space(20.0);
            app.section(ui, "ABOUT");
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(format!("ipod-emulator {}", update::VERSION)).small());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("check for updates").clicked() {
                        app.update_asked = true;
                        app.update_line = Some("checking…".into());
                        update::spawn(Arc::clone(&app.update_slot));
                    }
                    let mut on = app.settings.check_updates_on_start;
                    if ui
                        .checkbox(&mut on, "on launch")
                        .on_hover_text(
                            "One HTTPS GET of GitHub's releases API and a version comparison. \
                             Nothing is downloaded, nothing is installed, nothing is run, and a \
                             check that fails says nothing at all.",
                        )
                        .changed()
                    {
                        app.settings.check_updates_on_start = on;
                        app.settings.save();
                    }
                });
            });
            if let Some(l) = app.update_line.clone() {
                ui.label(egui::RichText::new(l).small().color(Color32::from_gray(0x9A)));
            }
        });
    }

    /// A section heading: the one piece of chrome that makes a long page scannable.
    fn section(&mut self, ui: &mut egui::Ui, title: &str) {
        ui.label(
            egui::RichText::new(title).small().strong().color(Color32::from_gray(0x88)),
        );
        ui.add_space(2.0);
        ui.separator();
        ui.add_space(6.0);
    }

    /// Leave the settings. Nothing is applied here that was not applied as it was changed.
    /// Take the case colour from the NOR, when a *different* dump is chosen.
    ///
    /// `SysCfg`'s `Mod#` states which iPod the dump came from, and libgpod's table turns that into
    /// a colour — so the first answer comes from the hardware rather than from a constant. Our own
    /// reference dump is an `MA146`, a 30 GB black 5G, and it draws a white Apple logo on black.
    ///
    /// **Only on a change of dump.** Doing it on every launch would overwrite a deliberate choice
    /// in Appearance, and a setting that will not stay set is worse than no setting. A dump whose
    /// `Mod#` is absent or unknown changes nothing — silence is not an instruction to go white.
    fn adopt_chassis_from_nor(&mut self, nor: &std::path::Path) {
        if self.settings.flash.as_deref() == Some(nor) {
            return;
        }
        let Ok(bytes) = std::fs::read(nor) else { return };
        if let Some(m) = eapp_loader::inspect::syscfg(&bytes).and_then(|c| c.model_info()) {
            self.settings.chassis = m.colour;
        }
    }

    fn close_settings(&mut self) {
        self.cold_at_open = None;
        // The images may have been changed without a restart, in which case the machine on screen
        // is still the old pair's and the new one takes effect next launch. What is remembered is
        // what was *chosen*, because that is what the next launch will open.
        self.adopt_chassis_from_nor(&PathBuf::from(self.images.flash.trim()));
        self.settings.flash = Some(PathBuf::from(self.images.flash.trim()));
        self.settings.disk = Some(PathBuf::from(self.images.disk.trim()));
        self.settings.save();
        self.screen = if self.link.is_some() { Screen::Device } else { Screen::FirstRun };
    }

    /// Park the machine and build a new one from what the settings now say.
    ///
    /// Ends on the iPod, not back in the settings: somebody who pressed this wants to see the
    /// machine they just restarted. `start` sets the screen, and `close_settings` writes the chosen
    /// paths down first so that what boots and what is remembered cannot disagree.
    fn restart(&mut self) {
        self.close_settings();
        self.stop_machine();
        self.start();
    }

    // ------------------------------------------------------------ the footer, in both modes

    /// The one line that is in user mode and debug mode alike.
    ///
    /// The speed ratio lives here because hiding it would teach somebody the wrong thing about the
    /// device: a 5G's UI is not this slow, and an emulator that presents itself as the machine
    /// without saying it runs at about a third of its rate is lying by omission. Timing is also
    /// where this emulator's remaining unknowns are, which makes it the number most worth having in
    /// front of a person who is about to report that something felt sluggish.
    fn footer(&mut self, ui: &mut egui::Ui, out: &emu::Out) {
        let s = out.stats;
        let pct = s.executed_here as f64 / s.wall_secs.max(1e-6) / HARDWARE_MIPS * 100.0;
        egui::Panel::bottom("footer").show(ui, |ui| {
            // A cold boot spends most of its time on a white screen, because that is what the
            // hardware shows too — the Apple logo is drawn early and then RetailOS does several
            // minutes of simulated work before it draws anything else. Without this, user mode
            // gives no evidence the machine is doing anything at all, and a blank window that is
            // busy is indistinguishable from a blank window that has hung. The same bar has been
            // in the debug panel all along; there was no reason it was only there.
            if let Phase::Booting { target } = &out.phase {
                let f = (s.executed as f32 / (*target).max(1) as f32).min(1.0);
                // A bar with no text in it. At 6 points there is no room for a label inside,
                // and shrinking the label to fit is how it became unreadable — so the words go
                // in the row below, at the left, where the rest of the footer's text already is.
                ui.add_space(4.0);
                ui.add(egui::ProgressBar::new(f).desired_height(6.0).corner_radius(3));
                ui.add_space(3.0);
            }
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                // During a cold boot the useful number is how much of it is left, not how fast
                // it is going, so that is what the leftmost slot says while it lasts.
                let badge = if let Phase::Booting { target } = &out.phase {
                    // **An estimate, and it says so.** The fraction is instructions against
                    // `snap_at`, which is where the snapshot is taken and not where the boot ends —
                    // the machine is finished when RetailOS asks for wheel frames, which is what
                    // ends this phase. So the bar is a rough sense of progress and the words no
                    // longer promise a deadline the program cannot keep.
                    let f = (s.executed as f32 / (*target).max(1) as f32).min(1.0);
                    format!("cold boot — roughly {:.0} %", f * 100.0)
                } else if s.executed_here == 0 {
                    "≈30 % of real-time — emulated".to_string()
                } else {
                    format!("{pct:.0} % of real-time — emulated")
                };
                ui.label(egui::RichText::new(badge).monospace().size(11.0))
                    .on_hover_text(
                        "The interpreter executes about 21 M instructions a second; a PP5021C \
                         does about 72 M. So everything here happens at roughly a third of the \
                         speed the real device would. The emulator's own microsecond clock is a \
                         different number again — the readout shows both.",
                    );
                if self.link.as_ref().is_some_and(|l| l.saving.load(Ordering::Relaxed)) {
                    ui.separator();
                    ui.label(egui::RichText::new("parking the machine…").size(11.0));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // **Settings, not setup, and it no longer ends anything.** The images chosen
                    // on the first run used to be the images for ever unless you were willing to
                    // reboot to look at them, because this button and the first-run screen were
                    // the same screen.
                    if ui
                        .button("settings…")
                        .on_hover_text("The iPod keeps running. Esc comes back.")
                        .clicked()
                    {
                        self.open_settings();
                    }
                    if let Some(line) = self.update_line.clone() {
                        ui.separator();
                        ui.label(egui::RichText::new(line).size(11.0));
                    }
                });
            });
            ui.add_space(2.0);
        });
    }

    fn keyboard(&mut self, ctx: &egui::Context) {
        use egui::Key;
        let (mut scroll, mut pressed, mut released, mut toggle_hold, mut shot, mut toggle_mode) =
            (0i32, Vec::new(), Vec::new(), false, false, false);
        let mut wheel_units = self.wheel_units;
        let dev_area = self.dev_area;
        ctx.input(|i| {
            // Held rather than pressed, so holding an arrow scrolls. One click per repaint is about
            // 60 clicks a second, which is a brisk but human thumb.
            if i.key_down(Key::ArrowRight) || i.key_down(Key::ArrowDown) {
                scroll += 1;
            }
            if i.key_down(Key::ArrowLeft) || i.key_down(Key::ArrowUp) {
                scroll -= 1;
            }
            for (k, b) in [
                (Key::Enter, Button::Select),
                (Key::Space, Button::Select),
                (Key::M, Button::Menu),
                (Key::P, Button::Play),
                (Key::Period, Button::Next),
                (Key::Comma, Button::Prev),
            ] {
                if i.key_pressed(k) {
                    pressed.push(b);
                }
                if i.key_released(k) {
                    released.push(b);
                }
            }
            toggle_hold = i.key_pressed(Key::H);
            shot = i.key_pressed(Key::S);
            toggle_mode = i.key_pressed(Key::D);

            // The mouse wheel drives the click wheel. It is the obvious input for this device and
            // every mouse has one; a user asked why it did not work and the answer was that nobody
            // had wired it.
            //
            // A physical notch reports about 40 units in egui, and a trackpad reports a continuous
            // stream of small ones. Accumulating and dividing gives one detent per notch and a
            // proportional glide from a trackpad, rather than either flying or doing nothing.
            //
            // ONLY over the device. The first version took scroll from anywhere in the window, so
            // scrolling the instrument panel in debug mode turned the click wheel at the same time
            // — the panel moved and RetailOS's menu moved with it. The pointer decides whose scroll
            // it is, and `dev_area` is last frame's rectangle because input is read before the
            // device is painted.
            let over_device = i
                .pointer
                .latest_pos()
                .is_some_and(|p| dev_area.contains(p));
            let dy = i.smooth_scroll_delta.y;
            if dy != 0.0 && over_device {
                wheel_units += dy;
            }
        });

        // Down the page scrolls the same way down the menu does.
        let detents = (wheel_units / SCROLL_UNITS_PER_DETENT) as i32;
        if detents != 0 {
            wheel_units -= detents as f32 * SCROLL_UNITS_PER_DETENT;
            scroll -= detents;
        }
        self.wheel_units = wheel_units;

        if scroll != 0 {
            self.touch();
            self.kbd_touch_until = Some(Instant::now() + Duration::from_millis(300));
            self.rotate(scroll);
        }
        // A keyboard scroll has no "finger up", so one is synthesised on a short idle. Without it
        // the wheel would report a finger resting on it forever, which is a state a real one cannot
        // be left in and which RetailOS would be entitled to treat as a fault.
        if let Some(t) = self.kbd_touch_until {
            if Instant::now() > t && self.drag.is_none() {
                self.kbd_touch_until = None;
                self.untouch();
            }
        }
        for b in pressed {
            self.press(b);
        }
        for b in released {
            self.release(b);
        }
        if toggle_hold {
            let on = !self.hold;
            self.set_hold(on);
        }
        if toggle_mode {
            let m = if self.settings.mode == Mode::Debug { Mode::User } else { Mode::Debug };
            self.set_mode(m);
        }
        // A screenshot is an instrument, so it is a debug-mode action. The device is still there in
        // user mode; the file it would write is not part of "an iPod and nothing else".
        if shot && self.settings.mode == Mode::Debug {
            let (fb, addr) = match &self.link {
                Some(l) => {
                    let o = l.out.lock().unwrap();
                    (o.fb.clone(), o.fb_addr)
                }
                None => return,
            };
            self.screenshot(&fb, addr);
        }
    }

    // ------------------------------------------------------------ drawing the device


    /// The key list, drawn into the empty column left of the device.
    ///
    /// `left_edge` is where the case starts, in points. Everything from the panel's left edge to
    /// there is space nothing else wants.
    fn keys_in_margin(&self, p: &egui::Painter, area: Rect, left_edge: f32) {
        const KEYS: &[(&str, &str)] = &[
            ("arrows", "scroll the wheel"),
            ("enter / space", "select"),
            ("M P , .", "menu · play · prev · next"),
            ("H", "hold switch"),
            ("S", "save a PNG into _out/"),
            ("D", "user / debug"),
        ];
        // Wide enough for the longest line, with a gap before the case, or it is not drawn.
        let margin = left_edge - area.left();
        if margin < 190.0 {
            return;
        }
        let x = area.left() + 22.0;
        let line = 17.0;
        let mut y = area.center().y - (KEYS.len() as f32 * line) / 2.0;
        let font = egui::FontId::proportional(11.0);
        // Deliberately low contrast. It is a reference, not a thing to read every time — the wheel,
        // the buttons and the switch all take clicks, so nobody has to use these at all.
        let dim = Color32::from_gray(96);
        let dimmer = Color32::from_gray(72);
        for (k, what) in KEYS {
            p.text(Pos2::new(x, y), egui::Align2::LEFT_TOP, k, font.clone(), dim);
            p.text(Pos2::new(x + 92.0, y), egui::Align2::LEFT_TOP, what, font.clone(), dimmer);
            y += line;
        }
    }

    fn device(&mut self, ui: &mut egui::Ui, area: Rect, out: &emu::Out) {
        // For the next frame's input handling, which runs before this does.
        self.dev_area = area;
        let ppp = ui.ctx().pixels_per_point();
        // Everything is derived from ONE integer: the number of physical pixels per emulator pixel.
        // Deriving the device from the panel rather than the panel from the device is what makes
        // the screen exact by construction instead of by rounding afterwards.
        let pad = 16.0 * ppp;
        let avail_w = (area.width() * ppp - pad).max(1.0);
        let avail_h = (area.height() * ppp - pad).max(1.0);
        let by_w = avail_w / (FB_W as f32 * CASE_W / SCREEN_W);
        let by_h = avail_h / (FB_W as f32 * (CASE_H + SWITCH_PROUD) / SCREEN_W);
        let k = by_w.min(by_h).floor().max(1.0);
        self.scale = k as u32;

        let px_per_mm = FB_W as f32 * k / SCREEN_W;
        let dev_w = CASE_W * px_per_mm;
        let dev_h = (CASE_H + SWITCH_PROUD) * px_per_mm;

        // Snap the device's origin to the physical pixel grid: the screen's offset within it is a
        // whole number of physical pixels, so if the origin is on the grid the screen is too.
        let cx = area.center().x * ppp;
        let cy = area.center().y * ppp;
        let ox = (cx - dev_w / 2.0).round();
        let oy = (cy - dev_h / 2.0).round();
        // `y` is measured from the top of the CASE; the switch protrudes into negative y, which is
        // the room `SWITCH_PROUD` reserves above it.
        let at = |xmm: f32, ymm: f32| -> Pos2 {
            Pos2::new((ox + xmm * px_per_mm) / ppp, (oy + (ymm + SWITCH_PROUD) * px_per_mm) / ppp)
        };
        let mm = |v: f32| v * px_per_mm / ppp;

        let p = ui.painter_at(area);
        let (body, wheel_fill, ring_text, glass) = self.palette();

        // The keys, printed in the margin the device does not use.
        //
        // Not a tooltip and not a popover: this window covers nothing with anything. A drawn iPod
        // in a rectangular window leaves two columns of empty space either side of it, and a list
        // that lives there is always readable, never in the way, and costs no interaction to find.
        // It is skipped when the margin is too narrow to hold it rather than being allowed to
        // collide with the case — at which point the window is small enough that the keys are the
        // least of it.
        self.keys_in_margin(&p, area, ox / ppp);

        // A detected fault, in the gap below the case. Same rule as the keys: it sits in space
        // nothing else wants, so it never covers the screen it is describing. Amber rather than
        // red — none of this stops the emulator running, it explains what is on the screen.
        if let Some(n) = &self.notice {
            let below = (oy + dev_h) / ppp;
            if area.bottom() - below > 34.0 {
                p.text(
                    Pos2::new(area.center().x, below + 12.0),
                    egui::Align2::CENTER_TOP,
                    n,
                    egui::FontId::proportional(12.0),
                    Color32::from_rgb(214, 158, 74),
                );
                p.text(
                    Pos2::new(area.center().x, below + 12.0 + 15.0),
                    egui::Align2::CENTER_TOP,
                    "press D for the log",
                    egui::FontId::proportional(11.0),
                    Color32::from_gray(96),
                );
            }
        }

        // The hold switch, drawn BEFORE the body so the body's rounded corner covers the part of it
        // that is inside the case — which is what makes it read as seated in a slot rather than
        // stuck on top of one.
        self.hold_switch(&p, &at, &mm, body);

        // The body.
        let face = Rect::from_min_max(at(0.0, 0.0), at(CASE_W, CASE_H));
        p.rect_filled(face, CornerRadius::from(mm(5.5) as u8), body);
        p.rect_stroke(
            face,
            CornerRadius::from(mm(5.5) as u8),
            Stroke::new(1.0, Color32::from_black_alpha(40)),
            StrokeKind::Inside,
        );

        // The panel: glass first, then the framebuffer inside it at exactly `k` physical pixels per
        // emulator pixel, snapped to the grid, nearest-neighbour.
        let sx = ox + ((CASE_W - SCREEN_W) / 2.0 * px_per_mm).round();
        let sy = oy + ((SWITCH_PROUD + SCREEN_TOP) * px_per_mm).round();
        let screen = Rect::from_min_size(
            Pos2::new(sx / ppp, sy / ppp),
            Vec2::new(FB_W as f32 * k / ppp, FB_H as f32 * k / ppp),
        );
        // The glass is a physical object, so it is laid out in millimetres — 1.6 mm of bezel
        // around a 50.8 x 38.1 mm active area. It lands on the same rectangle the pixel-derived
        // `screen` does only because the panel's pixels are square; that is asserted by a test
        // rather than assumed here.
        let glass_rect = Rect::from_min_max(
            Pos2::new(
                (sx - 1.6 * px_per_mm) / ppp,
                (oy + (SWITCH_PROUD + SCREEN_TOP - 1.6) * px_per_mm) / ppp,
            ),
            Pos2::new(
                (sx + (SCREEN_W + 1.6) * px_per_mm) / ppp,
                (oy + (SWITCH_PROUD + SCREEN_TOP + SCREEN_H + 1.6) * px_per_mm) / ppp,
            ),
        );
        p.rect_filled(glass_rect, CornerRadius::from(mm(1.0) as u8), glass);
        p.image(
            self.tex.id(),
            screen,
            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
            Color32::WHITE,
        );

        // The wheel.
        let c = at(CASE_W / 2.0, WHEEL_CY);
        let ring = WheelRing::new(c.x, c.y, mm(WHEEL_D / 2.0));
        p.circle_filled(c, ring.outer, wheel_fill);
        p.circle_stroke(c, ring.outer, Stroke::new(1.0, Color32::from_black_alpha(30)));
        for b in Button::ALL {
            let Some(cl) = b.centre_click() else { continue };
            let (lx, ly) = ring.point_at(cl as u8);
            let lit = self.down.contains(&b);
            let ink = if lit { Color32::from_rgb(0x2f, 0x6f, 0xd0) } else { ring_text };
            let at = Pos2::new(lx, ly);
            let size = (ring.outer * 0.19).max(7.0);
            if b == Button::Menu {
                p.text(at, Align2::CENTER_CENTER, "MENU", FontId::proportional(size), ink);
            } else {
                transport(&p, b, at, size, ink);
            }
        }
        let select_lit = self.down.contains(&Button::Select);
        p.circle_filled(
            c,
            ring.select,
            if select_lit { wheel_fill.gamma_multiply(0.82) } else { wheel_fill.gamma_multiply(0.94) },
        );
        p.circle_stroke(c, ring.select, Stroke::new(1.0, Color32::from_black_alpha(26)));

        // Where the machine believes the finger is. Drawn from the emulator's `position`, not from
        // the pointer, so a click the model never received is visibly a click that did not happen.
        if out.stats.touched {
            let (fx, fy) = ring.point_at(out.stats.position);
            p.circle_filled(
                Pos2::new(fx, fy),
                (ring.outer - ring.inner) * 0.30,
                Color32::from_rgb(0x2f, 0x6f, 0xd0).gamma_multiply(0.55),
            );
        }

        self.wheel_input(ui, area, ring);

        // The scale, stated rather than assumed — an instrument's self-description, so debug only.
        if self.settings.mode == Mode::Debug {
            p.text(
                Pos2::new(face.center().x, face.max.y + 14.0),
                Align2::CENTER_TOP,
                format!("{}x, nearest-neighbour, {} px/pt", self.scale, ppp),
                FontId::monospace(11.0),
                Color32::GRAY,
            );
        }
    }

    /// The three the 5G shipped as: `(body, wheel, ring text, glass)`.
    ///
    /// The U2 is the reason this is not a boolean. Its case is the black one and only the WHEEL
    /// differs — which is why Apple's own asset for it is named `iPod6-BlackRed` rather than being
    /// a colour of its own. Getting that wrong by tinting the whole case red would be a device
    /// nobody ever sold.
    fn palette(&self) -> (Color32, Color32, Color32, Color32) {
        palette_for(self.settings.chassis)
    }

    /// The hold switch, as a control **protruding from the top edge of the front view**.
    ///
    /// It used to be drawn on a sliver of the top face laid above the front — a top-down view
    /// pasted onto a front view, which is two viewpoints in one drawing. What you see from the
    /// front is the part of the switch that stands proud of the case, so that is what is drawn: a
    /// small nub above the top edge, with a knob that slides left to right through visible travel
    /// and uncovers an orange band on the side it has left, the way the real one does.
    ///
    /// Drawn before the body, so the body's own rounded top edge overlaps the bottom of it and the
    /// switch reads as seated in the case rather than resting on it.
    fn hold_switch(
        &mut self,
        p: &egui::Painter,
        at: &dyn Fn(f32, f32) -> Pos2,
        mm: &dyn Fn(f32) -> f32,
        body: Color32,
    ) {
        // The slot runs from `SWITCH_PROUD` above the case down to 1.2 mm inside it. The lower part
        // is hidden by the body, which is the overlap that seats it.
        let slot = Rect::from_min_max(
            at(SWITCH_X, -SWITCH_PROUD),
            at(SWITCH_X + SWITCH_W, 1.2),
        );
        // The hit region is the visible part only, expanded a little for a fingertip.
        self.hold_slot = Rect::from_min_max(at(SWITCH_X, -SWITCH_PROUD), at(SWITCH_X + SWITCH_W, 0.4));
        let r = CornerRadius::from(mm(0.7) as u8);

        // The recess the switch travels in, a shade darker than the case.
        p.rect_filled(slot, r, body.gamma_multiply(0.66));
        // Orange shows through on the side the knob has left. On a real 5G the band is uncovered
        // when the switch is toward the outside of the case, which is `hold` engaged.
        if self.hold {
            let orange = Rect::from_min_max(
                Pos2::new(slot.min.x + slot.width() * 0.08, slot.min.y + slot.height() * 0.18),
                Pos2::new(slot.min.x + slot.width() * 0.46, slot.min.y + slot.height() * 0.72),
            );
            p.rect_filled(orange, CornerRadius::from(1u8), Color32::from_rgb(0xe0, 0x6c, 0x18));
        }
        // The knob: half the slot wide, travelling the other half. The travel is the whole point —
        // a switch that only changed colour would not read as a switch.
        let w = slot.width() * 0.5;
        let x = if self.hold { slot.max.x - w } else { slot.min.x };
        let knob = Rect::from_min_max(
            Pos2::new(x, slot.min.y),
            Pos2::new(x + w, slot.min.y + slot.height() * 0.82),
        );
        p.rect_filled(knob, r, body.gamma_multiply(0.94));
        p.rect_stroke(knob, r, Stroke::new(1.0, Color32::from_black_alpha(55)), StrokeKind::Inside);
        // A grip line, so the knob is legible as a thing to push at small scales.
        let g = knob.center();
        p.line_segment(
            [Pos2::new(g.x, knob.min.y + knob.height() * 0.28), Pos2::new(g.x, knob.max.y - knob.height() * 0.18)],
            Stroke::new(1.0, Color32::from_black_alpha(45)),
        );
    }

    /// Pointer handling for the wheel, the five buttons and the hold switch.
    ///
    /// Driven off the raw "is the button down on this widget" state rather than off egui's
    /// click-vs-drag classification. The distinction egui draws is about *intent* and depends on a
    /// movement threshold; the wheel needs the *physical* fact, because a press that becomes a
    /// scroll is one continuous gesture on the hardware, and splitting it into a click and a drag
    /// produced a spurious Menu press at the start of every scroll begun at twelve o'clock.
    fn wheel_input(&mut self, ui: &mut egui::Ui, area: Rect, ring: WheelRing) {
        let resp = ui.interact(area, ui.id().with("device"), egui::Sense::click_and_drag());
        let down = resp.is_pointer_button_down_on();
        let at = ui.ctx().input(|i| i.pointer.interact_pos());

        match (down, at, self.drag.is_some()) {
            // ---- the gesture begins
            (true, Some(p), false) => {
                if self.hold_slot.expand(6.0).contains(p) {
                    let on = !self.hold;
                    self.set_hold(on);
                    // A latch, not a press: the gesture is marked consumed so the release below
                    // has something to close and the switch does not throw again next frame.
                    self.drag = Some(Drag { last: 0, button: None, consumed: true });
                    return;
                }
                match ring.hit(p.x, p.y) {
                    Hit::Select => {
                        self.press(Button::Select);
                        self.drag =
                            Some(Drag { last: 0, button: Some(Button::Select), consumed: false });
                    }
                    Hit::RingButton(b, pos) => {
                        self.press(b);
                        self.drag = Some(Drag { last: pos, button: Some(b), consumed: false });
                    }
                    Hit::Ring(pos) => {
                        self.touch();
                        self.drag = Some(Drag { last: pos, button: None, consumed: false });
                    }
                    // A press off the device is still a gesture — recorded, so that dragging back
                    // onto the wheel does not start a scroll from a stale angle.
                    Hit::None => {
                        self.drag = Some(Drag { last: 0, button: None, consumed: true });
                    }
                }
            }
            // ---- the gesture continues
            (true, Some(p), true) => {
                let (mut release, mut turn) = (None, 0);
                if let Some(d) = self.drag.as_mut() {
                    if !d.consumed && !matches!(ring.hit(p.x, p.y), Hit::None | Hit::Select) {
                        let pos = wheel::position_at_angle(p.x - ring.cx, p.y - ring.cy);
                        let delta = wheel::shortest_delta(d.last, pos);
                        if delta != 0 {
                            // A press that turned into a scroll is a scroll. Let go of the button
                            // and put a finger on the wheel instead — otherwise every scroll begun
                            // on the Menu label would send Menu first.
                            release = d.button.take();
                            d.last = pos;
                            turn = delta;
                        }
                    }
                }
                if let Some(b) = release {
                    self.release(b);
                }
                if turn != 0 {
                    self.touch();
                    self.rotate(turn);
                }
            }
            // ---- the gesture ends
            (false, _, true) => {
                if let Some(d) = self.drag.take() {
                    if let Some(b) = d.button {
                        self.release(b);
                    }
                    self.untouch();
                }
            }
            _ => {}
        }
    }

    // ------------------------------------------------------------ power, and the two combos

    // ------------------------------------------------------------ the device's own controls

    /// The row under the iPod, and the conditions worth interrupting for. **In every mode.**
    ///
    /// These lived in the instrument panel, which meant they were debug-mode features — and two of
    /// them are not debug anything. *Power* is a thing you do to an iPod, and a user who could not
    /// reach it had no way to restart the device at all. *MENU+SELECT* is the hard reset, held with
    /// two thumbs on real hardware and needing a control here because a mouse has one pointer.
    ///
    /// The conditions are the other half. A machine that has halted, a hold switch that is on, a
    /// picture being drawn to the surface nobody is looking at: each of these makes the iPod appear
    /// broken, each has a one-line explanation, and hiding the explanation behind a mode is hiding
    /// it from exactly the person who needs it. What is *not* here is every counter that produced
    /// them, which is what the readout is for.
    fn device_controls(&mut self, ui: &mut egui::Ui, out: &emu::Out) {
        egui::Panel::bottom("controls").show(ui, |ui| {
            ui.add_space(4.0);
            for (colour, text) in self.conditions(out) {
                ui.colored_label(colour, text);
            }
            ui.horizontal_wrapped(|ui| {
                if out.phase == Phase::Off {
                    if ui.button("power on").clicked() {
                        if let Some(l) = &self.link {
                            l.command(emu::Cmd::PowerOn);
                        }
                        self.say("power on: cold boot from the reset vector");
                    }
                } else {
                    if ui.button("power off").clicked() {
                        self.down.clear();
                        self.touching = false;
                        if let Some(l) = &self.link {
                            l.command(emu::Cmd::PowerOff);
                        }
                        self.say("power off: the machine is dropped");
                    }
                    if ui
                        .button("restart")
                        .on_hover_text("A cold boot from the reset vector — about 75 seconds.")
                        .clicked()
                    {
                        self.down.clear();
                        self.touching = false;
                        if let Some(l) = &self.link {
                            l.command(emu::Cmd::PowerCycle);
                        }
                        self.say("power cycle: rebuilding at the reset vector (~75 s)");
                    }
                }
                ui.separator();
                // Two-thumb gestures, which a single pointer cannot make. Latched rather than
                // momentary for that reason, and shown latched so a forgotten one is visible.
                let combo = [Button::Menu, Button::Select];
                let held = combo.iter().all(|b| self.down.contains(b));
                if ui
                    .selectable_label(held, "hold MENU+SELECT")
                    .on_hover_text("The hard reset, on the real device.")
                    .clicked()
                {
                    for b in combo {
                        if held {
                            self.release(b);
                        } else {
                            self.press(b);
                        }
                    }
                }
                let play_held = self.down.contains(&Button::Play);
                if ui
                    .selectable_label(play_held, "hold PLAY")
                    .on_hover_text("Sleep, on the real device.")
                    .clicked()
                {
                    if play_held {
                        self.release(Button::Play);
                    } else {
                        self.press(Button::Play);
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("screenshot").on_hover_text("Also the S key.").clicked() {
                        let (fb, addr) = match &self.link {
                            Some(l) => {
                                let o = l.out.lock().unwrap();
                                (o.fb.clone(), o.fb_addr)
                            }
                            None => (vec![0u8; FB_W * FB_H * 3], FB_FRONT),
                        };
                        self.screenshot(&fb, addr);
                    }
                });
            });
            if let Some(n) = self.notice.clone() {
                ui.label(egui::RichText::new(n).small().color(Color32::from_rgb(0xE0, 0xA0, 0x40)));
            }
            if let Some(l) = self.log.front() {
                ui.label(egui::RichText::new(l.as_str()).small().color(Color32::from_gray(0x78)));
            }
            ui.add_space(4.0);
        });
    }

    /// The states that make a working emulator look broken, each with the sentence that explains it.
    ///
    /// **Rare by construction.** Every one of these is either a machine that has genuinely stopped
    /// or a machine whose picture is somewhere the window is not looking, and both are things a
    /// person will otherwise report as "it froze". None of them is a counter.
    fn conditions(&self, out: &emu::Out) -> Vec<(Color32, String)> {
        let mut v = Vec::new();
        if let Phase::Stopped(why) = &out.phase {
            v.push((Color32::from_rgb(0xd0, 0x50, 0x40), format!("stopped: {why}")));
        }
        // A machine that has stopped is not a machine that is drawing slowly, and the difference is
        // invisible without this line.
        if out.stalled_secs > 2.0 && out.phase == Phase::Running {
            v.push((
                Color32::from_rgb(0xd0, 0x50, 0x40),
                format!(
                    "HALTED — no instruction has executed for {:.0} s. The core is waiting for an \
                     interrupt that nothing is going to raise.",
                    out.stalled_secs
                ),
            ));
        }
        if out.stats.hold {
            v.push((
                Color32::from_rgb(0x6a, 0x9a, 0x60),
                "Hold is engaged, and RetailOS has been told.".to_string(),
            ));
        }
        // The one case where a still screen is the window's fault and not the machine's.
        if out.fb_other_moved && !out.fb_shown_moved {
            v.push((
                Color32::from_rgb(0xc8, 0x8a, 0x20),
                "The picture is being drawn to the OTHER surface.".to_string(),
            ));
        }
        v
    }

    // ------------------------------------------------------------ the readout

    /// The measurement, drawn **over** the device rather than in a panel beside it.
    ///
    /// The instrument panel was a resizable right-hand `Panel` holding six collapsing sections and
    /// about thirty numbers, and turning it on gave the window a different shape — a second layout
    /// to design, and a device that jumped sideways when you wanted to read a counter. Two things
    /// changed that: the controls and the conditions moved out to where they belong in every mode,
    /// and what remained turned out to be *measurement*, every line of which has a command-line
    /// instrument that answers the same question with more precision and a log to keep it in.
    ///
    /// So this is a corner overlay: no reflow, no second shape, and small enough that its cost is
    /// obvious. It is the last stop before the readout goes altogether — which is what should
    /// happen once each of these has either become a condition above or been left to the recipes.
    fn readout(&mut self, ui: &mut egui::Ui, area: Rect, out: &emu::Out) {
        let s = out.stats;
        let ips = s.executed_here as f64 / s.wall_secs.max(1e-6);
        let lines = [
            format!("{:.1} M/s   {:.0} % of hardware", ips / 1e6, ips / HARDWARE_MIPS * 100.0),
            format!("{} instructions", fmt_u64(s.executed)),
            format!("{:.1} s wall   {:.1} s simulated", s.wall_secs, s.sim_usec as f64 / 1e6),
            format!(
                "wheel {} / 96   {}   {}",
                s.position,
                if s.touched { "touched" } else { "—" },
                if s.reporting { "reporting" } else { "NOT reporting" }
            ),
            format!("frames {} posted, {} dropped", fmt_u64(s.frames_posted), fmt_u64(s.frames_dropped)),
            format!("panel {:#010x}   {} / {} lit", out.fb_addr, out.fb_nonzero, FB_W * FB_H),
            format!("backlight {} / 32", out.backlight),
            format!("bcm {} frames, {} commands", fmt_u64(s.bcm_frames), s.bcm_commands),
        ];
        egui::Area::new("readout".into())
            .fixed_pos(Pos2::new(area.right() - 232.0, area.top() + 12.0))
            .order(egui::Order::Foreground)
            .show(ui.ctx(), |ui| {
                egui::Frame::NONE
                    .fill(Color32::from_black_alpha(0xC0))
                    .inner_margin(10.0)
                    .corner_radius(6.0)
                    .show(ui, |ui| {
                        ui.set_width(200.0);
                        for l in lines {
                            ui.label(
                                egui::RichText::new(l).monospace().size(10.5).color(Color32::from_gray(0xC8)),
                            );
                        }
                        ui.add_space(4.0);
                        // The one control here that is not a number: which of the two surfaces the
                        // window samples. It belongs with the panel address above it.
                        if ui
                            .checkbox(&mut self.show_back_buffer, "back buffer")
                            .on_hover_text("Show the surface RetailOS is drawing to, not the one it is showing.")
                            .changed()
                        {
                            let a = if self.show_back_buffer { FB_BACK } else { FB_FRONT };
                            if let Some(l) = &self.link {
                                l.out.lock().unwrap().fb_addr = a;
                            }
                        }
                        ui.label(
                            egui::RichText::new("D hides this")
                                .size(9.5)
                                .color(Color32::from_gray(0x70)),
                        );
                    });
            });
    }

}

/// The platform's own file-open dialog, through the tool every platform already has.
///
/// A dialog crate would be the obvious answer and is the wrong trade here: on Linux the portal
/// backend brings an async runtime and a D-Bus stack into a program whose entire dependency
/// argument is "eframe and nothing else", and it can still fail at runtime when no portal is
/// running. So: `osascript` on macOS, PowerShell's `OpenFileDialog` on Windows, `zenity` or
/// `kdialog` on Linux — and **drag-and-drop plus the text field work regardless**, which is what
/// makes it acceptable for this to return [`None`].
/// **Several files, not one.** The window wants both, it sorts them itself, and a dialog that
/// allows one selection makes somebody open it twice to do a thing they could have done once. Each
/// platform spells multi-select differently and none of them does it by default:
///
/// ```text
///   macOS      choose file … with multiple selections allowed  → a list of aliases
///   Windows    OpenFileDialog.Multiselect = $true              → .FileNames
///   GNOME      zenity --file-selection --multiple              → separator-joined
///   KDE        kdialog --getopenfilename --multiple            → shell-quoted, space-joined
/// ```
///
/// The separator is the trap, and each is handled where it is chosen rather than guessed at the
/// end: zenity joins with `|` unless told otherwise (a character that is legal in a filename on
/// every platform this runs on, which is why it is set to a newline instead), and osascript's
/// `POSIX path of` does not distribute over a list, so the list is walked in AppleScript.
fn pick_files(title: &str, exts: &[&str]) -> Vec<String> {
    use std::process::{Command, Stdio};
    let out = if cfg!(target_os = "macos") {
        let types = exts
            .iter()
            .map(|e| format!("\"{e}\""))
            .collect::<Vec<_>>()
            .join(", ");
        // `POSIX path of` takes an alias, not a list, so the conversion is a loop — and the result
        // is joined on a newline, which cannot occur in a macOS filename.
        Command::new("osascript")
            .args([
                "-e",
                &format!(
                    "set fs to (choose file with prompt \"{title}\" of type {{{types}}} \
                     with multiple selections allowed)\n\
                     set out to \"\"\n\
                     repeat with f in fs\n\
                     set out to out & (POSIX path of f) & linefeed\n\
                     end repeat\n\
                     return out"
                ),
            ])
            .stderr(Stdio::null())
            .output()
            .ok()
    } else if cfg!(windows) {
        let filter = format!(
            "{title}|{}|All files|*.*",
            exts.iter().map(|e| format!("*.{e}")).collect::<Vec<_>>().join(";")
        );
        Command::new("powershell")
            .args([
                "-NoProfile",
                "-STA",
                "-Command",
                &format!(
                    "Add-Type -AssemblyName System.Windows.Forms; \
                     $d = New-Object System.Windows.Forms.OpenFileDialog; \
                     $d.Title = '{title}'; $d.Filter = '{filter}'; $d.Multiselect = $true; \
                     if ($d.ShowDialog() -eq 'OK') {{ $d.FileNames }}"
                ),
            ])
            .stderr(Stdio::null())
            .output()
            .ok()
    } else {
        // zenity's default separator is `|`, which is a legal filename character here. A newline
        // is not, on any filesystem this reaches.
        let zenity = Command::new("zenity")
            .args([
                "--file-selection",
                "--multiple",
                "--separator=\n",
                &format!("--title={title}"),
            ])
            .stderr(Stdio::null())
            .output();
        match zenity {
            Ok(o) if o.status.success() => Some(o),
            // `--separate-output` is not optional here. Without it kdialog joins its list with
            // spaces and shell-quotes each entry, which would have to be unquoted again — and
            // stripping quotes from a path is how a file legitimately named `'demo'.bin` stops
            // being findable. With it, one path per line and nothing added.
            _ => Command::new("kdialog")
                .args(["--getopenfilename", ".", "--multiple", "--separate-output", "--title", title])
                .stderr(Stdio::null())
                .output()
                .ok(),
        }
    };
    let Some(out) = out else { return Vec::new() };
    if !out.status.success() {
        return Vec::new();
    }
    // Only the line ending is stripped. Leading and trailing spaces are legal in a filename on
    // every platform this runs on, and a picker's output is a path the user selected rather than
    // something they typed, so there is nothing to tidy up and something to lose by tidying.
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| l.trim_end_matches(['\r', '\n']).to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

/// The three transport glyphs, drawn as geometry.
///
/// They were text once — `\u{25b6}\u{2016}` for play/pause — and the pause bars came out as a tofu
/// box, because whether a codepoint renders depends on which fonts the toolkit happened to bundle.
/// A device drawn as vector geometry should not have three of its five controls depend on that.
/// `h` is the glyph's height; everything else is a proportion of it.
fn transport(p: &egui::Painter, b: Button, at: Pos2, h: f32, ink: Color32) {
    let tri = |cx: f32, right: bool| {
        let d = if right { h * 0.42 } else { -h * 0.42 };
        p.add(egui::Shape::convex_polygon(
            vec![
                Pos2::new(cx - d, at.y - h * 0.45),
                Pos2::new(cx + d, at.y),
                Pos2::new(cx - d, at.y + h * 0.45),
            ],
            ink,
            Stroke::NONE,
        ));
    };
    let bar = |cx: f32| {
        p.rect_filled(
            Rect::from_center_size(
                Pos2::new(cx, at.y),
                Vec2::new(h * 0.20, h * 0.90),
            ),
            CornerRadius::ZERO,
            ink,
        );
    };
    match b {
        Button::Next => {
            tri(at.x - h * 0.34, true);
            tri(at.x + h * 0.34, true);
        }
        Button::Prev => {
            tri(at.x - h * 0.34, false);
            tri(at.x + h * 0.34, false);
        }
        Button::Play => {
            tri(at.x - h * 0.52, true);
            bar(at.x + h * 0.22);
            bar(at.x + h * 0.58);
        }
        _ => {}
    }
}

/// Grouped in threes, because a bare `1610279157` is unreadable and this project's numbers are all
/// nine and ten digits long.
fn fmt_u64(v: u64) -> String {
    let s = v.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(' ');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The 5G's pixels are square, and the whole "integer scale, nearest-neighbour" argument rests
    /// on it: if the panel's physical aspect did not match its pixel grid, drawing 320x240 into a
    /// 50.8 x 38.1 mm rectangle at one scale factor would be *wrong*, and the honest thing would be
    /// two different scales. It does match — 4:3 both ways — so one integer suffices, and this test
    /// is what would object if one of the four millimetre figures were ever mistyped.
    #[test]
    fn the_panel_has_square_pixels_so_one_scale_factor_is_enough() {
        let physical = SCREEN_W / SCREEN_H;
        let pixels = FB_W as f32 / FB_H as f32;
        assert!(
            (physical - pixels).abs() < 0.001,
            "{SCREEN_W}x{SCREEN_H} mm is {physical}, {FB_W}x{FB_H} px is {pixels}"
        );
    }

    #[test]
    fn thousands_are_grouped_from_the_right() {
        assert_eq!(fmt_u64(0), "0");
        assert_eq!(fmt_u64(999), "999");
        assert_eq!(fmt_u64(1_000), "1 000");
        assert_eq!(fmt_u64(1_610_279_157), "1 610 279 157");
    }

    /// The panel's scale must be a whole number of physical pixels or the nearest-neighbour
    /// sampling has nothing to land on. This reproduces the arithmetic `device` does, for a range
    /// of window sizes and both common device-pixel ratios, and asserts it never goes fractional
    /// and never goes below 1.
    #[test]
    fn the_panel_scale_is_always_a_positive_integer() {
        for ppp in [1.0f32, 1.25, 1.5, 2.0, 3.0] {
            for w in [200.0f32, 383.0, 640.0, 977.0, 1600.0] {
                for h in [200.0f32, 511.0, 800.0, 1201.0, 2000.0] {
                    let avail_w = (w * ppp - 16.0 * ppp).max(1.0);
                    let avail_h = (h * ppp - 16.0 * ppp).max(1.0);
                    let by_w = avail_w / (FB_W as f32 * CASE_W / SCREEN_W);
                    let by_h = avail_h / (FB_W as f32 * (CASE_H + SWITCH_PROUD) / SCREEN_W);
                    let k = by_w.min(by_h).floor().max(1.0);
                    assert!(k >= 1.0, "{ppp} {w}x{h} gave {k}");
                    assert_eq!(k, k.floor(), "{ppp} {w}x{h} gave a fractional scale {k}");
                }
            }
        }
    }

    /// The hold switch is drawn in the SAME front view as everything else: it protrudes above the
    /// case's top edge and overlaps into it, rather than living on a separate top-face band. This
    /// asserts the geometry that makes that true, so a later edit cannot quietly reintroduce a
    /// second viewpoint by moving the switch clear of the body.
    #[test]
    fn the_hold_switch_protrudes_from_the_top_edge_and_overlaps_the_body() {
        assert!(SWITCH_PROUD > 0.0, "the switch has to stand proud of the case to be visible");
        assert!(
            SWITCH_PROUD < 3.0,
            "a 5G's hold switch is a nub, not a second device; {SWITCH_PROUD} mm is too much"
        );
        // It sits on the top edge, inside the case's width, toward the right where the real one is.
        assert!(SWITCH_X > CASE_W / 2.0, "the 5G's hold switch is right of centre");
        assert!(SWITCH_X + SWITCH_W < CASE_W, "and inside the case");
    }

    /// The default mode is user mode, and a fresh install has nothing configured. These two
    /// together are what a stranger meets, and both are easy to invert by accident.
    #[test]
    fn a_fresh_install_opens_in_user_mode_with_nothing_configured() {
        let s = Settings::default();
        assert_eq!(s.mode, Mode::User);
        assert!(s.flash.is_none() && s.disk.is_none());
        assert!(!s.check_updates_on_start, "the update check is opt-in");
    }

    /// `--flash=` / `--disk=` beat the remembered paths, and the remembered paths beat the
    /// `resources/` defaults. Getting this order wrong would make a command-line override silently
    /// ineffective, which is the worst of the three failures.
    #[test]
    fn the_command_line_beats_the_remembered_paths_which_beat_the_defaults() {
        let saved = Settings {
            flash: Some(PathBuf::from("/saved/rom.bin")),
            disk: Some(PathBuf::from("/saved/disk.img")),
            ..Default::default()
        };
        let c = config(&["--flash=/cli/rom.bin".to_string()], &saved).unwrap();
        assert_eq!(c.flash, PathBuf::from("/cli/rom.bin"));
        assert_eq!(c.disk, PathBuf::from("/saved/disk.img"));

        // The defaults land on **our** copies, under names that say what they are — not on the
        // vendored archive's `A1238/internal_rom_000000-0FFFFF.bin`, which is upstream's directory
        // for the iPod Classic and is where the Video's ROM is mis-filed. Asserted by whole
        // filename, because `Path::ends_with` compares components and would otherwise pass on any
        // file of that name anywhere.
        let c = config(&[], &Settings::default()).unwrap();
        assert!(
            c.flash.ends_with("retail_5g_MA146_HwVr000B0005_internal_rom_000000-0FFFFF.bin"),
            "{:?}",
            c.flash
        );
        assert!(c.disk.ends_with("ipod8g-retail.img"), "{:?}", c.disk);
        assert!(c.flash.parent().is_some_and(|p| p.ends_with("roms")), "{:?}", c.flash);
        assert!(c.disk.parent().is_some_and(|p| p.ends_with("drives")), "{:?}", c.disk);
    }

    /// A different pair of images must get a different snapshot. Restoring one machine's snapshot
    /// into another is the silent failure `from-idle.sh` documents at length, and the cache key is
    /// the only thing standing between this window and it.
    #[test]
    fn different_images_get_different_snapshots() {
        let a = cache_key(Path::new("/a.bin"), Path::new("/x.img"), 5, 1_600_000_000);
        let b = cache_key(Path::new("/b.bin"), Path::new("/x.img"), 5, 1_600_000_000);
        let c = cache_key(Path::new("/a.bin"), Path::new("/x.img"), 75, 1_600_000_000);
        let d = cache_key(Path::new("/a.bin"), Path::new("/x.img"), 5, 1_700_000_000);
        assert_ne!(a, b, "the NOR is part of the machine");
        assert_ne!(a, c, "so is the clock");
        assert_ne!(a, d, "so is where the snapshot was taken");
    }

    /// The snapshot and the 8 GB working disk go in a cache directory, not in the temp directory.
    /// `/tmp` is `tmpfs` on most Linux distributions — RAM — and an 8 GB image there either fails
    /// or eats half the machine.
    #[test]
    fn the_working_disk_does_not_go_in_the_temp_directory_on_linux() {
        let c = cache_paths(Path::new("/a.bin"), Path::new("/x.img"), 5, 1);
        if cfg!(target_os = "linux") {
            for p in [&c.snap, &c.frozen, &c.work] {
                assert!(!p.starts_with("/tmp"), "{}", p.display());
            }
        }
        // Three distinct files. The working drive and the frozen one sharing a path would be the
        // stale pair again, with the machine overwriting the very drive its snapshot needs.
        assert_ne!(c.snap, c.work);
        assert_ne!(c.frozen, c.work);
        assert_ne!(c.snap, c.frozen);
    }

    /// The message a fresh clone gets has to name both files and say where to look. This is the
    /// single biggest usability cliff in the project, and it is one string.
    /// The prune keeps exactly one set and deletes the rest.
    ///
    /// Written after shipping the opposite: the cache accumulated an 8 GB working disk per image
    /// pair and nothing ever removed one. This test fails if that behaviour ever returns.
    ///
    /// The set is three files since the frozen drive landed, and the third is the one worth
    /// watching: a prune that dropped it would leave a snapshot that cannot be restored, which
    /// fails as a slow cold boot rather than as an error.
    ///
    /// Since reclamation became consensual this is two assertions in one: that `reclaimable` finds
    /// exactly the stale set and touches nothing, and that `reclaim` then removes exactly what was
    /// offered. Measuring and deleting being separate calls is the whole point -- the figure has to
    /// be shown to somebody before anything goes.
    #[test]
    fn pruning_keeps_only_the_set_in_use() {
        let dir = std::env::temp_dir().join(format!("ipod-prune-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);

        let keep = Cache {
            snap: dir.join("idle-KEEP.snap"),
            frozen: dir.join("idle-KEEP.frozen"),
            work: dir.join("idle-KEEP.img"),
            stamp: dir.join("idle-KEEP.drive"),
        };
        for p in [&keep.snap, &keep.frozen, &keep.work] {
            std::fs::write(p, vec![0u8; 10]).unwrap();
        }
        // Two previous sets, as four firmware attempts would leave.
        for k in ["OLD1", "OLD2"] {
            std::fs::write(dir.join(format!("idle-{k}.snap")), vec![0u8; 100]).unwrap();
            std::fs::write(dir.join(format!("idle-{k}.frozen")), vec![0u8; 100]).unwrap();
            std::fs::write(dir.join(format!("idle-{k}.img")), vec![0u8; 100]).unwrap();
        }
        // A file that is not ours must survive.
        std::fs::write(dir.join("settings.txt"), b"mode = user\n").unwrap();

        let (stale, paths) = reclaimable(&dir, &keep, true);
        assert_eq!(paths.len(), 6, "six stale files");
        // Not asserted in bytes. The size is now what the FILESYSTEM gave up, and a 100-byte file
        // occupies a whole block -- so the only honest assertions here are that something was
        // found and that it is at least as large as the bytes written. Asserting 600 would be
        // asserting that the measurement is the wrong one.
        assert!(stale >= 600, "the stale set must account for at least the bytes written");
        // Measuring must not delete: this is the assertion that would have caught the old
        // behaviour if the old behaviour had ever been questioned.
        assert!(dir.join("idle-OLD1.img").exists(), "reclaimable() deleted something");

        let freed = reclaim(&paths);
        assert_eq!(freed, stale, "reclaim must free exactly what was offered");
        assert!(keep.snap.exists() && keep.work.exists(), "the set in use must survive");
        assert!(keep.frozen.exists(), "the frozen drive belongs to the set and must survive");
        assert!(dir.join("settings.txt").exists(), "a non-cache file must not be touched");
        assert!(!dir.join("idle-OLD1.img").exists(), "a stale working disk must be gone");
        assert!(!dir.join("idle-OLD2.frozen").exists(), "a stale frozen drive must be gone");
        assert!(!dir.join("idle-OLD2.snap").exists(), "a stale snapshot must be gone");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Switching to direct mode offers the copy-mode drives back, rather than holding them.
    ///
    /// This is the whole of the disk saving, and it is the half that is easy to leave out: the
    /// default flipped to direct, so the two 8 GB drives for the images now loaded are never
    /// written and never read again — and a keep-set that matched on name alone would go on
    /// protecting them for ever, on the platforms where they are a real 8 GB rather than a reflink.
    /// The stamp replaces them and must survive, or every launch cold-boots.
    #[test]
    fn direct_mode_hands_back_the_drives_copy_mode_needed() {
        let dir = std::env::temp_dir().join(format!("ipod-direct-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);

        let keep = Cache {
            snap: dir.join("idle-KEEP.snap"),
            frozen: dir.join("idle-KEEP.frozen"),
            work: dir.join("idle-KEEP.img"),
            stamp: dir.join("idle-KEEP.drive"),
        };
        for p in [&keep.snap, &keep.frozen, &keep.work, &keep.stamp] {
            std::fs::write(p, vec![0u8; 100]).unwrap();
        }

        let (_, offered) = reclaimable(&dir, &keep, false);
        assert!(offered.contains(&keep.frozen), "the frozen drive is dead weight in direct mode");
        assert!(offered.contains(&keep.work), "the working copy is dead weight in direct mode");
        assert!(!offered.contains(&keep.snap), "the snapshot is in use in both modes");
        assert!(!offered.contains(&keep.stamp), "the stamp is what makes the snapshot restorable");

        // And the mirror image, so this asserts a rule rather than a direction.
        let (_, offered) = reclaimable(&dir, &keep, true);
        assert!(offered.contains(&keep.stamp), "the stamp is dead weight in copy mode");
        assert!(!offered.contains(&keep.frozen), "the frozen drive is the pair's other half");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sizes_read_as_sentences() {
        assert_eq!(human(0), "nothing");
        assert_eq!(human(8_000_000_000), "8.0 GB");
        assert_eq!(human(1_600_000_000), "1.6 GB");
    }

    /// A dropped file goes where it belongs, whatever order it arrives in and whatever it is called.
    ///
    /// This is the whole first-run screen in one method: there are no slots to put a file into, so
    /// the only thing that can be wrong is the routing. Both orders are asserted, because "drop the
    /// ROM first" is an instruction the screen deliberately does not give.
    #[test]
    fn dropped_files_route_themselves_in_either_order() {
        let dir = std::env::temp_dir().join(format!("ipod-accept-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let rom = dir.join("anything.bin");
        std::fs::write(&rom, {
            let mut v = vec![0u8; 1024 * 1024];
            v[..4].copy_from_slice(&[0xfe, 0x1f, 0x00, 0xea]);
            v
        })
        .unwrap();
        let drive = dir.join("some-ipod.img");
        std::fs::write(&drive, vec![0u8; 4 * 1024 * 1024]).unwrap();
        let unused = dir.join("built.img");

        for order in [[&rom, &drive], [&drive, &rom]] {
            let mut im = Images::new(&emu::Config::default());
            for p in order {
                im.accept(p, &unused);
            }
            assert_eq!(im.flash, rom.to_string_lossy(), "the ROM went to the ROM");
            assert_eq!(im.disk, drive.to_string_lossy(), "the drive went to the drive");
            assert!(im.rejected.is_none());
        }

        // A file that is none of the three is named rather than swallowed. A drop that appears to
        // do nothing is the same experience as a drop the window never received.
        let junk = dir.join("notes.txt");
        std::fs::write(&junk, b"not an iPod").unwrap();
        let mut im = Images::new(&emu::Config::default());
        let said = im.accept(&junk, &unused);
        assert!(said.contains("notes.txt"), "it says which file: {said}");
        assert!(im.rejected.is_some());
        assert!(im.flash.is_empty() && im.disk.is_empty(), "and it lands in neither row");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Only a change the machine cannot absorb asks for a restart.
    ///
    /// The complaint this exists for: opening the settings rebooted the iPod even when nothing was
    /// touched. Appearance, the readout and the update check all apply to a running machine; the
    /// two files and where the iPod writes do not.
    #[test]
    fn only_the_settings_a_running_machine_cannot_absorb_ask_for_a_restart() {
        let before = Cold {
            flash: "/a/rom.bin".into(),
            disk: "/a/disk.img".into(),
            work_on_copy: false,
        };
        assert!(before.differences(&before).is_empty(), "looking is not changing");

        let other_drive = Cold { disk: "/a/other.img".into(), ..before.clone() };
        assert_eq!(before.differences(&other_drive), vec!["the drive"]);

        let copy = Cold { work_on_copy: true, ..before.clone() };
        assert_eq!(before.differences(&copy), vec!["where the iPod writes"]);

        // Named in full when more than one moved, because "something changed" is not a sentence
        // anybody can act on.
        let both = Cold { flash: "/a/other.bin".into(), disk: "/a/other.img".into(), work_on_copy: true };
        assert_eq!(
            before.differences(&both),
            vec!["the boot ROM", "the drive", "where the iPod writes"]
        );
    }

    #[test]
    fn a_missing_image_produces_an_actionable_message() {
        let cfg = config(&["--flash=/nope/a.bin".into(), "--disk=/nope/b.img".into()], &Settings::default())
            .unwrap();
        let m = missing_images(&cfg).expect("both are missing");
        assert!(m.contains("/nope/a.bin") && m.contains("/nope/b.img"));
        assert!(m.contains("README"), "it has to point somewhere");
        assert!(m.contains("--check-images"), "and offer the check that needs no window");
    }

    /// Lay a screen out at a given size, with no window, no GPU and no eframe, and report how tall
    /// its content came out.
    ///
    /// egui is layout and tessellation; a renderer is what turns the result into pixels, and this
    /// question is answered before that. So `Context::run` does the whole thing in-process, which is
    /// why a rule about the size of the UI can be a test instead of a habit.
    ///
    /// The screens are called directly rather than through `eframe::App::ui`, because that wants an
    /// `eframe::Frame` and nothing outside eframe can make one. That is the only reason the three
    /// are separate methods rather than arms of one — worth stating, so nobody "tidies" it away and
    /// takes the test with it.
    /// What the two file rows hold when a page is measured.
    ///
    /// The third one is the point. A page whose files are simply *chosen* is not the tallest that
    /// page ever gets — a file the emulator can parse and reject prints the sentence saying why,
    /// and those sentences are several lines long by design, because "this is a 2 MiB dump; the
    /// 5G ROM is 1 MiB" is the whole value of the verdict. Measuring only the tidy state would set
    /// the window's minimum from a page nobody in trouble ever sees.
    #[derive(Clone, Copy, PartialEq, Debug)]
    enum Files {
        /// Nothing chosen yet — the first run before anything is dropped.
        None,
        /// Two paths that parse. Verdicts are absent because the files are not really there.
        Chosen,
        /// Two real files that the emulator reads, understands and refuses, each with its reason.
        Rejected,
    }

    /// Two files that will parse and be turned down, so the verdict text is real text.
    fn bad_pair(dir: &Path) -> (PathBuf, PathBuf) {
        let _ = std::fs::create_dir_all(dir);
        // Exactly 1 MiB, so it is classified as a boot ROM, with a word 0 that is not an ARM
        // branch — `inspect::flash`'s longest message.
        let rom = dir.join("not-really-a-rom.bin");
        std::fs::write(&rom, vec![0u8; eapp_loader::inspect::NOR_LEN as usize]).unwrap();
        // Large enough to be classified as a drive, with no MBR signature.
        let drive = dir.join("not-really-a-drive.img");
        std::fs::write(&drive, vec![0u8; 4 * 1024 * 1024]).unwrap();
        (rom, drive)
    }

    fn lay_out(screen: Screen, w: f32, h: f32, files: Files) -> f32 {
        let ctx = egui::Context::default();
        let mut used = 0.0;
        let mut app: Option<App> = None;
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(w, h))),
            ..Default::default()
        };
        // Twice: egui sizes some widgets from what they measured on the previous frame, so a
        // single pass reports a page that has not settled. The second pass is the honest one.
        for _ in 0..2 {
            used = 0.0;
            let mut out = ctx.run_ui(input.clone(), |ui| {
                let ctx = &ui.ctx().clone();
                let app = app.get_or_insert_with(|| {
                    let mut cfg = emu::Config::default();
                    match files {
                        Files::None => {}
                        Files::Chosen => {
                            cfg.flash = PathBuf::from("/some/where/internal_rom_000000-0FFFFF.bin");
                            cfg.disk = PathBuf::from("/some/where/an-ipod-drive.img");
                        }
                        Files::Rejected => {
                            let dir = std::env::temp_dir()
                                .join(format!("ipod-layout-{}", std::process::id()));
                            let (rom, drive) = bad_pair(&dir);
                            cfg.flash = rom;
                            cfg.disk = drive;
                        }
                    }
                    let mut a = App::new(ctx, cfg, Settings::default(), String::new());
                    a.screen = screen;
                    if files == Files::Rejected {
                        // Everything a page can grow at once: both fact lists open, and the pair
                        // warning under them. Injected rather than provoked, because provoking it
                        // needs a valid ROM fixture and what is being measured here is the height
                        // of the widgets, not the parser that decides to show them.
                        a.images.flash_facts = vec![
                            ("Images", "disk · diag · scan · logo · vmcs".into()),
                            ("Serial", "7Q7411K2VQK".into()),
                            ("FireWire GUID", "000A2700ABCD1234".into()),
                            ("Build", "iPod boot loader build 1.2.3 (fw 20.6.3)".into()),
                        ];
                        a.images.disk_facts = vec![
                            ("Size", "8.6 GB".into()),
                            ("Firmware images", "osos · rsrc · aupd".into()),
                            ("Operating system", "present".into()),
                            ("Flash updater", "armed — this drive boots the updater, not the OS".into()),
                            ("Updater family", "24".into()),
                        ];
                        a.images.mismatch = inspect::family_mismatch(IPOD_VIDEO.short, 20, Some(24));
                        assert!(a.images.mismatch.is_some(), "the tallest case needs its warning");
                    }
                    if screen == Screen::Settings {
                        // As `open_settings` leaves it, plus a change to restart for — the tallest
                        // this page ever gets.
                        a.cold_at_open = Some(Cold {
                            flash: "/somewhere/else.bin".into(),
                            disk: "/somewhere/else.img".into(),
                            work_on_copy: !a.cfg.work_on_copy,
                        });
                    }
                    a
                });
                match screen {
                    Screen::FirstRun => app.first_run(ui),
                    Screen::Settings => app.settings_screen(ui),
                    Screen::Help => app.help_screen(ui),
                    Screen::Details => app.details_screen(ui),
                    Screen::Device => {}
                }
                used = ui.min_rect().height();
            });
            // There is no renderer here to upload them to, and epaint panics rather than let a
            // texture delta be dropped unnoticed — which is the right call in a real program and
            // has to be answered explicitly in one that only wants the layout.
            out.textures_delta.clear();
        }
        used
    }

    /// **Nothing in this program scrolls but the click wheel**, and this is what keeps it true.
    ///
    /// Every screen is laid out at exactly the smallest window the program will open. If one needs
    /// more room than that, this fails — naming the screen and the shortfall — rather than a
    /// scrollbar appearing on the machine of somebody who is not us.
    ///
    /// Measured before it was written, which is how `MIN_H` got its value rather than the other way
    /// round: at the previous 520 px minimum the first run wanted 550, the settings 590 and the
    /// help 484, so all three scrolled and two of them scrolled at sizes people use.
    #[test]
    fn every_screen_fits_the_smallest_window() {
        for (screen, files) in [
            (Screen::FirstRun, Files::None),
            (Screen::FirstRun, Files::Chosen),
            (Screen::FirstRun, Files::Rejected),
            (Screen::Settings, Files::Chosen),
            (Screen::Settings, Files::Rejected),
            (Screen::Help, Files::None),
            // The details page carries every fact at full size; it is the page that exists
            // because they did not fit anywhere else, so it is the one that must be checked.
            (Screen::Details, Files::Rejected),
        ] {
            let used = lay_out(screen, MIN_W, MIN_H, files);
            assert!(
                used <= MIN_H,
                "{screen:?} with {files:?} files wants {used:.0} px at the {MIN_H:.0} px minimum \
                 — {:.0} px too tall. Either it loses a section or MIN_H goes up; it does not get \
                 a scrollbar.",
                used - MIN_H
            );
        }
        let _ = std::fs::remove_dir_all(
            std::env::temp_dir().join(format!("ipod-layout-{}", std::process::id())),
        );
    }

    /// The gate can fail, and it fails *for the right reason*, which is the only thing that makes
    /// it passing mean anything.
    ///
    /// A measurement that returned the same number whatever it was given would satisfy
    /// `every_screen_fits_the_smallest_window` completely and prove nothing — it would report
    /// "fits" for a screen twice the height of the window with equal confidence. So this asks it
    /// to tell apart things it must be able to tell apart:
    ///
    /// - a **short page** from a **long one**, which is the difference the gate exists to notice;
    /// - the settings **with a restart to offer** from the same page **without one**, which is the
    ///   smallest real content change on the tallest page;
    /// - and a window too small for any of them, which must come back over budget rather than
    ///   clipped to the window it was given.
    ///
    /// The third is the one that would have been the trap: if the measurement returned the *visible*
    /// height instead of the *wanted* height, every page would fit every window by definition.
    #[test]
    fn the_layout_measurement_tracks_the_content_and_not_the_window() {
        let help = lay_out(Screen::Help, MIN_W, MIN_H, Files::None);
        let settings = lay_out(Screen::Settings, MIN_W, MIN_H, Files::Chosen);
        assert!(help > 0.0, "a height of zero passes every other assertion for the wrong reason");
        assert!(
            settings > help + 40.0,
            "the settings are a much longer page than the help; measured {settings:.0} vs {help:.0}"
        );

        // The same page, one banner apart. `lay_out` gives Settings a restart to offer; this is
        // that page without one.
        let quiet = {
            let ctx = egui::Context::default();
            let mut cfg = emu::Config::default();
            cfg.flash = PathBuf::from("/some/where/internal_rom_000000-0FFFFF.bin");
            cfg.disk = PathBuf::from("/some/where/an-ipod-drive.img");
            let mut used = 0.0;
            let mut app: Option<App> = None;
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(MIN_W, MIN_H))),
                ..Default::default()
            };
            for _ in 0..2 {
                let mut out = ctx.run_ui(input.clone(), |ui| {
                    let app = app.get_or_insert_with(|| {
                        let mut a =
                            App::new(&ui.ctx().clone(), cfg.clone(), Settings::default(), String::new());
                        a.screen = Screen::Settings;
                        a.cold_at_open = Some(a.cold());
                        a
                    });
                    app.settings_screen(ui);
                    used = ui.min_rect().height();
                });
                out.textures_delta.clear();
            }
            used
        };
        assert!(
            settings > quiet,
            "offering a restart makes the page taller; measured {settings:.0} with, {quiet:.0} without"
        );

        // Squeezed into a window none of them fits, the answer must be what the page *wanted*.
        let squeezed = lay_out(Screen::Settings, MIN_W, 200.0, Files::Chosen);
        assert!(
            squeezed > 200.0,
            "measured {squeezed:.0} in a 200 px window — that is the window, not the content, and \
             a gate that measures the window can never fail"
        );
    }

    /// A device drawn anywhere is a device that boots. `ROADMAP.md` Ⅳ.
    #[test]
    fn nothing_offers_a_model_it_cannot_run() {
        assert!(MODELS.iter().any(|d| d.boots), "at least one, or the program has nothing to do");
        for d in bootable() {
            assert!(d.boots);
            assert!(!d.name.is_empty() && !d.model_no.is_empty());
            assert!(d.rom_len > 0, "{}: a model whose ROM length is unknown cannot be identified", d.name);
        }
        // The Video's ROM is 1 MiB, and `inspect` leads its verdict with that length. The two
        // numbers being the same number is what makes identifying a dump by model possible at all.
        assert_eq!(IPOD_VIDEO.rom_len, eapp_loader::inspect::NOR_LEN);
    }
}

/// `(body, wheel, ring text, glass)` for a chassis colour.
///
/// A free function because **two** things draw an iPod — the running device and the at-rest one on
/// the first-run screen — and the at-rest one had its own hardcoded white pair. So a person who
/// chose black met a white iPod on the setup screen and a black one afterwards.
fn palette_for(chassis: Colour) -> (Color32, Color32, Color32, Color32) {
    if chassis == Colour::U2 {
        (
            Color32::from_rgb(0x24, 0x25, 0x27),
            // Apple's "Product Red" wheel. Dark enough that the white ring text still reads.
            Color32::from_rgb(0xb8, 0x1c, 0x22),
            Color32::from_rgb(0xf0, 0xe2, 0xe2),
            Color32::from_rgb(0x0a, 0x0a, 0x0b),
        )
    } else if chassis == Colour::Black {
        (
            Color32::from_rgb(0x24, 0x25, 0x27),
            Color32::from_rgb(0x33, 0x34, 0x36),
            Color32::from_rgb(0x9a, 0x9b, 0x9d),
            Color32::from_rgb(0x0a, 0x0a, 0x0b),
        )
    } else {
        (
            Color32::from_rgb(0xf3, 0xf3, 0xf1),
            Color32::from_rgb(0xe6, 0xe6, 0xe3),
            Color32::from_rgb(0x87, 0x88, 0x86),
            Color32::from_rgb(0x14, 0x14, 0x15),
        )
    }
}
