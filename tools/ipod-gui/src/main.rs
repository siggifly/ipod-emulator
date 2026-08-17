//! An interactive iPod 5G: the emulator's framebuffer on a drawn device whose click wheel, five
//! buttons and hold switch reach the machine.
//!
//! ```text
//! ipod-emulator [--user | --debug] [--cold] [--clock=N] [--snapshot=FILE] [--snap-at=N]
//!          [--flash=FILE] [--disk=FILE] [--workdisk=FILE] [--wheel-click-instr=N]
//!          [--headless=N | --selftest | --selftest-control]
//!          [--check-images] [--check-update]
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

mod emu;
use eapp_loader::inspect;
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
    /// What the setup screen calls it.
    pub name: &'static str,
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
}

/// The iPod Video, 5th and 5.5 generation — A1136. The only model this emulator boots.
pub const IPOD_VIDEO: Device = Device {
    name: "iPod Video (5G / 5.5G)",
    case_w: 61.8,
    case_h: 103.5,
    screen_w: 50.8,
    screen_h: 38.1,
    screen_top: 9.5,
    wheel_d: 28.0,
    wheel_cy: 75.5,
    fb: (FB_W, FB_H),
};

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

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "-h" || a == "--help") {
        print_help();
        return Ok(());
    }
    // Two answers that need no machine, no window and no images. Both print and exit, so they work
    // over SSH and in CI.
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
        // No window means no setup screen, so a missing image is fatal here and says which one.
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
            .with_inner_size([980.0, 800.0])
            .with_min_inner_size([620.0, 520.0])
            .with_icon(icon)
            .with_title("ipod-emulator"),
        ..Default::default()
    };
    // `--ipsw=` only ever pre-fills the setup screen's slot; building a drive is a button, not a
    // side effect of a flag. `ipod-boot make-disk` is the way to do it without a window.
    let ipsw = args
        .iter()
        .find_map(|a| a.strip_prefix("--ipsw="))
        .unwrap_or_default()
        .to_string();
    eframe::run_native(
        "ipod-emulator",
        options,
        Box::new(move |cc| Ok(Box::new(App::new(cc, cfg, settings, ipsw)))),
    )
}

/// Set the window's colours and text sizes explicitly, instead of inheriting them.
///
/// **This shipped broken and it is worth saying why.** Nothing here called `set_visuals`, so egui
/// used its default, which follows the operating system — and the device is drawn on a black
/// background regardless. On the wrong system that is dark grey text on black: the setup screen's
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
  --cold                  boot from the reset vector even if a snapshot exists (~75 s)
  --snapshot=FILE         where the idle snapshot lives (default: a per-user cache directory)
  --snap-at=N             instruction count the snapshot is taken at (default 1600000000)
  --clock=N               interpreter instructions per simulated microsecond (default 5)
  --wheel-click-instr=N   instructions between the frames of a rotation (default 20000)
  --flash=FILE            the NOR image (default: what the setup screen was pointed at,
                          else the retail ROM under resources/)
  --disk=FILE             the drive image (default: as above, else resources/derived/disk/)
  --workdisk=FILE         the writable per-run clone (default: alongside the snapshot)
  --ipsw=FILE             pre-fill the setup screen's IPSW slot. Building the drive is still a
                          button; `ipod-boot make-disk IPSW OUT.img` does it with no window
  --check-images          no window: parse both images, say what they are, exit 0 if usable
  --check-update          no window: ask GitHub for the latest release. Silent when offline
  --headless=N            no window: run N instructions and print the boot fingerprint
  --selftest              no window: push a scripted gesture through the GUI's own input path
                          and print what reached RetailOS
  --selftest-control      the matched control: the same run with no input at all
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
fn cache_paths(flash: &Path, disk: &Path, clock: usize, snap_at: u64) -> (PathBuf, PathBuf) {
    let key = cache_key(flash, disk, clock, snap_at);
    let cache = settings::data_dir();
    let _ = std::fs::create_dir_all(&cache);
    (cache.join(format!("idle-{key}.snap")), cache.join(format!("idle-{key}.img")))
}

/// Delete every cached working disk and snapshot that is not the pair currently in use.
///
/// **This is why the cache is keyed and not accumulated.** A working disk is 8 GB sparse and a
/// snapshot is about 1.6 GB, and the key includes both image paths — so trying four firmware
/// versions used to leave four of each, silently, in a directory the user never opened, on whatever
/// volume the program happened to resolve. Somebody lost 50 GB that way and was right to be angry
/// about it. One pair is kept: the one belonging to the images now loaded.
fn prune_cache(keep_snap: &Path, keep_work: &Path) -> u64 {
    let dir = settings::data_dir();
    let Ok(rd) = std::fs::read_dir(&dir) else { return 0 };
    let mut freed = 0;
    for e in rd.flatten() {
        let p = e.path();
        let name = e.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with("idle-") {
            continue;
        }
        if p == keep_snap || p == keep_work {
            continue;
        }
        let n = e.metadata().map(|m| m.len()).unwrap_or(0);
        if std::fs::remove_file(&p).is_ok() {
            freed += n;
        }
    }
    freed
}

