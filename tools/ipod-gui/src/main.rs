//! The window.
//!
//! [`docs/GUI.md`](../../../docs/GUI.md) is the design this implements; the markup is
//! `ui/window.slint`, compiled to Rust by `build.rs`. This file is the wiring between the model —
//! which lives in `eapp-loader` and knows nothing about any toolkit — and that markup.
//!
//! **The separation is the point.** `settings.rs`, `compose.rs`, `identity.rs` and `nor.rs` hold
//! the device model, the compatibility rules and the identity validation, and none of them has ever
//! imported a UI crate. That is why replacing an 8,039-line window cost one file, and it is worth
//! keeping for whoever replaces this one. `rail.rs`, `nav.rs`, `fit.rs`, `geometry.rs` and
//! `motion.rs` are toolkit-free for the same reason; this file and `client_height.rs` are the only
//! two that name a Slint type at all.
//!
//! **§20 item 12, and it is why this revision exists.** Every action about to be reconnected — the
//! centre button, Create, Fetch, Build, Install — needs somewhere to narrate and somewhere to fail.
//! Until now `on_start_device` was an `eprintln!`, so the one refusal the model already produces —
//! *this device's boot ROM or its drive is no longer on disk* — had nowhere to be shown. It has one
//! now, and the drawer opens on it.

// The window is not a console program; on Windows a console would flash up behind it.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// **These five modules are not dead. They are not yet reconnected.**
//
// The view layer that called them went with `main.rs` when the old window was deleted, and each
// comes back as its surface is rebuilt: `emu` and `wheel` with Running (GUI.md §16), `control` with
// the readout Rail, `png` with the screenshot key, `update` with Reference (§17). Their own tests
// still run — 28 of them pass, counted with `--list` rather than copied — so this is unreferenced
// code, not unverified code.
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

// These six are wired.
//
// `geometry` is the single source of truth for every ratio and every size constant — `build.rs`
// compiles that same file and renders it into the `.slint` the markup imports, so the tests read
// what the markup reads (GUI.md §16.9). `fit` turns a measured height and a scale factor into the
// one `k` and the one too-short boolean (§6.6, §16.1); it is pure, so all of it is testable with no
// display. `client_height` is the only part that has to ask the platform (§9.6). `motion` asks the
// platform one further question §8.4 needs and Slint cannot answer. `rail` is where the program
// narrates and fails (§9.2, §9.3); `nav` is the single writer of where you are (§4, §16.8).
mod client_height;
mod composer;
mod fit;
mod geometry;
mod motion;
mod nav;
mod rail;
mod work;

// And these three are the producers for the drawer's remaining pages, **stubs today**: each holds
// the vocabulary that crosses its page's boundary and nothing that decides anything yet. They are
// declared now rather than with their first producer because `parts` owns two types `devices` uses
// — `Detail` and `RowAction` — and one definition written before either producer is what stops
// there being two afterwards. Every item in the three carries its own retirement condition; none of
// them carries a module blanket, which is the attribute that let a computed-and-dropped field hide
// in `composer.rs` for as long as it was there.
mod devices;
mod parts;
mod settings_page;

use std::cell::RefCell;
use std::rc::Rc;

use eapp_loader::compose;
use eapp_loader::identity::Colour;
use eapp_loader::settings::{Absent, Device, Presence, Settings};
use eapp_loader::volume;
// Only for `on_winit_window_event`: `Resized`, `Moved` and `ScaleFactorChanged` are the three
// moments §16.1 says the fit has to be recomputed at, and Slint exposes none of them.
use i_slint_backend_winit::WinitWindowAccessor;
use slint::{ComponentHandle, Model, ModelRc, SharedPixelBuffer, VecModel};

slint::include_modules!();

/// Where this test binary's data directory is redirected to. **Never the operator's.**
///
/// `AGENTS.md` §3. `Settings::save`, `Settings::load`, `settings::drives_dir` and
/// `firmware::cache_dir` all resolve through `settings::data_dir`, which declines a cargo build tree
/// and then lands on the **platform application-support directory** — `~/Library/Application
/// Support/ipod-emulator` on macOS, which holds the devices the operator built by hand and, today,
/// two 30 GB drive images. `wire` writes there: it saves, and it builds a `Queue` that names
/// `drives/`.
/// `IPOD_TEST_DATA` names it when the caller wants to keep what a run produced — the end-to-end
/// first run is the reason, since a directory this deletes cannot be looked at afterwards. It is
/// **not** `IPOD_EMULATOR_DATA`: this one is set deliberately, by somebody running the ignored
/// tests, and pointing it at a real library would be a decision rather than an accident. With
/// neither set, a per-process temp directory.
#[cfg(test)]
pub(crate) fn scratch_data_dir() -> &'static std::path::Path {
    static WHERE: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
    WHERE.get_or_init(|| match std::env::var_os("IPOD_TEST_DATA") {
        Some(d) if !d.is_empty() => std::path::PathBuf::from(d),
        _ => std::env::temp_dir().join(format!("ipod-gui-data-{}", std::process::id())),
    })
}

#[cfg(test)]
std::thread_local! {
    /// How many data-directory guards this thread already holds. Guards are locals and drop in
    /// reverse order, so only the outermost one owns the mutex.
    static DATA_DIR_DEPTH: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// **One lock for `IPOD_EMULATOR_DATA`, and it redirects before it hands the guard back.**
///
/// Two things, because they cannot safely be separate.
///
/// *The lock.* [`std::env::set_var`] is process-global and cargo runs tests on several threads. Two
/// test modules in this binary redirect the data directory — this file's, to one fixed scratch
/// directory, and `work.rs`'s, to a fresh one per test which it restores afterwards — and they used
/// to do it under **two different locks**, which is the same as no lock at all. The flake it
/// produced was a ledger test reporting a firmware bundle nobody had downloaded, because it read the
/// other module's cache.
///
/// *The redirect.* It used to live in an opt-in helper that three tests called, and **six** call
/// `wire`. Whether the operator's real library was written to therefore depended on which test the
/// scheduler happened to run first — and worse, `work.rs`'s guard restored the variable to whatever
/// it found, which on a run where nothing had redirected yet was *nothing at all*. Doing it here
/// means the redirect is a precondition of holding the lock rather than a courtesy somebody
/// remembers, and `every_test_that_reaches_the_data_directory_takes_the_lock` is what makes taking
/// the lock non-optional.
///
/// **It is re-entrant, and that is not a convenience.** One test reaches this twice, through two
/// helpers that each have every right to ask for it: `a_fresh_installation` claims the directory,
/// and `a_window` needs the redirect in place before it builds anything that could resolve
/// `data_dir()`. Taken twice on one thread a plain `std::sync::Mutex` deadlocks, and the test binary
/// then hangs with no output and no failing test name — a worse failure than the flake this exists
/// to fix, and the one it produced on the first attempt at this. `std::sync::ReentrantLock` is
/// exactly this, and is still unstable, so it is fifteen lines here instead.
///
/// A test that panicked holding it must not poison every later one, hence the `into_inner`.
#[cfg(test)]
pub(crate) fn data_dir_lock() -> DataDirLock {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    static ONCE: std::sync::Once = std::sync::Once::new();
    let outermost = DATA_DIR_DEPTH.with(|d| {
        let n = d.get();
        d.set(n + 1);
        n == 0
    });
    let held = outermost.then(|| LOCK.lock().unwrap_or_else(|e| e.into_inner()));
    ONCE.call_once(|| {
        // SAFETY: `Once::call_once` blocks every other thread until this returns, so no thread can
        // observe the variable half-set, and it runs exactly once for the life of the process.
        unsafe { std::env::set_var("IPOD_EMULATOR_DATA", scratch_data_dir()) };
    });
    // **Made on every claim rather than once**, because `DataDirGuard` takes it away again on the
    // way out: a run used to leave one `ipod-gui-data-<pid>/` per process in the system temp
    // directory for ever, and putting the creation inside the `Once` would mean the first test to
    // clean up left every later one with no data directory at all.
    std::fs::create_dir_all(scratch_data_dir()).expect("a scratch data directory");
    DataDirLock { _held: held }
}

/// A held claim on the test binary's data directory. Re-entrant; see [`data_dir_lock`].
///
/// `!Send`, because it holds a `MutexGuard` — which is what makes the thread-local depth count
/// correct: a guard cannot be dropped on a thread other than the one that took it.
#[cfg(test)]
pub(crate) struct DataDirLock {
    /// `Some` only on the outermost guard, which is the one that owns the mutex.
    _held: Option<std::sync::MutexGuard<'static, ()>>,
}

#[cfg(test)]
impl DataDirLock {
    /// Whether this is the guard that owns the mutex, rather than a re-entrant one nested inside it.
    ///
    /// **Anything that cleans up must ask.** A nested guard drops halfway through the test that
    /// took the outer one — `a_window` takes one and lets it go before it returns — so a `Drop`
    /// that deleted the data directory on every guard deleted it out from under the test that had
    /// just set it up.
    pub(crate) fn is_outermost(&self) -> bool {
        self._held.is_some()
    }
}

#[cfg(test)]
impl Drop for DataDirLock {
    fn drop(&mut self) {
        DATA_DIR_DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
        // **The outermost guard puts the variable back, and only the outermost.**
        //
        // A test may point it at a subdirectory of its own for the length of its run — the ignored
        // end-to-end tests do, because sharing one data directory makes them order-dependent, and an
        // order-dependent test is a flake with a schedule. Restoring on *every* guard would break
        // that the moment the test called `a_window`, whose nested guard would drop first and undo
        // the redirect while the test was still running.
        //
        // SAFETY: the mutex is still held until this function returns, and a `DataDirLock` is
        // `!Send`, so no other test can be reading the variable.
        if self._held.is_some() {
            unsafe { std::env::set_var("IPOD_EMULATOR_DATA", scratch_data_dir()) };
        }
    }
}

fn main() -> Result<(), slint::PlatformError> {
    opaque_window()?;

    // **Before the first read, and it had zero callers until now** (§20 item 13). `migrate_legacy`
    // declines the moment a settings file exists in the new directory, and `load_and_seed` writes
    // one back whenever seeding changed something — so a carry-forward that runs second never runs
    // at all.
    eapp_loader::settings::migrate_legacy();

    // `load_and_seed` rather than `load`: the window is the surface that owns the library, so it is
    // the one entitled to persist what seeding produced — without which the marker that makes a
    // removal stick never reaches the file. `Settings::load` stays a pure read for `ipod-boot`,
    // where a save on a path as incidental as `--print` rewrote the operator's own file.
    let settings = Rc::new(RefCell::new(Settings::load_and_seed()));
    let window = MainWindow::new()?;

    // **Everything a test can reach is in `wire`, and that is not tidiness.** The one defect this
    // file has shipped that no test could see was inside `on_start_device`'s closure — a `RefMut`
    // held across a `match` — and it was invisible because every test either called the helper
    // directly or replaced the closure with one of its own. A closure registered inside `fn main`
    // is reachable from nothing. `wire` registers the real ones on a window a test can make.
    //
    // **And what it hands back is held for the life of the window, deliberately.** A
    // `slint::Timer` stops the moment it is dropped (`i-slint-core-1.17.1/timers.rs:44`), so
    // `wire(&window, …);` on its own would compile, run, register everything — and leave the first
    // run with nothing moving it, with no error anywhere. `_wiring` is what stops that.
    let _wiring = wire(&window, settings.clone());

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
    push_fit(&window, &fitter.fit(), sf);
    client_height::dump_layout(
        window.window(),
        &fitter.fit(),
        geometry::PREF_HEIGHT,
        sf,
        Some(window.get_verb_width()),
    );

    // **And it is recomputed while you are looking at it**, which is the promise the previous
    // design made and the platform cannot keep. Drag the bottom edge up to make room for a
    // terminal, or drag the window onto a second monitor of the same scale factor: neither fires
    // `ScaleFactorChanged`, every term in §9.6's column except the top margin is a fixed height, so
    // Slint's shrink adjuster can take nothing from any of them, and the trailing children — the
    // shelf, carrying `write_target()` — are positioned past the bottom edge and drawn there. The
    // user is then writing to a disk with the warning off screen.
    //
    // **Exactly one registration, in this whole program.** The hook is stored in a
    // `Cell<Option<Box<…>>>` and registering calls `set`
    // (`i-slint-backend-winit-1.17.1/lib.rs:1088-1091`), so a second call silently destroys the
    // first and takes all of this with it, with no error anywhere.
    // `there_is_exactly_one_winit_event_filter_registration` is the mechanical form of that.
    let weak = window.as_weak();
    let mut shown = false;
    window.window().on_winit_window_event(move |win, event| {
        use i_slint_backend_winit::winit::event::WindowEvent;

        // **The scale factor comes from the EVENT; the size comes from the PLATFORM.** They are not
        // the same rule, and the difference between them is the whole of `tests/startup_fit.rs`.
        // `win.scale_factor()` may still report the old factor during a `ScaleFactorChanged`, so
        // the factor is taken off the event. The size cannot be taken off the event, because at
        // startup a `Resized` payload is the size Slint's minimum clamp gave
        // the window at CREATION — 880 × 400, a window that is resized to 1180 × 846 before it is
        // ever mapped, so no such window is ever on screen. `own_height_logical` carries the
        // citations and asks the platform instead; `win.size()` is Slint's cache and is the one
        // source that is wrong in BOTH directions, because this filter runs before Slint applies
        // the event.
        //
        // **Every moment carries both measurements**, because they answer different questions:
        // `k` from the display's usable height, the too-short boolean from the window we actually
        // got (§6.6, §9.5, §16.1). One value fed to both meant that after a move the warning was
        // computed from the display — so a window dragged short and then moved reported that it
        // had room it did not have.
        let moment = match event {
            WindowEvent::Resized(_) => {
                let sf = live_scale(win);
                let own = own_height_logical(win, sf);
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
            WindowEvent::Moved(_) => {
                // One read, used for both: the height below is that size divided by this factor,
                // and asking twice across a display boundary is how they come from two moments.
                let sf = live_scale(win);
                fit::Moment::Moved {
                    display_logical: ceiling_logical(win),
                    window_logical: own_height_logical(win, sf),
                    sf,
                }
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                // The event's own scale factor, not the window's: `win.scale_factor()` may still
                // report the old one here, which would misconvert the physical size.
                let sf = sane_scale(*scale_factor);
                fit::Moment::ScaleFactorChanged {
                    display_logical: ceiling_logical(win),
                    window_logical: own_height_logical(win, sf),
                    sf: *scale_factor,
                }
            }
            _ => return i_slint_backend_winit::EventResult::Propagate,
        };

        let sf = sane_scale(moment_scale(&moment, win));
        // Taken BEFORE `apply` consumes the moment, because `apply` takes it by value. It is the
        // height the fit was computed from, and `dump_layout` prints it beside the platform's own
        // answer so the two can be compared — they agree, and a printed difference is a defect
        // rather than the one-event cache lag the `window` line carries.
        let measured = moment_window_logical(&moment);
        let (fit, changed) = fitter.apply(moment);
        if changed {
            let mut verb_width = None;
            if let Some(w) = weak.upgrade() {
                push_fit(&w, &fit, sf);
                // §17.Q12: the text renderer's answer, read off the composed window rather than
                // computed from a budget. It is only available while the window is alive.
                verb_width = Some(w.get_verb_width());
            }
            client_height::dump_layout(win, &fit, measured, sf, verb_width);
        }
        // An observer, never a filter: every arm propagates.
        i_slint_backend_winit::EventResult::Propagate
    });

    window.run()
}

// ── The wiring, on a window a test can make ─────────────────────────────────────────────────────

/// Build the models, push what the window needs, and register every callback.
///
/// **It is a function rather than the body of `fn main` because a closure registered inside `main`
/// is reachable from nothing.** The one defect this file has shipped that no test could see lived
/// inside `on_start_device` — a `RefMut` held alive across a `match` — and every test that looked
/// at that button either called `resolve_for_start` directly or replaced the handler with one of
/// its own. `the_registered_centre_button_handler_survives_a_device_that_resolves` drives what
/// `main` actually registers, on the real markup, and it is the reason this split exists.
///
/// The fit and the winit event filter stay in `main`: both need a window on a display, and the
/// filter must be registered **exactly once** in the whole program.
fn wire(window: &MainWindow, settings: Rc<RefCell<Settings>>) -> Wiring {
    // §10.4: **every download in this program goes through `curl`.** `caps()` measures it once per
    // launch, by running the tool rather than by walking `PATH` — a `PATH` walk is a second
    // implementation of what the OS is about to do, and it is wrong on Windows where the extension
    // list is policy. It is carried on `Caps` from here, so the sentence the disabled `Retry` wears,
    // the gate on the press and the empty cradle's `startable` are one measurement and cannot
    // disagree.
    let caps = caps();

    // ── §10.3, and it is read BEFORE anything is written ────────────────────────────────────────
    //
    // The flag is what decides this, and the size of the library is not. That is the whole of
    // §10.3: a build that is cancelled or fails empties the device list, and a window that read
    // emptiness as *offer the welcome again* returned a person to step one for ever, with no error
    // shown and no way past. That shipped.
    let offer = first_run_offer(&settings.borrow());
    // **A latch, and it goes one way.** `ghost` is recomputed from emptiness on every pass and goes
    // both ways (§9.1 gives the later-empty bench the same drawing); the welcome copy does not come
    // back. The drawing may key on emptiness; the welcome may not.
    let showing_welcome = Rc::new(std::cell::Cell::new(offer == Offer::Welcome));

    // ── The library, and it is retained too (§16.9) ──
    //
    // **`set_devices` is called once.** It used to be called once because nothing re-read the
    // library — which meant the bench was a startup snapshot: delete a drive image while the window
    // is open and the cradle still said *press the centre button*, because nothing re-stat'd. Now it
    // is called once because the model is retained and [`refresh_devices`] mutates it in place,
    // which is what §16.9 asks for and what keeps focus, hover and the selection through a refresh.
    let devices: Rc<VecModel<DeviceRow>> = Rc::new(VecModel::default());
    window.set_devices(ModelRc::from(devices.clone()));
    window.set_screen_source(dark_screen());
    window.set_panel_description(panel_description(&phase()).into());

    // ── §8.4's reduced motion, and §16.6's one font family ──
    //
    // Both are platform questions Slint cannot answer, read once, before anything is drawn. The
    // multiplication that turns `scale` into a duration lives inside the `Motion` global, so no use
    // site can forget it.
    window.global::<Motion>().set_scale(motion::scale());
    window.global::<Metric>().set_mono_family(mono_family().into());

    // ── The Rail, and it is retained (§16.9) ──
    //
    // `set_rail` is called ONCE. Every change afterwards is a `push` / `set_row_data` / `remove` on
    // this same `VecModel`. Replacing the model wholesale tears down and reconstructs every
    // repeater instance, losing focus, hover and any in-flight animation — which is what
    // `device_rows` below still does, once, at startup, where nothing is focused yet.
    let rows: Rc<VecModel<RailRow>> = Rc::new(VecModel::default());
    window.set_rail(ModelRc::from(rows.clone()));

    let rail = Rc::new(RefCell::new(rail::Rail::new()));
    let stack = Rc::new(RefCell::new(nav::Stack::new()));
    let work = Rc::new(RefCell::new(work::Queue::new()));

    // ── §11.2's Composer: one recipe, and it is private ─────────────────────────────────────────
    //
    // `None` until somebody opens the page. **There is exactly one `compose::Recipe` in the running
    // window and nothing outside `composer.rs` and `work.rs` may ask the model what a recipe does**
    // — `the_window_computes_no_compatibility_rule_of_its_own` is the sweep that holds it, and this
    // is the one cell the whole of §11.2 hangs from.
    //
    // Its six models are retained for the same reason the Rail's is (§16.9): `push_composer` runs on
    // every keystroke in the serial field, and a fresh `VecModel` per keystroke takes the caret with
    // it.
    let composer: Rc<RefCell<Option<composer::Composer>>> = Rc::new(RefCell::new(None));
    let c_picks: Rc<VecModel<PickRow>> = Rc::new(VecModel::default());
    let c_fields: Rc<VecModel<FieldRow>> = Rc::new(VecModel::default());
    let c_ticks: Rc<VecModel<TickRow>> = Rc::new(VecModel::default());
    let c_opts: Rc<VecModel<OptionRow>> = Rc::new(VecModel::default());
    let c_plan: Rc<VecModel<PlanRow>> = Rc::new(VecModel::default());
    let c_refusals: Rc<VecModel<RefusalRow>> = Rc::new(VecModel::default());
    window.set_composer_picks(ModelRc::from(c_picks.clone()));
    window.set_composer_fields(ModelRc::from(c_fields.clone()));
    window.set_composer_ticks(ModelRc::from(c_ticks.clone()));
    window.set_composer_options(ModelRc::from(c_opts.clone()));
    window.set_composer_plan(ModelRc::from(c_plan.clone()));
    window.set_composer_refusals(ModelRc::from(c_refusals.clone()));

    // ── §10.1: the plan, on screen BEFORE the press ─────────────────────────────────────────────
    //
    // Five rows, each with its own sub-line, filed as `Kind::Planned` — and **nothing is
    // downloaded, minted, probed or written** to put them there. It costs one call to
    // `Recipe::steps()`, which already existed. Nobody has ever been given that list before
    // agreeing to a download.
    //
    // `Holes::Sparse` is the assumption the plan is drawn under; the volume is probed at the press,
    // because probing writes an 8 GiB file and §10.1's absolute is that nothing is written before
    // you agree. If the probe then answers `Full`, the gate refuses against the apparent size and
    // says so — the plan is not re-filed underneath somebody.
    let plan = work::plan(compose::Holes::Sparse);
    let cost = work::cost(compose::Holes::Sparse);
    // One `df`, synchronously, beside the `stat`s `Settings::missing` already does here — see
    // `push_ledger` for why the clause is honest only when it was measured.
    let space = volume::space(&eapp_loader::settings::drives_dir());

    if offer.has_plan() {
        work.borrow_mut().show(&mut rail.borrow_mut(), &plan);
        // §10.4, and it is filed **on top of** the plan rather than instead of it: the five steps
        // stay on screen, and the one that cannot run says why, with a command to paste.
        //
        // **Filed here and nowhere else.** `Queue::press` refuses the same way with the same class
        // and the same sentence, under the verb `make`, so a person with no `curl` opened the
        // window on *One thing failed.* and pressed once to reach *2 things failed.* — one absent
        // tool counted twice, under two verbs, with two identical paragraphs. The press's refusal
        // is now dropped when this one is already on the Rail; see `on_start_device`.
        if !caps.download {
            rail.borrow_mut().failed(
                "fetch",
                "Apple's firmware",
                rail::Failure::new(rail::Class::ToolMissing(rail::Tool::Curl), "the download"),
            );
        }
    }
    if offer == Offer::Welcome {
        // §10.1: the drawer is already open on Work, showing what pressing ● will do.
        stack.borrow_mut().go(nav::Page::Work, 1);
        // §10.3's flag, written **once**, before the first frame. Slint 1.17 exposes no Rust-side
        // first-render callback, and the two failure modes are not symmetric: writing early loses
        // one welcome if the process dies between here and the first frame; writing late re-runs
        // the welcome after any crash, which is §10.3's bug verbatim.
        welcome(&mut settings.borrow_mut());
        // Its failure lands ON the plan, which is why the plan is filed first.
        save(&settings.borrow(), &mut rail.borrow_mut());
    }

    refresh_devices(window, &devices, &settings.borrow(), &showing_welcome, caps, cost);
    push_ledger(
        window,
        offer.has_plan().then_some(cost),
        &eapp_loader::firmware::cache_dir(),
        space.as_ref(),
    );
    sync_rail(window, &rows, &rail.borrow(), caps, work.borrow().shape());
    push_nav(window, &stack.borrow());

    // ── Every open page is re-pushed by whoever moved the library ───────────────────────────────
    //
    // **`redraw` is reachable from nothing but the Composer's own callbacks**, so everything the
    // page draws about the *world* rather than about the recipe went stale between presses: a run
    // that started or finished while the page was open left `Lock::Building` on every picker and
    // `Create` disabled until somebody happened to touch an unrelated control, and a device filed
    // from anywhere else was a name the page did not know was taken. A page re-pushed only by its
    // own controls is stale the moment anything else moves, and three more pages are about to be
    // written against exactly that shape.
    //
    // So every page registers its re-push here and the tick runs all of them. Adding a page cannot
    // forget it: the registration is one line at the end of the page's own block, beside the
    // closure it registers, rather than a name to be remembered in a list somewhere else.
    let repaint: Rc<RefCell<Vec<Repaint>>> = Rc::new(RefCell::new(Vec::new()));
    let repaint_all: Repaint = {
        let repaint = repaint.clone();
        Rc::new(move || {
            // **Cloned out of the cell before any of them runs.** A re-push that reached this again
            // — one that files a Rail note, or a page that arrives at another page — would
            // otherwise borrow the registry while the loop walking it still holds it, which is §20
            // item 12 in a new place and on the path that works.
            let all: Vec<Repaint> = repaint.borrow().clone();
            for f in all {
                f();
            }
        })
    };

    // ── The one timer, and the one place it is started ──────────────────────────────────────────
    //
    // **A `slint::Timer` stops the moment it is dropped** — `i-slint-core-1.17.1/timers.rs:44`:
    // *"The timer will automatically stop when dropped. You must keep the Timer object around for
    // as long as you want the timer to keep firing."* One started inside this function and dropped
    // at its closing brace never fires once, which is why `wire` hands [`Wiring`] back.
    //
    // It is also `!Send` and thread-local (`_phantom: PhantomData<*mut ()>`, timers.rs:64), which is
    // exactly why its callback may hold every `Rc` the window has — and why, under
    // `i-slint-backend-testing`'s no-event-loop init, it never fires at all. So [`pump_once`] is a
    // plain function a test can call directly and this closure is one line of it.
    //
    // **Started in exactly one place**, in the same class as
    // `there_is_exactly_one_winit_event_filter_registration`: `ticking` is the only thing that ever
    // calls `Timer::start`, both the press and a Retry go through it, and starting an already
    // running timer restarts it rather than making a second one.
    let timer = Rc::new(slint::Timer::default());

    // **One tick, and the timer is one caller of it.** [`pump_once`]'s own doc says *a test drives
    // exactly what the timer drives*, and that was only half true: the closure below was reachable
    // from nothing but the timer, and under `i-slint-backend-testing` the timer never fires. So
    // everything after the press — the download, the build, the install, the handoff — was
    // unreachable from any test that went through `wire`, which is §20 item 12 one layer out.
    // `Wiring` hands this back, so the same closure the timer runs is the one a caller runs.
    let tick: Rc<dyn Fn()> = {
        let timer = timer.clone();
        let work = work.clone();
        let rail = rail.clone();
        let rows = rows.clone();
        let devices = devices.clone();
        let settings = settings.clone();
        let showing_welcome = showing_welcome.clone();
        let composer = composer.clone();
        let repaint_all = repaint_all.clone();
        let weak = window.as_weak();
        Rc::new(move || {
            let Some(w) = weak.upgrade() else { return };
            pump_once(
                &w,
                &work,
                &rail,
                &rows,
                &devices,
                &settings,
                &showing_welcome,
                &composer,
                &repaint_all,
                &timer,
                caps,
                cost,
            );
        })
    };
    let ticking: Rc<dyn Fn()> = {
        let timer = timer.clone();
        let tick = tick.clone();
        Rc::new(move || {
            // **Idempotent, because `Timer::start` restarts a running timer.** `i-slint-core`'s own
            // doc: *"If the timer has been started previously, then it will be restarted, no matter
            // if it has already been fired or not."* Every `Press::Busy` came through here, so
            // somebody mashing the centre button on a build that looked stuck pushed the next tick
            // out indefinitely — progress froze on screen while the work carried on and the reports
            // piled up in the channel. Which is precisely what a person does when a download looks
            // stuck.
            if timer.running() {
                return;
            }
            let tick = tick.clone();
            timer.start(slint::TimerMode::Repeated, work::TICK, move || tick());
        })
    };

    // ── The centre button, and §20 item 12's whole point ──
    {
        let settings = settings.clone();
        let rail = rail.clone();
        let stack = stack.clone();
        let rows = rows.clone();
        let devices = devices.clone();
        let work = work.clone();
        let showing_welcome = showing_welcome.clone();
        let ticking = ticking.clone();
        let repaint_all = repaint_all.clone();
        let weak = window.as_weak();
        window.on_start_device(move |index| {
            let Some(w) = weak.upgrade() else { return };
            // **Every borrow this press takes is scoped to this one block, and that line is why.**
            // Written as `match resolve_for_start(&mut settings.borrow_mut(), …) { … }`, the
            // `RefMut` is a temporary in the scrutinee and lives to the end of the whole `match` —
            // so the `Ok` arm's `settings.borrow()` panicked with *already mutably borrowed*. Only
            // the success path took it, which is why every refusal test stayed green: the press that
            // WORKED was the one that took the program down, which is §20 item 12 exactly inverted.
            //
            // `Queue::press` takes four `&mut` for the same reason: everything it needs is borrowed
            // here, once, and released before anything matches on what it said.
            let outcome = {
                let mut s = settings.borrow_mut();
                let mut r = rail.borrow_mut();
                // **§10.2, and the route is decided per press, by the row that was pressed.** An
                // empty bench has no device to start, so it is always the first run's — §9.1 and
                // §10.3 both give the later-empty bench the same one press. A half-made first-run
                // device resumes. Everything else is a device's, which is the path that existed
                // before.
                //
                // It used to be one boolean computed in `wire`: with an empty library and the
                // welcome already shown it was **false**, so the promise §9.1 makes was drawn and
                // then refused; and with a composed device sitting beside a half-made one it was
                // **true for every row**, so pressing the composed device resumed the first run.
                //
                // **The third route is a composed device**, and it is here rather than inside
                // `resolve_for_start` because there is nothing wrong with such a device to resolve:
                // it names every part it was composed from, `Settings::missing` sees nothing gone,
                // and `run_device` accepts it — so without this arm the press answered *resolves
                // and would start here* about an iPod with no drive.
                //
                // Read out before the branch, not in an `else if let`: the scrutinee of an `if let`
                // holds its borrow of `s` to the end of the whole chain, which is the borrow this
                // block's own comment above is about.
                let unwired = s
                    .devices
                    .get(index as usize)
                    .filter(|d| composed_and_unbuilt(d))
                    .map(|d| d.name.clone());
                if press_is_first_run(&s, index as usize) {
                    Route::First(work.borrow_mut().press(&mut s, &mut r, caps.download))
                } else if let Some(name) = unwired {
                    Route::Unwired(name)
                } else {
                    Route::Existing(resolve_for_start(&mut s, index as usize))
                }
            };
            // **A refusal mutates nothing, so it must not rewrite the settings file.**
            // `Settings::render` regenerates the whole file from the model and takes any comment
            // the operator added with it (§20 item 13, one level up), so a save is something to do
            // when there is something to save. Every other route may have moved the library: a
            // first-run press mints the identity and files it away — *even the one that then
            // refuses*, which is exactly the corner §10.3's argument turns on.
            let mutated = !matches!(outcome, Route::Existing(Err(_)) | Route::Unwired(_));
            match outcome {
                Route::First(work::Press::Running { from, embodied }) => {
                    // **§10.3, said out loud.** A press that did not mint anything and did not
                    // start at the beginning is a *resume*, and the whole reason identity is minted
                    // once is that the iPod which comes back is the same iPod. Three failed first
                    // runs used to leave three iPods with three different FireWire GUIDs; saying
                    // which of the two this press was is how a person can see that it did not.
                    if !embodied && from > 0 {
                        rail.borrow_mut().note(&format!(
                            "Carrying on from step {} — the same iPod, not a new one.",
                            from + 1
                        ));
                    }
                    // §12.3's bar and §9.2's Rail move from here on, and the timer is what moves
                    // them. **A `slint::Timer` stops the moment it is dropped**, which is why
                    // `Wiring` exists to hold it.
                    ticking();
                }
                Route::First(work::Press::Busy) => ticking(),
                Route::First(work::Press::Refused(f)) => {
                    // Refused before anything was fetched or built. It goes on the plan, and the
                    // drawer opens on the page that holds it.
                    //
                    // **Unless the Rail already says it.** `wire` files exactly this class with
                    // exactly this sentence when `curl` is absent, under the verb `fetch`; filing
                    // it again under `make` made one missing tool read as *2 things failed.*, with
                    // two identical paragraphs and two copies of the same command. `Rail::note`
                    // already folds a repeated sentence into one and failures deliberately do not,
                    // because two different failures are two things — so the de-duplication has to
                    // be here, where it is known that these two are one.
                    let already = rail
                        .borrow()
                        .entries()
                        .iter()
                        .any(|e| {
                            e.kind == rail::Kind::Failed
                                && e.failure.as_ref().is_some_and(|g| g.class == f.class)
                        });
                    if !already {
                        rail.borrow_mut().failed("make", "an iPod", f);
                    }
                    stack.borrow_mut().go(nav::Page::Work, 1);
                    push_nav(&w, &stack.borrow());
                }
                Route::First(work::Press::HandOff(name)) => {
                    // §12.2's handoff. Everything but the boot is done, and the boot is Phase 7 —
                    // so this falls through to the same note the existing path files, which names
                    // the escape hatch that does work today.
                    rail.borrow_mut().note(&format!(
                        "{name} is made and would start here. Running is not wired to the window \
                         yet — `ipod-boot retail` boots it from a terminal today."
                    ));
                }
                Route::Existing(Ok(name)) => {
                    // **Starting the machine is a later slice.** Until then this files a
                    // project-state note rather than an `eprintln!`, which is the whole reason the
                    // Rail exists before the first button is wired. The escape hatch is real:
                    // `ipod-boot retail` boots the configured device with no window at all.
                    rail.borrow_mut().note(&format!(
                        "{name} resolves and would start here. Running is not wired to the window \
                         yet — `ipod-boot retail` boots it from a terminal today."
                    ));
                }
                Route::Unwired(name) => {
                    // **The refusal, and no remedy after it.** The sentence this replaced on the
                    // Composer's own save named one — *press the centre button on it to finish
                    // making it* — and that button ran the fixed first-run plan, so the remedy built
                    // something else. §14.1: say what cannot be done and why, and stop there.
                    rail.borrow_mut().note(&format!(
                        "{name} was composed here, and building a composed device is not wired \
                         yet. Its drive has not been made, and this button cannot make one."
                    ));
                    stack.borrow_mut().go(nav::Page::Work, 1);
                    push_nav(&w, &stack.borrow());
                }
                Route::Existing(Err(f)) => {
                    // **No machine is started, and nothing is mutated** — the resolution refuses
                    // before it touches anything.
                    rail.borrow_mut().failed("start", &f.0, f.1);
                    // A refusal nobody can see is `eprintln!` with extra steps. The shelf row that
                    // would carry it is not built, so the drawer opens on the page that is.
                    stack.borrow_mut().go(nav::Page::Work, 1);
                    push_nav(&w, &stack.borrow());
                }
            }
            // **§10.2's save point, and it is here rather than only at close.** The press mints the
            // identity and files it away; a save that fails at close has nowhere left to be shown
            // (§20 item 13), and an identity that is minted and not written is the corner §10.3's
            // whole argument turns on — the next press must find the same iPod.
            if mutated {
                save(&settings.borrow(), &mut rail.borrow_mut());
            }
            // §7.3, §7.5: the library moved, so the bench's own account of it has to. The list is a
            // startup snapshot otherwise, and a drive deleted while the window was open stayed
            // invisible for the life of the process.
            //
            // **The bill is the measured one from here on.** The press probed the volume, and on
            // one without sparse files the real cost is 8.6 GB rather than 28 MB — the shelf and
            // the ledger were quoting the assumption the plan was drawn under, for the whole run.
            let cost = work.borrow().measured_cost().unwrap_or(cost);
            refresh_devices(&w, &devices, &settings.borrow(), &showing_welcome, caps, cost);
            push_ledger(
                &w,
                Some(cost),
                &eapp_loader::firmware::cache_dir(),
                volume::space(&eapp_loader::settings::drives_dir()).as_ref(),
            );
            sync_rail(&w, &rows, &rail.borrow(), caps, work.borrow().shape());
            // The press mints an iPod, files it and starts a run: the library moved and a build is
            // now in flight, which is both halves of what a registered page draws about the world.
            repaint_all();
        });
    }

    // ── §7.6's `why ›`, and it has to have something to explain ──
    {
        let settings = settings.clone();
        let rail = rail.clone();
        let stack = stack.clone();
        let rows = rows.clone();
        let work = work.clone();
        let weak = window.as_weak();
        window.on_explain(move |index| {
            let Some(w) = weak.upgrade() else { return };
            // The shelf's refusal is a fact about the device, computed when the row was built; the
            // Rail only ever got an entry from a press. So `why ›` opened Work on *"Nothing is
            // happening."* — a route the design describes as leading to the explanation leading
            // instead to the empty state. This files the sentence, once, and then opens the page.
            {
                let s = settings.borrow();
                if let Some(d) = s.devices.get(index as usize) {
                    let absent = s.missing(d);
                    if !absent.is_empty() {
                        let mut r = rail.borrow_mut();
                        // `failed` de-duplicates against the most recent identical entry, so
                        // pressing `why ›` twice does not stack two copies of one sentence.
                        r.failed("start", &d.name, refusal(d, &absent));
                    }
                }
            }
            stack.borrow_mut().go(nav::Page::Work, 1);
            push_nav(&w, &stack.borrow());
            sync_rail(&w, &rows, &rail.borrow(), caps, work.borrow().shape());
        });
    }

    // ── The drawer: four entrances, one writer ──
    {
        let stack = stack.clone();
        let weak = window.as_weak();
        window.on_drawer_toggled(move || {
            stack.borrow_mut().toggle();
            if let Some(w) = weak.upgrade() {
                push_nav(&w, &stack.borrow());
            }
        });
    }
    {
        let stack = stack.clone();
        let repaint_all = repaint_all.clone();
        let weak = window.as_weak();
        window.on_open_page(move |page, depth| {
            stack.borrow_mut().go(from_markup(page), depth);
            if let Some(w) = weak.upgrade() {
                push_nav(&w, &stack.borrow());
            }
            // **Arriving at a page pushes it.** Each registered re-push may return early when its
            // own page is not the one on screen — that is what keeps a library walk off the 10 Hz
            // tick — so a page that has been off screen has been told nothing since it left, and
            // this is the moment it is on screen again. After `push_nav`, because that is what
            // moves the stack the guards read.
            repaint_all();
        });
    }
    {
        let stack = stack.clone();
        let repaint_all = repaint_all.clone();
        let weak = window.as_weak();
        window.on_drawer_back(move || {
            stack.borrow_mut().back();
            if let Some(w) = weak.upgrade() {
                push_nav(&w, &stack.borrow());
            }
            repaint_all();
        });
    }

    // ── §11.2's Composer: ten callbacks, one redraw ─────────────────────────────────────────────
    //
    // **One function re-pushes the whole page**, and every writer below ends in it. `Composer`'s own
    // setters each end in `recompute()`, which rewrites the verdict, the plan and the two totals
    // *together* before control returns to the event loop — so no frame can render a recipe beside
    // another recipe's verdict. This is the second half of that: what `recompute` computed is what
    // reaches the pixels, in one pass, from one borrow.
    let redraw: Rc<dyn Fn()> = {
        let composer = composer.clone();
        let settings = settings.clone();
        let work = work.clone();
        let space = space.clone();
        let (picks, fields, ticks, opts, plan_m, refusals) = (
            c_picks.clone(),
            c_fields.clone(),
            c_ticks.clone(),
            c_opts.clone(),
            c_plan.clone(),
            c_refusals.clone(),
        );
        let weak = window.as_weak();
        Rc::new(move || {
            let Some(w) = weak.upgrade() else { return };
            // **Every borrow is scoped and released before anything is pushed.** §20 item 12: a
            // `RefMut` held across a `match` scrutinee is what panicked on the path that worked.
            let building = work.borrow().busy();
            let held = composer.borrow();
            let Some(c) = held.as_ref() else { return };
            push_composer(
                &w,
                c,
                &settings.borrow(),
                building,
                caps.clipboard.into(),
                space.as_ref(),
                &picks,
                &fields,
                &ticks,
                &opts,
                &plan_m,
                &refusals,
            );
        })
    };
    // **The Composer stops being a special case and becomes the registry's first member.** It is
    // one line, it sits at the end of the block that built the closure, and it is the only thing a
    // page has to do to stop going stale.
    repaint.borrow_mut().push(redraw.clone());

    // `+ New device ›`, from the Devices page. §11.2: it **mints nothing** — three cancelled visits
    // to this page leave zero iPods, because `nor::mint_seed` happens on `Make one` and not on a
    // page opening.
    {
        let composer = composer.clone();
        let stack = stack.clone();
        let redraw = redraw.clone();
        let weak = window.as_weak();
        window.on_device_new(move || {
            *composer.borrow_mut() = Some(composer::Composer::new());
            stack.borrow_mut().push(nav::Page::Composer);
            redraw();
            if let Some(w) = weak.upgrade() {
                push_nav(&w, &stack.borrow());
            }
        });
    }

    // Each `›` slides one level deeper. **`push`, never `go`** — the level a page is drawn at is
    // `Page::slot`'s answer and not the caller's, so there is no arithmetic here to get wrong.
    {
        let composer = composer.clone();
        let stack = stack.clone();
        let redraw = redraw.clone();
        let weak = window.as_weak();
        window.on_composer_open(move |field| {
            let Some(f) = composer::Field::from_i32(field) else { return };
            let page = match f.level() {
                composer::Level::WhichIpod => nav::Page::ComposerIpod,
                composer::Level::WhatItRuns => nav::Page::ComposerRuns,
                composer::Level::NameIt => nav::Page::ComposerName,
                composer::Level::Root => return,
            };
            if let Some(c) = composer.borrow_mut().as_mut() {
                c.set_level(f.level());
                // §7's rule: `Shown` is per-field and is cleared whenever level ① is left. Arriving
                // at a level closes whatever picker was open on the last one.
                c.set_open(None);
            }
            stack.borrow_mut().push(page);
            redraw();
            if let Some(w) = weak.upgrade() {
                push_nav(&w, &stack.borrow());
            }
        });
    }

    // **Opening a picker closes the one that was open.** `Composer::set_open` holds that; the
    // `Stack`'s expand id is what `Esc` closes before it leaves the page.
    {
        let composer = composer.clone();
        let stack = stack.clone();
        let redraw = redraw.clone();
        window.on_composer_expand(move |field, open| {
            let Some(f) = composer::Field::from_i32(field) else { return };
            if let Some(c) = composer.borrow_mut().as_mut() {
                c.set_open(if open { Some(f) } else { None });
            }
            let id = f.as_i32() as u32;
            if open {
                stack.borrow_mut().expand_opened(id);
            } else {
                stack.borrow_mut().expand_closed(id);
            }
            redraw();
        });
    }

    // A picked option, by **index into the same list that was drawn** — `Composer::choose` resolves
    // it against `options_of`, which is the function that produced the row, so the row drawn
    // disabled is the row that refuses to be picked.
    {
        let composer = composer.clone();
        let settings = settings.clone();
        let rail = rail.clone();
        let rows = rows.clone();
        let work = work.clone();
        let redraw = redraw.clone();
        let weak = window.as_weak();
        window.on_composer_pick(move |field, index| {
            let Some(f) = composer::Field::from_i32(field) else { return };
            if index < 0 {
                return;
            }
            let refused = {
                let s = settings.borrow();
                let mut held = composer.borrow_mut();
                let Some(c) = held.as_mut() else { return };
                c.choose(&s, f, index as usize).err()
            };
            if let Some(why) = refused {
                rail.borrow_mut().note(&why);
                if let Some(w) = weak.upgrade() {
                    sync_rail(&w, &rows, &rail.borrow(), caps, work.borrow().shape());
                }
            }
            redraw();
        });
    }

    // Typing. **`Field::Serial` and `Field::Guid` arrive here only while revealed** — a masked
    // `Field` is `read-only`, and `Show` reveals and enables in one act, so the drawn text and the
    // editable text are never different things.
    {
        let composer = composer.clone();
        let redraw = redraw.clone();
        window.on_composer_type(move |field, text| {
            let Some(f) = composer::Field::from_i32(field) else { return };
            let typed = text.to_string();
            {
                let mut held = composer.borrow_mut();
                let Some(c) = held.as_mut() else { return };
                // **The refusal each of these can return is drawn under the field**, not filed on
                // the Rail: it is about what was typed, `FieldState::reason` already carries it, and
                // a Rail entry per keystroke would be a log of somebody's typing.
                //
                // **None of the three writes the library, so none of them saves.** A typed identity
                // lives on the page until `Create` files it — `Settings::render` regenerates the
                // file whole and takes any comment the operator added with it, so a save on a
                // callback that mutated nothing is a rewrite of somebody's file for no reason.
                match f {
                    composer::Field::Serial => drop(c.set_serial(&typed)),
                    composer::Field::Guid => drop(c.set_guid(&typed)),
                    composer::Field::Name => c.set_name(&typed),
                    _ => {}
                }
            }
            redraw();
        });
    }

    // `Show` / `Hide`. Per field, never persisted — a `Show` that survived a relaunch would defeat
    // the mask on the next screenshot.
    {
        let composer = composer.clone();
        let redraw = redraw.clone();
        window.on_composer_reveal(move |field| {
            let Some(f) = composer::Field::from_i32(field) else { return };
            if let Some(c) = composer.borrow_mut().as_mut() {
                c.set_reveal(f);
            }
            redraw();
        });
    }

    // A system's tick box. `Composer::set_os` follows the bootloader **before** the verdict is
    // recomputed, so the window never draws — for one frame — a refusal the program is about to fix
    // itself.
    {
        let composer = composer.clone();
        let redraw = redraw.clone();
        window.on_composer_tick(move |os, on| {
            let Some(o) = compose::Os::ALL.get(os.max(0) as usize).copied() else { return };
            if let Some(c) = composer.borrow_mut().as_mut() {
                c.set_os(o, on);
            }
            redraw();
        });
    }

    // §11.3's one-press `Fix`. `Recipe::apply` is **the one applier**, in the model.
    {
        let composer = composer.clone();
        let redraw = redraw.clone();
        window.on_composer_fix_pressed(move || {
            if let Some(c) = composer.borrow_mut().as_mut() {
                c.apply_fix();
            }
            redraw();
        });
    }

    // Level ①'s two acting controls: `Make one`, and `Copy the command line`.
    {
        let composer = composer.clone();
        let rail = rail.clone();
        let rows = rows.clone();
        let work = work.clone();
        let redraw = redraw.clone();
        let weak = window.as_weak();
        window.on_composer_act(move |field| {
            let Some(f) = composer::Field::from_i32(field) else { return };
            match f {
                composer::Field::Ipod => {
                    let mut held = composer.borrow_mut();
                    let Some(c) = held.as_mut() else { return };
                    // **`Composer::make_one` mints into the page and files nothing**, so there is
                    // no save here. §11.2 asks for the iPod to be filed at the mint — *cancelling
                    // the device keeps the identity you just tuned* — and `composer.rs` files it in
                    // `commit` instead. Adding a `save` here would write the settings file on a
                    // press that changed nothing in it, which `render` turns into somebody's
                    // comments being deleted for no reason; adding a `file_away` here would be a
                    // second place that decides what an iPod is called. Both belong on the same
                    // side of the boundary as the minting, and that side is `composer.rs`.
                    c.make_one();
                }
                composer::Field::Serial => {
                    // §7 gate 3: the command carries a **recipe**, never a value. `Composer::
                    // command_line` is what words it and refuses the typed case; this only routes
                    // it, and `on_copy_text`'s own gate is under both of them.
                    let line = composer
                        .borrow()
                        .as_ref()
                        .map(|c| c.command_line())
                        .unwrap_or_default();
                    if let Some(w) = weak.upgrade() {
                        w.invoke_copy_text(line.into());
                    }
                }
                _ => {}
            }
            if let Some(w) = weak.upgrade() {
                sync_rail(&w, &rows, &rail.borrow(), caps, work.borrow().shape());
            }
            redraw();
        });
    }

    // `Create` / `Save`. **Save first**, and it is the ordering rather than a preference: a device
    // that was filed and not built is §10's unfinished device, which every surface already words;
    // a build that ran and was not filed is a drive on disk the library never learned about.
    {
        let composer = composer.clone();
        let settings = settings.clone();
        let rail = rail.clone();
        let rows = rows.clone();
        let work = work.clone();
        let stack = stack.clone();
        let devices = devices.clone();
        let showing_welcome = showing_welcome.clone();
        let redraw = redraw.clone();
        let weak = window.as_weak();
        window.on_composer_commit(move || {
            let outcome = {
                let mut s = settings.borrow_mut();
                let mut held = composer.borrow_mut();
                let Some(c) = held.as_mut() else { return };
                c.commit(&mut s)
            };
            match outcome {
                Err(why) => {
                    // **Nothing was written**, which is what `commit`'s own contract promises for
                    // every refusal — so there is no save here and the page stays where it is.
                    rail.borrow_mut().note(&why);
                }
                Ok(done) => {
                    // §20 item 13's Composer save point.
                    save(&settings.borrow(), &mut rail.borrow_mut());
                    // §9.2 wants the work where work is reported. **The build is not wired**:
                    // `work::Queue` has no `compose`, so this says so rather than leaving a device
                    // that looks built.
                    //
                    // **And it offers no remedy, which is the fix.** It used to end *press the
                    // centre button on it to finish making it*, and that button did not build this
                    // recipe: `press_is_first_run` sent a freshly composed device into the **fixed**
                    // first-run plan — Apple's firmware, an 8 GiB drive, Apple's software, no
                    // `Recipe` consulted — so a device composed as Rockbox-only was told to press a
                    // button that builds an Apple drive. The press refuses it now, and §14.1 is what
                    // this sentence follows instead: state the refusal, state what follows from it,
                    // and name no route that does something different.
                    rail.borrow_mut().note(&format!(
                        "{} is in the library. Building a composed device is not wired yet, so no \
                         drive has been made for it.",
                        done.device
                    ));
                    stack.borrow_mut().go(nav::Page::Work, 1);
                }
            }
            if let Some(w) = weak.upgrade() {
                refresh_devices(&w, &devices, &settings.borrow(), &showing_welcome, caps, cost);
                sync_rail(&w, &rows, &rail.borrow(), caps, work.borrow().shape());
                push_nav(&w, &stack.borrow());
            }
            redraw();
        });
    }

    // ── `Esc`, which has ONE definition (§16.8) ──
    {
        let stack = stack.clone();
        let weak = window.as_weak();
        window.on_escape_pressed(move || {
            // §12.2's four phases, `Off` included. There is no machine, so `Off` is not a default
            // somebody typed at this call site — it is the only phase this build can be in, and
            // when the bench starts one these two booleans are the whole of the handoff.
            let p = phase();
            let what = stack.borrow_mut().escape(is_booting(&p), is_running(&p));
            match what {
                // Both of these are reachable only once there is a machine, and `phase()` says
                // there is not. They are matched rather than defaulted so that the day one exists,
                // the compiler points here.
                nav::Escape::Park | nav::Escape::PowerOff => {}
                nav::Escape::LeftFullscreen
                | nav::Escape::ClosedExpand
                | nav::Escape::WentBack
                | nav::Escape::ClosedDrawer
                | nav::Escape::Nothing => {}
            }
            if let Some(w) = weak.upgrade() {
                push_nav(&w, &stack.borrow());
            }
        });
    }

    // ── The Rail's own three controls ──
    {
        let rail = rail.clone();
        let rows = rows.clone();
        let work = work.clone();
        let weak = window.as_weak();
        window.on_rail_dismiss(move |id| {
            rail.borrow_mut().dismiss(id as u64);
            if let Some(w) = weak.upgrade() {
                sync_rail(&w, &rows, &rail.borrow(), caps, work.borrow().shape());
            }
        });
    }
    {
        let rail = rail.clone();
        let rows = rows.clone();
        let work = work.clone();
        let weak = window.as_weak();
        window.on_rail_cancel(move |id| {
            // **The queue is asked first, and a file the worker owns is deleted by the worker.**
            // A second `unlink` from this thread while a thread is still writing that path is a
            // race, and `Queue::cancel` answers `false` for anything it is not running — in which
            // case this falls through to the direct path, which is the one every entry outside a
            // run uses.
            if !work.borrow_mut().cancel(id as u64) {
                cancel_write(&rail, id as u64);
            }
            if let Some(w) = weak.upgrade() {
                sync_rail(&w, &rows, &rail.borrow(), caps, work.borrow().shape());
            }
        });
    }
    {
        let rail = rail.clone();
        let rows = rows.clone();
        let stack = stack.clone();
        let settings = settings.clone();
        let devices = devices.clone();
        let work = work.clone();
        let showing_welcome = showing_welcome.clone();
        let ticking = ticking.clone();
        let composer = composer.clone();
        let redraw = redraw.clone();
        let weak = window.as_weak();
        window.on_rail_next(move |id, which| {
            let Some(w) = weak.upgrade() else { return };
            let retried = take_next_step(&rail, &stack, &composer, id as u64, which, caps);
            // **`Fix` is the one next step that navigates**, and a Composer page pushed without its
            // contents pushed too is the blank panel `Stack::go` spends two guards preventing. This
            // is the same `redraw` all ten Composer callbacks end in; it returns at once when
            // nothing is being composed, which is every other press that reaches this closure.
            redraw();
            // **§10.3: a retry resumes, and it goes through the same press the centre button
            // goes through.** Two routes that both claim to retry and only one of which actually
            // runs anything is how §10.3's bug came back the first time — `Rail::retry` alone puts
            // the entry back to `Planned` and starts nothing, so the plan would sit there looking
            // ready for ever.
            if retried && work.borrow().owns(id as u64) {
                let press = {
                    let mut s = settings.borrow_mut();
                    let mut r = rail.borrow_mut();
                    work.borrow_mut().press(&mut s, &mut r, caps.download)
                };
                if let work::Press::Refused(f) = press {
                    rail.borrow_mut().failed("make", "an iPod", f);
                } else {
                    ticking();
                }
                save(&settings.borrow(), &mut rail.borrow_mut());
                refresh_devices(&w, &devices, &settings.borrow(), &showing_welcome, caps, cost);
            }
            push_nav(&w, &stack.borrow());
            sync_rail(&w, &rows, &rail.borrow(), caps, work.borrow().shape());
        });
    }

    // **The single route to a clipboard, and the gate on it.** `Copy the details` is gated on
    // `caps.clipboard`, which is false because **this program declares no clipboard dependency and
    // calls no clipboard API** — so no control can fire this today. It says so rather than silently
    // succeeding — a handler that swallows the request is the visible-control-that-does-nothing
    // defect one level down — and it refuses an identifier **before** it says that, because the
    // refusal is the rule and the missing clipboard is only the state of this build.
    //
    // **That is deliberately not the claim this comment used to make**, which was *nothing in this
    // dependency graph provides a clipboard*, and which is false: `cargo tree -p ipod-gui | grep -c
    // copypasta` is **1** — copypasta 0.10.2 arrives under `i-slint-backend-winit`, which is where
    // Slint's own text fields get their copy and paste. In the graph and available to us are
    // different claims: a transitive dependency of the backend is not an API this crate can call,
    // `use copypasta::` does not compile without a `copypasta` line in `Cargo.toml`, and there is
    // none. What is absent is a route **we** can reach, not a pasteboard on the machine. See
    // [`caps`] for the route that does exist and is not taken here.
    {
        let rail = rail.clone();
        let rows = rows.clone();
        let work = work.clone();
        let weak = window.as_weak();
        window.on_copy_text(move |text| {
            let note = clipboard_refusal(&text)
                .unwrap_or("this build has no clipboard, so nothing was copied");
            rail.borrow_mut().note(note);
            if let Some(w) = weak.upgrade() {
                sync_rail(&w, &rows, &rail.borrow(), caps, work.borrow().shape());
            }
        });
    }
    // ── §20 item 13's last save point ──
    //
    // The one that matters happens where the change happens, above, because the window is still on
    // screen there and a failure has somewhere to go. This is the backstop, and its failure has
    // nowhere left to be shown — which is why it is the only `eprintln!` left in this file and why
    // it says so.
    {
        let settings = settings.clone();
        let work = work.clone();
        let rail = rail.clone();
        window.window().on_close_requested(move || {
            // **Asked to stop BEFORE the save, and it does not join.** `JoinHandle::join` has no
            // timeout, so a window that waited for a worker stuck on a hung network mount would
            // refuse to close — which is worse than a stray `.part`. `Queue::stop` waits `GRACE`
            // for an acknowledgement and abandons it after that.
            //
            // It takes the library and the Rail because a worker that finished in the last 100 ms
            // has a `Done` still in the channel, and throwing it away is how a close lands a
            // settings file that does not mention the drive on disk. Scoped, so the save below
            // takes its own borrow.
            {
                let mut s = settings.borrow_mut();
                let mut r = rail.borrow_mut();
                work.borrow_mut().stop(&mut s, &mut r);
            }
            if let Err(e) = settings.borrow().save() {
                eprintln!(
                    "the settings could not be written on the way out ({e}); the window is closing \
                     and there is nowhere left to show this"
                );
            }
            slint::CloseRequestResponse::HideWindow
        });
    }

    Wiring { _tick: timer, work, tick }
}

/// What [`wire`] hands back so it stays alive.
///
/// **A `slint::Timer` stops the moment it is dropped** — `i-slint-core-1.17.1/timers.rs:44`: *"The
/// timer will automatically stop when dropped. You must keep the Timer object around for as long as
/// you want the timer to keep firing."* A timer started inside `wire` and dropped at its closing
/// brace never fires once, and nothing anywhere reports that.
///
/// `work` is handed back so a test can drive a press and a tick with no display, which is the only
/// way to test either: the testing backend has no event loop, so the timer never fires there.
struct Wiring {
    _tick: Rc<slint::Timer>,
    #[allow(dead_code)]  // retired when: a test outside this module drives the queue; today only `wire`'s own tests do, and they reach it through the returned struct
    work: Rc<RefCell<work::Queue>>,
    /// **One tick of the window, and it is the one the timer runs.**
    ///
    /// Not a convenience: under `i-slint-backend-testing`'s no-event-loop init a `slint::Timer`
    /// never fires, so without this everything after the press is reachable from nothing that can
    /// be driven without a display — the download, the build, the install and §12.2's handoff. That
    /// is §20 item 12 one layer out from where it was found, and `pump_once`'s own doc claims the
    /// opposite (*"a test drives exactly what the timer drives"*), which is now true because both
    /// call this.
    #[allow(dead_code)]  // retired when: `main` needs to tick by hand — it does not, the timer does it
    tick: Rc<dyn Fn()>,
}

/// One page's whole re-push, in one call.
///
/// `Rc` rather than `Box` because each one is held twice over: once by the registry the tick walks,
/// and once by every callback of the page that owns it. Named because the registry is a `Vec` of
/// them inside an `Rc<RefCell<…>>`, and three levels of that written out is a type nobody reads.
type Repaint = Rc<dyn Fn()>;

/// Which of the two things the centre button is, on this press.
///
/// Named rather than nested so that `on_start_device` can scope every borrow it takes to one block
/// and then match on what came out — see the comment at the press, and §20 item 12.
enum Route {
    /// §10.2's first run, resumed or started.
    First(work::Press),
    /// A device already in the library, resolved (or refused) the way it always was.
    Existing(Result<String, (String, rail::Failure)>),
    /// A device the Composer filed and this build cannot make a drive for — see
    /// [`composed_and_unbuilt`]. It is neither of the other two: the first run's plan is not this
    /// device's, and there is nothing to resolve because the drive was never built. It mutates
    /// nothing and files one sentence.
    Unwired(String),
}

/// **Everything one tick does to the window, in one function**, so a test drives exactly what the
/// timer drives.
///
/// That is §20 item 12's lesson applied to the timer: a closure registered inside `wire` is
/// reachable from nothing, and under `i-slint-backend-testing`'s no-event-loop init a `slint::Timer`
/// never fires — so a test that could only get at this through the timer could not get at it at all.
///
/// **The queue is the only thing that writes the library during a run**, and it does it here, on
/// this thread, in `pump`: §10.2 wants a save after every completed step, so a run interrupted
/// between two of them resumes from the one it reached.
#[allow(clippy::too_many_arguments)] // every one of these is a distinct thing the window owns; bundling them into a struct would be one indirection and the same fields under a new name
fn pump_once(
    window: &MainWindow,
    work: &Rc<RefCell<work::Queue>>,
    rail: &Rc<RefCell<rail::Rail>>,
    rows: &Rc<VecModel<RailRow>>,
    devices: &Rc<VecModel<DeviceRow>>,
    settings: &Rc<RefCell<Settings>>,
    showing_welcome: &Rc<std::cell::Cell<bool>>,
    composer: &Rc<RefCell<Option<composer::Composer>>>,
    // Every page that has registered a re-push, run as one. The registry is built in `wire`.
    repaint: &Repaint,
    timer: &slint::Timer,
    caps: rail::Caps,
    cost: compose::Cost,
) {
    // Scoped exactly like the press, and for the same reason: nothing below re-enters a borrow.
    let tick = {
        let mut s = settings.borrow_mut();
        let mut r = rail.borrow_mut();
        work.borrow_mut().pump(&mut s, &mut r)
    };

    // **No file is deleted here, and that is `work.rs`'s decision rather than an omission.** A
    // cancel is observed by the worker at a step boundary, and the worker removes the `.part` it
    // itself created — deleting from this thread while that thread may still be writing the path is
    // a race, and `AGENTS.md` §3 makes deletion the operator's decision, taken when they pressed a
    // control that had already named the file and its size.
    //
    // **Retirement condition**, in the shape `research/04` uses: if `work::Tick` ever hands a path
    // back — an abandoned worker's partial, which `Stopped::Abandoned` already names — it is
    // deleted here, because that is the one case no worker is left to do it.

    // §10.2: **`Settings::save()` after every completed step**, so a window closed mid-run resumes
    // from the step it reached rather than from the beginning. `pump` has already written the
    // library; this is what puts it on disk.
    if !tick.completed.is_empty() || tick.library_changed {
        save(&settings.borrow(), &mut rail.borrow_mut());
    }
    // **The bill, as measured.** The plan was drawn assuming a filesystem with holes, because the
    // only honest way to find out is to write an 8 GiB file and §10.1 forbids writing before you
    // agree. On a volume without them the real cost is 8.6 GB rather than 28 MB — 300× — and every
    // figure on screen was the assumption. `Queue::measured_cost` is `None` until a press has
    // probed, so nothing changes on the common path.
    let cost = work.borrow().measured_cost().unwrap_or(cost);
    if tick.library_changed {
        // **The Composer is asked whether its device left underneath it**, and it is asked here
        // because this is the one place the library is known to have changed. §11.2's Edit mode
        // holds a device *name*; a run that renamed or replaced it leaves the page editing
        // something that is not there, and the honest answer is that this is a new device now —
        // said once, on the Rail, rather than discovered at `Save`.
        //
        // The borrow is scoped and released before the note is filed: §20 item 12.
        let note = {
            let s = settings.borrow();
            let mut held = composer.borrow_mut();
            held.as_mut().and_then(|c| c.device_vanished(&s))
        };
        if let Some(note) = note {
            rail.borrow_mut().note(&note);
        }
        refresh_devices(window, devices, &settings.borrow(), showing_welcome, caps, cost);
    }
    if tick.changed {
        sync_rail(window, rows, &rail.borrow(), caps, work.borrow().shape());
    }
    // **§10.1's third ledger line is checked rather than asserted, and it was checked once.**
    // `push_ledger` ran only in `wire`, so *Nothing has been downloaded yet.* stayed on screen
    // after the bundle had arrived and been SHA-256 checked — the one line the design singles out
    // as needing to be true, left asserting an absence the program had just disproved. A completed
    // step is exactly when it can have changed.
    if !tick.completed.is_empty() {
        push_ledger(
            window,
            Some(cost),
            &eapp_loader::firmware::cache_dir(),
            volume::space(&eapp_loader::settings::drives_dir()).as_ref(),
        );
    }
    // §12.2's handoff. Every step but the boot is done; the boot is Phase 7, so this says so once
    // rather than starting a machine this build does not have.
    if let Some(name) = tick.ready {
        rail.borrow_mut().note(&format!(
            "{name} is made and would start here. Running is not wired to the window yet — \
             `ipod-boot retail` boots it from a terminal today."
        ));
        sync_rail(window, rows, &rail.borrow(), caps, work.borrow().shape());
    }

    // ── Every open page, and not only the Rail, the shelf and the ledger ────────────────────────
    //
    // Everything above this line pushes one of those three. A drawer page drawn from `building` or
    // from the library held whatever it had been told the last time somebody pressed one of *its*
    // controls, which is the whole of the registry's reason for existing.
    //
    // **`idle` is in the condition because it is the half that was actually visible.** The three
    // `Tick` fields cover a run in flight; the *end* of one need set none of them — `pump` drains
    // the channel twice, so the last report of a run is usually taken on the tick before the worker
    // exits, leaving a final tick that changed nothing and on which `busy()` goes false. That is
    // exactly the tick where `Lock::Building` has to come off. It is also the tick that stops the
    // timer, so one read of `busy()` decides both and there is no tick between the two answers.
    let idle = !work.borrow().busy();
    if idle || tick.changed || tick.library_changed || !tick.completed.is_empty() {
        repaint();
    }

    // **Nothing is running, so stop looking.** A 10 Hz wakeup for the life of a window that is not
    // building anything is a cost nobody agreed to, and `ticking` starts this again on the next
    // press. `sync_rail` has already pushed the final `progress`, which is negative.
    if idle {
        timer.stop();
    }
}

// ── The phase, and there is no machine ───────────────────────────────────────────────────────────

/// §12.2's four phases. **`Off` is genuinely one of them**: no machine exists, nothing is
/// executing, and the panel is dark — the state a 5G is in with a flat battery.
///
/// Running is a later slice, so this is the whole of the handoff: one function that says which
/// phase the window is in, and two predicates over it. When the bench starts a machine this reads
/// `emu::Link`'s `Out.phase` and nothing else in the window changes.
fn phase() -> emu::Phase {
    emu::Phase::Off
}

fn is_booting(p: &emu::Phase) -> bool {
    matches!(p, emu::Phase::Booting { .. })
}

fn is_running(p: &emu::Phase) -> bool {
    matches!(p, emu::Phase::Running)
}

// ── What this build can actually do ──────────────────────────────────────────────────────────────

/// §9.3's next steps are only offered as live controls where the mechanism exists.
///
/// **Four are literals about this build, two are asked of `Page::slot`, and one is measured about
/// the machine.** The four: `cargo tree -p ipod-gui | grep -iE "rfd|native-dialog|ashpd"` is empty,
/// §16.4's winit drop hook is not written, nothing here reaches a pasteboard, and nothing opens a
/// file manager. A control whose route does not exist is drawn disabled with its reason (§14.1),
/// never live and never quietly dropped.
///
/// The two derived ones are `devices_page` and `composer`, and **both pages now exist** — the
/// drawer draws Devices at level 1 and the Composer at level 2, with its three sub-levels under it.
/// They are read from [`nav::Page::slot`] rather than typed, for the reason written on the line
/// itself. This doc claimed the opposite of both for as long as it took the pages to land, which is
/// §16.9's stale claim in prose rather than in a boolean.
///
/// The last, `download`, is `eapp_loader::tooling::can_download()` — it runs `curl --version`,
/// because a `PATH` walk is a second implementation of what the OS is about to do and is wrong on
/// Windows, where the extension list is a policy rather than a suffix.
///
/// **It is called once per launch**, in [`wire`], and the answer is carried on `Caps` from there.
/// Asking again per control would spawn a process inside a binding.
///
/// **One of the four is a decision rather than an absence, and it is `clipboard`.** *Nothing here
/// reaches a pasteboard* is a fact about this crate's own code and stays true; *nothing could* is
/// not, and this doc must not be read as saying it. Measured against the pinned `slint = "1.17"`,
/// in `~/.cargo/registry/src/`, rather than assumed:
///
/// - **There is no Rust-side clipboard API.** `slint/lib.rs:422` is `pub mod platform { pub use
///   i_slint_core::platform::*; }`, and what that exports is the `Platform` **trait** — whose
///   `set_clipboard_text` and `clipboard_text` a *backend* implements — plus the `Clipboard` enum
///   and `set_platform`. There is no accessor for the platform already installed: reaching it goes
///   through `i_slint_core::context::with_global_context`, in a crate this one does not depend on.
///   Nothing on `slint::Window` copies text.
/// - **There is a markup-side one, and it is documented and stable.** `i-slint-compiler-1.17.1/
///   builtins.slint` declares `TextInput::select-all()` at `:1718` and `TextInput::copy()` at
///   `:1727`, both as documented functions on a built-in element this crate's `.slint` files
///   already have. `copy()` lands on `i-slint-core-1.17.1/items/text.rs:1919`, which asks the
///   window's context for the platform and calls `set_clipboard_text` — and winit's backend does
///   exactly that at `i-slint-backend-winit-1.17.1/lib.rs:883`, through `clipboard.rs`, through
///   `copypasta`. That is the same copypasta the `on_copy_text` comment names: it is reachable, but
///   only from the `.slint` side and only with a selection, since `copy_clipboard` returns early
///   when `anchor == cursor`.
///
/// So this is `false` because the route is not built, not because none exists. **Retirement
/// condition:** a `TextInput` — `read-only`, off-screen or zero-height — that `on_copy_text` fills,
/// focuses, `select-all()`s and `copy()`s, at which point this becomes `true` and every disabled
/// reason in `rail.rs` and `composer.rs` that names the missing pasteboard goes with it. It is
/// deliberately **not** done in the commit that wrote this paragraph: a control that fires is a
/// behaviour to design and test, and the finding is worth having on its own.
fn caps() -> rail::Caps {
    rail::Caps {
        file_picker: false,
        drop_target: false,
        // Not *no clipboard exists* — see this function's doc. No route from here to the one that
        // does, and the route is a `.slint` `TextInput`, not a crate.
        clipboard: false,
        reveal: false,
        // **Derived, not typed.** `Page::slot()` returns `Some` on the day `ui/drawer.slint` gains
        // a child that draws the page and `None` until then — which is exactly the question this
        // cap asks. Written as a literal it is a second answer to it, and the two go out of step in
        // one direction only: the page lands, the literal stays `false`, and `Next::Choose`'s
        // disabled reason goes on naming a gap that has been closed. §16.9's rule about a stale
        // claim, applied to a boolean.
        devices_page: nav::Page::Devices.slot().is_some(),
        download: eapp_loader::tooling::can_download(),
        // **Derived from the same question, and it is now `true`.** `Page::Composer` answers
        // `Some(2)`, so the surface a `Next::Fix` goes to exists — the four Composer pages ship and
        // `mod composer;` is declared at the top of this file. This line read `false` beside them,
        // under a comment saying *there is no `Page::Composer` to ask*, so the Rail's `Fix` shipped
        // disabled wearing *there is no Composer in this build yet* while four Composer pages were
        // drawn one level away. That comment named its own retirement condition and the condition
        // had been met; §16.9 calls that a defect and not tidying.
        //
        // **Flipped in the same commit that gives `take_next_step` a real `Fix` arm**, as that
        // comment required. Flipping it alone is the live-but-inert control this file has shipped
        // twice, and `every_next_step_this_build_offers_is_wired_to_something` says so out loud:
        // *`build from Apple's firmware instead` is drawn live and pressing it changed nothing.*
        composer: nav::Page::Composer.slot().is_some(),
    }
}

// ── §10.3: whether the first run is offered at all ───────────────────────────────────────────────

/// What the bench should be, **decided from the flag and never from the size of a list**.
#[derive(Clone, PartialEq, Eq, Debug)]
enum Offer {
    /// §10.1 in full: the welcome copy, the plan in an already-open drawer, one press. Reached
    /// exactly once per installation.
    Welcome,
    /// §9.1's later-empty bench: **the same ghost and the same press, without the welcome copy.**
    ///
    /// §10.3 is explicit — *"a later empty bench is the same ghost iPod with `No devices yet` and
    /// both routes offered equally"* — and this variant exists because the code had no room for it.
    /// `welcomed` is set when the bench is wired, before any press, so opening the program, looking
    /// at it and closing it was enough to reach the state; and that state offered no route to an
    /// iPod at all. The cradle was drawn `fg-dim` and unpressable, the plan was not filed, the
    /// drawer stayed shut, and the press fell through to *"there are no devices in the library yet,
    /// so there is nothing to start"* — while shelf row 2 went on saying *the centre button makes
    /// one*. **The flag was meant to stop the welcome copy returning, not to disarm the button.**
    Again,
    /// A first run that did not get to the end. The identity is already minted, so pressing
    /// **resumes**; it does not start over and it does not mint a second iPod.
    Finish { device: String },
    /// A library with devices in it. No plan, no ghost; the centre button is a device's.
    Quiet,
}

impl Offer {
    /// Whether the first run's plan belongs on the Rail and its bill in the ledger.
    ///
    /// Three of the four. Only [`Offer::Quiet`] — a library that already has devices in it — has
    /// nothing to make.
    fn has_plan(&self) -> bool {
        !matches!(self, Offer::Quiet)
    }
}

/// **The only function in this program that decides whether the first run is offered.**
///
/// It reads `Settings::welcomed` and the shape of the library. It does **not** read
/// `Settings::devices.is_empty()`, and that is the entire point: the old window inferred *offer me*
/// from *the device list is empty*, and a cancelled or failed build is exactly what empties it — so
/// it re-opened its wizard for ever, returning a person to step one with no error shown and no way
/// past. That bug shipped. Emptiness is a state of the library; this is a fact about the person.
///
/// **`Finish` outranks `Welcome` defensively.** It cannot be reached with `welcomed == false`,
/// because nothing mints an identity without setting the flag first — but a hand-edited settings
/// file with a half-made device in it must not be told to start over, because starting over is what
/// would mint a second iPod.
///
/// **`Finish` is *unfinished*, never *broken*.** The discriminator is `names_a_disk` and nothing
/// else: a device that names no drive at all is a run that stopped between the mint and the
/// install, and pressing ● should carry it on. A device that names a drive whose **file** has gone
/// is a different thing entirely — `Settings::missing` sees it, the cradle draws a broken ring, and
/// `why ›` explains it (§7.3, §9.3). Offering to *finish* that one would promise to rebuild a drive
/// this program did not decide to replace, which is not a decision a window makes on its own.
///
/// **Emptiness is consulted in one direction only, and that is the whole distinction.** A library
/// with devices in it plainly is not somebody's first minute with this program, so a non-empty list
/// **suppresses** the welcome — an existing library upgrading into this build must not be greeted
/// and must not be offered a second iPod. What is banned is the other direction: an *empty* list
/// must never be read as *offer the welcome*, because a cancelled or failed build is exactly what
/// empties it. Suppressing cannot loop; offering did.
fn first_run_offer(s: &Settings) -> Offer {
    if let Some(d) = work::minted(s) {
        if !d.names_a_disk() {
            return Offer::Finish { device: d.name.clone() };
        }
    }
    // A library with devices in it plainly is not somebody's first minute with this program.
    if !s.devices.is_empty() {
        return Offer::Quiet;
    }
    // **Empty, and the flag decides which of the two empty benches it is.** Both offer the press;
    // only one carries §10.1's welcome copy, and `welcomed` is what stops that copy returning. The
    // flag never removes the *action* — an empty library with no route to an iPod is §10.3's own
    // bug inverted, and it is the state a cancelled build leaves you in.
    if s.welcomed {
        Offer::Again
    } else {
        Offer::Welcome
    }
}

/// Whether the centre button on row `index` starts the **first run** rather than a device.
///
/// **Asked per press, against the library as it is now.** It used to be one boolean computed in
/// `wire` and captured by the closure, which got two things wrong at once. With an empty library it
/// was false whenever the welcome had already been shown, so the one press §9.1 promises did
/// nothing; and with a half-made first-run device *plus* a device somebody composed by hand it was
/// true for **every** row, so pressing the composed one resumed the first run instead of starting
/// it.
///
/// Two rows route to the first run and no others: the empty bench, which has no device to start,
/// and the minted-but-unfinished device, which is a run that stopped part way.
///
/// **A composed device is neither, and it used to be the second one.** [`work::minted`] identified
/// the first-run device by its synthesised boot ROM, which is exactly the shape
/// `Composer::make_one` produces, so a device somebody had just composed answered to it — and what
/// this returned `true` for, `work::Queue::press` then built from the **fixed** first-run plan,
/// consulting no `Recipe`. `minted` is where that is fixed, so this function and `first_run_offer`
/// ask one question rather than two.
fn press_is_first_run(s: &Settings, index: usize) -> bool {
    if s.devices.is_empty() {
        return true;
    }
    match work::minted(s) {
        Some(d) if !d.names_a_disk() => {
            s.devices.get(index).is_some_and(|sel| sel.name == d.name)
        }
        _ => false,
    }
}

/// Set §10.3's flag, once, and say whether this was the first time.
///
/// **The caller saves.** `wire` has a window on screen and a Rail to file a `Class::Permission` on
/// if the write fails; this has neither, and a save whose failure has nowhere to go is §20 item 13.
///
/// **Nothing in this program ever clears it** — not a cancel, not a failure, not an empty library.
fn welcome(s: &mut Settings) -> bool {
    if s.welcomed {
        return false;
    }
    s.welcomed = true;
    true
}

/// Press a failure's next step — **after checking in Rust that it is one this build can take**.
///
/// The markup already refuses a disabled `Pressable`; this checks again, because a view is not an
/// authority on what the program can do and the two must not be able to disagree.
///
/// Returns whether the step was a **retry**, because a retry only puts the entry back to `Planned`
/// — something still has to run it, and that is the caller's press.
fn take_next_step(
    rail: &Rc<RefCell<rail::Rail>>,
    stack: &Rc<RefCell<nav::Stack>>,
    composer: &Rc<RefCell<Option<composer::Composer>>>,
    id: u64,
    which: i32,
    caps: rail::Caps,
) -> bool {
    let step = {
        let r = rail.borrow();
        let Some(e) = r.find(id) else { return false };
        let Some(f) = e.failure.as_ref() else { return false };
        let mut steps = f.class.next(e.retries, caps);
        if which < 0 || which as usize >= steps.len() {
            return false;
        }
        steps.remove(which as usize)
    };
    if !step.available(caps) {
        return false;
    }
    match step {
        rail::Next::Retry => {
            return rail.borrow_mut().retry(id);
        }
        rail::Next::Devices => stack.borrow_mut().go(nav::Page::Devices, 1),
        // **§12.7, and it was the second live-but-inert control in this file.** `CancelWrite` is
        // one of the three `Next::available` returns `true` for unconditionally — it is this
        // program talking to itself — so it passed the guard above and fell into the empty arm
        // below it. A control drawn live that does nothing is the defect `docs/GUI.md` indicts
        // twice, and this one was drawn under every `SpaceMidWrite` failure.
        //
        // It goes through the same function the drawn `Cancel` on the entry goes through, because
        // two routes that both claim to cancel and only one of which does is how the first one came
        // to be wrong.
        rail::Next::CancelWrite => cancel_write(rail, id),
        // **§11.3's route out of an impossible recipe, and it goes to the page that holds one.**
        //
        // `Fix` was the first of the two live-but-inert controls: `Next::available` returned `true`
        // for it unconditionally, `Class::Incompatible` drew it live, and pressing it did nothing.
        // It was then gated on `caps.composer` — which was `false` for as long as there was no
        // Composer, and stayed `false` for the four Composer pages after that, so the control went
        // on saying *there is no Composer in this build yet* beside one. Both halves are closed
        // here: the cap is derived from `Page::Composer.slot()`, and this is the arm it required.
        //
        // **Two hops, and neither is arithmetic.** `Page::Composer.slot()` is 2 and `Stack::go`
        // never jumps a level, so `go(Composer, 2)` from a closed drawer clamps to 1, misses the
        // slot and lands on the menu — which is the guard doing its job, not a route. Devices is
        // the level-1 page the Composer is entered from everywhere else (`+ New device ›`), so it
        // is the level below this one here too, and `back` walks out the way it came in.
        //
        // **The recipe does not come with it, and that is a fact about the failure rather than a
        // shortcut.** `compose::Fix::consequence` says it plainly — *the failure Rail builds its
        // `Incompatible` class with no `Recipe` in hand* — and `rail::Entry` carries no device and
        // no recipe either. So this opens the Composer on the recipe already being composed if
        // there is one, and mints a new one if there is not. It never replaces one: a compose in
        // flight is somebody's work, and `AGENTS.md` §3 does not let a press throw it away.
        //
        // ── **NOT REACHABLE IN THE RUNNING PROGRAM, and that is written here on purpose** ────────
        //
        // The arm is correct and the gate is now honest, and no press a person can make gets here.
        // Traced rather than assumed: the only `Class::Incompatible` constructed outside a test is
        // `work::Plan::of`'s defensive refusal at `work.rs:332`, which fires when a step's verb is
        // `Verb::Copy` — and the only producer of `Verb::Copy` is `compose::Recipe::steps` under
        // `Start::FromImage` or `Start::FromDisk` (`compose.rs:913` and `:926`). `work::plan` is
        // `Verb::Synthesise`, then `work::recipe()`'s steps, then `Verb::Start`, and `recipe()`
        // hard-codes `Start::FromIpsw`. So no plan this build files can carry a `Copy`, no
        // `Incompatible` reaches the Rail, no `Fix` is ever drawn, and this arm runs never.
        //
        // **What would make it reachable is the thing `composed_and_unbuilt` names as its own
        // retirement condition**: `work::Queue` taking a `Recipe` instead of the fixed first-run
        // plan. A device composed from a drive somebody already has then reaches `Plan::of` with a
        // `Copy` step, is refused with `Class::Incompatible`, and the `Fix` on that entry is the
        // first press that arrives here. Building that path is later work and is deliberately not
        // done here.
        //
        // It is recorded rather than deleted or disabled because it is a **control enabled this
        // week that no user action can reach**, which is exactly the shape that becomes a surprise:
        // `every_next_step_this_build_offers_is_wired_to_something` asserts `Fix` is live and wired,
        // and passes, and neither half of that is a claim that anything produces the failure it
        // hangs off. Un-recorded, the next reader takes a green sweep for a route.
        rail::Next::Fix { .. } => {
            let mut held = composer.borrow_mut();
            if held.is_none() {
                *held = Some(composer::Composer::new());
            }
            drop(held);
            let mut s = stack.borrow_mut();
            s.go(nav::Page::Devices, 1);
            s.push(nav::Page::Composer);
        }
        // Every remaining arm needs a mechanism `caps` says this build does not have, so the guard
        // above has already returned. They are enumerated rather than defaulted so the day one
        // arrives the compiler points here.
        rail::Next::Provide
        | rail::Next::ChooseElsewhere
        | rail::Next::CopyDetails
        | rail::Next::Reveal => {}
    }
    false
}

/// §12.7: stop a write and delete the partial file — **one function, and both routes to it.**
///
/// The entry's drawn `Cancel` and `Class::SpaceMidWrite`'s `Cancel` next step are the same request,
/// and until now only the first of them did anything. The entry said which file this deletes and
/// how big it is *before* either control was pressed (`Entry::cancel_cost`), so pressing one is the
/// consent `AGENTS.md` §3 requires — and `Rail::cancel` hands the path back rather than deleting it
/// itself, because deciding to delete is not the Rail's to make.
///
/// **The only file this can reach is one this program wrote in this run.** `Entry::temp` is set by
/// `Rail::writing`, which is called with the partial file a step is writing; nothing else fills it,
/// and an entry with none cancels nothing.
fn cancel_write(rail: &Rc<RefCell<rail::Rail>>, id: u64) {
    let Some(p) = rail.borrow_mut().cancel(id) else {
        // **Not silence.** `Rail::cancel` declines when the entry is not holding a write — which
        // includes a step that has already **failed**, because `Rail::fail` clears `cancellable`.
        // `Class::SpaceMidWrite` offers this control on exactly such an entry, so a person can
        // press `Cancel` under *the disk filled up* and have nothing happen; saying so is the least
        // this side of the boundary can do about it. The partial file is the Rail's to release and
        // it did not, so nothing here deletes anything (`AGENTS.md` §3).
        rail.borrow_mut()
            .note("nothing was cancelled: that step is not writing a file any more");
        return;
    };
    match std::fs::remove_file(&p) {
        Ok(()) => {
            rail.borrow_mut().note(&format!("deleted {}", p.display()));
        }
        Err(e) => {
            rail.borrow_mut().failed(
                "cancel",
                &p.display().to_string(),
                rail::Failure::saying(
                    rail::Class::Permission,
                    "deleting the partial file",
                    format!("{}: {e}", p.display()),
                ),
            );
        }
    }
}

// ── The refusal, and it is the model's ───────────────────────────────────────────────────────────

/// Resolve a device and make it the live one, **or say which part of it is gone**.
///
/// `Settings::run_device` *is* the resolution step and its `false` is the refusal (§20 item 1): a
/// device whose boot ROM or drive has left the library is refused rather than silently booting a
/// substituted generated 5.5G. This asks the model which part, in the model's own words, and hands
/// back a [`rail::Failure`] rather than starting anything.
///
/// **It may block.** `Settings::missing` stats every resolved path, and a path under a stale
/// network mount blocks until the mount times out. It runs here on the UI thread, in a callback, at
/// the moment somebody pressed a button — which is the one place `Presence`'s own caller rule says
/// it must not. That is unchanged from `device_rows` and is worth moving off the thread with
/// §11.4's `detect_mounted()`; until then, a share that is not up delays the press.
fn resolve_for_start(
    settings: &mut Settings,
    index: usize,
) -> Result<String, (String, rail::Failure)> {
    let Some(d) = settings.devices.get(index).cloned() else {
        // **An empty library is reachable from the bench and is not a programming error.** §7.4
        // keeps the drawn centre button live at all times — a control that goes dead is a control
        // that teaches nothing — so pressing it with nothing on the bench lands here, and the
        // sentence has to be the one a person would want. `Class::Missing`'s own next steps are
        // *Provide* and *Devices*, and `Devices` carries `ipod-boot setup`, which is real.
        //
        // An index past the end of a NON-empty library is the other thing, and it is a defect in
        // this file: the index came from a model built out of this same list.
        let said = if settings.devices.is_empty() {
            "there are no devices in the library yet, so there is nothing to start".to_string()
        } else {
            format!(
                "there is no device {index} in the library, which is a defect in the window rather \
                 than in the library"
            )
        };
        return Err((
            String::new(),
            rail::Failure::saying(rail::Class::Missing, "starting a device", said),
        ));
    };
    // **`Settings::missing` first, and that ordering is the fix for a real divergence.**
    //
    // `run_device` resolves NAMES — `firmware` to a `Resource::Firmware`, `disk` to a drive in the
    // list — and touches no file (`settings.rs:1440-1451`). `missing` stats every resolved path.
    // So a device whose `.img` or ROM dump had been **deleted** while its entry was still listed
    // read as `Absent::Gone(path)` to the cradle — broken ring, refusal on the shelf, `why ›` — and
    // as `true` to the press. The bench and the centre button were asking two different questions
    // and giving two different answers about the same device, and the file-only case is by far the
    // commonest way a drive leaves.
    //
    // Asking `missing` here keeps the rule in the model where it belongs and makes the two routes
    // one question. `run_device`'s own refusal is still consulted below: it catches the *unlisted*
    // case, which is a name that resolves to nothing and which no `stat` can see.
    let absent = settings.missing(&d);
    if !absent.is_empty() {
        return Err((d.name.clone(), refusal(&d, &absent)));
    }
    if settings.run_device(&d.name) {
        return Ok(d.name);
    }
    Err((d.name.clone(), refusal(&d, &absent)))
}

/// §7.5's row-1 trailing slot, and §12.2's own word for the phase this build is in.
///
/// §12.2's table gives that slot `off` / `booting · 62 %` / `running · 14.2 M instr/s` / `stopped`,
/// and §7.5's drawing shows `parked · 4 min ago` — *the state, and time since*. It used to carry
/// `no boot time learned yet`, which is a fact about the **progress bar's denominator** rather than
/// about the machine, and `phase()` — which already answers `Off` — reached this slot nowhere.
///
/// **Retirement condition**: when the bench holds an `emu::Link`, the first arm reads `Out.phase`
/// and the remaining three of §12.2's four rows arrive with it. `parked` is `parked_at.is_some()`
/// and is the model's; `parked_for` turns it into *time since*.
fn shelf_state(d: &Device) -> String {
    let word = match phase() {
        emu::Phase::Off => "off",
        emu::Phase::Booting { .. } => "booting",
        emu::Phase::Running => "running",
        emu::Phase::Stopped(_) => "stopped",
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    match eapp_loader::settings::parked_for(d, now) {
        Some(secs) => format!("{word}, parked {}", ago(secs)),
        None => word.to_string(),
    }
}

/// *Time since*, rounded to the unit somebody would say out loud.
///
/// The clock can be behind the timestamp — a settings file carried between machines, or one written
/// before a clock correction — and `parked_for` saturates rather than wrapping, so zero is the
/// honest answer for that and reads as *just now* rather than as a negative age.
fn ago(secs: u64) -> String {
    match secs {
        0..=89 => "just now".into(),
        90..=5399 => format!("{} min ago", (secs + 30) / 60),
        5400..=86_399 => format!("{} h ago", (secs + 1_800) / 3_600),
        _ => format!("{} days ago", (secs + 43_200) / 86_400),
    }
}

/// The sentence a refused device gets, built from `Absent` and nothing else.
///
/// §9: a failure names **what** is wrong, never that something is wrong. `Absent::label` is the
/// entry's name or the file's name — not its path, which does not fit the cradle's 24 px row — and
/// the path goes on the end where there is one, because that is the whole of the answer for a file
/// that moved.
fn refusal(d: &Device, absent: &[Absent]) -> rail::Failure {
    rail::Failure::saying(
        rail::Class::Missing,
        format!("starting {}", d.name),
        gone_sentence(d, absent),
    )
}

/// **One sentence, two places.** The cradle says it before the press (§7.3) and the Rail entry says
/// it after (§9.3), and they are the same string because a cradle that said something else would be
/// a second account of the same fact.
fn gone_sentence(d: &Device, absent: &[Absent]) -> String {
    if absent.is_empty() {
        // `run_device` refused and `missing` names nothing. Say exactly that rather than inventing
        // a part: the two functions disagreeing is a defect, and a sentence that guessed which part
        // would hide it.
        return format!(
            "{} could not be resolved, and the library does not say which part is gone.",
            d.name
        );
    }
    let mut parts: Vec<String> = Vec::new();
    for a in absent {
        parts.push(match a.path() {
            Some(p) => format!("{} is not where it was — {}", a.label(), p.display()),
            None => format!("{} is not in the library any more", a.label()),
        });
    }
    format!("{}.", parts.join(", and "))
}

/// Whether this is a device the Composer filed whose drive this program cannot yet make.
///
/// **One boolean, consulted by the ring, the label and the press**, for the reason `empty_device`
/// gives for its own: three answers to *can this be pressed* is how two of them come to disagree
/// silently. `work::Queue` has no compose entry — a device composed here is saved and not built,
/// and nothing in this build can build it — so §14.1 applies exactly as written: the control is
/// drawn, refused, and says why, rather than being drawn live over a route that does something
/// else.
///
/// **Retirement condition**, in the shape `research/04` uses: when `work::Queue` takes a `Recipe`,
/// this predicate is deleted and the press routes a composed device into that queue. Nothing here
/// becomes true again — it becomes a build.
fn composed_and_unbuilt(d: &Device) -> bool {
    d.composed && !d.names_a_disk()
}

/// §7.3's cradle label: **what pressing will cost, or why it cannot be pressed**, before it is
/// pressed.
///
/// §7.3's own table is the machine's — parked, cold boot, first run, a title — and every one of its
/// rows needs a `Config` this build does not hold, so this is the two states it *can* answer
/// truthfully today. The startable line says what pressing actually does in this build rather than
/// what it will do: `on_start_device` resolves the device and then files a note saying running is
/// not wired, and a cradle promising *to start* would be the window making a claim the program does
/// not keep.
///
/// **Retirement condition**, in the shape `research/04` uses: when the bench holds an `emu::Config`,
/// this becomes §7.3's table — `may_restore()` for the parked line, `boot_instructions` for the cold
/// one — and the second arm here is deleted with it.
///
/// **The startable line fits [`geometry::CRADLE_LABEL_MAX_CHARS`], and it did not.** It read *Press
/// the centre button — running is not wired to the window yet*: 63 characters against a budget of
/// 48, on the one line the whole bench is built around, eliding unmeasured at every window size this
/// program allows. `every_typed_cradle_label_fits_its_own_row` is what keeps the next one honest.
/// The refusal arm is deliberately **not** held to the budget — `gone_sentence` carries a path, and
/// §7.3's own note says the path goes on the end where there is one, so that line elides by design
/// and the first words are what survive it.
fn cradle_label(d: &Device, absent: &[Absent]) -> String {
    if !absent.is_empty() {
        return gone_sentence(d, absent);
    }
    // **Before the unfinished arm, because a composed device is unfinished in the same way and the
    // remedy below is not its remedy.** See [`composed_and_unbuilt`].
    if composed_and_unbuilt(d) {
        return "Building a composed device is not wired yet".into();
    }
    // **§10.3: a first run that stopped part-way must not be captioned with a promise to start.**
    // A device that names no drive is *unfinished*, not *broken* — `Settings::missing` sees nothing
    // wrong with it, so without this arm the cradle read *press the centre button* on an iPod with
    // no disk, and the press would then quietly resume a build the label said nothing about.
    // Pressing it does resume; this is the label saying so first.
    if !d.names_a_disk() {
        return format!("Press the centre button to finish making {}", d.name);
    }
    "Press the centre button — running is not wired".into()
}

/// §9.1's empty bench, as a whole row, so the markup composes it exactly like a real device.
///
/// **Every string here is this file's rather than the markup's**, which is the same rule the other
/// eight fields follow. The chassis is the model's own `Unspecified` — a neutral case, deliberately
/// not black, because drawing an unknown iPod black invents a fact about somebody's device.
/// **Two sets of words, one row.** `first` selects §10.1's welcome copy over §9.1's later-empty
/// copy; every other field is identical between them, because they are the same bench in the same
/// state and only the sentence a person needs is different.
///
/// The chassis stays `Colour::Unspecified` `#E4E4E2` in both. That is the ghost's colour and it was
/// already right — what makes the drawing a ghost is the **opacity**, which is a separate property
/// (`MainWindow.ghost`), because a real device whose ROM did not state a colour is drawn in this
/// same neutral case and must not be ghosted.
///
/// **Row 3's trailing slot is not drawn here and needs no rule.** `bench.slint` wraps it in
/// `if !root.drawer-open`, and §10.1 has the drawer open — which is fortunate, because
/// §10.1's `MENU › Parts, if you have files · or drop them here` names a mechanism §16.4 defers,
/// and `caps().drop_target` is false. A phantom route in the one place §19.1 calls it fatal.
fn empty_device(first: bool, caps: rail::Caps, cost: compose::Cost) -> DeviceRow {
    let si = eapp_loader::si;
    DeviceRow {
        name: if first { "No iPod yet".into() } else { "No devices yet".into() },
        summary: if first {
            // §10.1, and the clause naming the machine is not decoration: the two facts a person
            // needs before agreeing to anything are *you do not have to own one* and *this is what
            // you will get*. The words come from the model table, so this line and the plan's own
            // synthesise sub-line cannot drift apart.
            match eapp_loader::identity::Model::lookup(compose::FIRST_RUN_MODEL) {
                Some(m) => format!(
                    "You do not need an iPod, or any files off one. The centre button makes one: \
                     a {}, {} GB, {}.",
                    m.generation.label(),
                    m.capacity_gb,
                    m.colour().label().to_lowercase()
                )
                .into(),
                None => "You do not need an iPod, or any files off one. The centre button makes \
                         one."
                    .into(),
            }
        } else {
            "The centre button makes one; ipod-boot setup composes one from files you have.".into()
        },
        state: "nothing mounted".into(),
        // §7.5's rule is that row 3 never goes away, and with no device there is nothing to write
        // to — so the line that normally says what will be written says what it will **cost**, and
        // the number is still first on the line, which is what survives a hard elide.
        //
        // **Built from the plan's own `Cost`, never typed.** One number per axis, and both come
        // from `Recipe::steps()` by way of `work::cost` — which is the same call `push_ledger`
        // makes, so the shelf and the ledger cannot print two different bills for one press.
        write_target: if cost.down == 0 {
            "nothing to download".into()
        } else {
            format!("{} to download, about {} on disk.", si(cost.down), si(cost.disk)).into()
        },
        write_target_is_warning: false,
        chassis: chassis_colour(Colour::Unspecified),
        dark_chassis: is_dark(Colour::Unspecified),
        // **The same measurement the press consults**, so the cradle's ring, its
        // `accessible-enabled` and what pressing actually does all come from one boolean rather than
        // three. §7.4 is untouched: the drawn centre button and the cradle's Enter/Space handler
        // stay ungated, so pressing a refused cradle still produces the sentence saying why.
        //
        // **It arrives on `caps` rather than being probed here**, and that is not tidiness: this
        // function is called from `refresh_devices`, which runs on every completed step of a build,
        // and `tooling::can_download` **spawns a process**. Probing here put a `curl --version`
        // inside a display refresh — and, worse, made a second source for a fact `wire` had already
        // measured, so a `curl` that went away mid-session would leave the ring and the press
        // disagreeing with nothing to say which was right.
        //
        // **`first` is gone from this expression, and that was the fatal half of it.** An empty
        // bench is an empty bench: §9.1's later-empty row and §10.1's welcome row are the same
        // bench with different copy, and §10.3 says both routes are offered equally. With `first &&`
        // here, a person who had opened the program once and closed it had the cradle drawn
        // unpressable for ever, with `press_is_first_run` on the other side saying the press would
        // have worked. One boolean, or they disagree silently.
        startable: caps.download,
        // Both fit [`geometry::CRADLE_LABEL_MAX_CHARS`]; the 58-character sentence this replaced
        // elided on every window this program allows, which took the escape hatch off the end of
        // the one line that carried one.
        //
        // §7.3 asks for **what pressing will cost, or why it cannot be pressed** — so the refusal
        // arm is keyed on `startable` and not on `first`. Without it the cradle was drawn `fg-dim`
        // and non-interactive under a label promising a press, and its `accessible-description` is
        // that same label: a keyboard user focusing it was told nothing at all, which is what §9.4
        // and §16.5 exist to prevent. The remedy itself is `Class::ToolMissing`'s command, filed on
        // the plan; this is the cradle saying which of the two states it is in.
        //
        // **One label for both empty benches**, because §9.1 gives the later-empty one the same
        // cradle line §10.1 gives the first: `press ● to make an iPod`. The `No iPod yet. Compose
        // one: ipod-boot setup` this replaced was the sentence that made the second bench a dead
        // end — it named the escape hatch and dropped the route.
        cradle_label: if caps.download {
            "Press the centre button to make an iPod".into()
        } else {
            "No curl, so nothing can be downloaded".into()
        },
    }
}

/// §7.3: what an assistive technology is told is on the panel.
///
/// It describes the **machine**, never the program — the drawn device is `AccessibleRole::Image` and
/// the cradle is the Button. With no machine the panel is dark, which is the state a 5G with a flat
/// battery is in, and saying so is more use than "iPod screen".
fn panel_description(p: &emu::Phase) -> &'static str {
    match p {
        emu::Phase::Off => "The panel is dark. No machine is running.",
        emu::Phase::Booting { .. } => "The panel is showing the boot sequence.",
        emu::Phase::Running => "The panel is showing what the machine is drawing.",
        // A machine that stopped and will not resume. The sentence saying **why** is the model's and
        // belongs on the cradle label and in the Rail, not in a description of a picture.
        emu::Phase::Stopped(_) => "The panel is dark. The machine stopped.",
    }
}

// ── Pushing (one direction only) ─────────────────────────────────────────────────────────────────

/// Tell the markup what size to draw at. **One direction only** — nothing reads these back.
fn push_fit(window: &MainWindow, fit: &fit::Fit, sf: f64) {
    window.set_hero(fit.hero_logical as f32);
    window.set_screen_w(fit.panel_w as f32);
    window.set_screen_h(fit.panel_h as f32);
    window.set_screen_scale(fit.k);
    window.set_too_short(fit.too_short);
    window.set_fidelity(fidelity(fit.k, sf).into());
    window.set_select_d(select_d(fit.hero_logical));
}

/// §7.4's centre-button hit region, **from `wheel.rs` and nowhere else**.
///
/// `WheelRing::select` is `outer × 0.465`, which is 39 % wider than `CENTRE_D`'s drawn disc. Two
/// rules for that one region is precisely what §7.4 records as having disagreed with each other and
/// with the model, so the diameter is computed here from the same struct `WheelRing::hit` uses and
/// pushed in; neither `window.slint` nor `ipod.slint` may write the ratio.
fn select_d(hero_logical: f64) -> f32 {
    let outer = (geometry::WHEEL_D * hero_logical / 2.0) as f32;
    2.0 * wheel::WheelRing::new(0.0, 0.0, outer).select
}

/// §7.5's row-2 trailing slot: *a number a bug report can quote*.
///
/// It names the display scale only where it differs from `k`, because otherwise the two numbers are
/// the same fact written twice and the shorter line is the one that fits the narrowed measure.
///
/// **ASCII, and the `·` and `×` it used to carry are the point.** This string is drawn on shelf row
/// 2, twenty pixels above a row 3 that goes to the trouble of drawing `·` as a `Path` because §6.7
/// considers it unproven — and `·` is the exact character §6.7 names as the one the shipped window
/// built into UI strings with no coverage gate at all. One band cannot have two answers to one
/// question, and Rust has no drawn-Path escape hatch, so Rust types ASCII.
fn fidelity(k: i32, sf: f64) -> String {
    let scale_pct = (sf * 100.0).round();
    if (scale_pct - 100.0).abs() < 0.5 {
        format!("panel {k}x, 320x240, nearest neighbour")
    } else {
        format!("panel {k}x, 320x240 physical, display scale {scale_pct:.0} %")
    }
}

/// Where the drawer is, as three `in` properties the markup never writes.
fn push_nav(window: &MainWindow, stack: &nav::Stack) {
    window.set_drawer_open(stack.open());
    window.set_drawer_depth(stack.depth());
    window.set_drawer_page(to_markup(stack.page()));
}

/// §16.9: **mutate the retained model, never replace it.**
///
/// Rows that already exist are written in place, new ones are pushed, and the tail is removed from
/// the end. Handing `set_rail` a fresh `VecModel` instead would tear down and reconstruct every
/// repeater instance — losing focus, hover and any in-flight animation — on every byte of progress.
fn sync_rail(
    window: &MainWindow,
    rows: &Rc<VecModel<RailRow>>,
    rail: &rail::Rail,
    caps: rail::Caps,
    // **What the queue is doing** — `Queue::shape`, not `showing_welcome`. Those were the same
    // boolean and are two different questions: §9.1's later-empty bench carries the plan with no
    // welcome copy at all, and keying the heading on the welcome left five `Planned` rows sitting
    // under *This is what happened.*
    queue: work::Shape,
) {
    let want: Vec<RailRow> = rail.entries().iter().map(|e| to_row(e, caps)).collect();
    for (i, row) in want.iter().enumerate() {
        match rows.row_data(i) {
            Some(old) if old == *row => {}
            Some(_) => rows.set_row_data(i, row.clone()),
            None => rows.push(row.clone()),
        }
    }
    while rows.row_count() > want.len() {
        rows.remove(rows.row_count() - 1);
    }
    window.set_rail_failures(rail.failures() as i32);
    window.set_rail_announce(rail.announce().into());
    // §6.5: one material per page. The Rail knows its own order, so the index is computed once here
    // rather than every repeater instance searching the model for itself.
    window.set_rail_first_failure(
        rail.entries()
            .iter()
            .position(|e| e.kind == rail::Kind::Failed)
            .map_or(-1, |i| i as i32),
    );
    // §7.5's row 2. Empty means the Rail has nothing to say and the device's facts stand.
    window.set_rail_line(rail.line().unwrap_or_default().into());
    // §9.2, §12.3: the shelf's 3 px bar, **and this is the first thing that has ever bound it**.
    // `Bench.progress` has been declared and drawn since the drawer landed with nothing behind it —
    // a drawn instrument with no producer, which is §20 item 15's defect.
    //
    // **The sign is the whole contract.** `Entry::fraction()` is negative for `Progress::None` and
    // for a `Bytes` with a zero denominator, and `bench.slint` reads `progress >= 0` as *there is a
    // bar*. So a step with no honest denominator draws a number that moves and no bar, and a failure
    // takes the bar away rather than freezing it at a fraction — a frozen bar is a paused machine
    // pretending.
    //
    // The **last** working entry, not the first: a plan runs in order, so the last one to have
    // started is the one under way.
    window.set_progress(
        rail.entries()
            .iter()
            .rev()
            .find(|e| e.kind == rail::Kind::Working)
            .map_or(-1.0, |e| e.fraction()),
    );
    // **§9.1's heading is pushed from HERE, not once at startup.** It was set by `push_ledger` and
    // never again, so the Work page read *"Nothing is happening."* above a warning icon and a
    // paragraph naming a missing file — and that page is the one the drawer auto-opens onto when a
    // press is refused, so it was the first thing anybody saw.
    let (heading, empty) = work_page_text(rail, queue);
    window.set_work_heading(heading.into());
    window.set_work_empty(empty.into());
    // §9.2's other half. The shelf's bar and its rail line moved while a build ran; the cradle did
    // not, so the one line the whole bench is built around read *Press the centre button to finish
    // making My 5.5G* for the entire download — a promise to press, on a machine already busy
    // doing it. It comes from the same place the shelf's line does, so the two cannot disagree.
    window.set_working_label(working_label(rail).into());
}

/// §9.2's cradle line — `making an iPod — 41 % — fetch Apple's firmware` — or empty.
///
/// **Generic, and the device is deliberately not named.** The cradle row is one elided line held to
/// [`geometry::CRADLE_LABEL_MAX_CHARS`], and a name somebody typed has no length. What a person
/// needs here is the same three things §9.2 asks for: that something is happening, how far, and
/// what — and the Rail beside it names the device.
///
/// The separator is the em dash and not §9.2's own `·`: U+00B7 is outside §16.6's closed glyph set,
/// and `no_ui_string_carries_a_glyph_outside_the_closed_set` sweeps this file.
fn working_label(rail: &rail::Rail) -> String {
    let Some(e) = rail.entries().iter().rev().find(|e| e.kind == rail::Kind::Working) else {
        return String::new();
    };
    let what = format!("{} {}", e.verb, e.what).trim().to_string();
    let f = e.fraction();
    // A negative fraction is *no denominator* rather than nothing done (§12.3), so the percentage
    // is dropped and the sentence still says what is under way.
    if f < 0.0 {
        format!("making an iPod — {what}")
    } else {
        format!("making an iPod — {:.0} % — {what}", f * 100.0)
    }
}

/// One Rail entry, flattened for the markup.
///
/// **The failure class is not carried across.** It decides the wording and the next steps, both of
/// which are strings by the time they get here, and `rail-next(id, n)` already identifies which
/// control was pressed — so Rust looks the entry up rather than being told what it already knows.
fn to_row(e: &rail::Entry, caps: rail::Caps) -> RailRow {
    let steps = e
        .failure
        .as_ref()
        .map(|f| f.class.next(e.retries, caps))
        .unwrap_or_default();
    let at = |n: usize| steps.get(n);
    let label = |n: usize| at(n).map(|s| s.label()).unwrap_or_default();
    let enabled = |n: usize| at(n).is_some_and(|s| s.available(caps));
    let reason = |n: usize| at(n).map(|s| s.reason().to_string()).unwrap_or_default();
    let escape = |n: usize| at(n).map(|s| s.escape_hatch(caps)).unwrap_or_default();
    let presses = |n: usize| at(n).map(|s| i32::from(s.presses())).unwrap_or(1);
    let consequence = |n: usize| at(n).map(|s| s.consequence()).unwrap_or_default();

    RailRow {
        id: e.id as i32,
        kind: match e.kind {
            rail::Kind::Planned => RailKind::Planned,
            rail::Kind::Working => RailKind::Working,
            rail::Kind::Done => RailKind::Done,
            rail::Kind::Failed => RailKind::Failed,
            rail::Kind::Cancelled => RailKind::Cancelled,
            rail::Kind::Note => RailKind::Note,
        },
        verb: e.verb.clone().into(),
        what: e.what.clone().into(),
        sub: e.sub.clone().into(),
        measure: e.measure().into(),
        fraction: e.fraction(),
        happened: e.happened().into(),
        // §9.3's last row. `ToolMissing` is the one class with no next step, and what it carries
        // instead is a command a person can paste. It had no field here, so `Class::mono_remedy()`
        // reached no pixel.
        mono: e
            .failure
            .as_ref()
            .map(|f| f.class.mono_remedy())
            .unwrap_or_default()
            .into(),
        cancellable: e.cancellable,
        cancel_cost: e.cancel_cost().into(),
        dismissible: e.dismissible,
        next_a_label: label(0).into(),
        next_a_enabled: enabled(0),
        next_a_reason: reason(0).into(),
        next_a_escape: escape(0).into(),
        next_a_presses: presses(0),
        next_a_consequence: consequence(0).into(),
        next_b_label: label(1).into(),
        next_b_enabled: enabled(1),
        next_b_reason: reason(1).into(),
        next_b_escape: escape(1).into(),
        next_b_presses: presses(1),
        next_b_consequence: consequence(1).into(),
    }
}

/// §10.1's ledger, pinned under the Work page.
///
/// **One number per axis, and both come from `Recipe::steps()`** — by way of `work::cost`, which is
/// the same call the shelf's row 3 makes, so the two surfaces cannot print two different bills for
/// one press. An earlier revision put three different sizes for one operation on the one screen
/// principle 7 was written for.
///
/// **The free-space clause arrived with its own measurement**, which was its stated retirement
/// condition: nothing in this tree could query free bytes, so `312 GB free on …` would have been
/// invented. `eapp_loader::volume::space` measures it now — and returns `None` where nothing could
/// say, in which case the clause is **absent** rather than zero. An unmeasured volume states
/// nothing and warns about nothing.
///
/// **The ledger does not move while a download runs.** It is the plan's cost, not its progress; a
/// ledger that counted down would be a second progress indicator on the one page that already has
/// the Rail's.
fn push_ledger(
    window: &MainWindow,
    cost: Option<compose::Cost>,
    cache: &std::path::Path,
    space: Option<&volume::Space>,
) {
    let (download, disk, warn) = ledger_lines(cost, space);
    window.set_ledger_download(download.into());
    window.set_ledger_disk(disk.into());
    window.set_ledger_note(cache_note(cache).into());
    window.set_ledger_warn(warn);
}

/// **§10.1's two figures, as strings, with no window in sight** — the download line, the disk line,
/// and whether the second one is a warning.
///
/// Lifted out of [`push_ledger`] whole, because §11.2's Composer root prints the same bill above
/// `Create` that the Work page prints under the plan, and **two pieces of arithmetic that are
/// supposed to agree, don't**. There is one of them; the two surfaces are two callers.
///
/// It takes a `Cost` and a measured `Space` and nothing else. `cache_note` is deliberately *not*
/// folded in: it reads a directory, so it belongs on the caller's side of the line between a pure
/// function and one that touches a filesystem — and only one of the two surfaces draws it.
fn ledger_lines(
    cost: Option<compose::Cost>,
    space: Option<&volume::Space>,
) -> (String, String, bool) {
    let si = eapp_loader::si;
    let Some(cost) = cost else {
        // No plan, so no figure. Saying there is none is the honest line; printing `0 B` would read
        // as a free download.
        return ("Nothing to download".into(), "Nothing to build".into(), false);
    };
    let download = if cost.down == 0 {
        // The catalogue lost the release. `0 B to download` would read as free.
        "nothing to download".to_string()
    } else {
        format!("{} to download", si(cost.down))
    };
    let free = match space {
        Some(s) => format!(" — {} free on {}", si(s.free), s.mount),
        // **Never an invented figure**, and never a zero: `volume::space` answers `None` for a
        // permission, a missing tool or a line it could not parse, and none of those is an
        // observation about somebody's disk.
        None => String::new(),
    };
    let disk = format!("about {} on disk{free}", si(cost.disk));
    // The warn colour is only ever shown against a figure somebody measured, which is why it reads
    // `is_some_and` rather than defaulting to true when nothing could be measured.
    let warn = space.is_some_and(|s| s.free < cost.disk.saturating_add(work::HEADROOM));
    (download, disk, warn)
}

/// §10.1's third ledger line — **checked rather than asserted.**
///
/// *Nothing has been downloaded yet.* is false the moment the bundle is already in the cache, and a
/// ledger that says it anyway is the first sentence of this program a person reads being wrong.
/// Reads a directory, so it is called from `wire` and from a press, never from a binding.
fn cache_note(cache: &std::path::Path) -> String {
    let held = std::fs::read_dir(cache)
        .map(|d| {
            d.flatten()
                .filter(|e| e.path().extension().is_some_and(|x| x == "ipsw"))
                .count()
        })
        .unwrap_or(0);
    match held {
        0 => "Nothing has been downloaded yet.".into(),
        1 => "One bundle is already downloaded.".into(),
        n => format!("{n} bundles are already downloaded."),
    }
}

/// §9.1's two slots on the Work page — **and it is derived from the Rail's own state**, so the
/// heading cannot go on saying *Nothing is happening.* above a failure.
///
/// **The heading is never empty**, and that is a layout fact rather than a preference: the Work
/// page draws its heading unconditionally, and `visible: false` still reserves the slot in a Slint
/// layout (§16.3), so an empty string is a `title`-height hole plus its spacing rather than
/// nothing. With nothing on the Rail the two slots read as §9.1's one sentence — *Nothing is
/// happening. Fetches, builds and installs report here.* — with the state in the heading and what
/// the surface is for underneath it, which is the shape §9.1 asks for. The empty line is drawn only
/// where it is true; the heading always is.
///
/// §10.1's heading arrived with the plan: *This is what pressing the centre button does*, above the
/// five steps and the ledger, **before** anything has been downloaded. It is the last arm rather
/// than the first, so a first run that failed reads *One thing failed.* and not the plan heading —
/// a page that offered the plan again above a failure would be the window ignoring what it just did.
///
/// ASCII, and not a compromise: this is a Rust string drawn by a `Text`, §6.7's answer for a symbol
/// is that it is **drawn** as a `Path`, and Rust has no drawn-Path escape hatch.
fn work_page_text(rail: &rail::Rail, queue: work::Shape) -> (String, &'static str) {
    let empty = "Fetches, builds and installs report here.";
    let failures = rail.failures();
    let planned = rail.entries().iter().filter(|e| e.kind == rail::Kind::Planned).count();
    let done = rail.entries().iter().filter(|e| e.kind == rail::Kind::Done).count();
    let heading = if failures == 1 {
        "One thing failed.".to_string()
    } else if failures > 1 {
        format!("{failures} things failed.")
    // **A worker running is work, whether or not a step has reported yet.** Between the press and
    // the worker's first `Started` there is no `Working` entry at all, so the heading read *This is
    // what happened.* over a run that had just begun, with the Rail beside it announcing
    // `1 of 5 done.` And the mirror of it: at the end there is one `Planned` step left — the boot —
    // and reading *that* as work in progress claimed a run was under way after it had finished.
    // Neither is a question the Rail can answer; the queue can.
    } else if rail.entries().iter().any(|e| e.kind == rail::Kind::Working) || queue.running {
        "Working.".to_string()
    } else if queue.has_plan && planned > 0 && done == 0 {
        "This is what pressing the centre button does".to_string()
    } else if rail.entries().is_empty() {
        "Nothing is happening.".to_string()
    } else {
        "This is what happened.".to_string()
    };
    (heading, empty)
}

/// The drawer's pages, across the boundary. **Exhaustive both ways**, so a page added on either
/// side is a compile error rather than a silent `none`.
fn to_markup(p: nav::Page) -> DrawerPage {
    match p {
        nav::Page::None => DrawerPage::None,
        nav::Page::Devices => DrawerPage::Devices,
        nav::Page::Parts => DrawerPage::Parts,
        nav::Page::Games => DrawerPage::Games,
        nav::Page::Work => DrawerPage::Work,
        nav::Page::Readout => DrawerPage::Readout,
        nav::Page::Settings => DrawerPage::Settings,
        nav::Page::Reference => DrawerPage::Reference,
        nav::Page::Composer => DrawerPage::Composer,
        nav::Page::ComposerIpod => DrawerPage::ComposerIpod,
        nav::Page::ComposerRuns => DrawerPage::ComposerRuns,
        nav::Page::ComposerName => DrawerPage::ComposerName,
    }
}

fn from_markup(p: DrawerPage) -> nav::Page {
    match p {
        DrawerPage::None => nav::Page::None,
        DrawerPage::Devices => nav::Page::Devices,
        DrawerPage::Parts => nav::Page::Parts,
        DrawerPage::Games => nav::Page::Games,
        DrawerPage::Work => nav::Page::Work,
        DrawerPage::Readout => nav::Page::Readout,
        DrawerPage::Settings => nav::Page::Settings,
        DrawerPage::Reference => nav::Page::Reference,
        DrawerPage::Composer => nav::Page::Composer,
        DrawerPage::ComposerIpod => nav::Page::ComposerIpod,
        DrawerPage::ComposerRuns => nav::Page::ComposerRuns,
        DrawerPage::ComposerName => nav::Page::ComposerName,
    }
}

/// §16.6: Slint takes **one** `font-family` per element with no fallback list, and appends only
/// `SansSerif` then `SystemUi` (`i-slint-common-1.17.1/sharedfontique.rs:188-192`).
///
/// An unknown family renders in sans-serif with no error at all, so this names a face that is part
/// of the operating system rather than one somebody might have installed. Nothing in `.slint` can
/// ask whether a glyph exists, which is the other half of the same limitation.
fn mono_family() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "Menlo"
    }
    #[cfg(target_os = "windows")]
    {
        "Consolas"
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        "DejaVu Sans Mono"
    }
}

// ── §11.2's Composer, across the boundary ───────────────────────────────────────────────────────
//
// One flattener per boundary struct, each one total and each one dumb: it renames fields and turns
// a Rust `String` into a `SharedString`, and it decides nothing. Every sentence on every row was
// worded in `composer.rs` out of `compose.rs`, which is what
// `every_composer_sentence_comes_from_the_model_or_composer_rs` holds — a flattener that computed
// anything would be the window having an opinion about a recipe, which §6 of the contract forbids
// in the one place it is easiest to slip in.

fn to_fix(f: &composer::FixRow) -> FixRow {
    FixRow {
        label: f.label.clone().into(),
        enabled: f.enabled,
        reason: f.reason.clone().into(),
        escape_hatch: f.escape.clone().into(),
        machine_rule: f.machine_rule,
        presses: i32::from(f.presses),
        consequence: f.consequence.clone().into(),
    }
}

/// One line inside an expanded row, for **both** pages that draw one.
///
/// Written here, once, in the same pass that froze `parts::Detail`: `push_parts` and
/// `push_devices_detail` are written by different hands and a flattener each would be two answers
/// to *what is a `DetailRow`*.
///
/// It renames fields and decides nothing. The one thing that looks like a decision is not one:
/// `machine_rule` comes from the `Detail` rather than from the `FixRow` inside it because
/// `DetailRow` has exactly one such property and the markup binds it in both renderings —
/// `parts.slint:63` on the act, and the paragraph branch below it. One property, one producer.
#[allow(dead_code)] // retired when: `push_parts` or `push_devices_detail` calls it — the two `push_*` land with their producers
fn to_detail(d: &parts::Detail) -> DetailRow {
    let (action, has_action, act) = match &d.action {
        Some((a, f)) => (a.as_i32(), true, f.clone()),
        None => (0, false, composer::FixRow::default()),
    };
    DetailRow {
        label: d.label.clone().into(),
        value: d.value.clone().into(),
        mono: d.mono,
        machine_rule: d.machine_rule,
        action,
        has_action,
        act_label: act.label.into(),
        enabled: act.enabled,
        reason: act.reason.into(),
        escape_hatch: act.escape.into(),
        presses: i32::from(act.presses),
        consequence: act.consequence.into(),
    }
}

fn to_pick(p: &composer::Pick) -> PickRow {
    PickRow {
        field: p.field.as_i32(),
        label: p.label.clone().into(),
        value: p.value.clone().into(),
        // **`enabled` is the negation of `locked` and not a second decision.** §11.1: a locked
        // picker stays a picker — same control, same position, same height — so the row is drawn
        // and greyed with its reason rather than replaced by a line of text.
        enabled: !p.locked,
        locked: p.locked,
        note: p.note.clone().into(),
        reason: p.reason.clone().into(),
        escape_hatch: p.escape.clone().into(),
        machine_rule: p.machine_rule,
        chevron: true,
        open: p.open,
    }
}

fn to_choice(c: &composer::Choice) -> OptionRow {
    OptionRow {
        id: c.id as i32,
        label: c.label.clone().into(),
        sub: c.sub.clone().into(),
        enabled: c.enabled,
        chosen: c.chosen,
        reason: c.reason.clone().into(),
        escape_hatch: c.escape.clone().into(),
        machine_rule: c.machine_rule,
    }
}

/// **The masking boundary, at the one place it crosses into the toolkit.**
///
/// `raw` is `composer::Secret::editable()`'s answer, which is `None` while masked — so while a
/// field is masked the markup holds no identifier at all, and neither does the accessible tree,
/// which captions `value`. There is no arm here that reaches for the full string.
fn to_field(f: &composer::FieldState) -> FieldRow {
    FieldRow {
        field: f.field.as_i32(),
        label: f.label.clone().into(),
        value: f.value.clone().into(),
        raw: f.raw.clone().into(),
        masked: f.masked,
        locked: f.locked,
        mono: f.mono,
        note: f.note.clone().into(),
        reason: f.reason.clone().into(),
        action: f.action.clone().into(),
    }
}

fn to_tick(t: &composer::Tick) -> TickRow {
    TickRow {
        // The ordinal in `Os::ALL`, which is what `composer-tick(int, bool)` hands back — the
        // markup never names a system and the vocabulary stays in Rust.
        os: compose::Os::ALL.iter().position(|o| *o == t.os).unwrap_or(0) as i32,
        label: t.label.clone().into(),
        on: t.on,
        enabled: t.enabled,
        reason: t.reason.clone().into(),
        escape_hatch: t.escape.clone().into(),
    }
}

fn to_plan(s: &compose::Step) -> PlanRow {
    PlanRow {
        verb: s.verb().into(),
        what: s.what().into(),
        sub: s.sub().into(),
    }
}

fn to_refusal(r: &composer::Refused) -> RefusalRow {
    RefusalRow {
        why: r.why.clone().into(),
        has_fix: r.fix.is_some(),
        fix: r.fix.as_ref().map(to_fix).unwrap_or_default(),
        // §16.11: an Expand that opens below the fold scrolls its own top edge into view. Which one
        // is open is `nav::Stack`'s, not this row's, and this phase draws the refusal paragraph
        // open — there is at most one and it is the thing the page is about.
        open: true,
    }
}

/// Everything the four Composer pages read, pushed **in place** (§16.9).
///
/// **Never a fresh `VecModel`.** Handing `set_composer_picks` a new model tears down every repeater
/// instance under it, and this function runs on every keystroke in the serial field — so a fresh
/// model would take focus, hover and the caret with it on every character typed.
///
/// It is called from a callback and from the tick, **never from a binding**: it calls
/// `Settings::missing` by way of `Composer::which`, and `Presence`'s own rule is that a `stat` may
/// block on a stale network mount.
#[allow(clippy::too_many_arguments)] // one argument per retained model, which is what §16.9's in-place rule costs; bundling them changes nothing but the count
fn push_composer(
    window: &MainWindow,
    c: &composer::Composer,
    settings: &Settings,
    building: bool,
    // **A fact about the build, carried in rather than assumed.** `Composer` is toolkit-free and
    // knows nothing of `rail::Caps`; `wire` holds both, so the conversion happens here and the copy
    // control is only drawn live where there is somewhere for it to copy to.
    clipboard: composer::Clipboard,
    space: Option<&volume::Space>,
    picks: &Rc<VecModel<PickRow>>,
    fields: &Rc<VecModel<FieldRow>>,
    ticks: &Rc<VecModel<TickRow>>,
    opts: &Rc<VecModel<OptionRow>>,
    plan: &Rc<VecModel<PlanRow>>,
    refusals: &Rc<VecModel<RefusalRow>>,
) {
    let root = c.root(settings, building);
    let which = c.which(settings, building, clipboard);
    let runs = c.runs(settings, building);
    let named = c.named(settings, building);

    window.set_composer_title(
        match c.mode() {
            composer::Mode::New => "New device".to_string(),
            composer::Mode::Editing { device } => device.clone(),
        }
        .into(),
    );
    window.set_composer_which_value(root.which.value.clone().into());
    window.set_composer_runs_value(root.runs.value.clone().into());
    window.set_composer_named_value(root.named.value.clone().into());
    window.set_composer_which_enabled(root.which.enabled);
    window.set_composer_runs_enabled(root.runs.enabled);
    window.set_composer_named_enabled(root.named.enabled);
    window.set_composer_which_reason(root.which.reason.clone().into());
    window.set_composer_runs_reason(root.runs.reason.clone().into());
    window.set_composer_named_reason(root.named.reason.clone().into());

    // §11.3's four renderings. The **variant** decides the colour; `Verdict` gains none.
    window.set_composer_region_text(root.region.text().into());
    window.set_composer_region_emphatic(root.region.emphatic());
    window.set_composer_create(to_fix(&root.create));
    window.set_composer_copy_command(to_fix(&which.copy_command));
    window.set_composer_title_auth(which.title_auth.clone().into());
    window.set_composer_no_ipod(runs.disabled_reason.clone().into());
    window.set_composer_runs_disabled(runs.disabled_reason.clone().into());
    window.set_composer_stem(named.stem.clone().into());
    window.set_composer_taken(named.taken.clone().into());
    window.set_composer_name_field(to_field(&named.name));

    // **The same function the Work page's ledger goes through** — one bill, two surfaces.
    let (download, disk, warn) = ledger_lines(Some(root.cost), space);
    window.set_composer_ledger_download(download.into());
    window.set_composer_ledger_disk(disk.into());
    window.set_composer_ledger_warn(warn);

    // The root's own `Fix`, which is the verdict's when it has one.
    match &root.region {
        composer::Region::No { why, fix } => {
            window.set_composer_refusal_why(why.clone().into());
            window.set_composer_refusal_open(true);
            match fix {
                Some(f) => {
                    window.set_composer_has_fix(true);
                    window.set_composer_fix(to_fix(&composer::FixRow::of(f, &c.recipe().start)));
                }
                None => {
                    window.set_composer_has_fix(false);
                    window.set_composer_fix(FixRow::default());
                }
            }
        }
        _ => {
            window.set_composer_refusal_why("".into());
            window.set_composer_refusal_open(false);
            window.set_composer_has_fix(false);
            window.set_composer_fix(FixRow::default());
        }
    }

    // **The generation, and it has exactly one job.** Every two-press control in `ui/composer.slint`
    // binds `for-recipe: root.composer-generation` and disarms when it changes: `Pressable.armed` is
    // component-local, and a `Fix` armed against one recipe must not fire against another.
    // `ui/rail.slint` solves the identical problem for repeater reuse with `for-entry: root.e.id`.
    window.set_composer_generation(c.generation() as i32);
    window.set_composer_open_field(c.open().map_or(-1, |f| f.as_i32()));

    let want_picks: Vec<PickRow> = match c.level() {
        composer::Level::WhichIpod => [&which.ipod, &which.model, &which.colour]
            .into_iter()
            .map(to_pick)
            .collect(),
        composer::Level::WhatItRuns => [&runs.disk, &runs.from, &runs.loader]
            .into_iter()
            .map(to_pick)
            .collect(),
        _ => Vec::new(),
    };
    let want_fields: Vec<FieldRow> = match c.level() {
        composer::Level::WhichIpod => vec![to_field(&which.serial), to_field(&which.guid)],
        composer::Level::NameIt => vec![to_field(&named.name)],
        _ => Vec::new(),
    };
    let want_ticks: Vec<TickRow> = match c.level() {
        composer::Level::WhatItRuns => runs.systems.iter().map(to_tick).collect(),
        _ => Vec::new(),
    };
    let want_opts: Vec<OptionRow> = c.options(settings).iter().map(to_choice).collect();
    let want_plan: Vec<PlanRow> = root.plan.iter().map(to_plan).collect();
    let want_refusals: Vec<RefusalRow> = runs.refusals.iter().map(to_refusal).collect();

    in_place(picks, &want_picks);
    in_place(fields, &want_fields);
    in_place(ticks, &want_ticks);
    in_place(opts, &want_opts);
    in_place(plan, &want_plan);
    in_place(refusals, &want_refusals);
}

/// §16.9's rule, once, for every retained model in this file: **set in place, push for new, remove
/// from the end.**
///
/// `refresh_devices` open-codes the same three lines for `DeviceRow`, and this is what every model
/// added since goes through — a fourth and a fifth copy of it is how one of them comes to rebuild.
fn in_place<T: Clone + PartialEq + 'static>(model: &Rc<VecModel<T>>, want: &[T]) {
    for (i, row) in want.iter().enumerate() {
        match model.row_data(i) {
            Some(old) if old == *row => {}
            Some(_) => model.set_row_data(i, row.clone()),
            None => model.push(row.clone()),
        }
    }
    while model.row_count() > want.len() {
        model.remove(model.row_count() - 1);
    }
}

/// **Whether this string may leave the window, and why not when it may not.**
///
/// §11.2 masks a serial and a FireWire GUID on screen because *a screenshot of this page must not
/// carry somebody's identifiers* — **and the clipboard is not presentation.** A screenshot is a
/// picture of one moment; a clipboard outlives the screen, survives the window closing, and is
/// pasted somewhere nobody was thinking about masking. So a value that is masked until `Show` is
/// pressed does not become copyable when it is: `Show` reveals, it does not unlock.
///
/// This is the **last** gate rather than the only one, and the order matters. The producers refuse
/// first — `composer::Secret` has no `raw()` and hands the markup `None` while masked, and
/// `parts::copyable` answers `Some` only for a path. This sits under both of them, on the one
/// callback in this program that can reach a pasteboard, so a producer added later without those
/// rules cannot quietly become a leak. `AGENTS.md` §7: verify what you ship, not what is on disk.
///
/// **The predicates are the model's own, not a second guess at what a serial looks like.**
/// `Identity::check_serial_for(_, None)` is the same function that refuses a typed serial, asked
/// with no model so it tests the shape rather than the generation. A GUID is sixteen hexadecimal
/// digits; that is checked here rather than through `Identity::check_guid`, which additionally
/// requires Apple's OUI — a *non*-Apple GUID is still somebody's identifier, and refusing only the
/// Apple ones would be the gate letting through exactly the values `identity.rs` warns about.
///
/// **Masked text passes, and that is the design working.** `7B******X3N` splits at the asterisks
/// into `7B` and `X3N`, neither of which is a serial — so the mask is what makes a string copyable,
/// which is the property this whole arrangement is for.
///
/// A refusal, never a silent drop: the sentence goes on the Rail. A control that appears to copy
/// and does not is worse than one that says why it will not.
fn clipboard_refusal(text: &str) -> Option<&'static str> {
    use eapp_loader::identity::Identity;
    for token in text.split(|c: char| !c.is_ascii_alphanumeric()) {
        if Identity::check_serial_for(token, None).is_ok() {
            return Some(
                "nothing was copied — that carries a serial number, and a clipboard outlives \
                 the screen that masked it",
            );
        }
        if token.len() == 16 && token.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Some(
                "nothing was copied — that carries a FireWire GUID, and a clipboard outlives \
                 the screen that masked it",
            );
        }
    }
    None
}

/// Write the settings, and put a failure where somebody can see it.
///
/// §20 item 13: a read-only home, a full disk or a second process holding the file used to be
/// swallowed, and the caller went on to say *"Saved to …"*.
fn save(settings: &Settings, rail: &mut rail::Rail) {
    if let Err(e) = settings.save() {
        let where_ = Settings::path()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "the settings file".into());
        rail.failed(
            "save",
            &where_,
            rail::Failure::saying(rail::Class::Permission, "writing the settings", format!("{e}")),
        );
    }
}

// ── The rest of the window's own arithmetic ──────────────────────────────────────────────────────

/// The window height a moment was measured at. **Every arm carries one** — that is what §16.1's
/// two-measurements rule bought, and it is what lets `IPOD_LAYOUT` print the height the fit came
/// from rather than the one the window has got round to reporting.
fn moment_window_logical(moment: &fit::Moment) -> f64 {
    match moment {
        fit::Moment::Shown { window_logical, .. }
        | fit::Moment::Moved { window_logical, .. }
        | fit::Moment::ScaleFactorChanged { window_logical, .. }
        | fit::Moment::Resized { window_logical } => *window_logical,
    }
}

/// The scale factor a moment carries, or the window's own where it carries none.
fn moment_scale(moment: &fit::Moment, win: &slint::Window) -> f64 {
    match moment {
        fit::Moment::Shown { sf, .. }
        | fit::Moment::Moved { sf, .. }
        | fit::Moment::ScaleFactorChanged { sf, .. } => *sf,
        fit::Moment::Resized { .. } => live_scale(win),
    }
}

/// The scale factor, **asked of the platform**, falling back to Slint's cache. Slint's is an `f32`.
///
/// **Both halves of a logical height have to come from the same moment.** The numerator is
/// `winit::Window::inner_size()` — the platform's, now — and taking the denominator from
/// `slint::Window::scale_factor()` pairs it with Slint's cache, which this filter runs ahead of
/// (`event_loop.rs:192-194` calls the filter, `:216-222` applies the event). Drag a window onto a
/// display of a different backing scale and `Moved` can arrive carrying the new physical size while
/// the cached factor is still the old one: new physical over old scale is a logical height wrong by
/// exactly that ratio, which on a 1× → 2× move is double.
///
/// `ScaleFactorChanged` is the one moment that does not use this. It carries its own factor, which
/// is the whole reason that arm exists.
fn live_scale(win: &slint::Window) -> f64 {
    let sf = win
        .with_winit_window(|w| w.scale_factor())
        .unwrap_or_else(|| f64::from(win.scale_factor()));
    sane_scale(sf)
}

/// The same guard, for a scale factor that arrived on an event rather than from the window.
fn sane_scale(sf: f64) -> f64 {
    if sf.is_finite() && sf > 0.0 { sf } else { 1.0 }
}

/// The window's own height, logical — **asked of the platform, at `sf`**.
///
/// There are three sizes in play during a `Resized` and they are not interchangeable. This is the
/// one that is true at the moment the event filter runs:
///
/// * `win.size()` is Slint's **cache**, and the filter runs BEFORE Slint applies the event
///   (`i-slint-backend-winit-1.17.1/event_loop.rs:192-194` calls the filter; `:216-222` handles the
///   event and `:222` is the only writer of that cache), so during a `Resized` it holds a size from
///   one event ago;
/// * the `Resized` **payload** is a size the window HAD when the platform queued the event;
/// * `winit::Window::inner_size()` asks the platform, now — `contentRectForFrameRect` on macOS
///   (`winit-0.30.13/src/platform_impl/macos/window_delegate.rs:944-949`), `XGetGeometry` on X11
///   (`.../linux/x11/window.rs:1242-1254`), `GetClientRect` on Windows (`.../windows/window.rs:
///   214-222`), and winit's own configure state on Wayland (`.../linux/wayland/window/mod.rs:
///   277-281`), which is updated before the `Resized` it belongs to is emitted. So it is never
///   staler than the payload, and on macOS it is materially fresher. **The macOS half is measured**
///   — `tests/startup_fit.rs` drives a resize from outside the process and reads the answer back;
///   the other three are read from those backends' source and not run. The fallback when there is no
///   winit window at all is `win.size()`, which is what this used to do unconditionally, so the
///   worst case on a platform where `inner_size()` lags is the behaviour it already had.
///
/// **And the payload at startup is a size this window never had on screen.** Slint clamps a
/// not-yet-existing window's size UP to the declared minimum: `WindowInner::show` calls
/// `update_window_properties()` before `set_visible` (`i-slint-core-1.17.1/window.rs:1635-1636`),
/// the adapter's size is still 0 × 0, and `adjust_window_size_to_satisfy_constraints`
/// (`i-slint-backend-winit-1.17.1/winitwindowadapter.rs:1690-1722`) writes `min-width × min-height`
/// into the pending `WindowAttributes::inner_size` (`:810-818`). The window is therefore CREATED at
/// the minimum and resized to the preferred size before it is ever mapped (`:1114-1124`, then
/// `set_visible(true)` at `:1133`). macOS's `request_inner_size` returns `None`
/// (`winit-0.30.13/src/platform_impl/macos/window_delegate.rs:957-963`), so both sizes arrive
/// afterwards as two queued `Resized` events **in creation order**. Reading the first payload
/// computes the whole fit — `k` and the too-short boolean — for a window `min-height` tall that was
/// never on screen, and `min-height` is 400.
fn own_height_logical(win: &slint::Window, sf: f64) -> f64 {
    let physical = win
        .with_winit_window(|w| w.inner_size().height)
        // No winit window — a headless backend, or before creation. Slint's cache is the only
        // answer there is, and at that point it is not stale because nothing has moved.
        .unwrap_or_else(|| win.size().height);
    f64::from(physical) / sane_scale(sf)
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
    client_height::client_height_logical(win)
        .unwrap_or_else(|| own_height_logical(win, live_scale(win)))
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
/// times out, and this runs on the UI thread. The pass belongs off it, together with §11.4's
/// `detect_mounted()`; until then a share that is not up delays the press rather than one row of it.
fn device_rows(settings: &Settings) -> Vec<DeviceRow> {
    let mut seen = Presence::new();
    let rows: Vec<DeviceRow> = settings
        .devices
        .iter()
        .map(|d| {
            let chassis = d.chassis.unwrap_or_default();
            // **The same question the centre button asks**, asked here so the cradle can answer it
            // before it is pressed rather than only after. It costs no extra `stat`: `seen` is the
            // pass's shared [`Presence`] and `summary` below asks again into the same cache.
            let gone = settings.missing_with(d, &mut seen);
            DeviceRow {
                name: d.name.clone().into(),
                summary: summary(settings, d, &mut seen).into(),
                // **The label's own boolean**, so a cradle that says *not wired* is not also drawn
                // pressable. `gone.is_empty()` alone answered `true` for a composed device, because
                // nothing about it is missing — it was never made.
                startable: gone.is_empty() && !composed_and_unbuilt(d),
                cradle_label: cradle_label(d, &gone).into(),
                // §7.5's row-1 trailing slot: **the state, and time since.**
                state: shelf_state(d).into(),
                write_target: write_target(settings, d).into(),
                write_target_is_warning: writes_to_your_own_image(settings, d),
                chassis: chassis_colour(chassis),
                dark_chassis: is_dark(chassis),
            }
        })
        .collect();
    rows
}

/// Re-read the library into the **retained** model, and re-push everything derived from it.
///
/// §16.9: rows that already exist are written in place, new ones are pushed, and the tail is removed
/// from the end. Handing `set_devices` a fresh `VecModel` would tear down and reconstruct every
/// repeater instance — losing focus, hover and any in-flight animation — which is what the first cut
/// did, once, and then never did again: nothing in the running window ever re-read the library, so a
/// drive or a ROM removed while the window was open stayed invisible for the life of the process.
/// The cradle promised *press the centre button* on a device whose image had been deleted an hour
/// earlier.
///
/// `showing_welcome` is §10.3's latch and this is the only place it is cleared — see the body.
fn refresh_devices(
    window: &MainWindow,
    model: &Rc<VecModel<DeviceRow>>,
    settings: &Settings,
    showing_welcome: &Rc<std::cell::Cell<bool>>,
    caps: rail::Caps,
    cost: compose::Cost,
) {
    let want = device_rows(settings);
    for (i, row) in want.iter().enumerate() {
        match model.row_data(i) {
            Some(old) if old == *row => {}
            Some(_) => model.set_row_data(i, row.clone()),
            None => model.push(row.clone()),
        }
    }
    while model.row_count() > want.len() {
        model.remove(model.row_count() - 1);
    }
    // The selection has to stay inside the list it indexes. `move()` clamps in markup; a device
    // removed from under a selection past the end is the other direction, and it lands here.
    let last = want.len().saturating_sub(1) as i32;
    if window.get_selected() > last {
        window.set_selected(last.max(0));
    }
    // **The transition out of the first run, and it is one-way** (§10.3). The welcome copy never
    // returns: a build that is cancelled or fails empties the list again, and this must NOT put it
    // back. Which is why the latch is here rather than an expression over `want.len()` — an
    // expression would go both ways, and going back is the bug.
    if !want.is_empty() {
        showing_welcome.set(false);
    }
    // §9.1: an empty library is a state with something to say. The window composes `current` out of
    // this when there is no device, so every sentence on the bench stays the model's — a struct
    // literal in the markup is how the previous revision came to invent a chassis colour there.
    window.set_empty_device(empty_device(showing_welcome.get(), caps, cost));

    // §10.1's ghost, and **it is an emptiness state rather than a first-run state** — §9.1 gives
    // the later-empty bench the same drawing. So it is recomputed from the library on every pass
    // and it goes both ways: build a device and the ghost solidifies; remove the last one and it
    // comes back. That is deliberately not the same rule as the welcome copy one line above, and
    // the difference is the whole answer to *how does the bench know it is a first run without that
    // being "the device list is empty"*: the **drawing** may key on emptiness, the **welcome** may
    // not.
    //
    // **Retirement condition**, in the shape `research/04` uses: this gains `&& !minted` when
    // §10.2's cross-dissolve is wanted at the moment `Source::identity()` answers rather than at
    // the moment the device reaches the list.
    window.set_ghost(want.is_empty());

    // ── §7.2's Devices page ─────────────────────────────────────────────────────────────────────
    //
    // §9.1: an empty list is a state with something to say, and the one row that fills it is
    // **always present** — pinned outside the Scroll, at the same place whether there are no
    // devices or nine.
    window.set_devices_empty_line("No devices yet.".into());
    // **Derived from the page's own slot**, exactly like `caps.devices_page`: `Page::Composer`
    // answers `Some` on the day `ui/drawer.slint` gains a child that draws it, which is the day
    // this control can do what it says. Written as a literal it is a second answer to the same
    // question, and a stale `false` here is a row disabled beside a page that exists.
    let composer_exists = nav::Page::Composer.slot().is_some();
    window.set_devices_new(FixRow {
        label: "New device".into(),
        enabled: composer_exists,
        reason: if composer_exists {
            "".into()
        } else {
            // §9.4's project-state wording — *this is not finished, by us* — with what does work.
            "the Composer has no page in this build yet".into()
        },
        escape_hatch: if composer_exists {
            "".into()
        } else {
            "ipod-boot setup".into()
        },
        machine_rule: false,
        presses: 1,
        consequence: "".into(),
    });
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

    // ASCII: this lands on shelf row 2, and `·` is drawn as a `Path` one row below it. See
    // [`fidelity`] for the whole of the argument.
    parts.join(", ")
}

/// §7.5's row 3 — whose file is about to be written to, said out loud, before the machine starts.
///
/// **Four sentences, not two.** `work_on_copy`'s `None` and `built_from`'s `Some`/`None` are two
/// different questions, and collapsing them lost the two that teach: *nobody has said, so a copy it
/// is* is what makes the safe default legible, and *we built it from `<bundle>`, so it is
/// regenerable* is what makes the unsafe-looking case safe to agree to. `write_target` is
/// `d.work_on_copy.unwrap_or(true)` and nothing else; `built_from` decides only the qualifier and
/// (through [`writes_to_your_own_image`]) whether the warn colour appears.
///
/// The verb is the first thing on the line in every one of the four, so a hard truncation at the
/// narrowed measure preserves the dangerous one.
fn write_target(settings: &Settings, d: &Device) -> String {
    let Some(p) = d.disk_path.as_ref() else {
        // **A device with no drive still has to fill this row**, and not only because §7.5 says row
        // 3 never goes away: the row is a control, its `accessible-label` is this string, and an
        // empty one is a button with no name. It is also the true answer — a machine with no drive
        // writes nowhere.
        return "no drive yet — nothing will be written".into();
    };
    let name = p
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    match d.work_on_copy {
        Some(true) => format!("works on a copy of {name}"),
        // `None` is not "no". With nobody having said, a drive this program built is regenerable
        // and a drive the operator supplied might be their only copy — so the honest default is a
        // copy, and the line says which of the two it is being.
        None => format!("works on a copy of {name} — nobody has said, so a copy it is"),
        Some(false) => match built_from(settings, d) {
            Some(bundle) => {
                format!("writes to {name} — we built it from {bundle}, so it is regenerable")
            }
            None => format!("writes to {name} — you chose this, and we did not build it"),
        },
    }
}

/// The `.ipsw` this device's drive was built from, by name, when the library recorded one.
fn built_from(settings: &Settings, d: &Device) -> Option<String> {
    d.disk
        .as_ref()
        .and_then(|n| settings.disks.iter().find(|k| &k.name == n))
        .and_then(|k| k.built_from.clone())
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
    built_from(settings, d).is_none()
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
pub(crate) mod tests {
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

    /// §10.3's latch, for a test that is not testing the latch itself.
    fn latch(first: bool) -> Rc<std::cell::Cell<bool>> {
        Rc::new(std::cell::Cell::new(first))
    }

    /// The plan's real cost, for a test that is not testing the cost itself.
    ///
    /// **Not `Cost::default()`.** The shelf's row 3 and the ledger both render this, and a zero
    /// would exercise the *nothing to download* arm rather than the one a first run takes.
    fn a_cost() -> compose::Cost {
        work::cost(compose::Holes::Sparse)
    }

    /// No plan at all — the shape a bench with nothing on it is in.
    fn no_cost() -> compose::Cost {
        compose::Cost::NONE
    }

    /// **Point the whole test binary at a data directory of its own, once, and hold the lock.**
    ///
    /// `AGENTS.md` §3: never write to the operator's real library. `Settings::save`,
    /// `settings::drives_dir` and `firmware::cache_dir` all resolve through `settings::data_dir`,
    /// which under `cargo test` declines the build tree and lands on the **platform** application
    /// support directory — the operator's own. `wire` saves, and now also builds a `Queue` that
    /// names `drives/`, so every test that calls it was writing there.
    ///
    /// Set **once per process, to one directory**: `std::env::set_var` is process-global and cargo
    /// runs tests on several threads, so a per-test directory would have two tests interleaving and
    /// one reading the other's. One value, set under a `Once`, cannot interleave with itself.
    ///
    /// **The returned guard is the other half, and it is why this returns anything at all.** This
    /// file's tests are not the only ones in this binary that redirect the variable: `work.rs`'s do
    /// too, per test, and they restore it afterwards. So *not varying the value* is only half a
    /// defence — it stops these tests colliding with each other, not with those. The comment here
    /// used to claim *every reader in this binary goes through this function first*, and that was
    /// false the moment `work.rs` landed; the flake it produced was a ledger test reporting a
    /// firmware bundle nobody had downloaded, because it read `work.rs`'s cache directory.
    ///
    /// One lock, [`crate::data_dir_lock`], taken by both, and **it is what performs the redirect** —
    /// so this is now the act of claiming the directory rather than the act of creating it. Hold
    /// the guard for as long as the test reads or writes the data directory; dropping it
    /// immediately is the bug this exists to stop, which is what `#[must_use]` is for.
    #[must_use = "dropping the guard releases the data directory to another test mid-run"]
    fn use_a_scratch_data_dir() -> DataDirGuard {
        DataDirGuard {
            _guard: crate::data_dir_lock(),
            at: crate::scratch_data_dir(),
        }
    }

    /// The data directory this test owns for as long as it is alive.
    struct DataDirGuard {
        _guard: crate::DataDirLock,
        at: &'static std::path::Path,
    }

    impl Drop for DataDirGuard {
        /// **Takes the directory's contents with it**, unless the operator asked to keep them.
        ///
        /// Nothing did, and a `cargo test` run left one `ipod-gui-data-<pid>/` per process behind
        /// for ever: 93 of them on this machine when somebody counted, each holding a settings
        /// file and — after a test that reached a worker — a `drives/` and a `firmware/`.
        ///
        /// **Only the outermost guard cleans**, and that is the whole of the care this needs. The
        /// lock is re-entrant because one test takes it twice — `a_fresh_installation` claims the
        /// directory and `a_window` needs the redirect in place before it builds anything — and a
        /// first cut of this deleted the tree when the *inner* one dropped, which is halfway
        /// through the test that set it up. It cost an afternoon and looked like `wire` deleting
        /// files.
        ///
        /// It runs **before** the fields drop, so the mutex is still held while the tree goes.
        /// `data_dir_lock` makes the directory on every claim rather than once, which is what lets
        /// this take it away entirely instead of emptying it and leaving the husk behind.
        ///
        /// `IPOD_TEST_DATA` is somebody saying *keep what this run produced*, which is the whole
        /// reason it exists, so it is honoured here as well as in `scratch_data_dir`.
        fn drop(&mut self) {
            if !self._guard.is_outermost() || std::env::var_os("IPOD_TEST_DATA").is_some() {
                return;
            }
            let _ = std::fs::remove_dir_all(self.at);
        }
    }

    /// **Every test that can reach the data directory claims it first.**
    ///
    /// `AGENTS.md` §3, and it is the one rule in this file whose cost is somebody else's disk.
    /// `settings::data_dir` declines a cargo build tree and then lands on the **platform**
    /// application-support directory — `~/Library/Application Support/ipod-emulator` on macOS, which
    /// today holds the operator's own devices and two 30 GB drive images. `wire` writes there: it
    /// saves the settings file, and it builds a `Queue` that names `drives/`.
    ///
    /// The redirect away from it used to be an opt-in call three tests made while **six** called
    /// `wire`, so whether it happened at all depended on which test the scheduler ran first. Now
    /// `crate::data_dir_lock` performs it — but only for whoever takes the lock, so a test that
    /// reaches `wire` without claiming the directory is still a test that can be running while
    /// `work.rs` has the variable pointed somewhere else. This is what stops one being written.
    ///
    /// **A sweep of this file's own text**, which is what the two markup sweeps beside it do, and
    /// cheaper than a convention nobody enforces. It reads the test module — the half `rust_sources`
    /// deliberately cuts off — splits it at `fn`, and requires any function whose body reaches the
    /// data directory to also take the guard, directly or through one of the two helpers that do.
    #[test]
    fn every_test_that_reaches_the_data_directory_takes_the_lock() {
        let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/main.rs"))
            .expect("this file");
        let (_, tests) = text
            .split_once("pub(crate) mod tests {")
            .expect("the test module");

        // Anything that resolves through `settings::data_dir()`. `Queue::new` is on the list because
        // it is `Queue::at(drives_dir(), cache_dir())`, and `wire` because it does all of it.
        const REACHES: [&str; 6] = [
            "wire(",
            "Settings::load()",
            "settings::drives_dir()",
            "firmware::cache_dir()",
            "work::Queue::new()",
            "Queue::new()",
        ];
        // The guard, however it is come by. `a_window` and `a_fresh_installation` both take it and
        // both hand it back, which is why naming them here is not a loophole.
        const CLAIMS: [&str; 3] =
            ["use_a_scratch_data_dir()", "a_fresh_installation()", "a_window()"];

        // **A mention inside a string is not a call**, and this module is full of sweeps that search
        // for the very text this one searches for: `the_work_timer_is_started_in_exactly_one_place_
        // and_is_held` contains the literal `"let _wiring = wire("` and calls `wire` nowhere. That
        // was this sweep's first finding and it was wrong, which is the only kind of finding worth
        // building an instrument around.
        // **Char literals have to be skipped as well, and that was this sweep's second finding.**
        // A stripper that only knows about `"` desynchronises on the `'"'` in its own body and then
        // reports every function after it inverted — an instrument that lies, produced by the
        // instrument. A `'` is a char literal when the two or three characters after it close it,
        // and a lifetime (`&'static str`) otherwise.
        let code_only = |body: &str| -> String {
            let c: Vec<char> = body.chars().collect();
            let mut out = String::with_capacity(body.len());
            let mut i = 0usize;
            while i < c.len() {
                match c[i] {
                    '"' => {
                        i += 1;
                        while i < c.len() && c[i] != '"' {
                            i += if c[i] == '\\' { 2 } else { 1 };
                        }
                        i += 1;
                    }
                    '\'' if c.get(i + 2) == Some(&'\'') && c[i + 1] != '\\' => i += 3,
                    '\'' if c.get(i + 3) == Some(&'\'') && c[i + 1] == '\\' => i += 4,
                    ch => {
                        out.push(ch);
                        i += 1;
                    }
                }
            }
            out
        };

        // Split at function boundaries. Every function in this module is indented four spaces, so
        // `\n    fn ` is unambiguous and does not match a call, a closure or a nested item.
        let mut bodies: Vec<(&str, String)> = Vec::new();
        for chunk in tests.split("\n    fn ").skip(1) {
            let name = chunk.split(['(', '<']).next().unwrap_or("").trim();
            bodies.push((name, code_only(chunk)));
        }
        assert!(bodies.len() > 40, "the sweep found {} functions", bodies.len());

        let mut unclaimed: Vec<(&str, &str)> = Vec::new();
        for (name, body) in &bodies {
            // The three that define the mechanism are what everything else goes through.
            if CLAIMS.iter().any(|c| c.trim_end_matches("()") == *name) {
                continue;
            }
            if let Some(hit) = REACHES.iter().find(|r| body.contains(**r)) {
                if !CLAIMS.iter().any(|c| body.contains(c)) {
                    unclaimed.push((name, hit));
                }
            }
        }
        assert!(
            unclaimed.is_empty(),
            "these reach the data directory without claiming it, so they can run while `work.rs` \
             has `IPOD_EMULATOR_DATA` pointed at one of its own scratch directories — and on a run \
             where nothing has redirected yet, at the operator's real library: {unclaimed:?}"
        );

        // **Two controls.** A sweep that matched nothing would pass vacuously.
        let claimed = bodies
            .iter()
            .filter(|(_, b)| {
                REACHES.iter().any(|r| b.contains(*r)) && CLAIMS.iter().any(|c| b.contains(c))
            })
            .count();
        assert!(
            claimed > 3,
            "the sweep found only {claimed} tests that both reach the data directory and claim it, \
             so it is not reading what it thinks it is"
        );
        // And the stripper has to work on both shapes, or the two false positives it exists to stop
        // come straight back with nothing to report them.
        let body_of = |name: &str| -> String {
            bodies
                .iter()
                .find(|(n, _)| *n == name)
                .unwrap_or_else(|| panic!("{name} is not in this module any more"))
                .1
                .clone()
        };
        assert!(
            !body_of("the_work_timer_is_started_in_exactly_one_place_and_is_held").contains("wire("),
            "the stripper let a quoted `wire(` through, so this sweep is about to report a test \
             that calls nothing"
        );
        assert!(
            !body_of("every_test_that_reaches_the_data_directory_takes_the_lock").contains("wire("),
            "the stripper desynchronised on a char literal — most likely the `'\"'` in its own \
             body — so everything after it is being read inside-out"
        );
    }

    /// A directory of our own, named after the test, so two running at once cannot collide.
    fn temp_dir(what: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("ipod-gui-test-{what}-{}", std::process::id()));
        std::fs::create_dir_all(&d).expect("a temp directory");
        d
    }

    /// §7.5's four sentences, and each one is a different pair of answers.
    #[test]
    fn a_device_with_a_disk_always_says_what_it_writes_to() {
        let mut s = Settings::default();
        s.disks.push(eapp_loader::settings::Disk {
            name: "mine".into(),
            path: "/tmp/my-5.5g.img".into(),
            built_from: None,
            installed: vec![],
        });
        let mut d = Device {
            disk: Some("mine".into()),
            disk_path: Some(std::path::PathBuf::from("/tmp/my-5.5g.img")),
            ..Device::default()
        };

        d.work_on_copy = Some(false);
        let line = write_target(&s, &d);
        assert!(
            line.starts_with("writes to my-5.5g.img"),
            "the dangerous verb has to be the first thing on the line: {line}"
        );
        assert!(
            line.contains("you chose this"),
            "an image we did not build has to say so: {line}"
        );

        s.disks[0].built_from = Some("iPod_25.1.3.ipsw".into());
        let line = write_target(&s, &d);
        assert!(
            line.contains("regenerable") && line.contains("iPod_25.1.3.ipsw"),
            "one we built has to name the bundle, or writing to it looks as dangerous as the \
             other case: {line}"
        );

        d.work_on_copy = Some(true);
        assert!(
            write_target(&s, &d).starts_with("works on a copy of"),
            "a device on a copy has to say that too — silence reads as 'writes to it'"
        );

        d.work_on_copy = None;
        let line = write_target(&s, &d);
        assert!(
            line.starts_with("works on a copy of") && line.contains("nobody has said"),
            "the unanswered case reads exactly like the answered one, so the safe default is \
             invisible: {line}"
        );
        // Four sentences, four different strings.
        let mut seen = std::collections::HashSet::new();
        for (copy, built) in [
            (Some(true), None),
            (None, None),
            (Some(false), Some("iPod_25.1.3.ipsw".to_string())),
            (Some(false), None),
        ] {
            s.disks[0].built_from = built;
            d.work_on_copy = copy;
            assert!(
                seen.insert(write_target(&s, &d)),
                "two of §7.5's four cases render the same sentence"
            );
        }
    }

    /// **Nobody having said is not the same as having said no.**
    ///
    /// A drive this program built is regenerable byte for byte, so writing to it costs nothing; one
    /// the operator supplied might be the only image of an iPod they own, and defaulting to writing
    /// on it is how an afternoon disappears.
    #[test]
    fn an_unanswered_device_works_on_a_copy() {
        let s = Settings::default();
        let d = Device {
            disk_path: Some(std::path::PathBuf::from("/tmp/x.img")),
            ..Device::default()
        };
        assert!(
            write_target(&s, &d).contains("copy"),
            "with nobody having said, the default has to be the safe one"
        );
        assert!(!writes_to_your_own_image(&s, &d));
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

    /// The caption says what the device *is*, never the heading again.
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
    /// `boot_instructions` is what §12.3's progress bar divides by, and `Settings::set_boot_shape`
    /// clears it whenever the recipe changes — so a device booted a dozen times has none the moment
    /// its bootloader is swapped. Whether it has ever run is a fact the model does not carry, and no
    /// row may render one either way. It reached the shelf's row-1 slot for a while, which is worse
    /// than useless: that slot is §12.2's phase.
    #[test]
    fn a_device_whose_recipe_changed_does_not_claim_it_never_started() {
        let mut s = Settings::default();
        s.remember_as("mine");
        // A device that HAS booted, whose denominator was then dropped — which is exactly what
        // §12.3's rule does to a device whose bootloader changed.
        s.devices[0].boot_instructions = None;
        let without = device_rows(&s)[0].state.to_string();
        assert!(
            !without.contains("never"),
            "the row claims history the model does not carry: {without:?}"
        );

        s.devices[0].boot_instructions = Some(3_000_000);
        assert_eq!(
            device_rows(&s)[0].state.to_string(),
            without,
            "the shelf's state slot changed when only the progress bar's denominator did"
        );
    }

    /// **No row claims a verification the model did not record**, re-expressed onto the model.
    ///
    /// The four Resources groups printed `fetched and verified` and `dumped from a real iPod` as
    /// string literals, for files `Resource` carried nothing but a path for. Those groups are gone
    /// with the Resources tab, so the assertion moves down to the thing that has to hold for
    /// whoever writes §11.4's Parts page: **an item nobody recorded a provenance for contributes
    /// the empty string, and no provenance renders `verified` unless it is one.**
    ///
    /// `eapp_loader::settings::a_size_only_row_never_says_verified` holds the second half at the
    /// model's own level and more strictly; this is the window-side half — `None` is not a claim.
    #[test]
    fn no_row_claims_a_verification_the_model_did_not_record() {
        use eapp_loader::settings::{Provenance, Verification};

        let says = |from: Option<Provenance>| from.map(|p| p.line()).unwrap_or_default();
        assert_eq!(
            says(None),
            "",
            "an item nobody recorded a provenance for invented a claim"
        );
        for p in [
            Provenance::Dumped,
            Provenance::Provided,
            Provenance::Built,
            Provenance::Synthesised { seed: 1 },
            Provenance::Fetched { verified: Verification::SizeOnly },
            Provenance::Fetched { verified: Verification::None },
        ] {
            assert!(
                !says(Some(p)).contains("verified"),
                "{p:?} renders {:?}, which claims a check the model did not record",
                says(Some(p))
            );
        }
        assert!(says(Some(Provenance::Fetched { verified: Verification::Sha256 }))
            .contains("verified"));

    }

    /// **§7.5's row-1 trailing slot is the machine's state and the time since — not a denominator.**
    ///
    /// It read `no boot time learned yet`, which is a fact about what the *progress bar divides by*
    /// (§12.3) and not about the machine at all, while `phase()` — which already answers `Off` — was
    /// consulted for the panel's description and for `Esc` and reached this slot nowhere. §12.2's
    /// table gives it `off` / `booting` / `running` / `stopped`, and §7.5's drawing shows
    /// `parked · 4 min ago`.
    ///
    /// The old `DeviceRow.parked` boolean went with the change: it was computed and bound to nothing
    /// in any markup file, which is §20 item 15's defect, and the sentence carries the fact now.
    #[test]
    fn the_shelf_says_which_phase_the_machine_is_in() {
        let mut s = Settings::default();
        let rom = s.file_away(
            eapp_loader::settings::Resource::Firmware(eapp_loader::nor::Source::default()),
            "an iPod",
            None,
        );
        s.devices.push(Device { name: "mine".into(), firmware: rom.clone(), ..Device::default() });
        let state = device_rows(&s)[0].state.to_string();
        assert_eq!(state, "off", "the shelf does not say which of §12.2's phases this is: {state:?}");
        assert!(
            !state.contains("boot time"),
            "the state slot is carrying the progress bar's denominator again: {state:?}"
        );

        // …and a parked device says how long ago, which is what §7.5 draws.
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        s.devices.push(Device {
            name: "parked".into(),
            firmware: rom,
            parked_at: Some(now - 240),
            ..Device::default()
        });
        let parked = device_rows(&s)[1].state.to_string();
        assert_eq!(parked, "off, parked 4 min ago", "{parked:?}");

        // A clock behind the timestamp saturates rather than wrapping, and reads as *just now*.
        assert_eq!(ago(0), "just now");
        assert_eq!(ago(90), "2 min ago");
        assert_eq!(ago(7_200), "2 h ago");
        assert_eq!(ago(172_800), "2 days ago");
    }

    /// **T-21. The centre button refuses a device whose disk left the library — all three ways.**
    ///
    /// §20 item 12, mechanically: the refusal produces a Rail entry naming the file, in the
    /// `Missing` class, and nothing is started.
    ///
    /// **Three cases, because the first cut only covered one and the one it covered is the rarest.**
    /// A drive leaves in three shapes: the **file** is deleted and the entry survives; the **entry**
    /// is dropped and the file survives; or both go. `Settings::run_device` resolves NAMES and
    /// touches no file, and `Settings::missing` stats every resolved path — so for months the
    /// file-only case, which is by far the commonest, had the cradle drawing a broken ring and the
    /// centre button reporting success. The test removed the file *and* cleared `s.disks`, so it
    /// only ever exercised the shape where the two agreed.
    #[test]
    fn the_centre_button_refuses_a_device_whose_disk_left_the_library() {
        /// How a drive leaves the library: the file, the entry that names it, or both.
        struct Case {
            what: &'static str,
            leave: fn(&mut Settings, &std::path::Path),
            must_say: &'static str,
        }
        let cases = [
            Case {
                what: "the file only",
                leave: |_s, dir| {
                    std::fs::remove_file(dir.join("x.img")).unwrap();
                },
                must_say: "x.img",
            },
            Case {
                what: "the entry only",
                leave: |s, _dir| {
                    s.disks.clear();
                },
                must_say: "iPod 1",
            },
            Case {
                what: "both",
                leave: |s, dir| {
                    std::fs::remove_file(dir.join("x.img")).unwrap();
                    s.disks.clear();
                },
                must_say: "iPod 1",
            },
        ];

        for Case { what, leave, must_say } in cases {
            let dir = temp_dir("refusal");
            let (mut s, d) = a_composed_device(&dir);
            s.devices.push(d);
            leave(&mut s, &dir);

            let mut rail = rail::Rail::new();
            match resolve_for_start(&mut s, 0) {
                Ok(name) => panic!("{name} started with {what} gone"),
                Err((name, f)) => {
                    assert_eq!(f.class, rail::Class::Missing, "the wrong class of refusal");
                    rail.failed("start", &name, f);
                }
            }
            // Nothing was made live. `run_device` is what sets `current`, and a refusal must not
            // have reached it.
            assert!(
                s.current.is_none(),
                "{what}: a refused device was made the live one anyway"
            );

            assert_eq!(rail.failures(), 1, "{what}: the refusal did not reach the Rail");
            let e = rail.entries().first().expect("the entry");
            assert_eq!(e.verb, "start");
            assert!(
                e.happened().contains(must_say),
                "{what}: the refusal does not name what is gone: {:?}",
                e.happened()
            );
            std::fs::remove_dir_all(&dir).ok();
        }

        // And it offers a way on, even where this build cannot take it for you.
        let steps = rail::Class::Missing.next(0, caps());
        assert!(!steps.is_empty(), "a refusal with no next step at all");
        for s in &steps {
            assert!(
                s.available(caps()) || !s.reason().is_empty(),
                "{} is disabled and says nothing about why",
                s.label()
            );
        }
    }

    /// **The bench and the centre button ask ONE question about a device, and get one answer.**
    ///
    /// `startable` on the shelf and the cradle comes from `Settings::missing`, which stats; the
    /// press used to come from `Settings::run_device`, which resolves names and stats nothing. So a
    /// device whose `.img` had been deleted while its entry survived drew a broken ring, a refusal
    /// and a `why ›` — and then started. Two surfaces, two answers, one device.
    #[test]
    fn what_the_cradle_says_and_what_the_press_does_are_the_same_question() {
        for leave_the_entry in [true, false] {
            let dir = temp_dir("agree");
            let (mut s, d) = a_composed_device(&dir);
            s.devices.push(d);
            std::fs::remove_file(dir.join("x.img")).unwrap();
            if !leave_the_entry {
                s.disks.clear();
            }

            // What the bench draws.
            let row = &device_rows(&s)[0];
            // What the press does.
            let pressed = resolve_for_start(&mut s, 0);

            assert_eq!(
                row.startable,
                pressed.is_ok(),
                "the cradle says startable={} and the press says {:?} — with the drive's entry {} \
                 in the library",
                row.startable,
                pressed.map(|_| "started"),
                if leave_the_entry { "still" } else { "no longer" }
            );
            std::fs::remove_dir_all(&dir).ok();
        }
    }

    /// **The handler `main` actually registers, driven end to end.**
    ///
    /// This is the test that was missing, and its absence is why a panic on the success path shipped
    /// green. Every other test of the centre button either called `resolve_for_start` directly or
    /// replaced the handler with a closure of its own; `main`'s real one was exercised by nothing.
    /// It held `settings.borrow_mut()` alive across a `match` — a scrutinee temporary lives to the
    /// end of the match in every edition — and then took `settings.borrow()` inside the `Ok` arm.
    /// Only the success path panicked, so the refusal tests all stayed green: **the press that
    /// worked was the one that took the program down.**
    #[test]
    fn the_registered_centre_button_handler_survives_a_device_that_resolves() {
        let dir = temp_dir("registered");
        let (mut s, d) = a_composed_device(&dir);
        s.devices.push(d);
        // The settings file goes in the same scratch directory, so `save()` writes nothing of the
        // operator's. `Settings::save` uses the process-wide data directory, which this cannot
        // redirect — so a failure there becomes a Rail entry, which is the point, rather than a
        // panic.
        let settings = Rc::new(RefCell::new(s));

        let w = a_window();
        wire(&w, settings.clone());
        assert!(!w.get_devices().row_count() == 0 || w.get_devices().row_count() == 1);

        // The real, registered handler. Before the fix this line panicked with
        // `RefCell already mutably borrowed`.
        w.invoke_start_device(0);

        assert_eq!(
            settings.borrow().current.as_deref(),
            Some("iPod 1"),
            "the press did not make the device live, so the handler did not run"
        );
        assert!(
            w.get_rail().row_count() >= 1,
            "the press produced no Rail entry at all"
        );
        // Pressing twice is a thing people do. It must not panic and it must not stack two copies
        // of one sentence.
        let after_one = w.get_rail().row_count();
        w.invoke_start_device(0);
        assert_eq!(
            w.get_rail().row_count(),
            after_one,
            "a second identical press filed a second identical entry; the Rail grows without bound"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// **`why ›` leads to the explanation, not to the empty state.**
    ///
    /// §7.6: *the paragraph and its next steps live under the control that caused them.* The shelf's
    /// refusal is a fact about the device computed when its row was built; the Rail only ever got an
    /// entry from a press. So on a freshly opened window with a broken device, `why ›` slid the
    /// drawer open onto *"Nothing is happening. Fetches, builds and installs report here."*
    #[test]
    fn why_puts_the_refusal_it_was_pressed_for_on_the_rail() {
        let dir = temp_dir("why");
        let (mut s, d) = a_composed_device(&dir);
        s.devices.push(d);
        std::fs::remove_file(dir.join("x.img")).unwrap();
        let settings = Rc::new(RefCell::new(s));

        let w = a_window();
        wire(&w, settings.clone());
        assert_eq!(w.get_rail().row_count(), 0, "nothing has happened yet");

        w.invoke_explain(0);

        assert_eq!(w.get_drawer_page(), DrawerPage::Work, "`why` did not open the Work page");
        assert!(w.get_drawer_open(), "`why` did not open the drawer");
        let row = w.get_rail().row_data(0).expect("the refusal");
        assert_eq!(row.kind, RailKind::Failed);
        assert!(
            row.happened.contains("x.img"),
            "the page `why` opens does not name what is gone: {:?}",
            row.happened
        );
        assert!(
            !w.get_work_heading().to_lowercase().contains("nothing is happening"),
            "the page says nothing is happening above the failure it was opened to explain"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A device whose parts are all there resolves, and **nothing is refused**.
    ///
    /// The control for the test above: a refusal path that refused everything would pass it.
    #[test]
    fn a_device_whose_parts_are_all_there_resolves() {
        let dir = temp_dir("resolves");
        let (mut s, d) = a_composed_device(&dir);
        s.devices.push(d);
        let name = resolve_for_start(&mut s, 0).expect("an intact device");
        assert_eq!(name, "iPod 1");
        assert_eq!(s.current.as_deref(), Some("iPod 1"), "run_device did not make it live");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// **T-22. The work timer is started in exactly one place, and it is held for the window's
    /// life.**
    ///
    /// Two failure modes, both silent, and neither has any diagnostic anywhere.
    ///
    /// *Dropped*: `slint::Timer` stops the moment it goes out of scope
    /// (`i-slint-core-1.17.1/timers.rs:44` — *"You must keep the Timer object around for as long as
    /// you want the timer to keep firing"*). A timer created inside `wire` and not handed back
    /// never fires once, and the first run's Rail sits on `Planned` for ever.
    ///
    /// *Started twice*: `Timer::start` restarts the same timer with a new callback, so a second
    /// call site silently replaces the first — the same class as
    /// `there_is_exactly_one_winit_event_filter_registration`, and there is no error there either.
    #[test]
    fn the_work_timer_is_started_in_exactly_one_place_and_is_held() {
        let mut starts: Vec<String> = Vec::new();
        let mut held = false;
        for (name, text) in rust_sources() {
            for (n, line) in text.lines().enumerate() {
                let t = line.trim_start();
                if t.starts_with("//") {
                    continue;
                }
                if line.contains("TimerMode::") {
                    starts.push(format!("{name}:{}", n + 1));
                }
                // `wire` hands the timer back, and `main` binds what it hands back.
                if name == "main.rs" && line.contains("let _wiring = wire(") {
                    held = true;
                }
            }
        }
        assert_eq!(
            starts.len(),
            1,
            "the work timer is started at {starts:?}; a second `Timer::start` silently replaces \
             the first callback and nothing reports it"
        );
        assert!(
            held,
            "`main` calls `wire` without binding what it returns, so the timer is dropped at the \
             end of that statement and never fires — with no error anywhere"
        );
    }

    /// **Starting the timer is idempotent**, because `Timer::start` restarts a running one.
    ///
    /// `i-slint-core`'s own doc: *"If the timer has been started previously, then it will be
    /// restarted, no matter if it has already been fired or not."* Every `Press::Busy` came through
    /// `ticking`, so somebody mashing the centre button on a build that looked stuck pushed the
    /// next tick out indefinitely — progress froze on screen while the work carried on and the
    /// reports piled up in the channel. Which is exactly what a person does when a download looks
    /// stuck.
    ///
    /// A source sweep, because a `slint::Timer` under `init_no_event_loop` never fires and there is
    /// no way to observe a timeout being moved. What is checked is that the guard is there, on the
    /// one function that starts it.
    #[test]
    fn asking_the_timer_to_run_twice_does_not_move_the_next_tick() {
        let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/main.rs"))
            .expect("this file");
        let (_, after) = text
            .split_once("let ticking: Rc<dyn Fn()> = {")
            .expect("the one place the timer is started");
        let body = after.split("TimerMode::").next().expect("the start call");
        assert!(
            body.contains("timer.running()"),
            "`ticking` starts the timer without asking whether it is already running, so every \
             extra press restarts the countdown:\n{body}"
        );
        // …and the guard returns rather than starting a second one.
        assert!(
            body.contains("return"),
            "`ticking` reads `timer.running()` and starts it anyway:\n{body}"
        );
    }

    /// **The tick a test drives is the tick the timer drives.**
    ///
    /// Under `i-slint-backend-testing`'s no-event-loop init a `slint::Timer` never fires at all, so
    /// a `pump` reachable only through the timer would be reachable from nothing here — §20 item
    /// 12's defect, one layer up. `pump_once` is a plain function, and the timer's callback is one
    /// call to it.
    #[test]
    fn one_tick_on_an_idle_queue_changes_nothing_and_stops_looking() {
        let (settings, _held) = a_fresh_installation();
        let w = a_window();
        let wiring = wire(&w, settings.clone());

        let rail = Rc::new(RefCell::new(rail::Rail::new()));
        let rows: Rc<VecModel<RailRow>> = Rc::new(VecModel::default());
        let devices: Rc<VecModel<DeviceRow>> = Rc::new(VecModel::default());
        // **Armed first, or the assertion below is vacuous.** A `Timer::default()` is already not
        // running, so `pump_once` calling `stop()` on one would be indistinguishable from
        // `pump_once` doing nothing at all — which is how a check that tests nothing looks exactly
        // like a check that passes.
        let timer = slint::Timer::default();
        timer.start(slint::TimerMode::Repeated, work::TICK, || {});
        assert!(timer.running(), "the fixture's timer is not armed, so the check below proves nothing");

        let before = settings.borrow().devices.len();
        // The registry, counted rather than ignored: an idle tick is the tick a finished run ends
        // on, and it is the one that has to take `Lock::Building` back off every open page.
        let repaints = Rc::new(std::cell::Cell::new(0u32));
        let repaint: Repaint = {
            let repaints = repaints.clone();
            Rc::new(move || repaints.set(repaints.get() + 1))
        };
        pump_once(
            &w,
            &wiring.work,
            &rail,
            &rows,
            &devices,
            &settings,
            &latch(true),
            &Rc::new(RefCell::new(None)),
            &repaint,
            &timer,
            caps(),
            a_cost(),
        );
        assert_eq!(
            repaints.get(),
            1,
            "the tick that stops the timer did not re-push the registered pages, so a build that \
             finishes while one is open leaves it disabled"
        );
        assert_eq!(
            settings.borrow().devices.len(),
            before,
            "a tick with nothing running changed the library"
        );
        assert!(!timer.running(), "the timer keeps looking at 10 Hz with nothing to look at");
    }

    /// **A page is re-pushed by a change it did not make.**
    ///
    /// The Composer's `redraw` used to be reachable from nothing but the Composer's own callbacks,
    /// so everything the page draws about the *world* rather than about the recipe — the build's
    /// lock, the names the library has already taken — was whatever it had been at the last press
    /// of one of its own controls.
    ///
    /// Everything here goes through the registered handlers: `device-new` opens the page and
    /// `composer-type` names the device. Then the library gains a device of that name from
    /// somewhere that is not this page — a finished run, the Devices page, a second window — and
    /// **nothing on the Composer is touched afterwards.** One tick is.
    ///
    /// `composer-taken` is the property under test because [`composer::Composer::named`] reads it
    /// from `Settings::devices` on every push and from nothing the page holds, so the only way it
    /// can come true is by being pushed again. The middle assertion is what makes that argument
    /// rather than assumes it: the page must still be saying the old thing after the library has
    /// moved and before the tick, or the tick is not what is being measured.
    #[test]
    fn a_tick_re_pushes_a_page_that_a_change_elsewhere_made_stale() {
        let (settings, _held) = a_fresh_installation();
        let w = a_window();
        let wiring = wire(&w, settings.clone());

        w.invoke_device_new();
        w.invoke_composer_type(composer::Field::Name.as_i32(), "Zeppelin".into());
        assert_eq!(
            w.get_composer_taken(),
            "",
            "the fixture opens with the name already taken, so nothing below would prove anything"
        );

        // The world moves, and the Composer is not what moved it.
        settings.borrow_mut().devices.push(Device {
            name: "Zeppelin".into(),
            firmware: "an iPod".into(),
            ..Device::default()
        });
        assert_eq!(
            w.get_composer_taken(),
            "",
            "the page followed the library with no push at all, so the tick below is not what is \
             being measured"
        );

        (wiring.tick)();

        assert_eq!(
            w.get_composer_taken(),
            "There is already a device called Zeppelin.",
            "the tick did not re-push the Composer, so a page still goes stale the moment anything \
             but its own controls moves the world"
        );
    }

    /// **T-7. There is exactly one `on_winit_window_event` registration in this program.**
    ///
    /// The hook is stored in a `Cell<Option<Box<…>>>` and registering calls `set`, so a second
    /// call silently destroys the first — and with it every recompute of `k` and the too-short
    /// boolean. There is no error, no warning and no visible symptom until somebody drags the
    /// window onto a second display.
    #[test]
    fn there_is_exactly_one_winit_event_filter_registration() {
        let mut sites: Vec<String> = Vec::new();
        for (name, text) in rust_sources() {
            for (n, line) in text.lines().enumerate() {
                if line.contains("on_winit_window_event")
                    && !line.trim_start().starts_with("//")
                    && !line.trim_start().starts_with("///")
                {
                    sites.push(format!("{name}:{}", n + 1));
                }
            }
        }
        assert_eq!(
            sites.len(),
            1,
            "the winit event filter is registered at {sites:?}; a second registration silently \
             destroys the first"
        );
    }

    /// Every `set_*` model handoff in this crate, and whether it builds the model on the spot.
    ///
    /// Pulled out of the test below so it can be shown a line that IS a wholesale rebuild — which
    /// is the control, and it used to be provided by `device_rows`, back when the library model was
    /// the one being torn down and reconstructed. It is retained now too, so there is no such line
    /// left in the file and the control has to be a synthetic one.
    fn model_handoffs(setter: &str) -> (Vec<String>, Vec<String>) {
        let mut calls: Vec<String> = Vec::new();
        let mut wholesale: Vec<String> = Vec::new();
        for (name, text) in rust_sources() {
            for (n, line) in text.lines().enumerate() {
                let t = line.trim_start();
                if t.starts_with("//") {
                    continue;
                }
                if t.contains(setter) {
                    calls.push(format!("{name}:{}", n + 1));
                    if t.contains("VecModel::from") {
                        wholesale.push(format!("{name}:{}", n + 1));
                    }
                }
            }
        }
        (calls, wholesale)
    }

    /// The dead-code allow **in either spelling**, and the only reader of it in this file.
    ///
    /// `Some(true)` for the inner `#![allow(dead_code)]`, which blankets a whole module;
    /// `Some(false)` for the outer `#[allow(dead_code)]`, which sits on one item; `None` for
    /// anything else, including prose *about* the attribute — `geometry.rs` explains twice in doc
    /// comments why a constant is `#[cfg(test)]` "rather than `#[allow(dead_code)]`", and an
    /// instrument that read those as allows would be reporting a defect it created by looking.
    ///
    /// **The two spellings do not contain one another**, which is the whole reason this exists:
    /// `"#![allow(dead_code)]".contains("#[allow(dead_code)]")` is `false`. Both sweeps below were
    /// written with the outer form as their filter, so a module-wide blanket read as *no allow
    /// here at all* — §6's shape, an absence the instrument could not have observed.
    fn dead_code_allow(line: &str) -> Option<bool> {
        if line.trim_start().starts_with("//") {
            return None;
        }
        if line.contains("#![allow(dead_code)]") {
            Some(true)
        } else if line.contains("#[allow(dead_code)]") {
            Some(false)
        } else {
            None
        }
    }

    /// The two spellings this crate uses for a retirement condition, in either case.
    fn names_a_condition(text: &str) -> bool {
        let lower = text.to_ascii_lowercase();
        lower.contains("retired when") || lower.contains("retirement condition")
    }

    /// Whether the allow at `n` says what would retire it — **on its own line, or in the comment
    /// block directly above it**.
    ///
    /// **The second form has one live shape, and it is not the one this was written for.** It was
    /// written for a blanket: an attribute with no item has no line to write a condition on, so
    /// `composer.rs`'s wrote its own in the paragraph above itself. That blanket is deleted and
    /// this crate has none, so that justification now describes nothing — and a rule justified by
    /// nothing is §16.9's defect wearing a doc comment. The rule stays because a different shape
    /// keeps it true: the five parked modules at the top of `main.rs` are a run of
    /// `#[allow(dead_code)] mod x;` pairs that share **one** condition, and five copies of one
    /// sentence is five sentences that drift. That is why the scan steps over the run's own
    /// attribute and `mod x;` lines on its way up. It steps over **nothing else** — not a blank
    /// line, not code — so a condition cannot be inherited from a paragraph that was written about
    /// something else.
    ///
    /// Measured with this function, over the whole crate: 23 conditions sit beside their attribute
    /// and 5 above it, and all five above are that one run.
    fn says_what_retires_it(lines: &[&str], n: usize) -> bool {
        if names_a_condition(lines[n]) {
            return true;
        }
        let mut i = n;
        while i > 0 {
            let above = lines[i - 1].trim_start();
            let in_run = dead_code_allow(lines[i - 1]).is_some()
                || (above.starts_with("mod ") && above.ends_with(';'));
            if !in_run {
                break;
            }
            i -= 1;
        }
        let mut block = String::new();
        while i > 0 && lines[i - 1].trim_start().starts_with("//") {
            block.insert(0, '\n');
            block.insert_str(0, lines[i - 1]);
            i -= 1;
        }
        names_a_condition(&block)
    }

    /// **No `#[allow(dead_code)]` is bare.** Every one carries the observation that retires it.
    ///
    /// `rail.rs` and `nav.rs` are written before their producers — that is the whole point of §20
    /// item 12, the Rail existing before the first button is wired — so most of their surface is
    /// exercised only by their own tests. That is the designed shape and not a defect. What *is* a
    /// defect is a bare `#[allow(dead_code)]`: it is the annotation that lets an unwired module look
    /// finished, and after enough of them nobody can tell which are waiting and which are dead.
    /// `research/04`'s rule for a bypass applies unchanged — *a bypass with no retirement condition
    /// is a lie with a comment on it.*
    ///
    /// **The blanket used to be invisible here**, and it was the strongest form of the thing this
    /// sweep exists to police. `composer.rs` carried `#![allow(dead_code)]` on line 41, over the
    /// 3,272 lines below it, while the filter read `!line.contains("#[allow(dead_code)]")` — and
    /// `"#![allow(dead_code)]".contains("#[allow(dead_code)]")` is **false**, because the `!` sits
    /// between the `#` and the `[`. So the one attribute in the crate that silenced a whole module
    /// was the one attribute the sweep never counted. [`dead_code_allow`] is now the only reader of
    /// either spelling, in both sweeps.
    ///
    /// That blanket is gone — deleted, not narrowed round, in the commit that measured what it was
    /// hiding — and **this paragraph is past tense on purpose**. The crate has no `#![...]` anywhere
    /// today. The span above is corrected too: the figure two of these doc comments carried was
    /// short by 75 lines, having been measured once and copied twice. A gate whose prose describes
    /// a world that ended is the same defect §16.9 names one level up; a gate that also gets the
    /// world wrong is that defect with a number on it, and neither is fixed by silence. **How a
    /// file's own length gets written into prose at all is the recurring half of this**, and it is
    /// why no current line count appears anywhere above — a count of a file that is still being
    /// edited is stale on the next commit, by construction.
    ///
    /// **And the condition may be stated above rather than beside** — see [`says_what_retires_it`]
    /// for which shape still needs that and which one no longer exists. Both shapes were previously
    /// handled by exempting the whole of `main.rs`, a hole wide enough to hide a blanket in, and
    /// no file is exempt from this sweep any more.
    #[test]
    fn every_dead_code_allow_says_what_would_retire_it() {
        let mut bare: Vec<String> = Vec::new();
        let mut seen = 0usize;
        for (name, text) in rust_sources() {
            let lines: Vec<&str> = text.lines().collect();
            for n in 0..lines.len() {
                if dead_code_allow(lines[n]).is_none() {
                    continue;
                }
                seen += 1;
                if !says_what_retires_it(&lines, n) {
                    bare.push(format!("{name}:{}", n + 1));
                }
            }
        }
        assert!(seen > 10, "only {seen} `#[allow(dead_code)]` were found; the sweep read nothing");
        assert!(
            bare.is_empty(),
            "{bare:?} carry no retirement condition. Write `// retired when: <the observation that \
             makes this reachable>` on the same line, or — for a blanket, which has no item to sit \
             beside — in the comment block directly above it"
        );
    }

    /// Method names `std` defines as well, which a text sweep cannot tell apart from these.
    const AMBIGUOUS: [&str; 1] = ["as_str"];

    /// A call to `name` in `text`, ignoring comments and requiring both boundaries — so
    /// `set_fullscreen(` is not a call to `fullscreen`, and `expanded(` is not a call to `expand`.
    fn calls(name: &str, text: &str) -> bool {
        text.lines().filter(|l| !l.trim_start().starts_with("//")).any(|line| {
            let mut rest = line;
            while let Some(i) = rest.find(name) {
                let before = rest[..i].chars().next_back();
                let after = rest[i + name.len()..].chars().next();
                if before.is_none_or(|c| !c.is_alphanumeric() && c != '_') && after == Some('(') {
                    return true;
                }
                rest = &rest[i + name.len()..];
            }
            false
        })
    }

    /// The modules whose blanket has stopped being true: every `pub fn` they ship is already
    /// called from somewhere else, so *nothing in here is reconnected* is false of all of it.
    ///
    /// **Lifted out of the test so it can be handed something other than this crate**, which is
    /// the whole reason it is a function. This crate carries **no** `#![allow(dead_code)]` — the
    /// last one was deleted from `composer.rs` rather than narrowed — so the loop below hits its
    /// `continue` on every file and decides nothing at all. A sweep that reads nothing is green,
    /// and this repository has been bitten by that shape often enough that a zero here must not
    /// be mistaken for a verdict. The controls beside the call site hand it two synthetic modules
    /// and require both answers, so the emptiness is proved to be *the tree's* and not the
    /// instrument's.
    fn redundant_blankets(sources: &[(String, String)]) -> Vec<String> {
        let mut redundant: Vec<String> = Vec::new();
        for (file, text) in sources {
            if !text.lines().any(|l| dead_code_allow(l) == Some(true)) {
                continue;
            }
            let mut surface = 0usize;
            let mut waiting: Vec<&str> = Vec::new();
            for line in text.lines() {
                let t = line.trim_start();
                if t.starts_with("//") {
                    continue;
                }
                let Some(rest) = ["pub fn ", "pub(crate) fn ", "pub const fn "]
                    .iter()
                    .find_map(|kw| t.strip_prefix(kw))
                else {
                    continue;
                };
                let name = &rest[..rest
                    .find(|c: char| !c.is_alphanumeric() && c != '_')
                    .unwrap_or(rest.len())];
                if name.is_empty() {
                    continue;
                }
                surface += 1;
                // An `AMBIGUOUS` name counts as still waiting. The skip is conservative in this
                // direction on purpose: it can only keep this quiet, never make it fire.
                if AMBIGUOUS.contains(&name)
                    || !sources.iter().any(|(o, t)| o != file && calls(name, t))
                {
                    waiting.push(name);
                }
            }
            // `surface > 0` is not decoration: a module with no public surface at all would
            // otherwise satisfy "every one of them is called" by having none.
            if surface > 0 && waiting.is_empty() {
                redundant.push(format!("{file} ({surface} `pub fn`, all called from elsewhere)"));
            }
        }
        redundant
    }

    /// **No `#[allow(dead_code)]` sits on a function the program already calls.**
    ///
    /// The other half of the rule above, and the half that was false. `expand_opened` and
    /// `expand_closed` carried *retired when: an Expand exists — §11.3* while `main.rs` called both
    /// of them from `on_composer_expand` — so `nav.rs` said it was waiting for a caller it already
    /// had, and §16.9's rule about a stale claim is exactly that. **The compiler cannot report it:
    /// the allow is the thing that silences the compiler.** So it is read here instead.
    ///
    /// **What a text sweep can decide is the call.** For every `fn` directly under an allow, this
    /// looks for a call to it in the shipped half of every *other* module. `rust_sources` has
    /// already cut each file at its own `mod tests {`, so a module exercising its own surface from
    /// its own tests does not count — that is §20 item 12's designed shape, `rail.rs` and `nav.rs`
    /// being written before their producers, and it is not a defect.
    ///
    /// **The honest boundary of a textual instrument, written down.** A method name `std` also
    /// defines cannot be resolved this way: `name.as_str()` on a `String` is not a call to
    /// `Class::as_str`, and `composer.rs` has nine of the former in the half this sweep reads.
    /// Those names are listed in `AMBIGUOUS` and skipped. The list is the limit of the method
    /// rather than a carve-out for convenience — a name joins it when `std` defines it too, never
    /// because a sweep went red.
    ///
    /// **A blanket is not on an item; it is on the module**, and it used to be filtered out here
    /// along with everything else the outer spelling misses — see [`dead_code_allow`]. What
    /// `#![allow(dead_code)]` claims is *this module is not reconnected yet*, and the falsification
    /// of that claim is not one caller — a blanket legitimately covers a **mixture** — it is the
    /// module's whole public surface already being reached from outside. At that point nothing
    /// under it is waiting for a caller, and whatever the compiler still warns about is per-item
    /// debt that belongs on the items. Only a `pub fn` counts towards it: another module cannot
    /// call a private one, so a textual match on a private name is a false positive by
    /// construction.
    ///
    /// **The paragraph that used to end this doc is deleted rather than reworded.** It carved
    /// `composer.rs` out by name — *41 of its 68 shipped `pub fn`s are named in another module, so
    /// its blanket is not redundant and this does not fire on it* — and that sentence has stopped
    /// being about anything twice over: the file's blanket is deleted, so [`redundant_blankets`]
    /// skips it before counting anything, and the count itself had drifted to 40 of 72 by the time
    /// anybody checked. A carve-out is an exemption, and an exemption for a thing that is gone is
    /// the widest hole a sweep can carry. Deleting it changes nothing this test catches — the
    /// carve-out was prose, never a branch — and what it costs is the one worked example of the
    /// verdict, which [`redundant_blankets`]'s own controls now supply instead.
    #[test]
    fn no_dead_code_allow_sits_on_a_function_the_program_already_calls() {
        let sources = rust_sources();

        // **The control, and it is the very call that made this test necessary.** A matcher that
        // cannot see `main.rs` calling `expand_opened` would report every allow in the tree clean,
        // which is the shape of instrument this repository keeps being bitten by.
        let main_text = &sources.iter().find(|(n, _)| n == "main.rs").expect("main.rs").1;
        assert!(
            calls("expand_opened", main_text),
            "the matcher cannot find `main.rs`'s call to `expand_opened`, so it is reading nothing"
        );
        assert!(
            !calls("expand", main_text),
            "the matcher counts `expand_opened(` as a call to `expand(`, so both boundaries are \
             not being checked and every longer name matches its own prefix"
        );

        // **And the second control is the one this sweep shipped without.** The blanket half below
        // reads nothing at all if the attribute matcher cannot see the inner spelling, and a sweep
        // that reads nothing is green — which is exactly how a module-wide allow over 3,272 lines
        // stayed invisible to both of these tests. Pinned as three facts about the matcher rather
        // than as a fact about the tree, so it keeps its meaning after the last blanket is gone.
        //
        // **The last blanket is now gone**, so this is no longer a precaution: these three are the
        // only reason the attribute matcher is known to work at all, and the two beside the blanket
        // half below are the only reason its empty answer is known to be a reading.
        assert_eq!(
            dead_code_allow("#![allow(dead_code)]"),
            Some(true),
            "the matcher does not see the inner spelling, so the blanket sweep reads nothing"
        );
        assert_eq!(
            dead_code_allow("    #[allow(dead_code)]  // retired when: something"),
            Some(false),
            "the matcher does not see the outer spelling, so the item sweep reads nothing"
        );
        assert_eq!(
            dead_code_allow("// prose about #[allow(dead_code)] is not the attribute"),
            None,
            "the matcher reads a comment about the attribute as the attribute"
        );

        // Each allow, paired with the `fn` on the next line that is neither an attribute nor a doc
        // comment. Anything else under an allow — a field, a variant, a `const`, a `mod` — is a
        // different question and is not swept here.
        let mut swept = 0usize;
        let mut stale: Vec<String> = Vec::new();
        for (file, text) in &sources {
            let lines: Vec<&str> = text.lines().collect();
            for (n, line) in lines.iter().enumerate() {
                if dead_code_allow(line).is_none() {
                    continue;
                }
                let Some(item) = lines[n + 1..]
                    .iter()
                    .map(|l| l.trim_start())
                    .find(|t| !t.starts_with("///") && !t.starts_with("#["))
                else {
                    continue;
                };
                let Some(after_fn) = item.split_once("fn ") else { continue };
                let name: String =
                    after_fn.1.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
                if name.is_empty() || AMBIGUOUS.contains(&name.as_str()) {
                    continue;
                }
                swept += 1;
                if let Some((caller, _)) =
                    sources.iter().find(|(o, t)| o != file && calls(&name, t))
                {
                    stale.push(format!("{file}:{} `{name}` (called from {caller})", n + 1));
                }
            }
        }
        assert!(
            swept >= 5,
            "only {swept} `#[allow(dead_code)]` were found on a `fn`; the sweep read nothing"
        );
        assert!(
            stale.is_empty(),
            "{stale:?}: the retirement condition has already been met — the program calls it. \
             Delete the allow, or say what is still waiting"
        );

        // ── The blanket half, and **it reads zero files today** ───────────────────────────────
        //
        // A module carrying `#![allow(dead_code)]` says *nothing in here is reconnected*; it has
        // stopped being true when every `pub fn` the module ships is called from another module.
        // No module in this crate carries one any more — `composer.rs`'s was the only one and it
        // was deleted, not narrowed — so [`redundant_blankets`] `continue`s past every file and
        // returns empty without deciding anything. **That is a green from reading nothing**, which
        // is the exact shape §6 names and the one this file has now shipped twice.
        //
        // So the emptiness is proved to be the tree's rather than the instrument's, here, before
        // it is trusted. Two synthetic modules, differing in one call: with the caller present the
        // blanket is redundant and must be named; with it removed the same blanket is legitimate
        // and must not be. Neither control mentions a real file, so they keep their meaning
        // whatever the crate does next — and the day a blanket comes back, the sweep that meets it
        // is one that has been proved able to fire.
        let blanketed = "#![allow(dead_code)]\npub fn arm(&self) -> u8 { 0 }\n";
        let both = [
            ("parked.rs".to_string(), blanketed.to_string()),
            ("caller.rs".to_string(), "fn go() { thing.arm(); }\n".to_string()),
        ];
        assert_eq!(
            redundant_blankets(&both),
            vec!["parked.rs (1 `pub fn`, all called from elsewhere)".to_string()],
            "the blanket verdict does not fire on a module whose whole public surface is called \
             from another module, so its empty answer about this crate means nothing"
        );
        let alone =
            [both[0].clone(), ("caller.rs".to_string(), "fn go() { thing.other(); }\n".to_string())];
        assert!(
            redundant_blankets(&alone).is_empty(),
            "the blanket verdict fires on a module nothing outside calls, so it reports a legitimate \
             blanket as redundant"
        );

        let redundant = redundant_blankets(&sources);
        assert!(
            redundant.is_empty(),
            "{redundant:?}: the module blanket says nothing here is reconnected, and the whole of \
             each module's public surface is already called from another module. Narrow it to the \
             items the compiler still warns about, each with its own retirement condition"
        );
    }

    /// Every `file.rs:line` a comment writes, paired with what the comment's own sentence says is
    /// there — and whether the line still says it.
    ///
    /// Returns the ones that no longer do, and **how many were checked at all**, because a
    /// citation sweep that finds no citations is the same green as one that finds no defects.
    ///
    /// **The method, and its two honest boundaries.** A backticked span spelled exactly
    /// `something.rs:123` is a citation. Every *other* backticked span in the same sentence is an
    /// anchor — verbatim, plus its last `::` segment, so `Composer::open` anchors on `open` as well
    /// as on itself. The citation holds if any one anchor appears on the cited line. First
    /// boundary: a citation whose file is not in the set is **skipped**, which is how a pointer
    /// into a dependency's source or into a file this crate deleted stays writable. Second: a
    /// citation whose sentence names nothing is reported, because a line number with no claim
    /// beside it is a thing no reader and no sweep can check.
    ///
    /// **It is deliberately exact about the line, and that costs something.** Insert a line above
    /// a cited one and this goes red on prose that was true when it was written. That is the
    /// trade taken on purpose: a `file:line` citation *is* brittle, and the choice is between a
    /// sweep that says so and a comment that quietly stops being true. The report names the line
    /// the anchor moved to, so the correction is a number and not an investigation.
    fn stale_citations(sources: &[(String, String)]) -> (Vec<String>, usize) {
        /// The text after this line's comment marker, or `None` if it is not a comment.
        fn comment_body(line: &str) -> Option<&str> {
            let t = line.trim_start();
            ["///", "//!", "//"].iter().find_map(|m| t.strip_prefix(*m))
        }

        /// The backticked spans of `text`, in order.
        fn spans(text: &str) -> impl Iterator<Item = &str> {
            text.split('`').skip(1).step_by(2)
        }

        /// The sentences of a joined comment paragraph.
        ///
        /// Splits on a `.` followed by a space, stepping over the emphasis and closing marks this
        /// file writes between the two — `not.** Each` ends a sentence and `main.rs` does not.
        /// Never inside a backticked span, so a citation cannot be cut in half.
        fn sentences(para: &str) -> Vec<&str> {
            let b = para.as_bytes();
            let mut out: Vec<&str> = Vec::new();
            let (mut start, mut i, mut tick) = (0usize, 0usize, false);
            while i < b.len() {
                if b[i] == b'`' {
                    tick = !tick;
                } else if !tick && b[i] == b'.' {
                    let mut j = i + 1;
                    while j < b.len() && matches!(b[j], b'*' | b'_' | b')' | b'"' | b'\'') {
                        j += 1;
                    }
                    if b.get(j) == Some(&b' ') {
                        out.push(&para[start..j]);
                        start = j + 1;
                        i = j + 1;
                        continue;
                    }
                }
                i += 1;
            }
            if start < b.len() {
                out.push(&para[start..]);
            }
            out
        }

        /// `Some((file, line))` for a span spelled exactly `something.rs:123`.
        fn citation(span: &str) -> Option<(&str, usize)> {
            let (file, num) = span.rsplit_once(':')?;
            let stem = file.strip_suffix(".rs")?;
            if stem.is_empty() || !stem.chars().all(|c| c.is_ascii_lowercase() || c == '_') {
                return None;
            }
            Some((file, num.parse().ok()?))
        }

        let mut found: Vec<String> = Vec::new();
        let mut checked = 0usize;
        for (name, text) in sources {
            let lines: Vec<&str> = text.lines().collect();
            let mut n = 0;
            while n < lines.len() {
                if comment_body(lines[n]).is_none() {
                    n += 1;
                    continue;
                }
                // One paragraph is one contiguous run of comment lines, joined — a sentence in
                // this file wraps across four of them as often as not.
                let start = n;
                let mut para = String::new();
                while let Some(body) = lines.get(n).and_then(|l| comment_body(l)) {
                    if !para.is_empty() {
                        para.push(' ');
                    }
                    para.push_str(body.trim());
                    n += 1;
                }
                for sentence in sentences(&para) {
                    let anchors: Vec<&str> = spans(sentence)
                        .filter(|s| citation(s).is_none() && s.len() >= 3)
                        .flat_map(|s| {
                            [
                                Some(s),
                                s.rsplit_once("::").map(|(_, seg)| seg).filter(|g| g.len() >= 3),
                            ]
                        })
                        .flatten()
                        .collect();
                    for span in spans(sentence) {
                        let Some((file, num)) = citation(span) else { continue };
                        let Some((_, target)) = sources.iter().find(|(o, _)| o == file) else {
                            continue;
                        };
                        checked += 1;
                        let at =
                            (start..n).find(|&i| lines[i].contains(span)).unwrap_or(start) + 1;
                        let tlines: Vec<&str> = target.lines().collect();
                        if num == 0 || num > tlines.len() {
                            found.push(format!(
                                "{name}:{at} cites `{span}`, and {file} has {} lines",
                                tlines.len()
                            ));
                            continue;
                        }
                        if anchors.is_empty() {
                            found.push(format!(
                                "{name}:{at} cites `{span}` and its sentence names nothing that \
                                 could be checked against that line"
                            ));
                            continue;
                        }
                        let line = tlines[num - 1];
                        if anchors.iter().any(|a| line.contains(*a)) {
                            continue;
                        }
                        // Where it moved to, hunted with the most specific anchor first and
                        // never in a comment: `open` occurs in the word *opens* in a paragraph at
                        // the top of `main.rs`, and a hint that points there is worse than none.
                        let mut by_length = anchors.clone();
                        by_length.sort_by_key(|a| std::cmp::Reverse(a.len()));
                        let elsewhere = by_length.iter().find_map(|a| {
                            tlines
                                .iter()
                                .position(|l| !l.trim_start().starts_with("//") && l.contains(*a))
                                .map(|i| (*a, i))
                        });
                        found.push(match elsewhere {
                            Some((a, i)) => format!(
                                "{name}:{at} cites `{span}`, which reads `{}` — `{a}` is at :{}",
                                line.trim(),
                                i + 1
                            ),
                            None => format!(
                                "{name}:{at} cites `{span}`, which reads `{}`, and {anchors:?} is \
                                 nowhere in that file",
                                line.trim()
                            ),
                        });
                    }
                }
            }
        }
        (found, checked)
    }

    /// **A comment that names a line of this crate still describes that line.**
    ///
    /// The general form of the defect this wave was written to clear. Sweeps, headers and field
    /// docs across these three files described a world that had ended: a blanket allow deleted two
    /// commits earlier, still present tense and still carrying a carve-out by name; a Rail cap
    /// saying its page was not written, beside a page that ships; an entrance that had stopped
    /// being the only one. **Prose is exactly what this project has repeatedly found its gates
    /// cannot see** — the compiler reads none of it, and every one of those was found by a person
    /// re-reading, which is the instrument this repository trusts least.
    ///
    /// Most of that is not mechanisable and this test does not pretend otherwise. *`Work` is the
    /// only page this phase builds* is a sentence about intent; no sweep decides it. But one shape
    /// is decidable and it is the sharpest one, because it is the shape that goes stale **without
    /// anybody editing the comment at all** — a comment that names `file.rs:line`, where inserting
    /// a line anywhere above the target silently moves what is being pointed at. Four citations
    /// into this crate's own files existed when this was written and **three of them were wrong**:
    /// two named the deleted blanket's line, and the third had `composer.rs` saying `push_composer`
    /// read `Composer::open` at a line that had drifted into the middle of a `match`. Nobody had
    /// touched that third comment. This test found it, and then found it again one line out after
    /// an unrelated edit above it — which is the whole argument for having it.
    ///
    /// What it cannot reach is written down in [`stale_citations`] rather than here, and the
    /// narrower honest thing is the count below: the sweep must have read at least two citations,
    /// so a crate that stops writing them cannot be mistaken for a crate whose citations are true.
    /// Two is exactly what it reads today — `main.rs` into `work.rs` and `composer.rs` back into
    /// `main.rs` — so the floor sits **on** the population rather than under it. Deleting either
    /// citation turns this red, which is the point: they are the two claims in this crate that no
    /// human re-reading would ever re-check.
    #[test]
    fn every_comment_that_names_a_line_still_describes_it() {
        // ── The controls, one per branch, none of them naming a real file ──────────────────────
        //
        // A citation sweep is the easiest kind of instrument to ship dead: read no comments, find
        // no citations, report nothing, go green. Each of these hands the matcher two synthetic
        // files and requires a specific answer, so the verdict on the real tree below is known to
        // be a reading.
        let target = ("target.rs".to_string(), "fn one() {}\nfn two() {}\n".to_string());
        let with = |text: &str| [target.clone(), ("cite.rs".to_string(), text.to_string())];

        let (found, checked) = stale_citations(&with("// `two` is what this points at, `target.rs:2`.\n"));
        assert!(found.is_empty(), "{found:?}: a citation whose anchor is on the cited line is reported");
        assert_eq!(checked, 1, "the sweep did not read the one citation it was handed");

        let (found, _) = stale_citations(&with("// `two` is what this points at, `target.rs:1`.\n"));
        assert_eq!(found.len(), 1, "a citation naming the wrong line is not reported");
        assert!(found[0].contains(":2"), "{found:?} does not name the line the anchor moved to");

        let (found, _) = stale_citations(&with("// `two` is what this points at, `target.rs:9`.\n"));
        assert_eq!(found.len(), 1, "a citation past the end of the file is not reported");
        assert!(found[0].contains("2 lines"), "{found:?} does not say how long the file is");

        let (found, _) = stale_citations(&with("// see `target.rs:1` for it.\n"));
        assert_eq!(found.len(), 1, "a citation with nothing to check it against is not reported");

        let (found, checked) = stale_citations(&with("// `two` is at `elsewhere.rs:9`.\n"));
        assert!(
            found.is_empty() && checked == 0,
            "a citation into a file this crate does not have is being decided rather than skipped"
        );

        // ── And the tree ──────────────────────────────────────────────────────────────────────
        //
        // Whole files, not `rust_sources`'s shipped halves: a doc comment in a test module names a
        // line as readily as one above `fn main`, and two of the four this crate had were in one.
        let (stale, checked) = stale_citations(&rust_sources_whole());
        assert!(
            checked >= 2,
            "only {checked} citations into this crate were read; below two, an empty verdict is \
             the sweep reading nothing rather than the comments being true"
        );
        assert!(
            stale.is_empty(),
            "{stale:?}: the comment names a line that no longer says what the comment says. \
             Re-measure the number, or say what is there now"
        );
    }

    /// **T-8. The Rail's model is retained and never rebuilt — and so is the library's.**
    ///
    /// Two halves. The first is the source: each setter is called once, and never with a freshly
    /// constructed model. The second is `sync_rail`'s own behaviour, and it needs a window — see
    /// `the_rail_model_survives_two_hundred_mutations`.
    #[test]
    fn the_rail_model_is_never_rebuilt() {
        // **The control first**, because a matcher that finds nothing looks exactly like a matcher
        // that is broken. `set_selected` is a plain `i32` setter called from `refresh_devices` and
        // is never a model handoff, so it proves the matcher counts; the wholesale half is proved
        // against a line written here.
        let (control, _) = model_handoffs("set_selected(");
        assert!(
            !control.is_empty(),
            "the matcher found no `set_selected(` at all, so it is not reading this crate"
        );
        let synthetic = "w.set_rail(ModelRc::from(Rc::new(VecModel::from(v))));";
        assert!(
            synthetic.contains("set_rail(") && synthetic.contains("VecModel::from"),
            "the wholesale-rebuild shape this test looks for no longer matches itself"
        );

        for setter in [
            "set_rail(",
            "set_devices(",
            // §11.2's six. `push_composer` runs on **every keystroke** in the serial field, so a
            // fresh `VecModel` per call would take the caret with it on every character typed —
            // which is the same defect as the Rail's, on the page that is typed into most.
            "set_composer_picks(",
            "set_composer_fields(",
            "set_composer_ticks(",
            "set_composer_options(",
            "set_composer_plan(",
            "set_composer_refusals(",
        ] {
            let (calls, wholesale) = model_handoffs(setter);
            assert_eq!(
                calls.len(),
                1,
                "`{setter}` is called at {calls:?}; §16.9 says once — every rebuild tears down \
                 every repeater instance under it"
            );
            assert!(
                wholesale.is_empty(),
                "`{setter}` is handed a freshly built model at {wholesale:?}, which tears down \
                 every repeater instance"
            );
        }
    }

    /// **The registered `+ New device ›` opens the Composer and mints nothing.**
    ///
    /// §11.2: `nor::mint_seed` is the one irreversible call in this program — the seed *is* the
    /// iPod — so it happens when somebody presses `Make one`, not when a page opens. Three
    /// cancelled visits to this page must leave zero iPods, and this is the first of the three.
    ///
    /// §20 item 12's lesson: it drives the callback `wire` registers, not a function beside it.
    #[test]
    fn the_registered_new_device_handler_opens_the_composer_and_mints_nothing() {
        let settings = Rc::new(RefCell::new(Settings::default()));
        let w = a_window();
        wire(&w, settings.clone());
        let before = settings.borrow().resources.len();

        w.invoke_device_new();

        assert!(w.get_drawer_open(), "the Composer did not open the drawer");
        assert_eq!(
            w.get_drawer_page(),
            DrawerPage::Composer,
            "`+ New device` did not land on the Composer's root"
        );
        assert_eq!(
            w.get_drawer_depth(),
            nav::Page::Composer.slot().expect("the Composer has a slot"),
            "the Composer is drawn at one level and was navigated to at another, which is a blank \
             420 px panel with no header"
        );
        assert_eq!(
            settings.borrow().resources.len(),
            before,
            "opening the page minted an iPod; the seed IS the iPod and cancelling must cost nothing"
        );
        // §11.3 rule (0): the region says nothing it will have to take back.
        assert_eq!(
            w.get_composer_region_text().to_string(),
            compose::NOTHING_CHOSEN,
            "the opening state asserts a plan for a firmware nobody has chosen"
        );
        assert_eq!(w.get_composer_plan().row_count(), 0, "an unchosen recipe drew a plan");
        assert!(
            !w.get_composer_create().enabled,
            "`Create` is live on a recipe the verdict refuses"
        );
        // Three visits, three cancellations, zero iPods — which is the promise in full.
        for _ in 0..2 {
            w.invoke_drawer_back();
            w.invoke_device_new();
        }
        assert_eq!(settings.borrow().resources.len(), before);
    }

    /// **A press that changes nothing in the library does not rewrite the library's file.**
    ///
    /// §20 item 13's other half, and it is the one that costs somebody something: `Settings::render`
    /// regenerates the file **whole**, from the model, so every comment the operator added goes with
    /// it. A save on a callback that mutated nothing is that deletion, for nothing. The rule is
    /// *every callback that mutated `Settings` ends in `save`, and one that did not must not*, and
    /// this is the second clause.
    ///
    /// `Make one` is the case that looks like an exception and is not: `nor::mint_seed` is the one
    /// irreversible act on the page — the seed **is** the iPod — but it mints into the Composer,
    /// and `composer.rs` files it at `Create`. So the mint changes the page and not the file.
    #[test]
    fn a_press_that_writes_no_library_does_not_rewrite_the_settings_file() {
        let settings = Rc::new(RefCell::new(Settings::default()));
        let w = a_window();
        wire(&w, settings.clone());
        w.invoke_device_new();

        let path = eapp_loader::settings::Settings::path().expect("a settings path");
        // A comment nobody's model holds, which is exactly what `render` cannot carry.
        std::fs::write(&path, "# a comment somebody added\nwelcomed = true\n").expect("scratch");

        w.invoke_composer_act(composer::Field::Ipod.as_i32());
        w.invoke_composer_type(composer::Field::Name.as_i32(), "My 5.5G".into());
        w.invoke_composer_reveal(composer::Field::Serial.as_i32());

        assert_eq!(
            settings.borrow().resources.len(),
            0,
            "`Make one` filed an iPod; §11.2 files it at `Create` and cancelling must cost nothing"
        );
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(
            text.contains("# a comment somebody added"),
            "a press that wrote nothing to the library rewrote the file and took the operator's \
             comment with it:\n{text}"
        );

        // The control: the mint DID happen — it is on the page rather than in the file, which is
        // the distinction this test is about and not an absence of any effect at all.
        assert!(
            !w.get_composer_which_value().is_empty(),
            "`Make one` did nothing at all, so the assertion above is vacuous"
        );
    }

    /// Every `.rs` file in this crate, name and text, **whole** — test module included.
    ///
    /// [`rust_sources`] is this with each file cut at its test module, and that is the only
    /// difference between them. One reader wants the uncut text: a doc comment can name a line of
    /// this crate from either half, and two of the four that did were in a test module.
    pub(crate) fn rust_sources_whole() -> Vec<(String, String)> {
        let dir = std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/src"));
        let mut out: Vec<(String, String)> = std::fs::read_dir(&dir)
            .expect("the src directory")
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "rs"))
            .map(|p| {
                let name = p.file_name().unwrap().to_string_lossy().into_owned();
                (name, std::fs::read_to_string(&p).expect("a source file"))
            })
            .collect();
        out.sort();
        assert!(out.len() > 5, "the source sweep found {} files", out.len());
        out
    }

    /// Every `.rs` file in this crate, name and text, **with its test module cut off**.
    ///
    /// The cut is load-bearing rather than tidy: the two sweeps below look for `set_rail(` and
    /// `on_winit_window_event`, and their own assertion messages name both. Without it each one
    /// counts itself and reports three registrations where there is one — an instrument reporting
    /// a defect it created by looking.
    pub(crate) fn rust_sources() -> Vec<(String, String)> {
        let mut out: Vec<(String, String)> = Vec::new();
        for (name, text) in rust_sources_whole() {
            let kept: Vec<&str> =
                text.lines().take_while(|l| !l.trim_end().ends_with("mod tests {")).collect();
            // **The cut must have landed on a declaration.** `take_while` stops at the first line
            // that *ends with* `mod tests {`, and a comment could end with it as easily as code —
            // the twin of the `#[cfg(test)]` split `composer.rs` was cut short by, where a header
            // bullet naming the attribute moved three sweeps' bodies from 2,043 lines to 42.
            // Checked here because a cut that lands early takes every sweep above it with it,
            // and does so in silence.
            let cut = text.lines().nth(kept.len());
            assert!(
                cut.is_none_or(|l| !l.trim_start().starts_with("//")),
                "{name}'s test-module cut landed on a comment at :{}, so every sweep that reads \
                 this file is reading {} lines of it",
                kept.len() + 1,
                kept.len()
            );
            out.push((name, kept.join("\n")));
        }
        // The control: the cut must not have taken the shipped half with it.
        assert!(
            out.iter().any(|(n, t)| n == "main.rs" && t.contains("fn main()")),
            "the test-module cut removed the program"
        );
        out
    }

    /// The other half of T-8, with a window: **the model object is the same one, 200 mutations
    /// later.**
    #[test]
    fn the_rail_model_survives_two_hundred_mutations() {
        let window = a_window();
        let rows: Rc<VecModel<RailRow>> = Rc::new(VecModel::default());
        window.set_rail(ModelRc::from(rows.clone()));

        let mut rail = rail::Rail::new();
        for i in 0..200 {
            if i % 3 == 0 {
                rail.note(&format!("note {i}"));
            } else if i % 3 == 1 {
                rail.failed(
                    "fetch",
                    &format!("thing {i}"),
                    rail::Failure::new(rail::Class::Network, "a download"),
                );
            } else if let Some(id) = rail.entries().first().map(|e| e.id) {
                rail.dismiss(id);
            }
            sync_rail(&window, &rows, &rail, caps(), work::Shape::default());
        }

        // **Identity first**, because it is the whole of the claim and because a replaced model
        // also has the wrong row count — a failure that reported the count would name the symptom
        // and not the cause.
        let held = window.get_rail();
        let same = held
            .as_any()
            .downcast_ref::<VecModel<RailRow>>()
            .is_some_and(|m| std::ptr::eq(m, Rc::as_ptr(&rows)));
        assert!(
            same,
            "the window is holding a different model object from the one it was handed; 200 \
             mutations rebuilt it at least once, and every rebuild tears down every repeater \
             instance"
        );
        assert_eq!(rows.row_count(), rail.entries().len(), "the model and the Rail disagree");
    }

    /// A headless `MainWindow`. **Every window test in this file goes through here.**
    ///
    /// **The guard is per THREAD, and a `static Once` is the wrong shape.** Slint's platform lives
    /// in `GLOBAL_CONTEXT`, which is a **thread-local**
    /// (`i-slint-core-1.17.1/platform.rs:257-277`), and `init_no_event_loop`'s own documentation
    /// says so: *"each test thread can use its own backend"*. A process-wide `Once` therefore
    /// installs the testing backend on whichever thread happens to run first and leaves every other
    /// test thread with no platform at all — which falls through to winit and fails with *"Error
    /// initializing winit event loop: EventLoop can't be recreated"*, or on macOS with *"`EventLoop`
    /// must be created on the main thread!"*. Both read like a threading bug in this crate and
    /// neither is one; the second one does not even name Slint.
    ///
    /// It was a `static Once` while exactly one test made a window, where the difference is
    /// invisible. Four tests made it visible in the most confusing way available.
    fn a_window() -> MainWindow {
        // Before anything that could reach `settings::data_dir` — `wire` saves, and its queue names
        // `drives/`. See [`use_a_scratch_data_dir`].
        let _held = use_a_scratch_data_dir();
        thread_local! {
            static READY: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
        }
        READY.with(|ready| {
            if !ready.replace(true) {
                i_slint_backend_testing::init_no_event_loop();
            }
        });
        MainWindow::new().expect("the headless backend makes a window")
    }

    /// **Every property this file pushes reaches a real composed window, and comes back.**
    ///
    /// `the_window_can_be_told_everything_this_file_pushes` is the compile-time half: it names the
    /// setters and fails to build if one is renamed. This is the run-time half, and it is what
    /// caught nothing until the window was composed out of five files rather than one — a property
    /// declared on `MainWindow` and never forwarded to the component that draws it compiles, pushes,
    /// and reaches nobody.
    #[test]
    fn the_composed_window_takes_everything_this_file_pushes() {
        let w = a_window();

        w.set_devices(ModelRc::from(Rc::new(VecModel::from(device_rows(&Settings::default())))));
        w.set_empty_device(empty_device(false, caps(), no_cost()));
        w.set_screen_source(dark_screen());
        w.set_panel_description(panel_description(&phase()).into());
        w.global::<Motion>().set_scale(motion::scale());
        w.global::<Metric>().set_mono_family(mono_family().into());

        let f = fit::Fit {
            k: 2,
            hero_logical: 655.751,
            panel_w: 320.0,
            panel_h: 240.0,
            too_short: true,
        };
        push_fit(&w, &f, 2.0);
        assert_eq!(w.get_screen_scale(), 2);
        assert!(w.get_too_short(), "the too-short input did not reach the window");
        assert!(
            w.get_fidelity().contains('2'),
            "the fidelity line does not carry k: {:?}",
            w.get_fidelity()
        );
        // §7.4: the hit region is the model's, and it is wider than the drawn disc — which is the
        // whole reason it is pushed rather than derived in markup.
        let drawn = (geometry::CENTRE_D * f.hero_logical) as f32;
        assert!(
            w.get_select_d() > drawn,
            "the centre button's target ({}) is no bigger than its drawing ({drawn}), so the \
             pushed value is not reaching it",
            w.get_select_d()
        );

        push_ledger(&w, None, &temp_dir("ledger"), None);
        assert!(!w.get_ledger_download().is_empty(), "the ledger has no download line");
        assert!(!w.get_ledger_disk().is_empty(), "the ledger has no disk line");

        // The heading comes from `sync_rail` now, not from `push_ledger` — because it has to follow
        // what the Rail is holding rather than being frozen at startup.
        let rows: Rc<VecModel<RailRow>> = Rc::new(VecModel::default());
        w.set_rail(ModelRc::from(rows.clone()));
        sync_rail(&w, &rows, &rail::Rail::new(), caps(), work::Shape::default());
        assert!(!w.get_work_heading().is_empty(), "the Work page has no heading");
        assert!(!w.get_work_empty().is_empty(), "the empty Work page says nothing");
        assert_eq!(w.get_rail_first_failure(), -1, "an empty Rail has no primary row");
        assert!(w.get_rail_line().is_empty(), "an empty Rail is not a shelf line");

        // §10.1's ghost and §12.3's bar, both of which had been drawable and unbound. `set_ghost`
        // is pushed by `refresh_devices`; `set_progress` by `sync_rail`.
        refresh_devices(&w, &Rc::new(VecModel::default()), &Settings::default(), &latch(true), caps(), no_cost());
        assert!(w.get_ghost(), "an empty library did not reach the bench as a ghost");
        assert!(w.get_progress() < 0.0, "an empty Rail claims a denominator it does not have");

        // §9.2's cradle line, and §17.Q12's measurement. `working-label` is pushed by `sync_rail`
        // and empty when nothing is running; `verb-width` is the one property that goes the other
        // way — the renderer's answer, read out — so it is checked for being a real number rather
        // than for reaching the window.
        let rows2: Rc<VecModel<RailRow>> = Rc::new(VecModel::default());
        w.set_rail(ModelRc::from(rows2.clone()));
        sync_rail(&w, &rows2, &rail::Rail::new(), caps(), work::Shape::default());
        assert!(w.get_working_label().is_empty(), "an idle bench claims to be working");
        assert!(
            w.get_verb_width() >= 0.0,
            "the verb probe measured {} px, which is not a width",
            w.get_verb_width()
        );

        let stack = nav::Stack::new();
        push_nav(&w, &stack);
        assert!(!w.get_drawer_open());
        assert_eq!(w.get_drawer_page(), DrawerPage::None);
    }

    /// **§12.3's bar reaches the bench, and a step with no denominator draws no bar.**
    ///
    /// `Bench.progress` has been declared, forwarded and drawn since the drawer landed, bound to
    /// nothing — the drawn-instrument-with-no-producer shape §20 item 15 names. `sync_rail` is the
    /// producer now, and the **sign** is the contract: `bench.slint` draws the bar on
    /// `progress >= 0`, so a negative value is *no denominator, no bar, a number that moves
    /// instead*.
    #[test]
    fn progress_reaches_the_bench_and_a_step_with_no_denominator_draws_no_bar() {
        let w = a_window();
        let rows: Rc<VecModel<RailRow>> = Rc::new(VecModel::default());
        w.set_rail(ModelRc::from(rows.clone()));

        let mut r = rail::Rail::new();
        sync_rail(&w, &rows, &r, caps(), work::Shape::default());
        assert!(w.get_progress() < 0.0, "an empty Rail is not 0 % of anything");

        // A step with real bytes on both sides: a real fraction, and the bar is drawn.
        let id = r.note("fetching");
        r.progress(id, rail::Progress::Bytes { done: 3_250_000, total: 6_500_000 });
        sync_rail(&w, &rows, &r, caps(), work::Shape::default());
        assert!(
            (w.get_progress() - 0.5).abs() < 0.01,
            "half of 6.5 MB reached the bench as {}",
            w.get_progress()
        );

        // **A denominator of zero is not zero progress.** The catalogue records no size for some
        // releases, and a bar drawn at 0 % against an unknown total is an instrument that lies.
        r.progress(id, rail::Progress::Bytes { done: 3_250_000, total: 0 });
        sync_rail(&w, &rows, &r, caps(), work::Shape::default());
        assert!(
            w.get_progress() < 0.0,
            "a download with no recorded size drew a bar at {}",
            w.get_progress()
        );

        // …and a failure takes the bar away rather than freezing it where it stopped.
        r.progress(id, rail::Progress::Bytes { done: 3_250_000, total: 6_500_000 });
        sync_rail(&w, &rows, &r, caps(), work::Shape::default());
        assert!(w.get_progress() >= 0.0, "the fixture is not in the state the next line tests");
        r.fail(id, rail::Failure::new(rail::Class::Network, "the download"));
        sync_rail(&w, &rows, &r, caps(), work::Shape::default());
        assert!(
            w.get_progress() < 0.0,
            "the bar froze at {} on a step that failed; a frozen bar is a paused machine \
             pretending",
            w.get_progress()
        );
    }

    /// **The ghost is an emptiness state, and it goes both ways.**
    ///
    /// §9.1 gives the later-empty bench the same drawing §10.1 gives the first one, so this is
    /// recomputed from the library on every pass rather than latched. The welcome copy is the
    /// latch, and it is deliberately a different rule: a cancelled build empties the list, and the
    /// bench may redraw the ghost for that without returning anybody to step one.
    #[test]
    fn the_ghost_follows_the_library_and_a_real_device_is_not_one() {
        let w = a_window();
        let devices: Rc<VecModel<DeviceRow>> = Rc::new(VecModel::default());
        w.set_devices(ModelRc::from(devices.clone()));

        let mut s = Settings::default();
        refresh_devices(&w, &devices, &s, &latch(false), caps(), no_cost());
        assert!(w.get_ghost(), "an empty library is not drawn as a ghost");

        // **A real device whose ROM did not state a colour is NOT a ghost**, and that is the case
        // this could most easily get wrong: `Colour::Unspecified` draws the same neutral `#E4E4E2`
        // chassis the ghost does, and the difference between them is the opacity alone.
        s = a_library_of_one();
        s.devices[0].chassis = Some(Colour::Unspecified);
        refresh_devices(&w, &devices, &s, &latch(false), caps(), no_cost());
        assert!(!w.get_ghost(), "a device in the library is drawn as a ghost");
        assert_eq!(
            devices.row_data(0).expect("the device").chassis,
            empty_device(false, caps(), no_cost()).chassis,
            "the fixture no longer shares the ghost's chassis colour, so the line above proves \
             nothing about the opacity being what separates them"
        );

        // …and the last device leaving brings it back.
        s.devices.clear();
        refresh_devices(&w, &devices, &s, &latch(false), caps(), no_cost());
        assert!(w.get_ghost(), "the last device left and the bench still draws a solid iPod");
    }

    /// **§9.5's boolean flips at the height the device actually needs, and it reaches the window.**
    ///
    /// `required_client_logical` is `hero + CHROME_MIN`, and the whole of §9.5 is that below it the
    /// device cannot be drawn at 1:1. Both sides are checked, because a flag that is always true is
    /// as useless as one that is always false — and the hysteresis is checked too, because a
    /// threshold with none flickers between two layouts while the mouse is down.
    #[test]
    fn the_too_short_boolean_flips_at_the_height_the_device_needs() {
        let w = a_window();
        let sf = 1.0;
        let mut fitter = fit::Fitter::new(sf);
        let (fit, _) = fitter.apply(fit::Moment::Shown {
            display_logical: 1200.0,
            window_logical: 1200.0,
            sf,
        });
        let need = fit::required_client_logical(fit.hero_logical);
        assert!(!fit.too_short, "a 1200 px window is not too short for a {need:.1} px device");

        // One pixel under, and it is.
        let (fit, _) = fitter.apply(fit::Moment::Resized { window_logical: need - 1.0 });
        assert!(fit.too_short, "{:.1} px is under {need:.1} and did not flip", need - 1.0);
        push_fit(&w, &fit, sf);
        assert!(w.get_too_short(), "the flip did not reach the window");

        // **Hysteresis, and it is not decoration.** Coming back up, it stays true until the window
        // clears the threshold by the whole band — otherwise a drag that hovers on the boundary
        // swaps the layout on every frame.
        let (fit, _) = fitter.apply(fit::Moment::Resized { window_logical: need + 1.0 });
        assert!(
            fit.too_short,
            "it cleared the threshold by 1 px and flipped straight back; the hysteresis band is \
             {} px",
            geometry::HYSTERESIS
        );
        let (fit, _) =
            fitter.apply(fit::Moment::Resized { window_logical: need + geometry::HYSTERESIS + 1.0 });
        assert!(!fit.too_short, "it cleared the whole band and stayed too short");
        push_fit(&w, &fit, sf);
        assert!(!w.get_too_short(), "the flip back did not reach the window");
    }

    /// **The push arithmetic: the drawer takes 420 px from the well and the device still fits.**
    ///
    /// §9.6's `min-width` derivation is `DRAWER_W + the device + its fixture + the well's air`, and
    /// `the_min_width_derivation_sums_to_the_declared_minimum` checks that sum. What that one cannot
    /// see is the consequence: at the narrowest window the program allows, **with the drawer open**,
    /// there has to be room left for the device at `k = 1`. If there is not, opening the drawer at
    /// the minimum width clips the thing the program is for.
    #[test]
    fn opening_the_drawer_at_the_narrowest_window_still_leaves_room_for_the_device() {
        let well = geometry::MIN_WIDTH - geometry::DRAWER_W;
        let device = geometry::BODY_ASPECT * geometry::HERO_PHYS_1X;
        let fixture = 2.0 * (geometry::CRADLE_OVERHANG + geometry::FOCUS_GAP);
        let needed = device + fixture;
        assert!(
            well >= needed,
            "with the drawer open the well is {well:.1} px wide and the device and its cradle need \
             {needed:.1}"
        );
        // And the air §9.6 puts round it is what is left, not something borrowed from the device.
        let air = (well - needed) / 2.0;
        assert!(
            air >= geometry::WELL_AIR - 0.05,
            "the well has {air:.1} px of air on each side and §9.6 declares {}",
            geometry::WELL_AIR
        );
    }

    /// **The centre button is reachable with the keyboard alone, and the pointer is never touched.**
    ///
    /// §7.3 makes the cradle the Button and §16.5 is why it is never `enabled: false`: a
    /// `FocusScope { enabled: false }` refuses focus even programmatically
    /// (`i-slint-core-1.17.1/items/input_items.rs:643-645`), so a control gated that way cannot be
    /// reached by anybody who does not use a mouse.
    ///
    /// This drives the whole route: the window's `init` hands focus to the cradle, Return reaches
    /// `cradle-focus`'s own `key-pressed`, and the callback the drawn centre button fires is the
    /// same one. No pointer event is dispatched anywhere in this test.
    #[test]
    fn the_centre_button_is_reachable_from_the_keyboard_with_no_pointer() {
        let w = a_window();
        w.set_devices(ModelRc::from(Rc::new(VecModel::from(device_rows(&a_library_of_one())))));
        w.set_empty_device(empty_device(false, caps(), no_cost()));

        let fired = Rc::new(std::cell::Cell::new(0));
        {
            let fired = fired.clone();
            w.on_start_device(move |i| fired.set(fired.get() + 1 + i));
        }

        w.show().expect("the headless backend shows a window");
        // `i-slint-backend-testing`'s own keyboard helpers are behind its `internal` feature and
        // take a `WindowAdapterRc`. `slint::platform::WindowEvent` is the supported public route and
        // is the same thing one event lower down.
        let key: slint::SharedString = slint::platform::Key::Return.into();
        w.window()
            .dispatch_event(slint::platform::WindowEvent::KeyPressed { text: key.clone() });
        w.window().dispatch_event(slint::platform::WindowEvent::KeyReleased { text: key });
        assert_eq!(
            fired.get(),
            1,
            "Return did not start the selected device. §7.3 makes the cradle the Button and the \
             window's `init` is what hands focus to it; with neither, the one control this whole \
             program is built around is unreachable without a mouse."
        );
    }

    /// A one-device library whose parts are all present, built in memory. No filesystem, because
    /// what is under test is the keyboard route rather than resolution.
    fn a_library_of_one() -> Settings {
        let mut s = Settings::default();
        let rom = s.file_away(
            eapp_loader::settings::Resource::Firmware(eapp_loader::nor::Source::default()),
            "an iPod",
            None,
        );
        s.devices.push(Device {
            name: "iPod 1".into(),
            firmware: rom,
            ..Device::default()
        });
        s
    }

    /// **A disabled control states its reason to an assistive technology, with no pointer.**
    ///
    /// T-17. Six of the drawer's seven rows are disabled because the page behind them is not built
    /// (§9.4's second kind), and a row that is present and dead without saying why is the shape §19.1
    /// indicts. The two ways to get this wrong are both silent: `enabled:` on the inner `FocusScope`
    /// means the control cannot be focused at all, and `enabled:` on the inner `TouchArea` forcibly
    /// sets `has_hover = false` (`input_items.rs:80-91`) so a hover-gated reason never appears.
    ///
    /// Read through the accessible tree rather than off the screen, which is the same thing a screen
    /// reader gets and needs no pixels.
    /// Every `Row` in the tree, which is every row of the drawer's menu and nothing else.
    ///
    /// **Matched by type, never by label.** `find_by_accessible_label(&w, "Work")` looks like the
    /// obvious query and finds the wrong element: §7.5's shelf draws `MENU › Devices · Parts · Games
    /// · Work · Readout` as separate `Text` runs — because those two glyphs are drawn `Path`s, not
    /// characters — and a `Text` reports its own string as its accessible label. The bench comes
    /// before the drawer in tree order, so the shelf's word wins. `Row` exists only in `MenuPage`.
    ///
    /// **Matched by role rather than by type name**, too: `match_type_name("Row")` returns 70 for
    /// seven rows. A chain of elements optimised into one `ItemRc` is reported once per element
    /// index (`search_api.rs:330-336`), and `Row inherits Pressable inherits Rectangle` is ten of
    /// them. The role is declared once, on the outermost.
    ///
    /// **And deduplicated by position**, which is the third thing this query needed. A chain of
    /// elements optimised into a single `ItemRc` is reported once per element index
    /// (`search_api.rs:329-336`), and `Row inherits Pressable inherits Rectangle` is ten of them
    /// carrying one role between them — so an undeduplicated query answers 70 for seven rows.
    fn drawer_rows(w: &MainWindow) -> Vec<i_slint_backend_testing::ElementHandle> {
        let mut seen: Vec<(u32, u32)> = Vec::new();
        let mut out = Vec::new();
        for e in i_slint_backend_testing::ElementQuery::from_root(w)
            .match_descendants()
            .match_accessible_role(i_slint_backend_testing::AccessibleRole::ListItem)
            .find_all()
        {
            let at = e.absolute_position();
            let key = (at.x.to_bits(), at.y.to_bits());
            if !seen.contains(&key) {
                seen.push(key);
                out.push(e);
            }
        }
        out
    }

    /// Let the drawer's slide finish. **Not decoration**: the testing backend runs on mock time, so
    /// an animation that nobody advances stays on its first frame for ever — and `Drawer::on-screen`
    /// is read off the animated `x` on purpose, so that both the outgoing and the incoming page stay
    /// drawn mid-slide. Without this a just-opened drawer is still at `x == client.width` and every
    /// row in it is geometrically clipped away, which reads exactly like the gating being broken.
    fn let_the_drawer_settle() {
        i_slint_backend_testing::mock_elapsed_time(std::time::Duration::from_millis(1_000));
    }

    // **Ignored in a release profile rather than failing there.** `build.rs` emits Slint's debug
    // info only when `PROFILE` is `debug`, and `ElementHandle` refuses to run without it — so under
    // `cargo test --release` this would panic with *"The use of the ElementHandle API requires the
    // presence of debug info"*, which says nothing about this program. It says so here instead.
    #[cfg_attr(
        not(debug_assertions),
        ignore = "needs SLINT_EMIT_DEBUG_INFO, which build.rs emits in debug profiles only"
    )]
    #[test]
    fn a_disabled_row_states_its_reason_to_an_assistive_technology() {
        let w = a_window();
        w.show().expect("the headless backend shows a window");
        w.window().set_size(slint::LogicalSize::new(
            geometry::PREF_WIDTH as f32,
            geometry::PREF_HEIGHT as f32,
        ));
        let mut stack = nav::Stack::new();
        stack.toggle();
        push_nav(&w, &stack);
        let_the_drawer_settle();

        let rows = drawer_rows(&w);
        assert_eq!(
            rows.len(),
            7,
            "the drawer's menu is not seven rows; §9.1 keeps a page you cannot open present and \
             greyed rather than absent, so that count is the design"
        );

        let by_label = |want: &str| {
            rows.iter()
                .find(|r| r.accessible_label().is_some_and(|l| l == want))
                .unwrap_or_else(|| panic!("no drawer row labelled {want:?}"))
        };

        // **`Devices` came off this test and `Games` took its place**, which is three deliberate
        // lines rather than a slip: `ui/drawer.slint`'s depth-1 slot draws Devices, Parts and
        // Settings now, and a row that goes on saying *the page behind it is not built* beside a
        // page that exists is the stale claim §16.9 deletes. The three that remain are exactly the
        // three `Page::slot` still answers `None` for.
        let unbuilt = by_label("Games");
        assert_eq!(
            unbuilt.accessible_enabled(),
            Some(false),
            "the `Games` row claims to work; the page behind it is not built"
        );
        assert!(
            !unbuilt.accessible_description().unwrap_or_default().is_empty(),
            "the `Games` row is disabled and says nothing about why, which is §19.1's finding \
             with the label changed"
        );

        // The control: the pages that ARE built have to read differently, or `accessible-enabled`
        // is not being set from anything and every answer above is the same answer.
        for built in ["Work", "Devices", "Parts", "Settings"] {
            assert_eq!(
                by_label(built).accessible_enabled(),
                Some(true),
                "the `{built}` row reads as disabled beside the page that draws it"
            );
        }
        // …and no row states a gap that has been closed.
        for live in ["Devices", "Parts", "Settings"] {
            assert_eq!(
                by_label(live).accessible_description().unwrap_or_default().to_string(),
                "",
                "the `{live}` row still explains why the page it opens does not exist"
            );
        }
    }

    /// **A closed drawer is gone, not parked off screen still being announced.**
    ///
    /// Focus traversal and the accessible tree are both filtered on the same thing, and it is not
    /// the `visible` property: `ItemRc::is_visible` intersects an item's absolute rect with its
    /// absolute clip rect (`i-slint-core-1.17.1/item_tree.rs:399-408`), `move_focus` calls
    /// `is_visible_or_clipped_by_flickable` for `TabNavigation` (`window.rs:1327`), and
    /// `visible: false` reaches both only because it is lowered to a `Clip` element that empties
    /// that rect (`passes/visible.rs`).
    ///
    /// **Which makes the parked position load-bearing, and it is true by a zero-width margin.** The
    /// drawer parks at exactly `x == client.width` and clips its own contents, so its clip
    /// intersects the client's to a box of width 0 — empty, and its rows are gone. Measured, and
    /// then measured the other way: park it 12 px short of the edge and, with `Drawer::on-screen`
    /// gated on the on-stage boolean, this still passes; take that gate away as well and it fails
    /// with *"the drawer is closed and 7 of its rows are still in the accessible tree"*.
    ///
    /// So the gate is what makes the guarantee independent of where the drawer parks — which is what
    /// `Drawer`'s `open` property was declared for and then read by nothing, §20 item 15's defect in
    /// a new place. The window supplies it as *on stage* rather than *open*, so the pages stay drawn
    /// through the closing slide and go the moment it finishes.
    // **Ignored in a release profile rather than failing there.** `build.rs` emits Slint's debug
    // info only when `PROFILE` is `debug`, and `ElementHandle` refuses to run without it — so under
    // `cargo test --release` this would panic with *"The use of the ElementHandle API requires the
    // presence of debug info"*, which says nothing about this program. It says so here instead.
    #[cfg_attr(
        not(debug_assertions),
        ignore = "needs SLINT_EMIT_DEBUG_INFO, which build.rs emits in debug profiles only"
    )]
    #[test]
    fn a_closed_drawer_is_out_of_the_accessible_tree() {
        let w = a_window();
        w.show().expect("the headless backend shows a window");
        // **The window has to have a size or nothing has a geometry**, and `ElementHandle`'s
        // traversal is geometry: `ItemRc::is_visible` is an intersection of the item's absolute rect
        // with its absolute clip rect (`i-slint-core-1.17.1/item_tree.rs:399-408`) and never looks
        // at the `visible` property itself — `visible: false` is lowered to a `Clip` element
        // (`passes/visible.rs`), which is what empties that rect. With no size every rect is zero,
        // every intersection degenerates the same way, and the query answers the same for a drawn
        // element and a hidden one.
        w.window().set_size(slint::LogicalSize::new(
            geometry::PREF_WIDTH as f32,
            geometry::PREF_HEIGHT as f32,
        ));

        // Closed — which is the state `push_nav` starts in, not something set here.
        push_nav(&w, &nav::Stack::new());
        assert!(!w.get_drawer_open(), "the drawer did not start closed");
        assert!(
            drawer_rows(&w).is_empty(),
            "the drawer is closed and {} of its rows are still in the accessible tree, 420 px off \
             the right edge",
            drawer_rows(&w).len()
        );

        // The control: opened, the very same query finds them — otherwise this test would pass
        // against a drawer that had been deleted.
        let mut stack = nav::Stack::new();
        stack.toggle();
        push_nav(&w, &stack);
        let_the_drawer_settle();
        assert!(
            !drawer_rows(&w).is_empty(),
            "opening the drawer does not put its rows in the accessible tree either, so the check \
             above was not looking at anything"
        );

        // …and closing it again takes them back out, once the slide has finished. Mid-slide they
        // stay — that is what *on stage* means and why the window supplies it rather than `open`.
        stack.toggle();
        push_nav(&w, &stack);
        assert!(
            !drawer_rows(&w).is_empty(),
            "the drawer blanked the instant it was told to close, so it slides out empty"
        );
        let_the_drawer_settle();
        assert!(
            drawer_rows(&w).is_empty(),
            "the drawer finished closing and {} of its rows are still announced",
            drawer_rows(&w).len()
        );
    }

    /// **The gate is a boolean, not a geometric accident — and this is the half that says so.**
    ///
    /// The behavioural test above fires only on the *conjunction* of two things going wrong: delete
    /// `Drawer::on-screen`'s `root.open` term and it still passes, because the drawer happens to park
    /// at exactly `client.width` where its clip degenerates to zero width; park it 12 px short and it
    /// still passes, because the gate catches it. Each defect alone ships green, so the test's name
    /// promises more than that test can deliver.
    ///
    /// This is the other half, read out of the markup: the gate exists, it is ANDed rather than
    /// or-ed, and the window supplies it as *on stage* rather than *open* so pages stay drawn through
    /// the closing slide. It is a text assertion because `on-screen` is a `pure function` inside a
    /// component and nothing in the public API can call one.
    #[test]
    fn the_closed_drawer_gate_is_a_boolean_and_not_only_geometry() {
        let drawer = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/ui/drawer.slint"
        ))
        .expect("ui/drawer.slint");
        let body = drawer
            .split_once("pure function on-screen(n: int) -> bool {")
            .expect("ui/drawer.slint declares the on-screen gate")
            .1
            .split_once('}')
            .expect("its closing brace")
            .0;
        assert!(
            body.contains("root.open"),
            "`Drawer::on-screen` no longer consults `root.open`, so a closed drawer is out of the \
             accessible tree only for as long as it happens to park exactly on the client's edge:\n\
             {body}"
        );
        assert!(
            body.contains("&&"),
            "`Drawer::on-screen` no longer ANDs the gate with the strip position, so it is not a \
             gate:\n{body}"
        );

        let window =
            std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/ui/window.slint"))
                .expect("ui/window.slint");
        assert!(
            window.contains("open: root.drawer-on-stage;"),
            "the window supplies `Drawer::open` from something other than the on-stage boolean; \
             `drawer-open` alone would blank the pages the instant Esc was pressed and the drawer \
             would slide out empty"
        );
    }

    /// The four phases, and this build is in exactly one of them.
    #[test]
    fn the_window_is_off_until_something_starts_a_machine() {
        let p = phase();
        assert_eq!(p, emu::Phase::Off, "the window claims a machine it does not have");
        assert!(!is_booting(&p) && !is_running(&p));
        // And `Esc` from `Off` with nothing open reaches the bench rather than parking a machine
        // that does not exist.
        let mut s = nav::Stack::new();
        assert_eq!(s.escape(is_booting(&p), is_running(&p)), nav::Escape::Nothing);
    }

    /// §9.1's empty state is one sentence, and **neither half of it is empty**.
    ///
    /// The Work page draws its heading unconditionally and `visible: false` still reserves the slot
    /// in a Slint layout, so an empty heading is a `title`-height hole with its spacing under it —
    /// which is the same defect that once left a 116 px gap and pushed a primary action off its own
    /// centre line.
    #[test]
    fn the_empty_work_page_says_what_the_surface_is_for() {
        let (heading, empty) = work_page_text(&rail::Rail::new(), work::Shape::default());
        assert!(!heading.trim().is_empty(), "the heading slot is drawn whether or not it is filled");
        assert!(!empty.trim().is_empty());
        assert_eq!(
            format!("{heading} {empty}"),
            "Nothing is happening. Fetches, builds and installs report here.",
            "the two slots no longer read as §9.1's sentence"
        );
        assert!(
            !heading.to_lowercase().contains("nothing here"),
            "§9.1: never a bare 'nothing here'"
        );
    }

    /// **The heading is derived from the Rail, so it cannot go on saying nothing is happening.**
    ///
    /// It was pushed once, at startup, by `push_ledger` and never again — so the page the drawer
    /// auto-opens onto when a press is refused read *"Nothing is happening."* above a warning icon
    /// and a paragraph naming the missing file. That is the first thing anybody sees this program
    /// do wrong.
    #[test]
    fn the_work_heading_follows_what_the_rail_is_actually_holding() {
        let mut r = rail::Rail::new();
        assert_eq!(work_page_text(&r, work::Shape::default()).0, "Nothing is happening.");

        r.failed(
            "start",
            "iPod 1",
            rail::Failure::saying(rail::Class::Missing, "starting iPod 1", "the drive is gone."),
        );
        let heading = work_page_text(&r, work::Shape::default()).0;
        assert!(
            !heading.to_lowercase().contains("nothing is happening"),
            "the heading still says nothing is happening above a failure it is drawn on top of: \
             {heading:?}"
        );
        assert_eq!(heading, "One thing failed.");

        r.failed(
            "start",
            "iPod 2",
            rail::Failure::saying(rail::Class::Missing, "starting iPod 2", "the ROM is gone."),
        );
        assert_eq!(work_page_text(&r, work::Shape::default()).0, "2 things failed.");

        // A note is a thing that happened, and the heading says so rather than counting to zero.
        let mut n = rail::Rail::new();
        n.note("iPod 1 resolves and would start here.");
        assert_eq!(work_page_text(&n, work::Shape::default()).0, "This is what happened.");
    }

    /// A library nobody has ever used: no devices, no flag, nothing on disk.
    ///
    /// **Hands the guard back with it.** It used to drop it on the way out, which meant every test
    /// built on this helper ran with the data directory unclaimed — the redirect held, because it is
    /// process-wide and permanent, but nothing stopped `work.rs` pointing the variable at one of its
    /// own scratch directories in the middle of the test. That is how a ledger assertion came to
    /// report a firmware bundle nobody had downloaded.
    #[must_use = "dropping the guard releases the data directory to another test mid-run"]
    fn a_fresh_installation() -> (Rc<RefCell<Settings>>, DataDirGuard) {
        let held = use_a_scratch_data_dir();
        (Rc::new(RefCell::new(Settings::default())), held)
    }

    /// A library nobody has ever used **on a disk nobody else is using either**.
    ///
    /// [`a_fresh_installation`] gives an empty `Settings` but shares the binary's one data
    /// directory, which is right for a test that only reads it and wrong for one that fills it: the
    /// three ignored end-to-end tests each download a bundle and build a drive, and sharing a
    /// directory makes the second one find the first one's cache. That is not a failure, it is
    /// worse — it is a pass that proves less than it says, and only in some orders.
    #[must_use = "dropping the guard releases the data directory to another test mid-run"]
    fn a_fresh_installation_in(name: &str) -> (Rc<RefCell<Settings>>, DataDirGuard) {
        let guard = crate::data_dir_lock();
        let at = crate::scratch_data_dir().join(name);
        let _ = std::fs::remove_dir_all(&at);
        std::fs::create_dir_all(&at).expect("a data directory of this test's own");
        // SAFETY: under the shared lock, which is what serialises every test in this binary that
        // touches this variable. `DataDirLock`'s own `Drop` puts it back.
        unsafe { std::env::set_var("IPOD_EMULATOR_DATA", &at) };
        (
            Rc::new(RefCell::new(Settings::default())),
            DataDirGuard {
                _guard: guard,
                at: Box::leak(at.into_boxed_path()),
            },
        )
    }

    /// **§10.1: the plan is on screen BEFORE anything is downloaded.**
    ///
    /// Five steps, each with its own sub-line, in an already-open drawer, with the ledger under
    /// them — and not one byte fetched to put them there. *Nobody has ever been given that list
    /// before agreeing to a download.*
    #[test]
    fn the_plan_is_on_screen_before_anything_is_downloaded() {
        let (settings, _held) = a_fresh_installation();
        let w = a_window();
        let _wiring = wire(&w, settings.clone());

        let rail = w.get_rail();
        let planned: Vec<RailRow> = (0..rail.row_count())
            .filter_map(|i| rail.row_data(i))
            .filter(|r| r.kind == RailKind::Planned)
            .collect();
        assert_eq!(
            planned.len(),
            5,
            "the first run's plan is five steps and {} reached the drawer",
            planned.len()
        );
        // §10.1's own drawing: a verb, a subject and a sub-line on every row. `RailRow.sub` has
        // been drawn with nothing behind it since the drawer landed.
        for (i, r) in planned.iter().enumerate() {
            assert!(!r.verb.is_empty(), "step {i} has no verb");
            assert!(!r.what.is_empty(), "step {i} has no subject");
            assert!(!r.sub.is_empty(), "step {i} has no sub-line: {:?} {:?}", r.verb, r.what);
            // §9.2: a plan nobody has agreed to shows no progress at all — never a spinner.
            assert!(r.fraction < 0.0, "step {i} draws a bar before anything has started");
            assert!(r.measure.is_empty(), "step {i} counts bytes nobody has fetched");
        }
        // The first row is the one that costs nothing and downloads nothing, and it says so.
        assert_eq!(planned[0].verb, "synthesise");
        assert!(
            planned[0].sub.contains("nothing downloaded"),
            "the ROM step does not say it costs nothing: {:?}",
            planned[0].sub
        );

        // The drawer is already open on the page holding it.
        assert!(w.get_drawer_open(), "§10.1 opens the drawer; it is shut");
        assert_eq!(w.get_drawer_page(), DrawerPage::Work);
        assert_eq!(w.get_drawer_depth(), 1);

        // …and nothing was written to get here.
        assert!(
            w.get_progress() < 0.0,
            "something claims progress before the button has been pressed"
        );
        // **Nothing was minted and nothing was filed**, which is what *before anything is
        // downloaded* means on this side of the boundary. The fetch itself is asserted through the
        // Rail rather than through the firmware cache: `firmware::cache_dir` resolves through a
        // process-wide environment variable that other tests in this binary set for themselves, so
        // reading that directory here would be reading somebody else's.
        assert!(
            settings.borrow().devices.is_empty(),
            "drawing the plan minted an identity, which is the one irreversible thing in this \
             program"
        );
        assert!(
            settings.borrow().resources.is_empty(),
            "drawing the plan filed something away"
        );
        for r in &planned {
            assert!(
                !r.cancellable && r.cancel_cost.is_empty(),
                "step {:?} names a file it is writing, and nothing has been pressed",
                r.verb
            );
        }
    }

    /// **§10.3: the wizard does not come back.**
    ///
    /// The failure this designs out is the one that shipped: a window that re-opened its wizard
    /// whenever the device list was empty — and a cancelled or failed build is exactly what empties
    /// it — so a person was returned to step one with no error shown and no way past.
    ///
    /// Both halves are checked, because they fail differently: the **flag** survives the file, and
    /// the **latch** holds within one session.
    #[test]
    fn the_wizard_does_not_come_back() {
        let (settings, _held) = a_fresh_installation();
        let first = a_window();
        let _w1 = wire(&first, settings.clone());
        assert!(first.get_drawer_open(), "the first launch did not show the welcome");
        assert!(settings.borrow().welcomed, "the welcome was shown and the flag was not set");

        // **The library is emptied**, which is what a cancelled or failed build leaves behind.
        settings.borrow_mut().devices.clear();
        assert!(
            settings.borrow().devices.is_empty(),
            "the fixture is not in the state that used to reopen the wizard"
        );

        // A second launch, on that same emptied library.
        let second = a_window();
        let _w2 = wire(&second, settings.clone());
        assert!(
            !second.get_drawer_open(),
            "the drawer opened again on an empty library, which is the wizard coming back"
        );
        assert!(settings.borrow().welcomed, "something cleared the flag");
        // §9.1's later-empty bench, which is the state it should be in: the ghost is back, and the
        // words are the ones that do not assume this is anybody's first minute.
        assert!(second.get_ghost(), "an empty bench is not drawn as a ghost");
        assert_eq!(second.get_empty_device().name, "No devices yet");
        assert!(
            !second.get_empty_device().summary.starts_with("You do not need an iPod"),
            "the welcome copy came back: {:?}",
            second.get_empty_device().summary
        );

        // **And the route did not go with it.** §9.1 gives the later-empty bench the cradle label
        // `press ● to make an iPod`; §10.3 says *both routes offered equally*. What the flag stops
        // is the welcome **copy** — the press, the plan and the ghost are all still there. This
        // half is the fatal one: `welcomed` is written when the bench is wired, so opening the
        // program, looking at it and closing it was enough to reach this state, and the state had
        // no route to an iPod at all.
        assert_eq!(
            second.get_rail().row_count(),
            first.get_rail().row_count(),
            "the later-empty bench is not offered the plan the welcome one was"
        );
        assert_eq!(
            second.get_empty_device().startable,
            caps().download,
            "the later-empty bench is drawn unpressable while the press would have worked"
        );
        assert!(
            second.get_empty_device().cradle_label.contains("centre button")
                || !caps().download,
            "the cradle promises nothing on a bench that can be pressed: {:?}",
            second.get_empty_device().cradle_label
        );
        assert!(
            second.get_ledger_download().contains("to download"),
            "the shelf quotes a bill while the ledger says there is nothing to make: {:?}",
            second.get_ledger_download()
        );
    }

    /// **The later-empty bench really does make an iPod when it is pressed.**
    ///
    /// The sibling of `the_wizard_does_not_come_back`, and it drives the registered handler rather
    /// than looking at properties. The gap it closes was reachable in the commonest possible way —
    /// open the program, close it, open it again — and left the promise the README is built on with
    /// no route behind it: the cradle drawn `fg-dim`, the drawer shut, and the press answering
    /// *there are no devices in the library yet, so there is nothing to start*.
    #[test]
    fn a_bench_that_is_empty_a_second_time_still_makes_an_ipod() {
        let (settings, _held) = a_fresh_installation();
        settings.borrow_mut().welcomed = true;
        // Nothing is written here: the press below is refused before the worker, because `drives`
        // is a file where the directory has to be. What is being checked is the ROUTE.
        let drives = eapp_loader::settings::drives_dir();
        let _ = std::fs::remove_dir_all(&drives);
        if let Some(parent) = drives.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::write(&drives, b"not a directory").expect("the blocking file");

        let w = a_window();
        let _wiring = wire(&w, settings.clone());
        assert_eq!(first_run_offer(&settings.borrow()), Offer::Again);
        assert!(
            press_is_first_run(&settings.borrow(), 0),
            "the centre button on an empty bench does not route to the first run"
        );

        w.invoke_start_device(0);
        assert_eq!(
            settings.borrow().devices.len(),
            1,
            "a press on the later-empty bench made no iPod"
        );
        let _ = std::fs::remove_file(&drives);
    }

    /// **First run is decided by the flag, and an empty library never turns it back on.**
    ///
    /// The 2 × 3 sweep. Emptiness may **suppress** the welcome — a library with devices in it is
    /// plainly not somebody's first minute — but it may never **offer** it, because a build that is
    /// cancelled or fails is what empties the list.
    #[test]
    fn first_run_is_decided_by_the_flag_and_never_by_an_empty_library() {
        let _held = use_a_scratch_data_dir();
        let library = |what: &str| -> Settings {
            let mut s = Settings::default();
            match what {
                "empty" => {}
                "one device" => s = a_library_of_one(),
                "a half-made one" => {
                    let rom = s.file_away(
                        eapp_loader::settings::Resource::Firmware(
                            eapp_loader::nor::Source::Synthetic {
                                model: compose::FIRST_RUN_MODEL.into(),
                                seed: 424_242,
                                serial: None,
                                guid: None,
                                splash: None,
                            },
                        ),
                        "Black 5.5G",
                        None,
                    );
                    s.devices.push(Device {
                        name: "My 5.5G".into(),
                        firmware: rom,
                        ..Device::default()
                    });
                }
                other => unreachable!("{other}"),
            }
            s
        };

        for shape in ["empty", "one device", "a half-made one"] {
            for welcomed in [false, true] {
                let mut s = library(shape);
                s.welcomed = welcomed;
                let got = first_run_offer(&s);
                let want = match (shape, welcomed) {
                    // The identity is already minted and the drive is not made. Pressing carries
                    // it on — and must NOT start over, because starting over mints a second iPod.
                    ("a half-made one", _) => Offer::Finish { device: "My 5.5G".into() },
                    ("empty", false) => Offer::Welcome,
                    // §9.1's later-empty bench. **Not `Quiet`** — the flag stops the welcome copy
                    // returning and takes nothing else with it. `Quiet` here was the fatal bug:
                    // an empty library with no route to an iPod, reached by opening the program
                    // and closing it.
                    ("empty", true) => Offer::Again,
                    _ => Offer::Quiet,
                };
                assert_eq!(got, want, "{shape}, welcomed={welcomed}");
                // **An empty library always has a plan**, whichever bench it is. That is the whole
                // of the fix: `welcomed` chooses the copy and never the route.
                assert!(
                    got.has_plan() || !s.devices.is_empty(),
                    "{shape}, welcomed={welcomed}: an empty library with nothing to make"
                );
            }
        }
    }

    /// **One producer, one consumer.** `welcomed` is read in exactly one function in this crate.
    ///
    /// The flag is the whole of §10.3, and a second reader is a second rule about when the welcome
    /// appears. `first_run_offer` is where the decision lives; anything else consulting it is how
    /// the two come to disagree.
    #[test]
    fn only_the_offer_decision_reads_the_welcome_flag() {
        let mut sites: Vec<String> = Vec::new();
        for (name, text) in rust_sources() {
            for (n, line) in text.lines().enumerate() {
                let t = line.trim_start();
                if t.starts_with("//") || !line.contains("welcomed") {
                    continue;
                }
                sites.push(format!("{name}:{}", n + 1));
            }
        }
        assert!(
            !sites.is_empty(),
            "no `welcomed` was found in this crate at all, so the sweep is reading nothing"
        );
        assert!(
            sites.iter().all(|s| s.starts_with("main.rs")),
            "the welcome flag is read outside main.rs at {sites:?}; §10.3 has one decision \
             function and `first_run_offer` is it"
        );
        assert!(
            sites.len() <= 3,
            "`welcomed` is touched at {sites:?}; that is more places than `first_run_offer` and \
             `welcome` between them"
        );
    }

    /// **§10.1's heading, and a failure outranks it.**
    #[test]
    fn the_work_page_says_what_pressing_does_and_a_failure_outranks_it() {
        let mut r = rail::Rail::new();
        r.plan(&work::plan(compose::Holes::Sparse));
        assert_eq!(
            work_page_text(&r, work::Shape { has_plan: true, running: false }).0,
            "This is what pressing the centre button does",
            "the plan does not introduce itself"
        );
        // Not a first run: the same Rail, and the heading is the ordinary one.
        assert_eq!(work_page_text(&r, work::Shape::default()).0, "This is what happened.");

        // A first run that failed reads as a failure, not as an invitation to press again.
        let id = r.entries()[1].id;
        r.fail(id, rail::Failure::new(rail::Class::Network, "the download"));
        assert_eq!(
            work_page_text(&r, work::Shape { has_plan: true, running: false }).0,
            "One thing failed.",
            "the page offered the plan again above the failure it just produced"
        );
    }

    /// **The whole first-run screen carries one bill, and one step's own cost.**
    ///
    /// §10.1's rule is `6.5 MB to download · about 28 MB on disk` **everywhere**, written against a
    /// revision that had put three different sizes for one operation on the one screen principle 7
    /// exists for. Taken to the letter it would put the whole run's 28 MB inside the build step's
    /// own sub-line, which attributes the bundle's 6.5 MB to the build — so the rule is narrowed
    /// here, in one place, and this is what holds it: **the bill is one number wherever the bill
    /// appears, and the build's sub-line states the drive's own cost, once.**
    ///
    /// The apparent 8 GiB appears exactly once, on the same line, as a fact about the drive rather
    /// than a bill.
    #[test]
    fn the_first_run_screen_carries_one_bill_and_one_step_cost() {
        let (settings, _held) = a_fresh_installation();
        let w = a_window();
        let _wiring = wire(&w, settings.clone());

        let mut drawn: Vec<String> = vec![
            w.get_ledger_download().to_string(),
            w.get_ledger_disk().to_string(),
            w.get_ledger_note().to_string(),
            w.get_empty_device().summary.to_string(),
            w.get_empty_device().write_target.to_string(),
            w.get_empty_device().cradle_label.to_string(),
            w.get_work_heading().to_string(),
        ];
        for i in 0..w.get_rail().row_count() {
            let r = w.get_rail().row_data(i).expect("a row");
            drawn.push(r.what.to_string());
            drawn.push(r.sub.to_string());
            drawn.push(r.measure.to_string());
        }

        let bill = eapp_loader::si(work::cost(compose::Holes::Sparse).disk);
        let drive = eapp_loader::si(compose::DRIVE_ON_DISK);
        assert_ne!(bill, drive, "the fixture cannot tell the two figures apart");

        // Every `… on disk` figure on the screen, in the order it is drawn.
        let mut figures: Vec<String> = Vec::new();
        for line in &drawn {
            for (n, _) in line.match_indices(" on disk") {
                let before = &line[..n];
                let word: String = before
                    .rsplit(' ')
                    .take(2)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect::<Vec<_>>()
                    .join(" ");
                figures.push(word);
            }
        }
        assert!(!figures.is_empty(), "no disk figure is drawn at all, so this reads nothing");
        let distinct: std::collections::BTreeSet<&String> = figures.iter().collect();
        assert!(
            distinct.len() <= 2,
            "{} different disk sizes on one screen: {:?}",
            distinct.len(),
            distinct
        );
        assert_eq!(
            figures.iter().filter(|f| **f == drive).count(),
            1,
            "the drive's own cost ({drive}) is drawn {} times; it belongs on the build step's \
             sub-line and nowhere else: {figures:?}",
            figures.iter().filter(|f| **f == drive).count()
        );
        assert!(
            figures.contains(&bill),
            "the bill ({bill}) is not on the screen at all: {figures:?}"
        );
        assert!(
            w.get_ledger_disk().contains(&bill),
            "the ledger does not quote the bill: {:?}",
            w.get_ledger_disk()
        );
        assert!(
            w.get_empty_device().write_target.contains(&bill),
            "the shelf does not quote the bill: {:?}",
            w.get_empty_device().write_target
        );

        // **8 GiB, exactly once**, and it is the volume's apparent size rather than a cost.
        let apparent = drawn.iter().filter(|l| l.contains("8 GiB")).count();
        assert_eq!(apparent, 1, "8 GiB is drawn {apparent} times: {drawn:?}");
    }

    /// **One missing tool is one failure on the Rail**, however many times it is pressed.
    ///
    /// `wire` files `ToolMissing` on the plan and `Queue::press` refuses with the same class and
    /// the same sentence under the verb `make`, so a person with no `curl` opened the window on
    /// *One thing failed.* and reached *2 things failed.* with one press — two identical
    /// paragraphs and two copies of the same command, for one absent tool.
    #[test]
    fn one_absent_tool_is_counted_once_however_often_it_is_pressed() {
        let _held = use_a_scratch_data_dir();
        let mut rail = rail::Rail::new();
        // What `wire` files.
        rail.failed(
            "fetch",
            "Apple's firmware",
            rail::Failure::new(rail::Class::ToolMissing(rail::Tool::Curl), "the download"),
        );
        assert_eq!(rail.failures(), 1);

        // What a press adds — and the de-duplication is the caller's, because two *different*
        // failures are deliberately two entries and `Rail::failed` only folds an identical repeat.
        let refusal =
            rail::Failure::new(rail::Class::ToolMissing(rail::Tool::Curl), "making an iPod");
        for _ in 0..3 {
            let already = rail.entries().iter().any(|e| {
                e.kind == rail::Kind::Failed
                    && e.failure.as_ref().is_some_and(|g| g.class == refusal.class)
            });
            if !already {
                rail.failed("make", "an iPod", refusal.clone());
            }
        }
        assert_eq!(
            rail.failures(),
            1,
            "one absent tool reads as {} failures: {}",
            rail.failures(),
            rail.announce()
        );
        assert_eq!(work_page_text(&rail, work::Shape { has_plan: true, running: false }).0, "One thing failed.");
    }

    /// **A run that has started reads as work, not as history.**
    ///
    /// Between the press and the worker's first `Started` there is no `Working` entry — the window
    /// has ticked the synthesise step and nothing else — so the heading read *This is what
    /// happened.* over a run that had just begun, with the Rail beside it announcing `1 of 5 done.`
    #[test]
    fn a_run_that_has_just_started_does_not_read_as_finished() {
        let mut rail = rail::Rail::new();
        let steps = work::plan(compose::Holes::Sparse);
        let ids = rail.plan(&steps);

        let waiting = work::Shape { has_plan: true, running: false };
        let running = work::Shape { has_plan: true, running: true };
        assert_eq!(
            work_page_text(&rail, waiting).0,
            "This is what pressing the centre button does",
            "the plan does not introduce itself"
        );
        // The press ticks step 0 and spawns; nothing has reported yet, and there is no `Working`
        // entry on the Rail for the heading to read.
        rail.done(ids[0]);
        assert_eq!(
            work_page_text(&rail, running).0,
            "Working.",
            "a run that has just begun reads as one that is over"
        );
        // …and the ordinary working state is unchanged.
        rail.progress(ids[1], rail::Progress::Bytes { done: 1, total: 2 });
        assert_eq!(work_page_text(&rail, running).0, "Working.");

        // **The mirror of it.** At the end there is one `Planned` step left — the boot — and the
        // worker has gone. Reading that as work in progress claimed a run was under way after it
        // had finished, which is what the whole first run reported on its last frame.
        for id in &ids[1..4] {
            rail.done(*id);
        }
        assert_eq!(
            work_page_text(&rail, waiting).0,
            "This is what happened.",
            "a run that is over reads as one still going"
        );
    }

    /// **The ledger is re-checked as the run goes, not asserted once at startup.**
    ///
    /// §10.1 calls the third line the one that is *checked rather than asserted* — and it was
    /// checked once, in `wire`, and then left on screen: *Nothing has been downloaded yet.* stayed
    /// there after the bundle had arrived and been SHA-256 checked. The free-space clause aged the
    /// same way. `push_ledger` had exactly one non-test caller.
    #[test]
    fn the_ledger_is_re_checked_when_a_step_completes() {
        let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/main.rs"))
            .expect("this file");
        let (code, _) = text.split_once("pub(crate) mod tests {").expect("the test module");
        let calls = code.matches("push_ledger(").count();
        // The declaration, plus every place that pushes it.
        assert!(
            calls >= 4,
            "`push_ledger` is called from {} place(s) in the code — it was one, in `wire`, which \
             made the line that has to be true a startup snapshot",
            calls - 1
        );
        assert!(
            code.split_once("fn pump_once(")
                .is_some_and(|(_, after)| after
                    .split("\n}\n")
                    .next()
                    .is_some_and(|body| body.contains("push_ledger("))),
            "a completed step does not re-check the ledger, so `Nothing has been downloaded yet.` \
             stays on screen after the bundle has arrived"
        );

        // …and the behaviour of the line itself, which is what the sweep is about.
        let (settings, _held) = a_fresh_installation();
        let w = a_window();
        let _wiring = wire(&w, settings.clone());
        let cache = eapp_loader::firmware::cache_dir();
        assert_eq!(w.get_ledger_note(), "Nothing has been downloaded yet.");
        std::fs::create_dir_all(&cache).expect("a cache");
        std::fs::write(cache.join("arrived.ipsw"), b"as if it had been fetched").expect("a bundle");
        push_ledger(
            &w,
            Some(work::cost(compose::Holes::Sparse)),
            &cache,
            volume::space(&cache).as_ref(),
        );
        assert_eq!(
            w.get_ledger_note(),
            "One bundle is already downloaded.",
            "the ledger goes on asserting an absence the program has disproved"
        );
        let _ = std::fs::remove_dir_all(&cache);
    }

    /// **The centre button starts the device that was pressed, not the one a session-wide flag
    /// remembers.**
    ///
    /// Reachable in one move: a first run fails at the fetch, leaving `My 5.5G` with no drive; the
    /// operator composes a second device with `ipod-boot setup`; on the next launch `Offer::Finish`
    /// made `has_plan` true for the whole session, so pressing the cradle on the **composed**
    /// device resumed the first run instead of starting it.
    #[test]
    fn the_press_routes_by_the_row_it_was_given() {
        let _held = use_a_scratch_data_dir();
        let mut s = Settings::default();
        let rom = s.file_away(
            eapp_loader::settings::Resource::Firmware(eapp_loader::nor::Source::Synthetic {
                model: compose::FIRST_RUN_MODEL.into(),
                seed: 909_090,
                serial: None,
                guid: None,
                splash: None,
            }),
            "Black 5.5G",
            None,
        );
        // The half-made first-run device: minted, and no drive.
        s.devices.push(Device { name: "My 5.5G".into(), firmware: rom, ..Device::default() });
        // …and one somebody composed by hand, which happens to sort after it.
        let mut composed = a_library_of_one();
        composed.devices[0].name = "Their iPod".into();
        s.resources.extend(composed.resources);
        s.disks.extend(composed.disks);
        s.devices.push(composed.devices[0].clone());

        assert_eq!(
            first_run_offer(&s),
            Offer::Finish { device: "My 5.5G".into() },
            "the fixture is not the state this is about"
        );
        assert!(press_is_first_run(&s, 0), "the half-made device does not resume");
        assert!(
            !press_is_first_run(&s, 1),
            "pressing a device somebody composed resumed the first run instead of starting it"
        );
        // An index past the end is nobody's device, so it is not the first run's either.
        assert!(!press_is_first_run(&s, 9), "an index past the end routed to the first run");

        // And with nothing in the library at all, every press is the first run's — there is no
        // device to start, and §9.1 gives that bench one press.
        assert!(press_is_first_run(&Settings::default(), 0));
    }

    /// **A device the Composer filed is not the first run's device, and the centre button must not
    /// treat it as one.**
    ///
    /// `work::minted` answers *the first-run device* by asking whether the boot ROM is a synthesised
    /// one with a seed somebody's press produced — and `Composer::make_one` mints exactly that
    /// shape. So a device composed here was indistinguishable from a half-made first run, and both
    /// routes that consult `minted` believed it: `press_is_first_run` sent it to
    /// `work::Queue::press`, which runs the **fixed** first-run plan — Apple's firmware, an 8 GiB
    /// drive, Apple's software — and reads no `Recipe` at all. A device composed as Rockbox-only was
    /// told to press a button that builds an Apple drive.
    #[test]
    fn a_composed_device_is_not_routed_to_the_first_run() {
        let _held = use_a_scratch_data_dir();
        let mut s = Settings::default();
        let mut c = composer::Composer::new();
        c.make_one();
        c.set_start(compose::Start::FromIpsw("iPod_25.1.3.ipsw".into()));
        c.set_name("Rockbox only");
        let done = c.commit(&mut s).expect("an empty library takes the name");

        let i = s
            .devices
            .iter()
            .position(|d| d.name == done.device)
            .expect("the Composer filed no device");
        assert!(
            !s.devices[i].names_a_disk(),
            "the fixture is not the state this is about: the drive has still to be built"
        );
        assert!(
            !press_is_first_run(&s, i),
            "the centre button on a composed device runs the fixed first-run plan — Apple's \
             firmware, an 8 GiB drive — and never the recipe it was composed from"
        );
        // The same mistake one surface up: `Offer::Finish` is the cradle promising to *finish
        // making* it, and finishing it means the same fixed plan.
        assert_ne!(
            first_run_offer(&s),
            Offer::Finish { device: done.device.clone() },
            "the bench offers to finish a composed device by running the first-run plan"
        );

        // **And §7.3's own line, because a press that refuses under a label promising to finish is
        // the same lie moved one control along.** §14.1: drawn, refused, and saying why.
        let row = &device_rows(&s)[i];
        assert!(
            !row.startable,
            "the cradle is drawn live over a device this build cannot make a drive for"
        );
        assert!(
            !row.cradle_label.contains("finish making"),
            "the cradle promises to finish making a composed device: {:?}",
            row.cradle_label
        );
        assert!(
            row.cradle_label.contains("not wired"),
            "the cradle refuses without saying why: {:?}",
            row.cradle_label
        );
    }

    /// **The sentence `Route::Unwired` files, word for word, because the sentence IS the press.**
    ///
    /// `a_composed_device_is_not_routed_to_the_first_run` above pins the routing — that this press
    /// does not reach `work::Queue::press` — and `cradle_label`'s arm pins the caption before it.
    /// Nothing pinned what a person actually reads *after* pressing, and on this route that is the
    /// whole of the behaviour: `mutated` is `false` for `Route::Unwired`, so nothing is started,
    /// nothing is minted and nothing is saved. One note is the entire observable effect.
    ///
    /// **It carried a remedy once and the remedy built something else.** The sentence this replaced
    /// on the Composer's own save said *press the centre button on it to finish making it*, and
    /// that button ran the fixed first-run plan — Apple's firmware onto an 8 GiB drive — for a
    /// device that may have been composed as Rockbox-only. §14.1: say what cannot be done and why,
    /// and stop there. Unasserted, the *and stop there* half can be rewritten back into a promise
    /// without one test moving, which is how it got there the first time.
    ///
    /// So this is an equality on the whole string rather than a `contains`: a `contains` on *not
    /// wired* would stay green through a sentence that then went on to name a button.
    #[test]
    fn the_press_on_a_composed_device_files_the_refusal_verbatim_and_offers_no_remedy() {
        let _held = use_a_scratch_data_dir();
        let mut s = Settings::default();
        let mut c = composer::Composer::new();
        c.make_one();
        c.set_start(compose::Start::FromIpsw("iPod_25.1.3.ipsw".into()));
        c.set_name("Rockbox only");
        let done = c.commit(&mut s).expect("an empty library takes the name");
        let i = s
            .devices
            .iter()
            .position(|d| d.name == done.device)
            .expect("the Composer filed no device");
        // The fixture is the state the route is about, not merely a device that happens to be
        // there: `composed_and_unbuilt` is the one boolean the press, the ring and the label share.
        assert!(
            composed_and_unbuilt(&s.devices[i]),
            "the fixture does not take the `Unwired` route, so what it asserts below is not it"
        );

        let settings = Rc::new(RefCell::new(s));
        let w = a_window();
        let _wiring = wire(&w, settings.clone());
        // Counted rather than assumed to be zero: `wire` files §10.1's plan, and one more entry on
        // top of it on a machine with no `curl`. What this test is about is the row the press adds.
        let before = w.get_rail().row_count();
        // The whole library as it would be written to disk. `Route::Unwired` is excluded from
        // `mutated`, so `save` is not called — and the model it would have rendered must be the one
        // it was. Taken *after* `wire`, which mints and writes §10.3's welcome flag, and compared
        // against itself: `current` is already `Some` here because `Composer::commit` made the
        // device live, so asserting `None` would be asserting something the press never did.
        let library_before = settings.borrow().render();

        w.invoke_start_device(i as i32);

        assert_eq!(
            w.get_rail().row_count(),
            before + 1,
            "the press filed something other than exactly one note"
        );
        let row = w.get_rail().row_data(before).expect("the note the press filed");
        assert_eq!(row.kind, RailKind::Note, "a refusal that mutates nothing is filed as a failure");
        // `Rail::note` puts its text in `what`, and `ui/rail.slint:173` draws `e.what` — so this is
        // the string that reaches a pixel, not a field beside it.
        assert_eq!(
            row.what.as_str(),
            format!(
                "{} was composed here, and building a composed device is not wired yet. Its \
                 drive has not been made, and this button cannot make one.",
                done.device
            ),
            "the sentence the refusal draws is not the one this route was written to say"
        );
        // §14.1's second half, said as its own assertion so a reworded sentence that puts a remedy
        // back fails on the reason rather than on the diff.
        assert!(
            !row.what.contains("centre button"),
            "the refusal points at a button again: {:?}",
            row.what
        );
        // And it mutated nothing, which is the other half of *say what cannot be done and stop*:
        // a refusal that quietly starts a build, mints an identity or renames something is the
        // same defect as one that promises a remedy, one layer down where nobody reads it.
        assert_eq!(
            settings.borrow().render(),
            library_before,
            "the refusing press changed the library"
        );
    }

    /// **The cradle carries the work while there is work**, which is §9.2's bench mirror.
    ///
    /// The shelf's bar and its rail line moved during a build and the cradle did not, so the one
    /// line the whole bench is built around read *Press the centre button to finish making My
    /// 5.5G* for the entire download — a promise to press, on a machine already busy doing it.
    #[test]
    fn the_cradle_carries_the_work_and_gives_the_row_back_afterwards() {
        let _held = use_a_scratch_data_dir();
        let w = a_window();
        let rows: Rc<VecModel<RailRow>> = Rc::new(VecModel::default());
        w.set_rail(ModelRc::from(rows.clone()));
        let mut rail = rail::Rail::new();

        sync_rail(&w, &rows, &rail, caps(), work::Shape { has_plan: true, running: false });
        assert_eq!(w.get_working_label(), "", "an idle bench claims to be working");

        let id = rail.note("fetch");
        rail.writing(id, std::path::PathBuf::from("/tmp/x.part"));
        rail.progress(id, rail::Progress::Bytes { done: 2_600_000, total: 6_500_000 });
        sync_rail(&w, &rows, &rail, caps(), work::Shape { has_plan: true, running: false });
        let said = w.get_working_label().to_string();
        assert!(said.contains("40 %"), "the cradle does not say how far: {said:?}");
        assert!(said.contains("fetch"), "the cradle does not say what: {said:?}");
        assert!(
            said.chars().count() <= geometry::CRADLE_LABEL_MAX_CHARS,
            "{said:?} is {} characters against a {}-character row",
            said.chars().count(),
            geometry::CRADLE_LABEL_MAX_CHARS
        );

        // No denominator is *not measured*, never nothing done: the sentence stays and the
        // percentage goes (§12.3).
        rail.progress(id, rail::Progress::Bytes { done: 2_600_000, total: 0 });
        sync_rail(&w, &rows, &rail, caps(), work::Shape { has_plan: true, running: false });
        assert!(!w.get_working_label().contains('%'), "{:?}", w.get_working_label());
        assert!(w.get_working_label().contains("fetch"));

        // …and when it is over the row's own label comes back.
        rail.done(id);
        sync_rail(&w, &rows, &rail, caps(), work::Shape { has_plan: true, running: false });
        assert_eq!(
            w.get_working_label(),
            "",
            "the cradle is still narrating a step that finished"
        );
    }

    /// **One missing tool is one failure**, and the cradle says which state it is in.
    ///
    /// With no `curl`, `wire` files `ToolMissing` on the plan and `Queue::press` refuses with the
    /// same class and the same sentence under a different verb — so the window opened on *One
    /// thing failed.* and one press made it *2 things failed.*, two identical paragraphs and two
    /// copies of the same command. Meanwhile the cradle was drawn unpressable under a label
    /// promising a press, and its `accessible-description` is that label.
    #[test]
    fn a_missing_downloader_is_one_failure_and_the_cradle_says_so() {
        let no_curl = rail::Caps { download: false, ..caps() };
        let cost = work::cost(compose::Holes::Sparse);
        let row = empty_device(true, no_curl, cost);
        assert!(!row.startable, "the bench is pressable with nothing to download with");
        assert!(
            !row.cradle_label.contains("Press"),
            "the cradle promises a press it will refuse: {:?}",
            row.cradle_label
        );
        assert!(
            row.cradle_label.to_lowercase().contains("curl"),
            "the cradle does not say why it cannot be pressed: {:?}",
            row.cradle_label
        );
        // …and with a downloader it is the promise again, on both empty benches.
        for first in [true, false] {
            let row = empty_device(first, caps(), cost);
            assert_eq!(row.startable, caps().download);
            if caps().download {
                assert!(row.cradle_label.contains("centre button"), "{:?}", row.cradle_label);
            }
        }
    }

    /// **§10.1's ledger: one number per axis, and both come from the plan.**
    ///
    /// An earlier revision put three different sizes for one operation on the one screen principle 7
    /// was written for — `about 300 MB, and four minutes`, `8 GiB sparse` and `8.02 GB needed` —
    /// and the free-space gate was written against the apparent size of a sparse file, so somebody
    /// with 4.1 GB free was refused on a machine with sixteen times the room the build needs.
    #[test]
    fn the_ledger_carries_one_number_per_axis_and_both_come_from_the_plan() {
        let w = a_window();
        let cache = temp_dir("ledger-empty");
        let cost = a_cost();
        push_ledger(&w, Some(cost), &cache, None);

        let down = w.get_ledger_download().to_string();
        let disk = w.get_ledger_disk().to_string();
        assert_eq!(down, format!("{} to download", eapp_loader::si(cost.down)));
        assert_eq!(disk, format!("about {} on disk", eapp_loader::si(cost.disk)));
        // **The apparent 8 GiB is not on the ledger at all.** It is a fact about the drive, and it
        // belongs in the build step's own sub-line where it is not a bill.
        for line in [&down, &disk] {
            assert!(!line.contains("GiB"), "the ledger bills an apparent size: {line}");
            assert!(!line.contains("8.6 GB"), "the ledger bills the sparse length: {line}");
        }
        // The plan's own sub-line is where it does appear — exactly once.
        let plan = work::plan(compose::Holes::Sparse);
        let apparent: Vec<&str> = plan
            .iter()
            .map(|s| s.sub())
            .filter(|s| s.contains("8 GiB"))
            .collect();
        assert_eq!(apparent.len(), 1, "8 GiB appears {} times in the plan", apparent.len());

        // **`None` free space states nothing and warns about nothing.** An unmeasured volume is not
        // a full one.
        assert!(!disk.contains("free"), "a clause was invented for a volume nobody measured");
        assert!(!w.get_ledger_warn(), "the warn colour is on against a figure nobody measured");

        // Measured, and short: the clause appears and the warning with it.
        let tight = volume::Space { free: 1_000_000, mount: "/scratch".into() };
        push_ledger(&w, Some(cost), &cache, Some(&tight));
        assert!(w.get_ledger_disk().contains("1.0 MB free on /scratch"), "{}", w.get_ledger_disk());
        assert!(w.get_ledger_warn(), "1 MB free for a {} build did not warn", eapp_loader::si(cost.disk));

        // Measured, and roomy: the clause appears and the warning does not.
        let roomy = volume::Space { free: 900_000_000_000, mount: "/".into() };
        push_ledger(&w, Some(cost), &cache, Some(&roomy));
        assert!(w.get_ledger_disk().contains("free on /"), "{}", w.get_ledger_disk());
        assert!(!w.get_ledger_warn(), "900 GB free warned about a 28 MB build");

        // **The third line is checked, not asserted.** *Nothing has been downloaded yet* is false
        // the moment the bundle is in the cache, and it is the first sentence of this program a
        // person reads.
        assert_eq!(w.get_ledger_note(), "Nothing has been downloaded yet.");
        std::fs::write(cache.join("iPod_x.ipsw"), b"not really a bundle").unwrap();
        push_ledger(&w, Some(cost), &cache, None);
        assert_eq!(w.get_ledger_note(), "One bundle is already downloaded.");
        std::fs::remove_dir_all(&cache).ok();
    }

    /// **§11.2's root and §10.1's Work page print one bill**, because there is one function that
    /// makes it and two callers of it.
    ///
    /// A second copy of this arithmetic on the Composer would be two figures for one press, on two
    /// surfaces a person moves between while deciding whether to agree to a download — which is
    /// principle 7's own complaint about three sizes for one operation, one page along.
    ///
    /// It drives [`ledger_lines`] directly as well as through [`push_ledger`], and the two are
    /// asserted **equal** rather than each asserted separately: the point is not that both are
    /// right, it is that they cannot differ.
    #[test]
    fn the_composer_and_the_work_page_print_one_bill() {
        let w = a_window();
        let cache = temp_dir("one-bill");
        let cost = a_cost();
        for space in [
            None,
            Some(volume::Space { free: 1_000_000, mount: "/scratch".into() }),
            Some(volume::Space { free: 900_000_000_000, mount: "/".into() }),
        ] {
            for plan in [None, Some(cost)] {
                push_ledger(&w, plan, &cache, space.as_ref());
                let (download, disk, warn) = ledger_lines(plan, space.as_ref());
                assert_eq!(w.get_ledger_download().to_string(), download);
                assert_eq!(w.get_ledger_disk().to_string(), disk);
                assert_eq!(w.get_ledger_warn(), warn);
            }
        }
        // The control: the two cases this has to be able to tell apart are actually different, or
        // the loop above is comparing one string against itself.
        let (_, roomy, roomy_warn) =
            ledger_lines(Some(cost), Some(&volume::Space { free: 900_000_000_000, mount: "/".into() }));
        let (_, tight, tight_warn) =
            ledger_lines(Some(cost), Some(&volume::Space { free: 1_000_000, mount: "/scratch".into() }));
        assert_ne!(roomy, tight);
        assert!(tight_warn && !roomy_warn);
        std::fs::remove_dir_all(&cache).ok();
    }

    /// **Nothing carrying an identifier reaches the clipboard, at any masking state.**
    ///
    /// §11.2 masks a serial and a FireWire GUID because *a screenshot of this page must not carry
    /// somebody's identifiers* — and the critics found `Copy the command line` putting the very
    /// values that rule hides onto the pasteboard. A screenshot is one moment; a clipboard outlives
    /// the screen. So `Show` reveals and it does not unlock, and this is the gate that makes that
    /// true from the window's side rather than by everybody remembering.
    ///
    /// The values below are **generated**, not read out of `resources/`: `Identity::generate` is a
    /// pure function of a model and a seed, so the fixture is reproducible and belongs to nobody.
    #[test]
    fn nothing_outside_the_composer_can_reach_an_unmasked_identifier() {
        // A synthesised iPod, so no real person's identifiers are in this file.
        let (serial, guid) = a_generated_identity();
        assert_eq!(serial.len(), 11, "the fixture is not a serial: {serial:?}");

        // Bare, and inside a sentence, and inside a command line — every shape a copy control could
        // hand this.
        //
        // **The failure message names the case by POSITION and never quotes the value**, which is
        // `Refusal::masked`'s own rule applied one layer out: §11.2's other named defect was a
        // masked validation sentence quoting the offending character back, and a test that prints
        // an identifier into a CI log to complain that identifiers escape is the same shape.
        for (case, carried) in [
            ("a bare serial", serial.clone()),
            ("a bare GUID", guid.clone()),
            ("a serial inside a command line", format!("ipod-boot retail --nor-serial {serial}")),
            ("a GUID inside a sentence", format!("GUID {guid} on /dev/disk4")),
            ("both, on two lines", format!("{serial}\n{guid}")),
        ] {
            assert!(
                clipboard_refusal(&carried).is_some(),
                "{case} reached the clipboard ({} characters)",
                carried.len()
            );
        }

        // **The mask is what makes a string copyable**, which is the property the whole arrangement
        // is for — so a masked value passes, and so does ordinary prose and a path.
        for ordinary in [
            "7B******X3N",
            "000A27**********",
            "/Users/somebody/Library/Application Support/ipod-emulator/settings.txt",
            "ipod-boot make-disk iPod_25.1.3.ipsw my-5.5g.img",
            "fetched — SHA-256 verified when it arrived",
            "synthesised, seed 4f2a",
            // A SHA-256 is sixty-four hex digits in one token, not sixteen, and is a fact about a
            // file rather than about a person.
            "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08",
        ] {
            assert!(
                clipboard_refusal(ordinary).is_none(),
                "the gate refuses text carrying no identifier: {ordinary:?}"
            );
        }
    }

    /// **The registered handler is the one that refuses**, not a function beside it.
    ///
    /// §20 item 12's lesson: the defect that shipped lived inside a closure no test reached. This
    /// invokes `copy-text` on a real composed window and reads the sentence back off the Rail.
    #[test]
    fn the_registered_copy_handler_refuses_an_identifier_and_says_so() {
        let settings = Rc::new(RefCell::new(Settings::default()));
        let w = a_window();
        wire(&w, settings);

        // **The last row, not row 0.** `wire` files the first run's plan before any press, so the
        // Rail already holds five `Planned` steps — reading index 0 reads the plan and reports a
        // refusal that is not there.
        let last_row = |w: &MainWindow| {
            let rail = w.get_rail();
            rail.row_data(rail.row_count() - 1).expect("the Rail is not empty")
        };

        let (serial, _) = a_generated_identity();
        w.invoke_copy_text(serial.clone().into());
        let row = last_row(&w);
        assert_eq!(row.kind, RailKind::Note, "the refusal did not reach the Rail");
        assert!(
            row.what.to_lowercase().contains("serial"),
            "the refusal does not say what it refused: {:?}",
            row.what
        );
        assert!(
            !row.what.contains(&serial),
            "the refusal quoted the identifier back, which is the leak one line along: {:?}",
            row.what
        );

        // The control: ordinary text gets the build's own sentence instead, so the arm above is
        // reached by the gate and not by every press.
        w.invoke_copy_text("ipod-boot facts my-5.5g.img".into());
        let last = last_row(&w);
        assert_eq!(last.kind, RailKind::Note);
        assert!(
            last.what.contains("no clipboard"),
            "text carrying nothing was refused as an identifier: {:?}",
            last.what
        );
    }

    /// A serial and a GUID that belong to nobody: `Identity::generate` is a pure function of a
    /// model and a seed, so the fixture is reproducible and `resources/` is not opened.
    ///
    /// **The one place this file makes one.** `AGENTS.md` §2 is the rule it exists to keep — a real
    /// serial, GUID, name or Apple ID never enters a tracked file, and a test fixture is a tracked
    /// file.
    fn a_generated_identity() -> (String, String) {
        let model = eapp_loader::identity::Model::lookup("A446")
            .expect("A446 is in this build's model table");
        let id = eapp_loader::identity::Identity::generate(model, 3);
        (
            id.serial.clone().expect("a generated identity carries a serial"),
            format!("{:016X}", id.guid),
        )
    }

    /// **The registered centre button starts the first run on an empty library.**
    ///
    /// §20 item 12's lesson: drive the callback `wire` actually registers, not a function beside it.
    /// The defect that shipped lived inside a closure no test reached, and the press that *worked*
    /// was the one that took the program down.
    ///
    /// **It also proves the press takes no borrow it does not give back.** `Queue::press` wants four
    /// `&mut`, and writing the call as the scrutinee of a `match` keeps every one of them alive to
    /// the end of the arms.
    ///
    /// **It writes nothing and downloads nothing**, and that is deliberate rather than incidental.
    /// It used to: three presses through the real handler each reached `volume::probe`'s
    /// `set_len(8 GiB)` on the developer's own disk and then spawned `curl` at Apple, in a test
    /// nobody had marked `#[ignore]` and which asserts nothing about either. Neither the identity
    /// nor the routing needs a worker, so the drives directory is a **file** here: `create_dir_all`
    /// refuses that on every platform, the probe answers `Refused`, and the press stops there —
    /// after the mint, which is the whole subject.
    #[test]
    fn the_registered_centre_button_starts_the_first_run_on_an_empty_library() {
        let (settings, _held) = a_fresh_installation();
        let drives = eapp_loader::settings::drives_dir();
        let _ = std::fs::remove_dir_all(&drives);
        if let Some(parent) = drives.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::write(&drives, b"not a directory").expect("the blocking file");
        let w = a_window();
        let wiring = wire(&w, settings.clone());
        assert_eq!(w.get_devices().row_count(), 0, "the fixture is not an empty library");

        // The real, registered handler. A panic here is the shipped defect in a new place.
        w.invoke_start_device(0);

        // **Identity is the one permanent decision in this program**, and one press makes one.
        let after_one = settings.borrow().devices.len();
        assert_eq!(after_one, 1, "a press on an empty bench made {after_one} devices");
        let guid = |s: &Settings| {
            s.resources
                .iter()
                .find_map(|i| match &i.what {
                    eapp_loader::settings::Resource::Firmware(
                        eapp_loader::nor::Source::Synthetic { seed, .. },
                    ) => Some(*seed),
                    _ => None,
                })
                .expect("a synthesised iPod")
        };
        let first = guid(&settings.borrow());
        assert_ne!(first, 0, "the minted seed is the never-chosen default");

        // Pressing again must not mint a second iPod. Whether it runs or refuses depends on whether
        // this machine has curl; neither answer may make a new identity.
        w.invoke_start_device(0);
        w.invoke_start_device(0);
        assert_eq!(
            settings.borrow().devices.len(),
            1,
            "three presses left {} devices; three failed first runs used to leave three iPods with \
             three different FireWire GUIDs",
            settings.borrow().devices.len()
        );
        assert_eq!(guid(&settings.borrow()), first, "the identity was minted twice");

        // And the queue is reachable for a tick, which is the only way to drive it with no display.
        assert!(wiring.work.borrow().owns(w.get_rail().row_data(0).expect("a plan").id as u64));

        // Nothing ran, so nothing was fetched and nothing was built.
        assert!(!wiring.work.borrow().busy(), "a worker was started");
        let cache = eapp_loader::firmware::cache_dir();
        let fetched = std::fs::read_dir(&cache)
            .into_iter()
            .flatten()
            .flatten()
            .count();
        assert_eq!(fetched, 0, "a test that asserts nothing about downloading downloaded something");
        let _ = std::fs::remove_file(&drives);
    }

    /// **A real first run, end to end, through the button the markup presses.**
    ///
    /// §10 in full: the plan on screen, one press, Apple's own servers, a drive that reads back
    /// bootable — and every one of the five steps narrated on the Rail as it happens. Nothing here
    /// is a stand-in: `invoke_start_device` is the callback `wire` registered, `(wiring.tick)()` is
    /// the closure the 100 ms timer runs, and the bytes come from `secure-appldnld.apple.com`.
    ///
    /// Ignored by default because it reaches a third party and writes about 28 MB. Run it with
    /// `IPOD_TEST_DATA` pointed somewhere disposable if you want to look at what it made:
    ///
    /// ```text
    /// IPOD_TEST_DATA=/tmp/run cargo test -p ipod-gui --bins -- --ignored --nocapture \
    ///     a_real_first_run_from_the_registered_centre_button
    /// ```
    #[test]
    #[ignore = "reaches Apple's servers and writes ~28 MB; run with --ignored --nocapture"]
    fn a_real_first_run_from_the_registered_centre_button() {
        let (settings, _held) = a_fresh_installation_in("e2e-first-run");
        let w = a_window();
        let wiring = wire(&w, settings.clone());
        let began = std::time::Instant::now();

        println!("\ndata directory  {}", eapp_loader::settings::data_dir().display());
        println!("ledger          {}", w.get_ledger_download());
        println!("                {}", w.get_ledger_disk());
        println!("                {}", w.get_ledger_note());
        println!("heading         {}", w.get_work_heading());
        println!("cradle          {}", w.get_empty_device().cradle_label);
        // §10.1's ghost: the full drawing in `Colour::Unspecified` at 45 %, an iPod that has not
        // been decided yet. It is an emptiness state, so it is true here and false the moment the
        // press files a device.
        println!("ghost           {}", w.get_ghost());
        println!("startable       {}", w.get_empty_device().startable);
        assert!(w.get_ghost(), "§10.1's bench is not drawing the ghost");
        assert!(w.get_empty_device().startable, "the empty cradle is not pressable");
        println!("\n── the plan, before the press ──");
        let plan = w.get_rail();
        assert_eq!(plan.row_count(), 5, "the plan is not five steps");
        for i in 0..plan.row_count() {
            let r = plan.row_data(i).unwrap();
            println!("  {} {:<10} {:<22} {}", i, r.verb, r.what, r.sub);
            assert_eq!(r.kind, RailKind::Planned, "step {i} is not planned before the press");
        }
        assert_eq!(
            std::fs::read_dir(eapp_loader::firmware::cache_dir())
                .map(|d| d.flatten().count())
                .unwrap_or(0),
            0,
            "something was downloaded to put the plan on screen"
        );

        // ── the press ───────────────────────────────────────────────────────────────────────────
        println!("\n── the press ──");
        w.invoke_start_device(0);
        assert_eq!(settings.borrow().devices.len(), 1, "the press made no device");

        // ── the run, one tick at a time, exactly as the timer drives it ─────────────────────────
        //
        // Each step is timed from the tick that first draws it `Working` (or, for the two the UI
        // thread does itself, `Done`) to the tick that draws it finished. That is a tick's
        // resolution — 100 ms — and it is deliberately the *window's* view rather than the worker's:
        // what is being reported is how long each step was on screen, which is the only duration a
        // person experiences.
        let mut kinds: Vec<RailKind> = vec![RailKind::Planned; 5];
        let mut started: Vec<Option<std::time::Instant>> = vec![None; 5];
        let mut took: Vec<Option<std::time::Duration>> = vec![None; 5];
        let mut peak: Vec<String> = vec![String::new(); 5];
        let mut last_finish = began;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(600);
        loop {
            (wiring.tick)();
            let rail = w.get_rail();
            for i in 0..rail.row_count().min(5) {
                let r = rail.row_data(i).unwrap();
                if !r.measure.is_empty() {
                    peak[i] = r.measure.to_string();
                }
                if r.kind == kinds[i] {
                    continue;
                }
                let was = std::mem::replace(&mut kinds[i], r.kind);
                if r.kind == RailKind::Working {
                    started[i] = Some(std::time::Instant::now());
                    println!("  {:>7.2?}  {} {} — {}", began.elapsed(), r.verb, r.what, r.sub);
                } else if matches!(r.kind, RailKind::Done | RailKind::Failed) {
                    // **A step never drawn `Working` is timed from the previous step's finish, not
                    // from the press.** Two of the five are like that: `synthesise` runs on the UI
                    // thread inside `press` itself, and the install can finish in the same 100 ms
                    // tick the build does — so timing it from `began` credited it with the whole
                    // run, and it reported 1.54s for work that took a fraction of a tick.
                    took[i] = Some(started[i].unwrap_or(last_finish).elapsed());
                    last_finish = std::time::Instant::now();
                    if was == RailKind::Planned {
                        println!(
                            "  {:>7.2?}  {} {} — finished inside one tick, never drawn working",
                            began.elapsed(),
                            r.verb,
                            r.what
                        );
                    }
                }
            }
            if !wiring.work.borrow().busy() || std::time::Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(work::TICK);
        }
        (wiring.tick)();

        println!("\n── how long each step took, as the window saw it ──");
        for i in 0..5 {
            let r = w.get_rail().row_data(i).unwrap();
            println!(
                "  {:<10} {:<22} {:>8}  {}",
                r.verb,
                r.what,
                took[i].map(|d| format!("{d:.2?}")).unwrap_or_else(|| "—".into()),
                peak[i]
            );
        }

        // ── what the Rail ended up holding ──────────────────────────────────────────────────────
        println!("── the Rail, at the end ──");
        let rail = w.get_rail();
        let mut failed = 0;
        for i in 0..rail.row_count() {
            let r = rail.row_data(i).unwrap();
            println!("  {:<9?} {:<10} {:<22} {}", r.kind, r.verb, r.what, r.sub);
            if r.kind == RailKind::Failed {
                failed += 1;
                println!("            ! {}", r.happened);
            }
        }
        println!("\n  announce  {}", w.get_rail_announce());
        println!("  heading   {}", w.get_work_heading());
        let shelf = w.get_devices().row_data(0).expect("the device the press made");
        println!("  shelf     {}", shelf.summary);
        println!("  state     {}", shelf.state);
        println!("  cradle    {}", shelf.cradle_label);
        println!("  ghost     {}", w.get_ghost());
        println!("  progress  {}", w.get_progress());
        println!("  total     {:.1?}", began.elapsed());

        // ── and what is on the disk ─────────────────────────────────────────────────────────────
        let s = settings.borrow();
        let d = s.devices.first().expect("the device the press made");
        println!("\n  device    {}", d.name);
        let img = s
            .disks
            .iter()
            .find(|x| Some(&x.name) == d.disk.as_ref())
            .map(|x| x.path.clone())
            .expect("the drive it built");
        let meta = std::fs::metadata(&img).expect("the drive is on the disk");
        println!("  drive     {}", img.display());
        println!("  apparent  {} bytes", meta.len());
        println!(
            "  on disk   {} bytes",
            eapp_loader::settings::on_disk_size(&meta)
        );

        assert_eq!(failed, 0, "a step failed; see the Rail above");
        assert!(
            !img.to_string_lossy().ends_with(".part"),
            "the drive still carries a partial file's name: {}",
            img.display()
        );
        // §10.2 step 4: the drive is Apple's software, and the flash updater is not armed — a drive
        // that would boot the updater instead of the OS looks broken later for a reason nobody
        // recorded.
        let state = eapp_loader::ipsw::firmware_state(&img).expect("the drive reads back");
        println!("  firmware  {state:?}");
        assert!(state.has_os, "the drive has no OS image on it");
        assert!(!state.aupd_armed, "Apple's flash updater is still armed on the drive");
    }

    /// **A retry resumes: it does not re-mint, and it does not download 6.5 MB again.**
    ///
    /// §10.3. The two failure runs beside this one both fail at the *first* step, so the only thing
    /// they can show is that nothing was undone. This one fails in the **middle**: the fetch gets
    /// all the way through — 6.5 MB from Apple, SHA-256 checked — and then the build is blocked, so
    /// the second press has real finished work to either keep or throw away.
    ///
    /// The block is a directory sitting where the drive's `.part` file wants to be. It is deliberate
    /// that the volume probe does not trip on it: `volume::probe` writes `.ipod-probe-<pid>`, so the
    /// directory is writable and the run is refused at exactly one step, which is the shape a
    /// half-finished first run really has.
    ///
    /// The proof that the fetch was not repeated is the bundle's **modification time**, which a
    /// re-download would move.
    #[test]
    #[ignore = "reaches Apple's servers; run with --ignored --nocapture"]
    fn a_retry_after_a_failed_build_does_not_download_again() {
        let (settings, _held) = a_fresh_installation_in("e2e-resume");
        let w = a_window();
        let wiring = wire(&w, settings.clone());
        let drives = eapp_loader::settings::drives_dir();
        let cache = eapp_loader::firmware::cache_dir();

        // The block: a directory where the drive's partial file has to be a file.
        std::fs::create_dir_all(drives.join("my-5.5g.img.part")).expect("the blocker");

        let run = |label: &str| {
            println!("\n── {label} ──");
            w.invoke_start_device(0);
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(300);
            while wiring.work.borrow().busy() && std::time::Instant::now() < deadline {
                (wiring.tick)();
                std::thread::sleep(work::TICK);
            }
            (wiring.tick)();
            let rail = w.get_rail();
            for i in 0..rail.row_count() {
                let r = rail.row_data(i).unwrap();
                println!("  {:<9?} {:<10} {:<22} {}", r.kind, r.verb, r.what, r.sub);
                if r.kind == RailKind::Failed {
                    println!("            ! {}", r.happened);
                }
            }
        };

        run("press 1 — the build is blocked");
        let bundle = cache.join("iPod_25.1.3.ipsw");
        let fetched = std::fs::metadata(&bundle)
            .unwrap_or_else(|e| panic!("the fetch did not finish, so this proves nothing: {e}"));
        let when = fetched.modified().expect("a modification time");
        println!("\n  fetched   {} bytes at {when:?}", fetched.len());
        let seed_of = |s: &Settings| -> u64 {
            s.resources
                .iter()
                .find_map(|i| match &i.what {
                    eapp_loader::settings::Resource::Firmware(
                        eapp_loader::nor::Source::Synthetic { seed, .. },
                    ) => Some(*seed),
                    _ => None,
                })
                .expect("a synthesised iPod")
        };
        let first_seed = seed_of(&settings.borrow());

        // Unblock, and press again. **The bundle is on disk and verifies**, so a resume must adopt
        // it rather than start over.
        std::fs::remove_dir_all(drives.join("my-5.5g.img.part")).expect("unblocking");
        run("press 2 — unblocked, and it must resume");

        let again = std::fs::metadata(&bundle).expect("the bundle is still there");
        assert_eq!(
            again.modified().expect("a modification time"),
            when,
            "the retry re-downloaded 6.5 MB that was already on disk and already verified"
        );
        assert_eq!(
            seed_of(&settings.borrow()),
            first_seed,
            "the retry minted a second iPod"
        );
        assert_eq!(settings.borrow().devices.len(), 1, "the retry made a second device");

        let rail = w.get_rail();
        let failed = (0..rail.row_count())
            .filter_map(|i| rail.row_data(i))
            .filter(|r| r.kind == RailKind::Failed)
            .count();
        assert_eq!(failed, 0, "the resumed run still has a failure on it");
        let d = settings.borrow().devices[0].clone();
        assert!(d.names_a_disk(), "the resumed run did not finish the drive");
        println!("\n  resumed, one iPod, one download.");
    }

    /// **A first run that fails, pressed again, and again — one iPod.**
    ///
    /// §10.3 and §19.2's finding, driven through the registered button rather than through the
    /// queue: *three failed first runs left three iPods with three different FireWire GUIDs.*
    /// Identity is the one permanent decision in this program — the DRM binds to the 8-byte FireWire
    /// GUID in `sysinfo_t`, and a synthesised iPod's identity is what makes the same iPod come back
    /// next launch. Mint once; a retry resumes.
    ///
    /// It does **not** inject a failure of its own. Point it at something that will fail — a `curl`
    /// on `PATH` that cannot reach Apple, or an `IPOD_TEST_DATA` on a volume with no room — and it
    /// reports what happened. Given neither, it is a first run that succeeds three times, which is
    /// the same assertion about identity from the other side and is worth having too.
    #[test]
    #[ignore = "for driving deliberate failures; run with --ignored --nocapture"]
    fn a_first_run_pressed_three_times_mints_one_ipod() {
        let (settings, _held) = a_fresh_installation_in("e2e-three-presses");
        let w = a_window();
        let wiring = wire(&w, settings.clone());

        println!("\ndata directory  {}", eapp_loader::settings::data_dir().display());
        println!("ledger          {}", w.get_ledger_disk());

        let seed = |s: &Settings| -> Option<u64> {
            s.resources.iter().find_map(|i| match &i.what {
                eapp_loader::settings::Resource::Firmware(
                    eapp_loader::nor::Source::Synthetic { seed, .. },
                ) => Some(*seed),
                _ => None,
            })
        };
        let mut seeds: Vec<u64> = Vec::new();
        let mut done_after: Vec<usize> = Vec::new();

        for attempt in 1..=3 {
            println!("\n── press {attempt} ──");
            w.invoke_start_device(0);
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(300);
            while wiring.work.borrow().busy() && std::time::Instant::now() < deadline {
                (wiring.tick)();
                std::thread::sleep(work::TICK);
            }
            (wiring.tick)();

            let rail = w.get_rail();
            let mut done = 0usize;
            for i in 0..rail.row_count() {
                let r = rail.row_data(i).unwrap();
                if r.kind == RailKind::Done {
                    done += 1;
                }
                let mark = match r.kind {
                    RailKind::Done => "done   ",
                    RailKind::Failed => "FAILED ",
                    RailKind::Planned => "planned",
                    RailKind::Working => "working",
                    _ => "       ",
                };
                println!("  {mark} {:<10} {:<22} {}", r.verb, r.what, r.sub);
                if r.kind == RailKind::Failed {
                    println!("          ! {}", r.happened);
                    if !r.mono.is_empty() {
                        println!("          $ {}", r.mono);
                    }
                    // The two next-step slots the markup draws, and whether each is live. §16.5:
                    // a control this build cannot take is drawn DISABLED wearing its reason.
                    for (label, on, why, escape) in [
                        (&r.next_a_label, r.next_a_enabled, &r.next_a_reason, &r.next_a_escape),
                        (&r.next_b_label, r.next_b_enabled, &r.next_b_reason, &r.next_b_escape),
                    ] {
                        if label.is_empty() {
                            continue;
                        }
                        println!(
                            "          > {label}{}{}",
                            if on { String::new() } else { format!("  (disabled: {why})") },
                            if escape.is_empty() { String::new() } else { format!("  [{escape}]") }
                        );
                    }
                }
            }
            println!("  shelf   {}", w.get_rail_line());
            done_after.push(done);
            let s = settings.borrow();
            println!(
                "  devices {}  disks {}  resources {}",
                s.devices.len(),
                s.disks.len(),
                s.resources.len()
            );
            seeds.push(seed(&s).expect("the press minted an iPod"));
        }

        println!("\n  seeds        {seeds:?}");
        println!("  steps done   {done_after:?}");
        let distinct: std::collections::BTreeSet<u64> = seeds.iter().copied().collect();
        assert_eq!(
            distinct.len(),
            1,
            "three presses left {} iPods with {} FireWire GUIDs: {seeds:x?}",
            distinct.len(),
            distinct.len()
        );
        assert_ne!(seeds[0], 0, "the minted seed is the never-chosen default");
        assert_eq!(settings.borrow().devices.len(), 1, "three presses left more than one device");

        // **A retry resumes; it does not restart.** Whatever a press got through stays through, so
        // the count can only go up. It going down would mean a later press undid finished work —
        // which for the fetch means re-downloading 6.5 MB that already verified on disk.
        for pair in done_after.windows(2) {
            assert!(
                pair[1] >= pair[0],
                "a press undid finished work: {done_after:?} — a retry is meant to resume from the \
                 first unticked step, not start over"
            );
        }
    }

    /// **Every next step this build draws LIVE is wired to something.**
    ///
    /// §16.5's rule has two halves and only the first was ever checked. A control this build cannot
    /// take is drawn disabled with its reason — `a_disabled_row_states_its_reason_to_an_assistive_
    /// technology` and `rail.rs`'s own sweeps hold that half. The other half is that a control this
    /// build draws **live** does something when it is pressed, and it was false twice:
    /// `Next::CancelWrite` and `Next::Fix` both returned `true` from `available` unconditionally —
    /// this program talking to itself — so both passed `take_next_step`'s guard and landed in the
    /// empty catch-all under it. `CancelWrite` is now wired to `cancel_write`; `Fix` was gated on
    /// `caps.composer` and drawn disabled, and **it went on being drawn disabled for the four
    /// Composer pages after that** — the gate was honest and the boolean behind it was stale. It is
    /// now wired to the route those pages made real, and this sweep is what caught the gap:
    /// flipping the cap without the arm fails on `build from Apple's firmware instead ... pressing
    /// it changed nothing`.
    ///
    /// This is the closed sweep of that: for every failure class, every next step this build's
    /// `caps()` says is pressable has to be a variant `take_next_step` acts on.
    #[test]
    fn every_next_step_this_build_offers_is_wired_to_something() {
        use eapp_loader::compose::Fix;
        use rail::{Class, Tool};

        // The ten, by value. `Class::ALL` is the names; an eleventh variant makes the length
        // assertion below fail until somebody sweeps it too.
        let classes = [
            Class::Network,
            Class::NotServed,
            Class::Verification,
            Class::Incompatible(Fix::BuildFromIpsw),
            Class::SpacePreflight,
            Class::SpaceMidWrite,
            Class::Volume,
            Class::Permission,
            Class::ToolMissing(Tool::Curl),
            Class::Missing,
        ];
        assert_eq!(
            classes.len(),
            Class::ALL.len(),
            "a failure class was added and this sweep does not know about it"
        );

        // **Pressed, not enumerated.** A list of variants `take_next_step` is supposed to act on is
        // a second copy of the `match`, and it would agree with an empty arm. This presses each
        // live control through the same function the markup's `rail-next(id, n)` reaches and
        // requires something to have changed — the Rail, or where you are.
        let mut live: Vec<String> = Vec::new();
        for c in &classes {
            // Both sides of the retry counter: `Verification` stops offering `Retry` after the
            // first, so a sweep at 0 alone would miss whatever the second press offers.
            for retries in [0u8, 1, 2] {
                let steps = c.next(retries, caps());
                for (which, step) in steps.iter().enumerate() {
                    if !step.available(caps()) {
                        // The other half of §16.5, checked here too: disabled and silent is the
                        // shape §19.1 indicts.
                        assert!(
                            !step.reason().is_empty(),
                            "{:?} offers `{}` disabled with nothing said about why",
                            c,
                            step.label()
                        );
                        continue;
                    }
                    live.push(step.label());

                    let rail = Rc::new(RefCell::new(rail::Rail::new()));
                    let stack = Rc::new(RefCell::new(nav::Stack::new()));
                    let composer: Rc<RefCell<Option<composer::Composer>>> =
                        Rc::new(RefCell::new(None));
                    let id = rail.borrow_mut().failed(
                        "fetch",
                        "Apple's firmware",
                        rail::Failure::new(c.clone(), "a download"),
                    );
                    // `failed` files at retries 0; wind it up to the count these steps came from,
                    // or `Verification`'s second press is tested against its first press's entry.
                    for _ in 0..retries {
                        rail.borrow_mut().retry(id);
                        rail.borrow_mut().fail(id, rail::Failure::new(c.clone(), "a download"));
                    }
                    let before = (
                        rail.borrow().entries().to_vec(),
                        stack.borrow().page(),
                        stack.borrow().depth(),
                    );

                    take_next_step(&rail, &stack, &composer, id, which as i32, caps());

                    let after = (
                        rail.borrow().entries().to_vec(),
                        stack.borrow().page(),
                        stack.borrow().depth(),
                    );
                    assert!(
                        before != after,
                        "{:?} draws `{}` live and pressing it changed nothing — neither the Rail \
                         nor where you are. A visible control that does nothing is the defect \
                         docs/GUI.md indicts twice",
                        c,
                        step.label()
                    );
                    // **And a page arrived at is a page with something on it.** `Fix` is the one
                    // step here that navigates, and `push_composer` returns immediately when
                    // nothing is being composed — so a press that moved the drawer to the Composer
                    // without minting one would satisfy the assertion above and still draw the
                    // blank 420 px panel `Stack::go` spends two guards preventing.
                    if stack.borrow().page() == nav::Page::Composer {
                        assert!(
                            composer.borrow().is_some(),
                            "{:?} sent the drawer to the Composer with no recipe in hand",
                            c
                        );
                    }
                }
            }
        }
        // **The control, and it names what it expects rather than counting.** A sweep that found no
        // live control at all would pass vacuously, and a bare `live > 3` would have been an
        // instrument that lies in the other direction: `Retry` is now gated on `caps.download`, so
        // on a computer with no `curl` the count drops to three and the control would go red about
        // the machine rather than about the program.
        //
        // `Cancel` needs no capability — the file is ours, on this computer — so it is live on every
        // machine, and `Retry` is live exactly when `curl` is. Both are checked; neither can pass
        // vacuously and neither can fail spuriously.
        assert!(
            live.iter().any(|l| l == "Cancel"),
            "the sweep found no live `Cancel`, so it is not reaching SpaceMidWrite's controls at \
             all: {live:?}"
        );
        assert_eq!(
            live.iter().any(|l| l == "Retry"),
            caps().download,
            "`Retry` is live exactly when this computer can download, and it is not: {live:?}"
        );
        // `Fix` was the second live-but-inert control, and it is now the third thing this sweep
        // pins rather than an exception to it: live exactly when there is a Composer page to send
        // it to. Written as an equality, like `Retry`'s, so it goes red in **both** directions —
        // a `Fix` drawn live with no page behind it, and a `Fix` still drawn disabled beside one.
        //
        // **The right-hand side is `Page::slot`, and it was `caps().composer` — which made this a
        // tautology.** `Next::available` gates `Fix` on `caps.composer`, so `live` can only hold the
        // label when that boolean is true; comparing the set against the same boolean is the
        // expression on both sides of an `assert_eq!`. It was proved by mutation rather than by
        // reading: `caps()`'s `composer` field set to a literal `false` moved **not one assertion
        // in the crate** — `234 passed; 0 failed` — so the guard written to catch a `Fix` drawn
        // disabled beside a Composer could not catch it. §6's rule about an instrument, applied to
        // a guard: before believing a green, run the control that makes it red.
        //
        // `Page::Composer.slot()` is what `caps().composer` is *derived from*, one file away and
        // answering the same question — so the two sides are now independent, and the direction
        // that matters goes red: a `caps()` that contradicts the page table it reads.
        assert_eq!(
            live.iter().any(|l| *l == compose::Fix::BuildFromIpsw.label()),
            nav::Page::Composer.slot().is_some(),
            "`Fix` is live exactly when there is a Composer page to send it to, and it is not — \
             `caps().composer` and `Page::Composer.slot()` disagree about whether this build has \
             one: {live:?}"
        );
    }

    /// **Cancelling deletes the partial file, and both routes to it are one function.**
    ///
    /// §12.7. The entry's drawn `Cancel` and `Class::SpaceMidWrite`'s `Cancel` next step are the
    /// same request; until now only the first did anything.
    #[test]
    fn cancelling_deletes_the_partial_file_and_says_so() {
        let dir = temp_dir("cancel");
        let part = dir.join("iPod_25.1.3.ipsw.part");
        std::fs::write(&part, b"half a download").unwrap();

        let rail = Rc::new(RefCell::new(rail::Rail::new()));
        let id = rail.borrow_mut().note("fetching Apple's firmware");
        rail.borrow_mut().writing(id, part.clone());
        rail.borrow_mut()
            .progress(id, rail::Progress::Bytes { done: 15, total: 6_533_633 });

        // §12.7: the cost is stated BEFORE the control is pressed, which is what makes pressing it
        // the consent `AGENTS.md` §3 requires.
        let cost = rail.borrow().find(id).expect("the entry").cancel_cost();
        assert!(
            cost.contains("iPod_25.1.3.ipsw.part") && cost.contains("deletes"),
            "the entry does not say what cancelling costs: {cost:?}"
        );

        cancel_write(&rail, id);
        assert!(!part.exists(), "the partial file survived a cancel");
        let r = rail.borrow();
        assert_eq!(
            r.find(id).map(|e| e.kind),
            Some(rail::Kind::Cancelled),
            "the entry did not become a cancellation"
        );
        assert!(
            r.entries().iter().any(|e| e.what.contains("deleted")),
            "nothing on the Rail says the file went"
        );
        drop(r);

        // **It deletes nothing it was not given.** An id with no partial file behind it is a
        // no-op, not a guess at what to remove.
        let other = rail.borrow_mut().note("nothing is being written");
        cancel_write(&rail, other);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// **Every cradle label this file TYPES fits the row it is drawn in.**
    ///
    /// §7.3's label is `width: frame.width` with `overflow: elide` (`ui/bench.slint`), and
    /// [`geometry::CRADLE_LABEL_MAX_CHARS`] is how many characters that row holds at the smallest
    /// window that draws the device at all. Both shipped sentences overran it — 63 and 58 against
    /// 48 — so the one control this whole program is built around was captioned with a line that
    /// elided, unmeasured, everywhere.
    ///
    /// **Only the typed ones.** A refusal is `gone_sentence`, which carries a path on purpose
    /// (§7.3), and holding a path to a character budget would mean truncating the answer to *where
    /// did my drive go*. Those elide by design, first words first.
    #[test]
    fn every_typed_cradle_label_fits_its_own_row() {
        let budget = geometry::CRADLE_LABEL_MAX_CHARS;
        let made = Device {
            disk: Some("mine".into()),
            disk_path: Some(std::path::PathBuf::from("/tmp/mine.img")),
            ..Device::default()
        };
        let half_made = Device { name: compose::FIRST_RUN_DEVICE.into(), ..Device::default() };
        let composed = Device { composed: true, ..Device::default() };
        let typed = [
            ("the startable device", cradle_label(&made, &[])),
            // §10.3's half-made one. `FIRST_RUN_DEVICE` is the name a first run gives, so this is
            // the longest label this program composes without a person having renamed anything.
            ("the half-made device", cradle_label(&half_made, &[])),
            ("the composed device", cradle_label(&composed, &[])),
            ("the first-run bench", empty_device(true, caps(), a_cost()).cradle_label.to_string()),
            ("the empty bench", empty_device(false, caps(), no_cost()).cradle_label.to_string()),
        ];
        // …and the half-made one says *finish*, not *start*, or the label promises what the press
        // does not do.
        assert!(
            typed[1].1.contains("finish making") && typed[1].1.contains(compose::FIRST_RUN_DEVICE),
            "a first run that stopped part-way is captioned with a promise to start: {:?}",
            typed[1].1
        );
        for (what, line) in &typed {
            let n = line.chars().count();
            assert!(
                n <= budget,
                "{what}'s cradle label is {n} characters against a {budget}-character row, so it \
                 elides on every window this program allows: {line:?}"
            );
        }
        // **The control.** A budget that nothing can overrun is not a budget, and the sentence that
        // shipped is the proof that this one can be — 63 characters, and it was on screen.
        const SHIPPED: &str = "Press the centre button — running is not wired to the window yet";
        assert!(
            SHIPPED.chars().count() > budget,
            "the line that shipped fits after all, so this check has never had anything to catch"
        );
        // And a label that fits is still a sentence rather than a word.
        for (what, line) in &typed {
            assert!(line.chars().count() > 12, "{what}'s label says nothing: {line:?}");
        }
    }

    /// §7.5's row-2 trailing slot names the display scale only where it differs from `k`.
    ///
    /// **And it is ASCII.** It used to read `panel 2× · 320×240 · nearest neighbour`, drawn on shelf
    /// row 2 — twenty pixels above a row 3 that draws `·` as a `Path` because §6.7 does not consider
    /// it proven. One band, two answers.
    #[test]
    fn the_fidelity_line_carries_a_number_a_bug_report_can_quote() {
        let one = fidelity(1, 1.0);
        assert!(one.contains("320x240"), "{one}");
        assert!(!one.contains("display scale"), "the same fact written twice: {one}");
        let two = fidelity(1, 1.25);
        assert!(two.contains("display scale 125 %"), "{two}");
        for s in [&one, &two] {
            assert!(s.is_ascii(), "the fidelity line carries a glyph nothing has proved: {s:?}");
        }
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

    /// The drawing's proportions are **measured**, and this is the check that says so.
    //
    // The overlap and physical-sanity assertions below relate declared constants to each other, so
    // clippy's "this assertion has a constant value" is describing the point rather than a defect.
    #[allow(clippy::assertions_on_constants)]
    #[test]
    fn the_drawn_ipod_is_the_shape_of_a_real_one() {
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

        // The drawing insets the screen by the same amount left, top and right.
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

    /// Every property this file pushes exists on the generated window.
    ///
    /// It asserts no value; it fails to BUILD if one is renamed or goes private, which is the only
    /// failure mode that matters — `hero` was once `property <length> hero` **inside the pane**, so
    /// the design's own `window.set_hero(hero as f32)` described a call that could not compile.
    #[test]
    fn the_window_can_be_told_everything_this_file_pushes() {
        let _: fn(&MainWindow, f32) = MainWindow::set_hero;
        let _: fn(&MainWindow, f32) = MainWindow::set_screen_w;
        let _: fn(&MainWindow, f32) = MainWindow::set_screen_h;
        let _: fn(&MainWindow, i32) = MainWindow::set_screen_scale;
        let _: fn(&MainWindow, bool) = MainWindow::set_too_short;
        let _: fn(&MainWindow, bool) = MainWindow::set_drawer_open;
        let _: fn(&MainWindow, i32) = MainWindow::set_drawer_depth;
        let _: fn(&MainWindow, DrawerPage) = MainWindow::set_drawer_page;
        let _: fn(&MainWindow, i32) = MainWindow::set_rail_failures;
        let _: fn(&MainWindow, bool) = MainWindow::set_ledger_warn;
        let _: fn(&MainWindow, bool) = MainWindow::set_ghost;
        let _: fn(&MainWindow, f32) = MainWindow::set_progress;
        // §9.2's cradle line. `verb-width` is deliberately absent: it is an `out property`, so
        // there is no setter, and that asymmetry is the point of it.
        let _: fn(&MainWindow, slint::SharedString) = MainWindow::set_working_label;
    }

    /// **The non-circular half of §20 item 11.**
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
