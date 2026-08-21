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
mod fit;
mod geometry;
mod motion;
mod nav;
mod rail;

use std::cell::RefCell;
use std::rc::Rc;

use eapp_loader::identity::Colour;
use eapp_loader::settings::{Absent, Device, Presence, Settings};
// Only for `on_winit_window_event`: `Resized`, `Moved` and `ScaleFactorChanged` are the three
// moments §16.1 says the fit has to be recomputed at, and Slint exposes none of them.
use i_slint_backend_winit::WinitWindowAccessor;
use slint::{ComponentHandle, Model, ModelRc, SharedPixelBuffer, VecModel};

slint::include_modules!();

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
    wire(&window, settings.clone());

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
    client_height::dump_layout(window.window(), &fitter.fit(), geometry::PREF_HEIGHT, sf);

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
            if let Some(w) = weak.upgrade() {
                push_fit(&w, &fit, sf);
            }
            client_height::dump_layout(win, &fit, measured, sf);
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
fn wire(window: &MainWindow, settings: Rc<RefCell<Settings>>) {
    // ── The library, and it is retained too (§16.9) ──
    //
    // **`set_devices` is called once.** It used to be called once because nothing re-read the
    // library — which meant the bench was a startup snapshot: delete a drive image while the window
    // is open and the cradle still said *press the centre button*, because nothing re-stat'd. Now it
    // is called once because the model is retained and [`refresh_devices`] mutates it in place,
    // which is what §16.9 asks for and what keeps focus, hover and the selection through a refresh.
    let devices: Rc<VecModel<DeviceRow>> = Rc::new(VecModel::default());
    window.set_devices(ModelRc::from(devices.clone()));
    refresh_devices(window, &devices, &settings.borrow());
    // §9.1: an empty library is a state with something to say. The window composes `current` out of
    // this when there is no device, so every sentence on the bench stays the model's — a struct
    // literal in the markup is how the previous revision came to invent a chassis colour there.
    window.set_empty_device(empty_device());
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
    let caps = caps();

    push_ledger(window);
    sync_rail(window, &rows, &rail.borrow(), caps);
    push_nav(window, &stack.borrow());

    // ── The centre button, and §20 item 12's whole point ──
    {
        let settings = settings.clone();
        let rail = rail.clone();
        let stack = stack.clone();
        let rows = rows.clone();
        let devices = devices.clone();
        let weak = window.as_weak();
        window.on_start_device(move |index| {
            let Some(w) = weak.upgrade() else { return };
            // **The borrow is scoped, and this line is why.** Written as
            // `match resolve_for_start(&mut settings.borrow_mut(), …) { … }`, the `RefMut` is a
            // temporary in the scrutinee and lives to the end of the whole `match` — so the `Ok`
            // arm's `settings.borrow()` panicked with *already mutably borrowed*. Only the success
            // path took it, which is why every refusal test stayed green: the press that WORKED was
            // the one that took the program down, which is §20 item 12 exactly inverted.
            let outcome = {
                let mut s = settings.borrow_mut();
                resolve_for_start(&mut s, index as usize)
            };
            match outcome {
                Ok(name) => {
                    // **Starting the machine is a later slice.** Until then this files a
                    // project-state note rather than an `eprintln!`, which is the whole reason the
                    // Rail exists before the first button is wired. The escape hatch is real:
                    // `ipod-boot retail` boots the configured device with no window at all.
                    rail.borrow_mut().note(&format!(
                        "{name} resolves and would start here. Running is not wired to the window \
                         yet — `ipod-boot retail` boots it from a terminal today."
                    ));
                    // The resolution mutated the library: `run_device` makes this the live device.
                    // Saving here rather than only on close is deliberate — a save that fails at
                    // close has nowhere left to be shown (§20 item 13).
                    save(&settings.borrow(), &mut rail.borrow_mut());
                    // §7.3, §7.5: the library moved, so the bench's own account of it has to. The
                    // list is a startup snapshot otherwise, and a drive deleted while the window was
                    // open stayed invisible for the life of the process.
                    refresh_devices(&w, &devices, &settings.borrow());
                }
                Err(f) => {
                    // **No machine is started, and nothing is mutated** — the resolution refuses
                    // before it touches anything.
                    rail.borrow_mut().failed("start", &f.0, f.1);
                    // A refusal nobody can see is `eprintln!` with extra steps. The shelf row that
                    // would carry it is not built, so the drawer opens on the page that is.
                    stack.borrow_mut().go(nav::Page::Work, 1);
                    push_nav(&w, &stack.borrow());
                    refresh_devices(&w, &devices, &settings.borrow());
                }
            }
            sync_rail(&w, &rows, &rail.borrow(), caps);
        });
    }

    // ── §7.6's `why ›`, and it has to have something to explain ──
    {
        let settings = settings.clone();
        let rail = rail.clone();
        let stack = stack.clone();
        let rows = rows.clone();
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
            sync_rail(&w, &rows, &rail.borrow(), caps);
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
        let weak = window.as_weak();
        window.on_open_page(move |page, depth| {
            stack.borrow_mut().go(from_markup(page), depth);
            if let Some(w) = weak.upgrade() {
                push_nav(&w, &stack.borrow());
            }
        });
    }
    {
        let stack = stack.clone();
        let weak = window.as_weak();
        window.on_drawer_back(move || {
            stack.borrow_mut().back();
            if let Some(w) = weak.upgrade() {
                push_nav(&w, &stack.borrow());
            }
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
        let weak = window.as_weak();
        window.on_rail_dismiss(move |id| {
            rail.borrow_mut().dismiss(id as u64);
            if let Some(w) = weak.upgrade() {
                sync_rail(&w, &rows, &rail.borrow(), caps);
            }
        });
    }
    {
        let rail = rail.clone();
        let rows = rows.clone();
        let weak = window.as_weak();
        window.on_rail_cancel(move |id| {
            // §12.7: the entry said which file this deletes and how big it is before the control
            // was pressed, so pressing it is the consent `AGENTS.md` §3 requires. The Rail hands
            // the path back rather than deleting it itself.
            let doomed = rail.borrow_mut().cancel(id as u64);
            if let Some(p) = doomed {
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
            if let Some(w) = weak.upgrade() {
                sync_rail(&w, &rows, &rail.borrow(), caps);
            }
        });
    }
    {
        let rail = rail.clone();
        let rows = rows.clone();
        let stack = stack.clone();
        let weak = window.as_weak();
        window.on_rail_next(move |id, which| {
            let Some(w) = weak.upgrade() else { return };
            take_next_step(&rail, &stack, id as u64, which, caps);
            push_nav(&w, &stack.borrow());
            sync_rail(&w, &rows, &rail.borrow(), caps);
        });
    }

    // **A defensive arm, and it is unreachable by construction.** `Copy the details` is gated on
    // `caps.clipboard`, which is false because nothing in this dependency graph provides a
    // clipboard, so no control can fire this. It says so rather than silently succeeding — a
    // handler that swallows the request is the visible-control-that-does-nothing defect one level
    // down.
    {
        let rail = rail.clone();
        let rows = rows.clone();
        let weak = window.as_weak();
        window.on_copy_text(move |_| {
            rail.borrow_mut()
                .note("this build has no clipboard, so nothing was copied");
            if let Some(w) = weak.upgrade() {
                sync_rail(&w, &rows, &rail.borrow(), caps);
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
        window.window().on_close_requested(move || {
            if let Err(e) = settings.borrow().save() {
                eprintln!(
                    "the settings could not be written on the way out ({e}); the window is closing \
                     and there is nowhere left to show this"
                );
            }
            slint::CloseRequestResponse::HideWindow
        });
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
/// **Every one of these is false in this build, and each is a fact rather than a policy**:
/// `cargo tree -p ipod-gui | grep -iE "rfd|native-dialog|ashpd"` is empty, §16.4's winit drop hook
/// is not written, nothing here reaches a pasteboard, nothing opens a file manager, and the
/// drawer's Devices page does not exist. A control whose route does not exist is drawn disabled
/// with its reason (§14.1), never live and never quietly dropped.
fn caps() -> rail::Caps {
    rail::Caps {
        file_picker: false,
        drop_target: false,
        clipboard: false,
        reveal: false,
        devices_page: false,
    }
}

/// Press a failure's next step — **after checking in Rust that it is one this build can take**.
///
/// The markup already refuses a disabled `Pressable`; this checks again, because a view is not an
/// authority on what the program can do and the two must not be able to disagree.
fn take_next_step(
    rail: &Rc<RefCell<rail::Rail>>,
    stack: &Rc<RefCell<nav::Stack>>,
    id: u64,
    which: i32,
    caps: rail::Caps,
) {
    let step = {
        let r = rail.borrow();
        let Some(e) = r.find(id) else { return };
        let Some(f) = e.failure.as_ref() else { return };
        let mut steps = f.class.next(e.retries, caps);
        if which < 0 || which as usize >= steps.len() {
            return;
        }
        steps.remove(which as usize)
    };
    if !step.available(caps) {
        return;
    }
    match step {
        rail::Next::Retry => {
            rail.borrow_mut().retry(id);
        }
        rail::Next::Devices => stack.borrow_mut().go(nav::Page::Devices, 1),
        // Every remaining arm needs a mechanism `caps` says this build does not have, so the guard
        // above has already returned. They are enumerated rather than defaulted so the day one
        // arrives the compiler points here.
        rail::Next::Provide
        | rail::Next::ChooseElsewhere
        | rail::Next::CopyDetails
        | rail::Next::Reveal
        | rail::Next::CancelWrite
        | rail::Next::Fix { .. } => {}
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
fn cradle_label(d: &Device, absent: &[Absent]) -> String {
    if absent.is_empty() {
        "Press the centre button — running is not wired to the window yet".into()
    } else {
        gone_sentence(d, absent)
    }
}

/// §9.1's empty bench, as a whole row, so the markup composes it exactly like a real device.
///
/// **Every string here is this file's rather than the markup's**, which is the same rule the other
/// eight fields follow. The chassis is the model's own `Unspecified` — a neutral case, deliberately
/// not black, because drawing an unknown iPod black invents a fact about somebody's device.
fn empty_device() -> DeviceRow {
    DeviceRow {
        name: "Nothing on the bench".into(),
        summary: "The library has no devices yet.".into(),
        state: "".into(),
        write_target: "no device yet — nothing will be written".into(),
        write_target_is_warning: false,
        chassis: chassis_colour(Colour::Unspecified),
        dark_chassis: is_dark(Colour::Unspecified),
        startable: false,
        cradle_label: "Nothing is on the bench. Compose one with: ipod-boot setup".into(),
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
    // **§9.1's heading is pushed from HERE, not once at startup.** It was set by `push_ledger` and
    // never again, so the Work page read *"Nothing is happening."* above a warning icon and a
    // paragraph naming a missing file — and that page is the one the drawer auto-opens onto when a
    // press is refused, so it was the first thing anybody saw.
    let (heading, empty) = work_page_text(rail);
    window.set_work_heading(heading.into());
    window.set_work_empty(empty.into());
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
/// **One number per axis, and both come from `Recipe::steps()`** — which is why there are none
/// here: nothing in this build composes a recipe yet, so the ledger says there is no plan rather
/// than printing a figure nobody derived. §10.1's `6.5 MB to download · about 240 MB on disk`
/// arrives with the Composer.
///
/// **The free-space clause is deliberately absent.** Nothing in this tree can query free bytes —
/// `grep -rn "statvfs|GetDiskFreeSpace|free_space" --include='*.rs' tools/` returns one unrelated
/// hit — so `312 GB free on …` would be invented, and `ledger-warn` stays false because the thing
/// it warns about cannot be measured. **Retirement condition**: when `eapp_loader::volume` exists,
/// the clause and the warn colour arrive together.
fn push_ledger(window: &MainWindow) {
    window.set_ledger_download("Nothing to download".into());
    window.set_ledger_disk("Nothing to build".into());
    window.set_ledger_note("Nothing has been downloaded yet.".into());
    window.set_ledger_warn(false);
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
/// **Retirement condition**: when a recipe can be composed, the planned heading becomes §10.1's
/// *This is what pressing the centre button does* — which is where the plan, rather than the Rail's
/// own tally, decides it.
fn work_page_text(rail: &rail::Rail) -> (String, &'static str) {
    let empty = "Fetches, builds and installs report here.";
    let failures = rail.failures();
    let heading = if failures == 1 {
        "One thing failed.".to_string()
    } else if failures > 1 {
        format!("{failures} things failed.")
    } else if rail.entries().iter().any(|e| e.kind == rail::Kind::Working) {
        "Working.".to_string()
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
                startable: gone.is_empty(),
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
fn refresh_devices(window: &MainWindow, model: &Rc<VecModel<DeviceRow>>, settings: &Settings) {
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
    /// The five at the top of `main.rs` are exempt: they are module-level and share one written
    /// condition, in prose, immediately above them.
    #[test]
    fn every_dead_code_allow_says_what_would_retire_it() {
        let mut bare: Vec<String> = Vec::new();
        let mut seen = 0usize;
        for (name, text) in rust_sources() {
            for (n, line) in text.lines().enumerate() {
                // Prose *about* the attribute is not the attribute. `geometry.rs` explains in a doc
                // comment why one constant is `#[cfg(test)]` "rather than `#[allow(dead_code)]`",
                // and an instrument that reads that as an allow is reporting a defect it created by
                // looking.
                if line.trim_start().starts_with("//") || !line.contains("#[allow(dead_code)]") {
                    continue;
                }
                seen += 1;
                // The module-level five in `main.rs`, whose shared condition is the paragraph above
                // them, in prose, naming the slice that retires all five at once.
                if name == "main.rs" {
                    continue;
                }
                if !line.contains("retired when:") {
                    bare.push(format!("{name}:{}", n + 1));
                }
            }
        }
        assert!(seen > 10, "only {seen} `#[allow(dead_code)]` were found; the sweep read nothing");
        assert!(
            bare.is_empty(),
            "{bare:?} carry no retirement condition. Write `// retired when: <the observation that \
             makes this reachable>` on the same line, or delete the item"
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

        for setter in ["set_rail(", "set_devices("] {
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

    /// Every `.rs` file in this crate, name and text, **with its test module cut off**.
    ///
    /// The cut is load-bearing rather than tidy: the two sweeps below look for `set_rail(` and
    /// `on_winit_window_event`, and their own assertion messages name both. Without it each one
    /// counts itself and reports three registrations where there is one — an instrument reporting
    /// a defect it created by looking.
    pub(crate) fn rust_sources() -> Vec<(String, String)> {
        let dir = std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/src"));
        let mut out: Vec<(String, String)> = std::fs::read_dir(&dir)
            .expect("the src directory")
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "rs"))
            .map(|p| {
                let name = p.file_name().unwrap().to_string_lossy().into_owned();
                let text = std::fs::read_to_string(&p).expect("a source file");
                let shipped: String = text
                    .lines()
                    .take_while(|l| !l.trim_end().ends_with("mod tests {"))
                    .collect::<Vec<_>>()
                    .join("\n");
                (name, shipped)
            })
            .collect();
        out.sort();
        assert!(out.len() > 5, "the source sweep found {} files", out.len());
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
            sync_rail(&window, &rows, &rail, caps());
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
        w.set_empty_device(empty_device());
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

        push_ledger(&w);
        assert!(!w.get_ledger_download().is_empty(), "the ledger has no download line");
        assert!(!w.get_ledger_disk().is_empty(), "the ledger has no disk line");

        // The heading comes from `sync_rail` now, not from `push_ledger` — because it has to follow
        // what the Rail is holding rather than being frozen at startup.
        let rows: Rc<VecModel<RailRow>> = Rc::new(VecModel::default());
        w.set_rail(ModelRc::from(rows.clone()));
        sync_rail(&w, &rows, &rail::Rail::new(), caps());
        assert!(!w.get_work_heading().is_empty(), "the Work page has no heading");
        assert!(!w.get_work_empty().is_empty(), "the empty Work page says nothing");
        assert_eq!(w.get_rail_first_failure(), -1, "an empty Rail has no primary row");
        assert!(w.get_rail_line().is_empty(), "an empty Rail is not a shelf line");

        let stack = nav::Stack::new();
        push_nav(&w, &stack);
        assert!(!w.get_drawer_open());
        assert_eq!(w.get_drawer_page(), DrawerPage::None);
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
        w.set_empty_device(empty_device());

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

        let devices = by_label("Devices");
        assert_eq!(
            devices.accessible_enabled(),
            Some(false),
            "the `Devices` row claims to work; the page behind it is not built"
        );
        assert!(
            !devices.accessible_description().unwrap_or_default().is_empty(),
            "the `Devices` row is disabled and says nothing about why, which is §19.1's finding \
             with the label changed"
        );

        // The control: the one page that IS built has to read differently, or `accessible-enabled`
        // is not being set from anything and every answer above is the same answer.
        assert_eq!(
            by_label("Work").accessible_enabled(),
            Some(true),
            "the one page that IS built reads as disabled too"
        );
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
        let (heading, empty) = work_page_text(&rail::Rail::new());
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
        assert_eq!(work_page_text(&r).0, "Nothing is happening.");

        r.failed(
            "start",
            "iPod 1",
            rail::Failure::saying(rail::Class::Missing, "starting iPod 1", "the drive is gone."),
        );
        let heading = work_page_text(&r).0;
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
        assert_eq!(work_page_text(&r).0, "2 things failed.");

        // A note is a thing that happened, and the heading says so rather than counting to zero.
        let mut n = rail::Rail::new();
        n.note("iPod 1 resolves and would start here.");
        assert_eq!(work_page_text(&n).0, "This is what happened.");
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