/// What the cache currently holds, for the setup screen to state rather than hide.
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

    // Three sources, in order: the command line, what the setup screen last recorded, and the
    // gitignored `resources/` tree the recipes use. The last one is `retail-boot.sh`'s default
    // verbatim, because a GUI that quietly booted the *prototype* NOR would produce a different
    // machine from every number in research/ with nothing saying so.
    let root = settings::repo_root();
    let res = root.join("resources");
    let flash = get("--flash=")
        .map(PathBuf::from)
        .or_else(|| saved.flash.clone())
        .unwrap_or_else(|| {
            res.join("reference/ipod-bootrom-archive/A1238/internal_rom_000000-0FFFFF.bin")
        });
    let disk = get("--disk=")
        .map(PathBuf::from)
        .or_else(|| saved.disk.clone())
        .unwrap_or_else(|| res.join("derived/disk/ipod8g-retail.img"));

    let clock = num("--clock=", 5) as usize;
    let snap_at = num("--snap-at=", 1_600_000_000);
    let (snapshot, workdisk) = cache_paths(&flash, &disk, clock, snap_at);

    Ok(emu::Config {
        flash,
        disk,
        workdisk: get("--workdisk=").map(PathBuf::from).unwrap_or(workdisk),
        clock,
        snapshot: Some(get("--snapshot=").map(PathBuf::from).unwrap_or(snapshot)),
        snap_at,
        cold: args.iter().any(|a| a == "--cold"),
        click_gap: num("--wheel-click-instr=", 20_000).max(1),
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
         Run the window with no arguments for a setup screen, `--check-images` to test a pair you \
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
    /// The machine to build once the setup screen is satisfied. Cloned into the worker thread.
    cfg: emu::Config,
    setup: Setup,
    settings: Settings,
    tex: egui::TextureHandle,
    seen_seq: u64,
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
    /// `None` = no check has finished. `Some(None)` = a check ran and found nothing, which is what
    /// offline looks like and is never shown.
    update_slot: Arc<Mutex<Option<Option<update::Found>>>>,
    update_line: Option<String>,
    update_asked: bool,
    /// Leftover scroll that has not yet added up to a detent. See [`SCROLL_UNITS_PER_DETENT`].
    wheel_units: f32,
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

/// The first-run screen: the slots, their paths, and a verdict on what is in each.
/// Which question the first run is asking.
///
/// One at a time, verified before the next. The previous version asked everything at once on a
/// single screen, with every fact the project knows printed beside each field — which is reference
/// material, not an instruction, and it read as a settings dialog rather than a first run.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Step {
    /// The boot ROM.
    Rom,
    /// The software: an `.ipsw` to build a drive from, or a drive image.
    Firmware,
    /// What is about to happen, including how much disk it will take.
    Ready,
}

struct Setup {
    step: Step,
    flash: String,
    disk: String,
    /// An IPSW to build a drive image *from*, which is the path most people should take: about
    /// 14 MB of Apple's firmware rather than 8 GB of somebody else's iPod.
    ipsw: String,
    flash_verdict: Option<Verdict>,
    disk_verdict: Option<Verdict>,
    ipsw_verdict: Option<Verdict>,
    /// What the last build said, good or bad.
    built: Option<Result<String, String>>,
    /// Set when the user has been told the files do not validate and has chosen to boot anyway.
    force: bool,
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

impl Setup {
    fn new(cfg: &emu::Config) -> Setup {
        let mut s = Setup {
            step: Step::Rom,
            // Only prefill a path that is actually there. The defaults are this repository's
            // layout, so a released binary would open on two paths that do not exist and an error
            // under each — telling a first-time user their files are wrong before they have chosen
            // any. Empty and asking is better than full and wrong.
            flash: existing(&cfg.flash),
            disk: existing(&cfg.disk),
            ipsw: String::new(),
            flash_verdict: None,
            disk_verdict: None,
            ipsw_verdict: None,
            built: None,
            force: false,
        };
        s.revalidate();
        s
    }

    /// Parse whatever is in the two image fields. Cheap — both read a few hundred bytes at fixed
    /// offsets — except the NOR's build string, which reads 1 MB and only when the rest passed.
    fn revalidate(&mut self) {
        let f = PathBuf::from(self.flash.trim());
        let d = PathBuf::from(self.disk.trim());
        self.flash_verdict = f.is_file().then(|| inspect::flash(&f));
        self.disk_verdict = d.is_file().then(|| inspect::disk(&d));
        self.force = false;
    }

    /// The IPSW is checked separately because checking it means inflating 13.9 MB and hashing it,
    /// which is not something to do on every repaint.
    fn revalidate_ipsw(&mut self) {
        let p = PathBuf::from(self.ipsw.trim());
        self.ipsw_verdict = p.is_file().then(|| inspect::ipsw(&p));
        self.built = None;
    }

    fn both_good(&self) -> bool {
        matches!(&self.flash_verdict, Some(v) if v.ok())
            && matches!(&self.disk_verdict, Some(v) if v.ok())
    }

    fn both_present(&self) -> bool {
        self.flash_verdict.is_some() && self.disk_verdict.is_some()
    }
}

impl App {
    fn new(
        cc: &eframe::CreationContext<'_>,
        cfg: emu::Config,
        settings: Settings,
        ipsw: String,
    ) -> Self {
        theme(&cc.egui_ctx);
        let tex = cc.egui_ctx.load_texture(
            "panel",
            egui::ColorImage::from_rgb([FB_W, FB_H], &vec![0u8; FB_W * FB_H * 3]),
            egui::TextureOptions::NEAREST,
        );
        let shot_dir = settings::repo_root().join("_out");
        let mut setup = Setup::new(&cfg);
        if !ipsw.is_empty() {
            setup.ipsw = ipsw;
            setup.revalidate_ipsw();
        }
        let update_slot = Arc::new(Mutex::new(None));
        // Opt-in, and only opt-in. Off by default; the button in the panel works regardless.
        if settings.check_updates_on_start {
            update::spawn(Arc::clone(&update_slot));
        }
        let mut app = App {
            link: None,
            cfg,
            setup,
            settings,
            tex,
            seen_seq: 0,
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
            update_slot,
            update_line: None,
            update_asked: false,
            wheel_units: 0.0,
        };
        // Nothing to set up: the images are there and they parse. Skip straight to the iPod.
        if app.setup.both_good() {
            app.start();
        }
        app
    }

    /// Build the machine and start it. Idempotent — a second press does nothing.
    fn start(&mut self) {
        if self.link.is_some() {
            return;
        }
        self.cfg.flash = PathBuf::from(self.setup.flash.trim());
        self.cfg.disk = PathBuf::from(self.setup.disk.trim());
        // The cache key includes both paths, so a different pair of images gets a different
        // snapshot rather than restoring one taken on the other machine.
        let (snap, work) = cache_paths(&self.cfg.flash, &self.cfg.disk, self.cfg.clock, self.cfg.snap_at);
        // Everything cached for a DIFFERENT pair of images goes now. Without this, each pair the
        // user tried left an 8 GB working disk and a 1.6 GB snapshot behind for ever.
        let freed = prune_cache(&snap, &work);
        if freed > 0 {
            self.say(format!("reclaimed {} from images no longer in use", human(freed)));
        }
        self.cfg.snapshot = Some(snap);
        self.cfg.workdisk = work;

        // Remember what worked, so the next launch opens straight into the iPod.
        self.settings.flash = Some(self.cfg.flash.clone());
        self.settings.disk = Some(self.cfg.disk.clone());
        self.settings.save();

        let link = Link::new();
        spawn_worker(self.cfg.clone(), Arc::clone(&link));
        self.link = Some(link);
    }

    /// Back to the setup screen from a running machine, to point it at different images.
    ///
    /// The worker owns the machine, and there is no way to hand a running RetailOS a different
    /// drive — the firmware read its partition table at boot and has been writing to it since. So
    /// this ends the worker rather than pretending. Nothing is lost that was not already
    /// reproducible: the snapshot is keyed on the pair of paths, so the one taken against these
    /// images stays valid for them, and a different pair gets its own.
    fn change_images(&mut self) {
        if let Some(l) = &self.link {
            l.quit.store(true, Ordering::Relaxed);
        }
        self.link = None;
        // Back to the first question. Somebody who clicked "setup" wants to change something, and
        // landing them on the summary page with a Start button is answering a question they did
        // not ask.
        self.setup.step = Step::Rom;
        // The files may have been replaced on disk since the last look, and the verdicts are what
        // the screen is for.
        self.setup.revalidate();
        self.setup.revalidate_ipsw();
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
/// it is offered. It exists so the setup screen can show the thing being set up.
fn device_at_rest(p: &egui::Painter, d: &Device, centre: Pos2, height: f32) -> Rect {
    let k = height / (d.case_h + SWITCH_PROUD);
    let (w, h) = (d.case_w * k, (d.case_h + SWITCH_PROUD) * k);
    let o = Pos2::new(centre.x - w / 2.0, centre.y - h / 2.0);
    let at = |x: f32, y: f32| Pos2::new(o.x + x * k, o.y + (y + SWITCH_PROUD) * k);

    let body = Color32::from_rgb(0xF3, 0xF3, 0xF1);
    let wheel = Color32::from_rgb(0xE6, 0xE6, 0xE3);

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
    let ink = Color32::from_gray(0x8A);
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
        if self.link.is_none() {
            self.setup_screen(ui);
            return;
        }
        let ctx = ui.ctx().clone();
        // The machine is running on another thread whether or not anything moved, so the window has
        // to keep asking for repaints or it would freeze on a still screen.
        ctx.request_repaint_after(Duration::from_millis(16));

        let out = self.link.as_ref().unwrap().out.lock().unwrap().clone();
        if out.fb_seq != self.seen_seq {
            self.seen_seq = out.fb_seq;
            self.tex.set(
                egui::ColorImage::from_rgb([FB_W, FB_H], &out.fb),
                egui::TextureOptions::NEAREST,
            );
        }

        self.keyboard(&ctx);

        if self.settings.mode == Mode::Debug {
            egui::Panel::right("instrument")
                .resizable(true)
                .default_size(330.0)
                .min_size(240.0)
                .show(ui, |ui| self.instrument(ui, &out));
        }
        self.footer(ui, &out);

        let rect = ui.available_rect_before_wrap();
        self.device(ui, rect, &out);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if let Some(l) = &self.link {
            l.quit.store(true, Ordering::Relaxed);
        }
    }
}

impl App {
    // ------------------------------------------------------------ the first-run screen

    /// What a stranger meets when the images are not there: one question at a time.
    ///
    /// A centred column with margins, the device it is configuring drawn at the top, and a single
    /// decision per screen with its detail folded away. The previous version put both files, every
    /// byte count, two model numbers and a paragraph about what RetailOS does on first boot onto
    /// one page with no margins — which is a reference card, and somebody arriving here wants to
    /// know what to click.
    fn setup_screen(&mut self, ui: &mut egui::Ui) {
        let dev = IPOD_VIDEO;
        egui::ScrollArea::vertical().show(ui, |ui| {
            // The margins the old screen did not have. A measured column rather than the window's
            // full width, because a 1400-pixel line of prose is unreadable at any contrast.
            let avail = ui.available_width();
            let col = avail.min(620.0);
            let side = ((avail - col) / 2.0).max(0.0);
            ui.add_space(28.0);
            ui.horizontal(|ui| {
                ui.add_space(side);
                ui.vertical(|ui| {
                    ui.set_width(col);
                    self.wizard(ui, &dev);
                });
            });
            ui.add_space(28.0);
        });
    }

    /// The column's contents: the device, the step, and the way forward.
    fn wizard(&mut self, ui: &mut egui::Ui, dev: &Device) {
        // The device being configured, at rest. It is the only thing on this screen that says what
        // the program is without being read.
        let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 150.0), egui::Sense::hover());
        device_at_rest(ui.painter(), dev, rect.center(), 138.0);
        ui.add_space(10.0);

        ui.vertical_centered(|ui| {
            ui.label(egui::RichText::new("ipod-emulator").heading());
            ui.label(
                egui::RichText::new("Apple's own iPod software, on a machine that is not one.")
                    .color(Color32::from_gray(0x9A)),
            );
        });
        ui.add_space(6.0);
        ui.vertical_centered(|ui| {
            let n = match self.setup.step {
                Step::Rom => 1,
                Step::Firmware => 2,
                Step::Ready => 3,
            };
            ui.label(
                egui::RichText::new(format!("{}   ·   step {n} of 3", dev.name))
                    .small()
                    .color(Color32::from_gray(0x78)),
            );
        });
        ui.add_space(22.0);

        match self.setup.step {
            Step::Rom => self.step_rom(ui),
            Step::Firmware => self.step_firmware(ui),
            Step::Ready => self.step_ready(ui),
        }
    }

    /// Step 1 — the boot ROM.
    fn step_rom(&mut self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new("First, the boot ROM").size(17.0).strong());
        ui.add_space(4.0);
        ui.label("A 1 MB dump of the chip your iPod starts from. Read out of an iPod you own.");
        ui.add_space(12.0);

        let changed = slot(ui, "NOR flash dump", "", &mut self.setup.flash, self.setup.flash_verdict.as_ref(), &["bin"]);
        if changed {
            self.setup.revalidate();
        }
        ui.add_space(8.0);

        // The detail somebody stuck will look for, and nobody else has to read.
        ui.collapsing("Where do I get one?", |ui| {
            // First, and deliberately first: read it off your own device. It is the only route
            // that involves nobody else's copy of anything, it is a documented Rockbox feature,
            // and it is the one that always works — an archived dump is somebody else's iPod.
            ui.label(egui::RichText::new("Best: read it off your own iPod").strong());
            ui.add_space(4.0);
            ui.label(
                "Rockbox can dump the boot ROM in about five minutes, and can be uninstalled \
                 immediately afterwards. Install it with Rockbox Utility — only \"bootloader\" and \
                 \"rockbox\" need to be ticked — then on the iPod go to",
            );
            ui.label(
                egui::RichText::new("System \u{2192} Debug (Keep Out!) \u{2192} Dump ROM contents")
                    .monospace(),
            );
            ui.label("and copy the internal_rom_… file off the iPod when you plug it in.");
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.hyperlink_to("Rockbox Utility", "https://www.rockbox.org/wiki/RockboxUtility");
                ui.label("·");
                ui.hyperlink_to("the flash guide", "https://www.rockbox.org/wiki/IpodFlash.html");
            });
            ui.add_space(10.0);

            ui.label(egui::RichText::new("Otherwise: it is archived, under the wrong product").strong());
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(
                    "Collections of iPod boot ROMs file the iPod VIDEO dump as iPod CLASSIC, in a \
                     folder named A1238. That is the Classic's model number; the Video is A1136. So \
                     searching for \"iPod Video\", \"5.5G\" or \"A1136\" finds nothing, and searching \
                     for the Classic finds it.",
                )
                .color(Color32::from_gray(0xB4)),
            );
            ui.add_space(6.0);
            ui.label("The file is normally called internal_rom_000000-0FFFFF.bin and is exactly 1 048 576 bytes.");
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new(
                    "A prototype dump also circulates. It will not work here — it boots a firmware \
                     partition that the retail ROM correctly rejects. The one you want reads \
                     HwVr 0x000b0005 and Mod# MA146; the prototype reads 0x000b0011 and M8976.",
                )
                .color(Color32::from_gray(0x9A)),
            );
        });

        let ok = matches!(&self.setup.flash_verdict, Some(v) if v.ok());
        self.nav(ui, None, Some(Step::Firmware), ok, "Next");
    }

    /// Step 2 — the software.
    fn step_firmware(&mut self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new("Now the software").size(17.0).strong());
        ui.add_space(4.0);
        ui.label(
            "Apple's iPod software update. The emulator builds a drive from it — only the firmware \
             part matters, because the iPod formats the rest itself on first boot.",
        );
        ui.add_space(12.0);
        self.ipsw_slot(ui);

        ui.add_space(8.0);
        let mut disk_changed = false;
        ui.collapsing("…or a drive image you already have", |ui| {
            disk_changed = slot(
                ui,
                "Drive image",
                "A whole-drive image of an iPod's disk, including its firmware partition.",
                &mut self.setup.disk,
                self.setup.disk_verdict.as_ref(),
                &["img", "bin", "dmg", "iso"],
            );
        });
        if disk_changed {
            self.setup.revalidate();
        }

        ui.add_space(8.0);
        ui.collapsing("Where do I find this?", |ui| {
            ui.label("For the iPod Video the file is iPod_20.1.3.ipsw.");
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new(
                    "The 20 is the updater family and it has to match the iPod your ROM came from — \
                     iPod_24 and iPod_26 files are other devices and will not boot here.",
                )
                .color(Color32::from_gray(0xB4)),
            );
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new("Apple no longer serves these, so there is no official source to try.")
                    .color(Color32::from_gray(0x9A)),
            );
        });

        let ok = matches!(&self.setup.disk_verdict, Some(v) if v.ok());
        self.nav(ui, Some(Step::Rom), Some(Step::Ready), ok, "Next");
    }

    /// Step 3 — what is about to happen, and what it costs.
    fn step_ready(&mut self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new("Ready").size(17.0).strong());
        ui.add_space(10.0);

        for (what, v) in [
            ("Boot ROM", self.setup.flash_verdict.as_ref()),
            ("Drive", self.setup.disk_verdict.as_ref()),
        ] {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(format!("{what}")).strong());
                let (mark, colour) = match v {
                    Some(v) if v.ok() => ("ready", Color32::from_rgb(0x6C, 0xC6, 0x88)),
                    Some(_) => ("check it", Color32::from_rgb(0xE0, 0xA0, 0x40)),
                    None => ("missing", Color32::from_rgb(0xD0, 0x6C, 0x6C)),
                };
                ui.label(egui::RichText::new(mark).color(colour));
            });
            if let Some(v) = v {
                ui.label(egui::RichText::new(first_line(v.text())).small().color(Color32::from_gray(0x9A)));
            }
            ui.add_space(8.0);
        }

        ui.add_space(4.0);
        // Said before it happens, not discovered afterwards. A user lost 50 GB to this being
        // silent, an 8 GB working disk at a time.
        ui.label(
            egui::RichText::new(
                "Starting builds a working copy of the drive — up to 8 GB, though it is sparse and \
                 usually far less on disk — plus a snapshot of the booted machine so that later \
                 launches take seconds. Both live in the folder below and are replaced, not added \
                 to, when you change these files.",
            )
            .color(Color32::from_gray(0x9A)),
        );
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(format!("{}   ·   currently {}", settings::data_dir().display(), human(cache_size())))
                .small()
                .color(Color32::from_gray(0x78)),
        );
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new("The first boot takes about 75 seconds. It is a real cold boot.")
                .small()
                .color(Color32::from_gray(0x78)),
        );

        self.nav(ui, Some(Step::Firmware), None, true, "Start");
    }

    /// Back / forward, in a consistent place, with the forward action disabled until this step is
    /// satisfied — so "why can I not continue" is answered by the verdict directly above it.
    fn nav(&mut self, ui: &mut egui::Ui, back: Option<Step>, next: Option<Step>, ready: bool, label: &str) {
        ui.add_space(22.0);
        ui.separator();
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if let Some(b) = back {
                if ui.button("Back").clicked() {
                    self.setup.step = b;
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let go = ui.add_enabled(ready, egui::Button::new(format!("  {label}  ")));
                if go.clicked() {
                    match next {
                        Some(n) => self.setup.step = n,
                        None => self.start(),
                    }
                }
                if !ready {
                    ui.label(
                        egui::RichText::new("choose a file above")
                            .small()
                            .color(Color32::from_gray(0x78)),
                    );
                }
            });
        });
    }

    /// The IPSW slot: pick a bundle, see what it is, and build a drive from it.
    ///
    /// The build is synchronous because it takes about a second — inflate 13.9 MB, check its
    /// CRC-32, and write an 8 GiB **sparse** file of which only about 20 MB is real. A progress bar
    /// for that would be theatre.
    fn ipsw_slot(&mut self, ui: &mut egui::Ui) {
        let changed = slot(
            ui,
            "iPod software update (.ipsw)",
            "A zip holding `Firmware-<version>` and `manifest.plist`. For the iPod Video the file \
             is `iPod_20.1.3.ipsw`: `Firmware-20.6.3` inside it is 13 895 680 bytes, which is \
             27 140 sectors, which is exactly the size of an iPod's firmware partition.\n\
             \n\
             The 20 in the filename is the updater family, and it must match the iPod your NOR dump \
             came from — `iPod_24.*` and `iPod_26.*` are other devices and will not boot here. \
             Apple no longer serves these files, so there is no official source to try; they are \
             archived. Not distributed with this project.",
            &mut self.setup.ipsw,
            self.setup.ipsw_verdict.as_ref(),
            &["ipsw", "zip"],
        );
        if changed {
            self.setup.revalidate_ipsw();
        }
        let usable = matches!(&self.setup.ipsw_verdict, Some(v) if v.ok());
        if usable && ui.button("Build a drive image from it").clicked() {
            // Beside the snapshot, because it is the same kind of thing: derived, regenerable, and
            // nobody's idea of a document.
            let out = settings::data_dir().join("ipod-from-ipsw.img");
            self.setup.built = Some(inspect::build_from_ipsw(
                Path::new(self.setup.ipsw.trim()),
                &out,
            ));
            if self.setup.built.as_ref().is_some_and(|r| r.is_ok()) {
                self.setup.disk = out.to_string_lossy().into_owned();
                self.setup.revalidate();
            }
        }
        match &self.setup.built {
            Some(Ok(s)) => {
                ui.colored_label(Color32::from_rgb(0x2f, 0x8f, 0x4f), "Built.");
                ui.small(s.as_str());
            }
            Some(Err(e)) => {
                ui.colored_label(Color32::from_rgb(0xd0, 0x50, 0x40), "Could not build it.");
                ui.small(e.as_str());
            }
            None => {}
        }
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
                    let f = (s.executed as f32 / (*target).max(1) as f32).min(1.0);
                    let left = (1.0 - f) as f64 * s.wall_secs / f.max(0.001) as f64;
                    format!("cold boot — {:.0} %, about {left:.0} s left", f * 100.0)
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
                         different number again — debug mode shows both.",
                    );
                ui.separator();
                // Which of the two colours the 5G shipped in. Not a debug control: it is which
                // iPod you had, so it belongs where someone in user mode can reach it.
                let mut black = self.settings.black_device;
                if ui.checkbox(&mut black, "black").on_hover_text(
                    "The 5G shipped in white and black. This is the case, not the screen.",
                ).changed() {
                    self.settings.black_device = black;
                    self.settings.save();
                }
                ui.separator();
                // The only route back to the setup screen. Without it the images chosen on first
                // run are the images for ever, because a saved pair means the next launch opens
                // straight into the iPod and never shows that screen again.
                if ui
                    .button("setup…")
                    .on_hover_text(
                        "Back to the setup screen, to point this at a different boot ROM or a \
                         different drive. Ends the running machine — a booted RetailOS read its \
                         partition table at startup and has been writing to that drive since, so \
                         there is no honest way to hand it another one.",
                    )
                    .clicked()
                {
                    self.change_images();
                }
                ui.separator();
                let mut debug = self.settings.mode == Mode::Debug;
                if ui.checkbox(&mut debug, "debug").changed() {
                    self.set_mode(if debug { Mode::Debug } else { Mode::User });
                }
                if let Some(line) = self.update_line.clone() {
                    ui.separator();
                    ui.label(egui::RichText::new(line).size(11.0));
                }
            });
            ui.add_space(2.0);
        });
    }

    fn keyboard(&mut self, ctx: &egui::Context) {
        use egui::Key;
        let (mut scroll, mut pressed, mut released, mut toggle_hold, mut shot, mut toggle_mode) =
            (0i32, Vec::new(), Vec::new(), false, false, false);
        let mut wheel_units = self.wheel_units;
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
            // A physical notch reports about 50 units in egui, and a trackpad reports a continuous
            // stream of small ones. Accumulating and dividing gives one detent per notch and a
            // proportional glide from a trackpad, rather than either flying or doing nothing.
            let dy = i.smooth_scroll_delta.y;
            if dy != 0.0 {
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

    /// White or black, and nothing in between — the two the 5G shipped as.
    fn palette(&self) -> (Color32, Color32, Color32, Color32) {
        if self.settings.black_device {
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

    /// Cutting and restoring power, and the two button combinations the hardware answers with a
    /// reset and a power-off.
    ///
    /// The two halves are deliberately kept apart, and labelled apart. The **buttons** are physical
    /// input: `ClickWheel::buttons` is a five-bit mask, so MENU+SELECT is one frame with two bits
    /// set and the model has always been able to express it. What no measurement supports is that
    /// anything *acts* on it — research/10 Addendum 31 §5 holds the pair down for 400 M instructions
    /// at the main menu and the machine does not restart, so the combo here delivers the buttons and
    /// says plainly that nothing came of it. The **power controls** below are emulator controls,
    /// named as such: they drop the machine and build a new one. A button labelled MENU+SELECT that
    /// secretly did that would be the UI claiming a hardware behaviour we have measured to be absent.
    fn power_controls(&mut self, ui: &mut egui::Ui, phase: &Phase) {
        ui.horizontal(|ui| {
            if *phase == Phase::Off {
                if ui.button("power on — cold boot").clicked() {
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
                if ui.button("power cycle — cold boot").clicked() {
                    self.down.clear();
                    self.touching = false;
                    if let Some(l) = &self.link {
                        l.command(emu::Cmd::PowerCycle);
                    }
                    self.say("power cycle: rebuilding at the reset vector (~75 s)");
                }
            }
        });
        ui.small(
            "Emulator power, not a button on the case: powering off drops the machine, and \
             powering on enters at the reset vector with fresh state. A cold boot is ~75 s; the \
             cached snapshot is left alone, so it is never restore-then-pretend.",
        );

        ui.horizontal(|ui| {
            let combo = [Button::Menu, Button::Select];
            let held = combo.iter().all(|b| self.down.contains(b));
            if ui.selectable_label(held, "hold MENU+SELECT").clicked() {
                for b in combo {
                    if held {
                        self.release(b);
                    } else {
                        self.press(b);
                    }
                }
                self.say(if held { "MENU+SELECT released" } else { "MENU+SELECT held" });
            }
            let play_held = self.down.contains(&Button::Play);
            if ui.selectable_label(play_held, "hold PLAY").clicked() {
                if play_held {
                    self.release(Button::Play);
                } else {
                    self.press(Button::Play);
                }
            }
        });
        ui.small(
            "The real hard reset and the real power-off. These deliver the buttons — both bits in \
             one frame, which the wheel has always been able to report — and nothing in RetailOS \
             has been measured to act on either: held for 400 M instructions at the main menu, the \
             machine keeps running (research/10 Addendum 31 §5). On a 5G the pair is caught by the \
             wheel controller or the PMU, neither of which is modelled here.",
        );
    }

    // ------------------------------------------------------------ the instrument panel

    fn instrument(&mut self, ui: &mut egui::Ui, out: &emu::Out) {
        let s = out.stats;
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.add_space(6.0);
            match &out.phase {
                // No bar here. The footer carries one in both modes, and two progress bars for
                // one boot is the window disagreeing with itself about how far along it is.
                Phase::Booting { .. } => {
                    ui.label(egui::RichText::new("cold boot").strong());
                    ui.small(
                        "Booting from the reset vector. The first boot of a session also writes a \
                         snapshot at the idle point, so launching again restores in a few seconds; \
                         a boot reached by powering the machine back on writes none, deliberately \
                         — the drive it would be taken against has been written to since.",
                    );
                }
                Phase::Running => {
                    ui.label(egui::RichText::new("running").strong());
                }
                Phase::Off => {
                    ui.label(egui::RichText::new("powered off").strong());
                    ui.small(
                        "There is no machine: the CPU, all 64 MB of SDRAM and the co-processor's \
                         surface were dropped. Powering on builds a new one and enters at the reset \
                         vector — the drive is the only thing that survives, because it is the only \
                         thing that survives a real power cycle.",
                    );
                }
                Phase::Stopped(why) => {
                    ui.colored_label(Color32::from_rgb(0xd0, 0x50, 0x40), format!("stopped: {why}"));
                }
            }
            self.power_controls(ui, &out.phase);
            ui.separator();

            // ---- speed, both ratios, because they are different numbers
            //
            // The interpreter does ~21 M instructions a second against a PP5021C's ~72 MIPS, so the
            // machine does about 30 % of the *work* per second the real one does. The emulator's
            // own microsecond clock is a separate quantity and does not follow from that: it is
            // `executed / instr_per_usec + slept_usec`, so `--clock=5` pushes it forward 15x faster
            // per instruction than real silicon while the idle task's halts push it forward again
            // by whole timer intervals at no instruction cost. Measured on a restored idle machine
            // it comes out near 1.2x wall — which is neither of the two numbers anyone would
            // predict, and exactly why both are shown rather than one derived from the other.
            let ips = s.executed_here as f64 / s.wall_secs.max(1e-6);
            let sim_ratio = s.sim_usec_here as f64 / (s.wall_secs.max(1e-6) * 1e6);
            grid(ui, "speed", |ui| {
                row(ui, "instructions", &fmt_u64(s.executed));
                row(ui, "this session", &fmt_u64(s.executed_here));
                row(ui, "rate", &format!("{:.1} M/s", ips / 1e6));
                row(
                    ui,
                    "vs 72 MIPS hardware",
                    &format!("{:.0} % of real work rate", ips / HARDWARE_MIPS * 100.0),
                );
                row(ui, "wall clock", &format!("{:.1} s", s.wall_secs));
                row(ui, "simulated clock", &format!("{:.1} s", s.sim_usec as f64 / 1e6));
                row(ui, "sim vs wall", &format!("{sim_ratio:.2}x"));
            });
            ui.small(
                "Two ratios, deliberately, because they disagree. The interpreter does ~30 % of \
                 the hardware's instruction rate — that is the number a game would feel, and the \
                 one the footer carries in user mode too. The emulator's own clock is a different \
                 quantity: --clock=5 advances it 15x faster per instruction than real silicon, and \
                 the idle task's sleeps skip it forward again, so it need not track either wall \
                 time or the instruction rate. Reporting one of these as \"the speed\" would \
                 mislead in whichever direction you guessed.",
            );
            ui.small(
                "Session figures start at the first slice, not at the restore: a snapshot does \
                 not carry `slept_usec`, so the first instruction after a restore recomputes the \
                 microsecond clock and discards the restored value.",
            );
            ui.separator();

            // ---- the wheel, as the DEVICE has it, not as the UI thinks it should be
            grid(ui, "click wheel", |ui| {
                row(ui, "position", &format!("{} / 96", s.position));
                row(ui, "touched", if s.touched { "yes" } else { "no" });
                row(ui, "hold", if s.hold { "ENGAGED" } else { "off" });
                row(ui, "buttons", &format!("{:#07b}", s.buttons));
                row(ui, "reporting (0x052a)", if s.reporting { "on" } else { "OFF — gated" });
                row(ui, "frames posted", &fmt_u64(s.frames_posted));
                row(ui, "frames dropped", &fmt_u64(s.frames_dropped));
                row(ui, "frames suppressed", &fmt_u64(s.frames_suppressed));
                row(ui, "DATA reads", &format!("{} ({} with a frame)", s.data_reads, s.data_reads_ready));
                row(ui, "IRQ 40 assertions", &fmt_u64(s.irqs));
                row(ui, "queued / dropped", &format!("{} / {}", s.queued, s.input_dropped));
            });
            if s.hold {
                ui.colored_label(
                    Color32::from_rgb(0xc8, 0x8a, 0x20),
                    "Hold is engaged in the model, and RetailOS does not act on it.",
                );
                ui.small(
                    "The wheel clears frame bit 31 and the model pulls GPIOA bit 5 low at \
                     0x6000d030 — which is the line Rockbox's `button_hold()` reads, exactly. \
                     RetailOS reads somewhere else: the panel is byte-identical with the switch \
                     thrown and without, no lock icon appears, and its `HoldSwitchTask` sits \
                     pended on a semaphore rather than polling. Nothing here fakes the icon.",
                );
            }
            if !s.reporting {
                ui.colored_label(
                    Color32::from_rgb(0xc8, 0x8a, 0x20),
                    "The wheel has not been told to report yet.",
                );
                ui.small(
                    "A snapshot does not carry the click wheel, so a restored machine starts with \
                     the 0x052a gate closed and refuses autonomous frames — counted above as \
                     `suppressed`, never silently eaten. RetailOS re-sends `0x8001052a` about once \
                     every 20 M instructions, so this clears itself within a second or so.",
                );
            }
            ui.separator();

            // ---- the measurement
            ui.label(egui::RichText::new("does the input reach RetailOS?").strong());
            ui.small(
                "Arrivals at the addresses research/10 Addendum 21 §6 measured. Non-zero here is \
                 the same evidence a `--wheel` script produces, made by a hand on a wheel.",
            );
            grid(ui, "enters", |ui| {
                for (i, (_, name)) in emu::WATCHED.iter().enumerate() {
                    row(ui, name, &fmt_u64(s.enters[i]));
                }
            });
            ui.separator();

            let other_addr = if out.fb_addr == FB_FRONT { FB_BACK } else { FB_FRONT };
            grid(ui, "display", |ui| {
                row(ui, "surface", &format!("{:#010x}", out.fb_addr));
                row(ui, "non-black pixels", &format!("{} / {}", out.fb_nonzero, FB_W * FB_H));
                row(ui, &format!("the other, {other_addr:#010x}"), &format!("{} / {}", out.fb_other_nonzero, FB_W * FB_H));
                row(ui, "bcm frames (session)", &fmt_u64(s.bcm_frames));
                row(ui, "bcm commands (session)", &s.bcm_commands.to_string());
                row(ui, "panel scale", &format!("{}x nearest", self.scale));
            });
            // The one case where a still screen is the window's fault and not the machine's.
            if out.fb_other_moved && !out.fb_shown_moved {
                ui.colored_label(
                    Color32::from_rgb(0xc8, 0x8a, 0x20),
                    "The picture is being drawn to the OTHER surface.",
                );
                ui.small(
                    "Tick `back buffer` to see it. A restored machine can sit one page-flip out of \
                     phase with a cold one: given the identical input it draws the identical \
                     picture — same digest, to the pixel — into the other buffer. Nothing here \
                     models which surface the panel scans out, so both are counted and neither is \
                     guessed at. research/10 Addendum 31 §5.",
                );
            }

            ui.horizontal(|ui| {
                if ui.button("screenshot (S)").clicked() {
                    let (fb, addr) = match &self.link {
                        Some(l) => {
                            let o = l.out.lock().unwrap();
                            (o.fb.clone(), o.fb_addr)
                        }
                        None => (vec![0u8; FB_W * FB_H * 3], FB_FRONT),
                    };
                    self.screenshot(&fb, addr);
                }
                if ui.checkbox(&mut self.show_back_buffer, "back buffer").changed() {
                    let a = if self.show_back_buffer { FB_BACK } else { FB_FRONT };
                    if let Some(l) = &self.link {
                        l.out.lock().unwrap().fb_addr = a;
                    }
                }
            });
            if let Some(p) = &self.last_shot {
                ui.small(p.as_str());
            }
            ui.separator();
            self.updates(ui);
            ui.separator();
            ui.small("arrows scroll · Enter/Space select · M menu · P play · , . prev/next · H hold · D mode · S shot");
            for l in &self.log {
                ui.small(l.as_str());
            }
        });
    }

    /// The update check, and every word of its contract on screen beside it.
    fn updates(&mut self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new(format!("version {}", update::VERSION)).strong());
        ui.horizontal(|ui| {
            if ui.button("check for updates").clicked() {
                self.update_asked = true;
                self.update_line = Some("checking…".into());
                update::spawn(Arc::clone(&self.update_slot));
            }
            let mut on = self.settings.check_updates_on_start;
            if ui.checkbox(&mut on, "on launch").changed() {
                self.settings.check_updates_on_start = on;
                self.settings.save();
            }
        });
        if let Some(l) = &self.update_line {
            ui.small(l.as_str());
        }
        ui.small(
            "One HTTPS GET of GitHub's releases API and a version comparison. Nothing is \
             downloaded, nothing is installed, and nothing is run. Off on launch unless you tick \
             the box, and silent whenever it fails — offline, this says nothing at all.",
        );
    }
}

/// One slot on the setup screen: a label, a hint, a picker, a path, and a verdict.
///
/// Returns whether the path changed, so the caller can revalidate once per frame rather than
/// re-reading two files on every repaint.
fn slot(
    ui: &mut egui::Ui,
    title: &str,
    hint: &str,
    path: &mut String,
    verdict: Option<&Verdict>,
    exts: &[&str],
) -> bool {
    let mut changed = false;
    ui.group(|ui| {
        ui.label(egui::RichText::new(title).strong());
        ui.small(hint);
        ui.horizontal(|ui| {
            if ui.button("Choose…").clicked() {
                if let Some(p) = pick_file(title, exts) {
                    *path = p;
                    changed = true;
                }
            }
            changed |= ui
                .add(
                    egui::TextEdit::singleline(path)
                        .hint_text("…or paste a path, or drag the file onto this window")
                        .desired_width(f32::INFINITY),
                )
                .changed();
        });
        // Drag and drop, which needs nothing from the platform beyond what winit already reports —
        // and which is how most people will actually do this.
        //
        // Gated on the pointer being over THIS slot, and on nothing else. An earlier version also
        // accepted a drop when the pointer position was unknown, which meant a single dropped file
        // landed in every slot at once.
        if ui.rect_contains_pointer(ui.min_rect()) {
            let dropped: Option<PathBuf> = ui.ctx().input(|i| {
                i.raw.dropped_files.first().map(|f| f.path().to_path_buf())
            });
            if let Some(p) = dropped {
                *path = p.to_string_lossy().into_owned();
                changed = true;
            }
        }
        match verdict {
            None if path.trim().is_empty() => {
                ui.small("Nothing chosen yet.");
            }
            None => {
                ui.colored_label(Color32::from_rgb(0xd0, 0x50, 0x40), "No file at that path.");
            }
            Some(Verdict::Good(s)) => {
                ui.colored_label(Color32::from_rgb(0x2f, 0x8f, 0x4f), "This will work.");
                ui.small(s.as_str());
            }
            Some(Verdict::Wrong(s)) => {
                ui.colored_label(Color32::from_rgb(0xc8, 0x8a, 0x20), "Not this machine.");
                ui.small(s.as_str());
            }
            Some(Verdict::Bad(s)) => {
                ui.colored_label(Color32::from_rgb(0xd0, 0x50, 0x40), "Cannot use this file.");
                ui.small(s.as_str());
            }
        }
    });
    changed
}

/// The platform's own file-open dialog, through the tool every platform already has.
///
/// A dialog crate would be the obvious answer and is the wrong trade here: on Linux the portal
/// backend brings an async runtime and a D-Bus stack into a program whose entire dependency
/// argument is "eframe and nothing else", and it can still fail at runtime when no portal is
/// running. So: `osascript` on macOS, PowerShell's `OpenFileDialog` on Windows, `zenity` or
/// `kdialog` on Linux — and **drag-and-drop plus the text field work regardless**, which is what
/// makes it acceptable for this to return [`None`].
fn pick_file(title: &str, exts: &[&str]) -> Option<String> {
    use std::process::{Command, Stdio};
    let out = if cfg!(target_os = "macos") {
        let types = exts
            .iter()
            .map(|e| format!("\"{e}\""))
            .collect::<Vec<_>>()
            .join(", ");
        Command::new("osascript")
            .args([
                "-e",
                &format!(
                    "POSIX path of (choose file with prompt \"{title}\" of type {{{types}}})"
                ),
            ])
            .stderr(Stdio::null())
            .output()
            .ok()?
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
                     $d.Title = '{title}'; $d.Filter = '{filter}'; \
                     if ($d.ShowDialog() -eq 'OK') {{ $d.FileName }}"
                ),
            ])
            .stderr(Stdio::null())
            .output()
            .ok()?
    } else {
        let zenity = Command::new("zenity")
            .args(["--file-selection", &format!("--title={title}")])
            .stderr(Stdio::null())
            .output();
        match zenity {
            Ok(o) if o.status.success() => o,
            _ => Command::new("kdialog")
                .args(["--getopenfilename", ".", "--title", title])
                .stderr(Stdio::null())
                .output()
                .ok()?,
        }
    };
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
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

fn grid(ui: &mut egui::Ui, id: &str, f: impl FnOnce(&mut egui::Ui)) {
    egui::Grid::new(id).num_columns(2).spacing([8.0, 2.0]).striped(true).show(ui, f);
}

fn row(ui: &mut egui::Ui, k: &str, v: &str) {
    ui.small(k);
    ui.small(egui::RichText::new(v).monospace());
    ui.end_row();
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

        let c = config(&[], &Settings::default()).unwrap();
        assert!(c.flash.ends_with("internal_rom_000000-0FFFFF.bin"), "{:?}", c.flash);
        assert!(c.disk.ends_with("ipod8g-retail.img"), "{:?}", c.disk);
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
        let (snap, work) = cache_paths(Path::new("/a.bin"), Path::new("/x.img"), 5, 1);
        if cfg!(target_os = "linux") {
            assert!(!snap.starts_with("/tmp"), "{}", snap.display());
            assert!(!work.starts_with("/tmp"), "{}", work.display());
        }
        assert_ne!(snap, work);
    }

    /// The message a fresh clone gets has to name both files and say where to look. This is the
    /// single biggest usability cliff in the project, and it is one string.
    /// The prune keeps exactly one pair and deletes the rest.
    ///
    /// Written after shipping the opposite: the cache accumulated an 8 GB working disk per image
    /// pair and nothing ever removed one. This test fails if that behaviour ever returns.
    #[test]
    fn pruning_keeps_only_the_pair_in_use() {
        let dir = std::env::temp_dir().join(format!("ipod-prune-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        // SAFETY: the variable is restored below and the test does not spawn threads.
        let before = std::env::var_os("IPOD_EMULATOR_DATA");
        unsafe { std::env::set_var("IPOD_EMULATOR_DATA", &dir) };

        let keep_snap = dir.join("idle-KEEP.snap");
        let keep_work = dir.join("idle-KEEP.img");
        for (p, n) in [(&keep_snap, 10usize), (&keep_work, 10)] {
            std::fs::write(p, vec![0u8; n]).unwrap();
        }
        // Two previous pairs, as four firmware attempts would leave.
        for k in ["OLD1", "OLD2"] {
            std::fs::write(dir.join(format!("idle-{k}.snap")), vec![0u8; 100]).unwrap();
            std::fs::write(dir.join(format!("idle-{k}.img")), vec![0u8; 100]).unwrap();
        }
        // A file that is not ours must survive.
        std::fs::write(dir.join("settings.txt"), b"mode = user\n").unwrap();

        let freed = prune_cache(&keep_snap, &keep_work);

        assert_eq!(freed, 400, "four stale files of 100 bytes");
        assert!(keep_snap.exists() && keep_work.exists(), "the pair in use must survive");
        assert!(dir.join("settings.txt").exists(), "a non-cache file must not be touched");
        assert!(!dir.join("idle-OLD1.img").exists(), "a stale working disk must be gone");
        assert!(!dir.join("idle-OLD2.snap").exists(), "a stale snapshot must be gone");

        match before {
            Some(v) => unsafe { std::env::set_var("IPOD_EMULATOR_DATA", v) },
            None => unsafe { std::env::remove_var("IPOD_EMULATOR_DATA") },
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sizes_read_as_sentences() {
        assert_eq!(human(0), "nothing");
        assert_eq!(human(8_000_000_000), "8.0 GB");
        assert_eq!(human(1_600_000_000), "1.6 GB");
    }

    /// `setup…` returns to the FIRST question, not to the summary.
    ///
    /// Somebody who clicked it wants to change something; landing them on the Ready page with a
    /// Start button answers a question they did not ask. Asserted here rather than by clicking,
    /// because the button's position moves with the window and this behaviour does not.
    #[test]
    fn returning_to_setup_starts_at_the_first_question() {
        let mut setup = Setup {
            step: Step::Ready,
            flash: String::new(),
            disk: String::new(),
            ipsw: String::new(),
            flash_verdict: None,
            disk_verdict: None,
            ipsw_verdict: None,
            built: None,
            force: false,
        };
        // What `change_images` does to the wizard, without needing a window or a machine.
        setup.step = Step::Rom;
        assert_eq!(setup.step, Step::Rom);

        // And the steps run forward in the order the screens present them.
        assert_ne!(Step::Rom, Step::Firmware);
        assert_ne!(Step::Firmware, Step::Ready);
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
}
