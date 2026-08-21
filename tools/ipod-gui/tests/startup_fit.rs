//! The one thing the arithmetic tests cannot see: what the platform does with the window.
//!
//! Every geometry test in this crate checks a number *about* a window — `the_column_terms_sum_to_
//! the_declared_chrome`, `the_minimum_height_is_a_floor_not_a_fit`, the whole `fit` module. **None
//! of them launches one**, and the headless backend cannot stand in for one either: `i-slint-
//! backend-testing`'s `update_window_properties` sets the size from `layout_constraints().preferred`
//! and nothing else (`i-slint-backend-testing-1.17.1/testing_backend.rs:404-409`) — it has no
//! `adjust_window_size_to_satisfy_constraints`, no minimum clamp, and no window attributes. A
//! headless assertion about a shown window's size reads the preferred size and passes whatever the
//! minimum is, which is to say it passes whether or not the defect is present.
//!
//! So this launches the real binary, with `IPOD_LAYOUT=1`, and reads the instrument. That is how the
//! defect was found and it is the only thing that can see it.
//!
//! **What it guards.** At startup this program is handed the window's height more than once, and the
//! first answer is a lie: Slint clamps a not-yet-existing window's size UP to `min-width ×
//! min-height` during `show()` (`i-slint-core-1.17.1/window.rs:1635-1636` calls
//! `update_window_properties` before `set_visible`; `i-slint-backend-winit-1.17.1/
//! winitwindowadapter.rs:1690-1722` does the clamp and `:810-818` writes it into the pending
//! `WindowAttributes::inner_size`), so the window is CREATED at the minimum and resized to the
//! preferred size before it is ever mapped (`:1114-1124`, `set_visible(true)` at `:1133`). macOS's
//! `request_inner_size` returns `None`, so both sizes arrive afterwards as two queued `Resized`
//! events in creation order. Believing the first one computes `k` and the too-short boolean for a
//! 400 px window that was never on screen.
//!
//! The invariant, stated so it cannot be satisfied by accident: **the fit is computed from the size
//! the platform reports, at startup and after any resize this program did not ask for.**
//!
//! ## Why it drives the window from outside, and what that bought
//!
//! An earlier cut of this file asserted only that the startup heights agreed with each other. That
//! is the program's **self-consistency**, not the platform's answer, and it was proved not to close
//! the class: replacing the body of `own_height_logical` with `geometry::PREF_HEIGHT` — no
//! `with_winit_window`, no `inner_size()`, no contact with the platform at all — passed it, while
//! the binary went on believing 846 with the window 700 px tall. Two things fix that, and both are
//! here:
//!
//! * **the trace states the platform's own answer**, on its own `platform` line, read in
//!   `client_height.rs` by a function `main.rs` does not call — so `measured` has something
//!   independent to be compared against; and
//! * **something outside the program resizes the window**, by an amount this file chooses after the
//!   program is already running, so no constant can satisfy the assertion.
//!
//! ## What it does not observe, and the condition for retiring each gap
//!
//! In the shape `research/04-bypass-ledger.md` uses, because an unstated gap reads as coverage:
//!
//! | not observed | why | retire when |
//! |---|---|---|
//! | `ScaleFactorChanged` | needs two displays of different backing scale, or a scale change while running; this machine has one display at 2× | there is a way to drive a scale change from a test — a second display, or a platform call that changes the backing factor |
//! | the external resize, off macOS | it is driven through System Events; there is no portable equivalent | an X11/Windows equivalent is written. The startup and `platform`-agreement assertions still run there, so the file is not inert — only its strongest assertion is |
//! | anything at all, on the Linux CI leg | that runner has no window server, and says so with `IPOD_GUI_NO_DISPLAY=1` in `.github/workflows/ci.yml` | `xvfb-run` plus the runtime GL/X11 libraries are added to that leg, the declaration is deleted, **and the leg is verified to go red under a deliberate break**. Until then that leg's green says nothing about this file, which `NEXT.md`'s instrument table records |
//! | the see-through title bar, in the shape it actually fails | `opaque_window`'s fallback is asserted against below, but on this machine its `Err` arm co-occurs with a window that does not open at all: the builder having run, an unset platform answers *"Could not initialize backend."* — measured. So the assertion guards a **future** degradation that keeps the window, not today's | a degradation exists that leaves the window openable, or `opaque_window` is given a fallback that does |
//!
//! **This test opens a real window for about a second** during `cargo test --workspace`, and takes
//! focus while it does. That is deliberate: this is the third defect in this project to survive a
//! green suite by living in the gap between what the code computes and what the platform does, and a
//! guard behind `#[ignore]` is a guard that runs when somebody remembers.

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// How long to wait for the window to reach a display. Every startup event on the machine this was
/// written on lands inside 250 ms; the margin is for a cold first launch, where skia's shader cache
/// is empty.
const OPENS_WITHIN: Duration = Duration::from_millis(3000);

/// How long to wait for the program to notice something the platform did.
const REACTS_WITHIN: Duration = Duration::from_millis(1500);

// ── Running the window ──────────────────────────────────────────────────────────────────────────

/// A live window and its trace. **Killed on drop**, so an assertion that unwinds past it cannot
/// leave a GUI process on the operator's desktop.
struct Window {
    child: Child,
    dir: PathBuf,
    log: PathBuf,
    pid: u32,
}

impl Drop for Window {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

impl Window {
    fn trace(&self) -> String {
        std::fs::read_to_string(&self.log).unwrap_or_default()
    }

    /// Poll the trace until `done` is satisfied, or the deadline passes. Returns the last trace read
    /// either way — a timeout is for the caller to describe, because only the caller knows what it
    /// was waiting for.
    fn wait_for(&self, within: Duration, done: impl Fn(&[Block]) -> bool) -> String {
        let deadline = Instant::now() + within;
        loop {
            let trace = self.trace();
            if done(&blocks(&trace)) || Instant::now() >= deadline {
                return trace;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    }
}

/// What happened when the binary was launched.
enum Started {
    /// A window reached a display: some block reports a size the platform measured.
    OnScreen(Window),
    /// The program did not open a window, on a machine that has declared it has none to open.
    NoWindowServer(String),
}

/// Whether this machine has **declared** that it cannot open a window.
///
/// **An absent display is a declaration, not an inference**, and this is the second thing this file
/// got wrong. The first version inferred it: the child exited, therefore no window server. It was
/// proved false in one control — `set_platform` left uncalled makes Slint answer *"Could not
/// initialize backend."*, on this machine, with a window server plainly present — and the test
/// reported that regression as *"this machine has no window server"* and passed. Green, silent, and
/// wrong about the one thing it exists to measure.
///
/// So the skip has to be licensed from outside. `.github/workflows/ci.yml` sets this on the Linux
/// leg, which has no window server, and on no other leg — which makes "this leg measured nothing" a
/// line a reader can find in the workflow file rather than a guess the test made at runtime. Every
/// undeclared machine must produce a window or go red.
fn declared_headless() -> bool {
    std::env::var_os("IPOD_GUI_NO_DISPLAY").is_some_and(|v| !v.is_empty() && v != "0")
}

/// Launch the real binary and wait for it to put a window on a display.
///
/// `IPOD_EMULATOR_DATA` points at a scratch directory: `main` calls `Settings::load_and_seed`, which
/// writes the file back whenever seeding changed something, and a test does not get to touch the
/// operator's library (`AGENTS.md` §3).
///
/// stderr goes to a file rather than a pipe. A pipe that nobody drains blocks the child at 64 KB,
/// and a test whose subject deadlocks when it prints too much is an instrument with a size limit.
///
/// **A dead child is not a machine without a display**, and the earlier cut of this file said it
/// was: it discriminated on liveness alone, so a panic, an early return or a crash while drawing the
/// first frame all reported *"this machine has no window server"* — a cause it had not observed,
/// which is exactly the shape `AGENTS.md` §6 is about. Three things separate them now, in order: a
/// panic exits **101**; a program that has already printed a platform-measured size **had** a
/// display, so an exit after that point is a crash whatever it prints; and anything else is a
/// refusal to open, which is red unless [`declared_headless`] licenses it.
fn launch() -> Started {
    // **One directory per launch, not one per process.** Two tests in this file each launch a
    // window, cargo runs them on two threads, and a shared name meant the second `remove_dir_all`
    // deleted the first window's log while it was still writing to it — a failure that only appears
    // when both run, which is the worst kind to leave in a file whose whole job is measuring.
    static NTH: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!(
        "ipod-gui-startup-fit-{}-{}",
        std::process::id(),
        NTH.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
    let log = dir.join("layout.txt");
    let sink = std::fs::File::create(&log).unwrap_or_else(|e| panic!("{}: {e}", log.display()));

    let child = Command::new(env!("CARGO_BIN_EXE_ipod-emulator"))
        .env("IPOD_LAYOUT", "1")
        .env("IPOD_EMULATOR_DATA", dir.join("data"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::from(sink))
        .spawn()
        .unwrap_or_else(|e| panic!("could not launch {}: {e}", env!("CARGO_BIN_EXE_ipod-emulator")));

    let pid = child.id();
    let mut window = Window { child, dir, log, pid };

    let deadline = Instant::now() + OPENS_WITHIN;
    loop {
        let trace = window.trace();
        let on_screen = blocks(&trace).iter().any(|b| b.platform.is_some());

        match window.child.try_wait() {
            Ok(Some(status)) => {
                let code = status.code();
                let trace = window.trace();
                assert!(
                    code != Some(101),
                    "the window PANICKED at startup. That is not a machine without a display; it \
                     is a crash. What it printed:\n{}",
                    trace.trim()
                );
                assert!(
                    !on_screen,
                    "the window reached a display — the trace carries a platform-measured size — \
                     and then exited with {code:?}. A program that had a window and lost it has \
                     crashed. What it printed:\n{}",
                    trace.trim()
                );
                assert!(
                    declared_headless(),
                    "the window refused to open, exiting with {code:?}, on a machine that has not \
                     declared it has no display. That is a regression until something says \
                     otherwise — a backend left unset answers `Could not initialize backend.` here \
                     and looks exactly like a headless runner, which is how an earlier version of \
                     this test passed green through one. If this machine genuinely cannot open a \
                     window, say so: `IPOD_GUI_NO_DISPLAY=1 cargo test`. What it printed:\n{}",
                    trace.trim()
                );
                return Started::NoWindowServer(trace);
            }
            Ok(None) => {}
            Err(e) => panic!("could not wait on the window: {e}"),
        }

        if on_screen {
            // One more beat: the startup burst is two `Resized` events, and the second must be in
            // the trace before the heights are compared with each other.
            std::thread::sleep(Duration::from_millis(250));
            return Started::OnScreen(window);
        }
        assert!(
            Instant::now() < deadline,
            "the window stayed up for {OPENS_WITHIN:?} and never reported a size the platform \
             measured, so this test measured nothing. That is deliberately loud rather than a \
             second skip: an instrument that goes quiet whenever it cannot measure is the shape \
             `AGENTS.md` §6 is about. The whole trace:\n{}",
            trace.trim()
        );
        std::thread::sleep(Duration::from_millis(25));
    }
}

// ── Reading the instrument ──────────────────────────────────────────────────────────────────────

/// One `IPOD_LAYOUT` block, as the fields the assertions read.
#[derive(Debug)]
struct Block {
    /// `VisibleFrame` / `PrimaryDisplay` / `Unpublished` — what this build claims it can measure.
    work_area: String,
    /// The whole `display` line: a number, or the sentence saying why there is none.
    display: String,
    /// The window height **the platform** reported, logical. `None` before the window exists.
    platform: Option<f64>,
    /// The window height **the fit was computed from**, logical.
    measured: f64,
    /// The client height this fit needs — `fit::required_client_logical`, as printed.
    needs: f64,
    too_short: bool,
    /// §17.Q12: what the text renderer says the longest Rail verb draws at `BODY_SIZE`, logical.
    /// `None` on the seed block, where there is no window to measure in.
    verb: Option<f64>,
}

/// Split a trace into blocks, dropping any that is not complete. The child is still running while
/// this reads, so the last block can be half-written.
fn blocks(trace: &str) -> Vec<Block> {
    trace
        .split("── IPOD_LAYOUT")
        .skip(1)
        .filter_map(|chunk| {
            let field = |name: &str| {
                chunk
                    .lines()
                    .find_map(|l| l.trim_start().strip_prefix(name))
                    .map(str::trim)
            };
            let number = |name: &str| {
                field(name)?
                    .split_whitespace()
                    .next()?
                    .parse::<f64>()
                    .ok()
            };
            // `needs` is the last line this reads, so a chunk carrying one is complete through
            // everything above it.
            let needs = number("needs")?;
            let platform = field("platform").and_then(|l| {
                let t: Vec<&str> = l.split_whitespace().collect();
                let i = t.iter().position(|w| *w == "logical")?;
                t.get(i.checked_sub(1)?)?.parse::<f64>().ok()
            });
            Some(Block {
                work_area: field("work area")?.split_whitespace().next()?.to_string(),
                display: field("display")?.to_string(),
                platform,
                measured: number("measured")?,
                needs,
                too_short: field("fit")?.contains("too short for 1:1"),
                verb: number("verb"),
            })
        })
        .collect()
}

/// The blocks describing a window that is on a display. The seed block — printed before the event
/// loop runs, when there is genuinely nothing to measure — is not one of them, and it says so on its
/// own `platform` line rather than being recognised by a sentence.
fn on_screen(trace: &str) -> Vec<Block> {
    blocks(trace).into_iter().filter(|b| b.platform.is_some()).collect()
}

/// The constants the MARKUP reads, as `build.rs` rendered them — `bench.rs`'s pattern, and the
/// reason no geometry number is typed into this file.
const GEOMETRY: &str = include_str!(concat!(env!("OUT_DIR"), "/geometry.slint"));

/// One `out property <length> <name>: <n>px;` out of that file.
fn geometry_px(name: &str) -> f64 {
    let needle = format!("> {name}: ");
    GEOMETRY
        .lines()
        .find_map(|l| l.split_once(&needle))
        .and_then(|(_, rest)| rest.trim().strip_suffix("px;"))
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or_else(|| panic!("`{name}` is not a length in the generated geometry.slint"))
}

// ── Driving the window from outside the process ─────────────────────────────────────────────────

/// What one System Events command did.
enum Ax {
    Said(String),
    /// This machine will not let a test drive another process's windows. A configuration fact about
    /// the machine, not about the program: on macOS the parent terminal needs Accessibility, and a
    /// CI runner does not have it.
    NotPermitted(String),
    /// No `osascript`, no such window, or anything else. Not macOS, most of the time.
    Unavailable(String),
}

/// Ask the accessibility API about **this** window, by pid. Not a screenshot and not the program's
/// own opinion: `System Events` reads the real `NSWindow` frame from outside the process.
///
/// No `cfg` — `osascript` simply fails to spawn anywhere else, which is the same answer for the same
/// reason and one fewer place for a platform list to go stale.
fn ax(pid: u32, verb: &str) -> Ax {
    let script =
        format!("tell application \"System Events\" to tell (first process whose unix id is {pid}) to {verb}");
    let out = match Command::new("osascript").arg("-e").arg(&script).output() {
        Ok(out) => out,
        Err(e) => return Ax::Unavailable(format!("osascript would not run: {e}")),
    };
    if out.status.success() {
        return Ax::Said(String::from_utf8_lossy(&out.stdout).trim().to_string());
    }
    let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
    // -25211 is `errAEEventNotPermitted`, -1743 is "not authorised to send Apple events".
    let permission = err.contains("-25211")
        || err.contains("-1743")
        || err.contains("assistive access")
        || err.contains("not allowed");
    if permission { Ax::NotPermitted(err) } else { Ax::Unavailable(err) }
}

/// The `w, h` or `x, y` pair one of those commands printed.
fn pair(said: &str) -> Option<(f64, f64)> {
    let (a, b) = said.split_once(',')?;
    Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
}

// ── The test ────────────────────────────────────────────────────────────────────────────────────

/// **The fit is computed from the size the platform reports.**
///
/// See the module header for the mechanism, for what this does not observe, and for the failures it
/// was proved to produce before it was believed.
#[test]
fn the_fit_is_computed_from_the_size_the_platform_reports() {
    let window = match launch() {
        Started::OnScreen(w) => w,
        Started::NoWindowServer(trace) => {
            // Not a pass and not a failure — an unanswerable question on a machine that said in
            // advance it could not answer it. **`cargo test` swallows this**, which is why the
            // licence is an environment variable a reader can find rather than this line: CI runs
            // the file again with `--nocapture` so it is readable there, and `NEXT.md`'s instrument
            // table records that the Linux leg's green says nothing about this test.
            eprintln!(
                "SKIPPED: IPOD_GUI_NO_DISPLAY is set and the window did not open, so nothing here \
                 was measured. What it printed:\n{}",
                trace.trim()
            );
            return;
        }
    };

    let trace = window.trace();
    let start = on_screen(&trace);
    assert!(!start.is_empty(), "no on-screen block, which `launch` should have caught");

    // **The window this program asked for is the window it got.** `opaque_window` degrades rather
    // than failing — it prints and carries on with a see-through title bar — so without this the
    // only thing that notices is the operator's eyes. Its reach is stated in the header's table and
    // is smaller than it looks: measured on this machine, the `Err` arm today co-occurs with a
    // window that does not open, and the assertion above catches that instead.
    assert!(
        !trace.contains("could not ask for an opaque window"),
        "the window fell back to the default backend, so the macOS title bar is see-through. \
         `opaque_window` says so and returns Ok, which is why nothing else catches it:\n{}",
        trace.trim()
    );

    // **The work-area reader answered.** `ceiling_logical` falls back to the window's own height
    // when `client_height_logical` returns `None`, and that fallback silently reinstates the defect
    // §16.10 records: `k` decided from the window instead of from the display. Nothing else would
    // notice — the fallback is not an error and prints no warning of its own.
    for b in &start {
        if b.work_area == "VisibleFrame" || b.work_area == "PrimaryDisplay" {
            assert!(
                b.display.split_whitespace().next().is_some_and(|n| n.parse::<f64>().is_ok()),
                "this build claims it can read a work area ({}) and did not: `display  {}`. `k` is \
                 being decided from the window instead of from the display. The whole trace:\n{}",
                b.work_area,
                b.display,
                trace.trim()
            );
        }
    }

    // **Every block was computed from the size the platform reported at that moment.** This is the
    // general form, and it holds for every moment rather than only for startup: the trace states
    // the platform's own answer on its own line, read by a function `main.rs` does not call.
    for b in &start {
        let platform = b.platform.expect("on_screen filtered these");
        assert!(
            (b.measured - platform).abs() < 1.0,
            "the fit was computed from {:.1} logical while the platform reported {platform:.1}. \
             `win.size()` is Slint's cache and the `Resized` payload is a size the window HAD; only \
             `winit::Window::inner_size()` answers for now. The whole trace:\n{}",
            b.measured,
            trace.trim()
        );
    }

    // **A window that nobody resized has one height.** The startup burst is two `Resized` events
    // carrying two different sizes — the creation size and the preferred one — and reading the
    // payload of the first computes the whole fit for a window that was never on screen.
    let first = start[0].measured;
    let heights: Vec<f64> = start.iter().map(|b| b.measured).collect();
    assert!(
        heights.iter().all(|h| (h - first).abs() < 0.5),
        "startup computed the fit from more than one window height for one window that nobody \
         resized: {heights:?}\n\nThe smallest of those is the size Slint gave the window at \
         CREATION — min-width x min-height, clamped up from 0 x 0 before any winit window existed — \
         and the window is resized to the preferred size before it is ever mapped, so no window \
         that height was ever on screen. Take the height from the platform \
         (`winit::Window::inner_size()`), not from the `Resized` payload. The whole trace:\n{}",
        trace.trim()
    );

    // **And it names the wrong height, because "they all agree" can agree on the wrong one.** The
    // assertion above only goes red when both sizes reach the filter as separate events; with
    // `MIN_HEIGHT` raised to the preferred height there is one sample and nothing to disagree with.
    //
    // It is a proxy, and the message says so rather than asserting a cause it did not observe: a
    // window whose real height happens to equal `min-height` would trip it too. `geometry::
    // the_minimum_height_is_a_floor_not_a_fit` already fails any `MIN_HEIGHT` at or above
    // `HERO_PHYS_1X + CHROME_MIN` = 809.8, which is below `PREF_HEIGHT`, so the raised-minimum half
    // of that ambiguity is red before it gets here.
    let min_height = geometry_px("min-height");
    assert!(
        heights.iter().all(|h| (h - min_height).abs() >= 0.5),
        "the fit was computed from {min_height} logical, which is `min-height` itself. From the \
         trace alone those two are indistinguishable: it is the size Slint gives a window at \
         CREATION when it clamps a not-yet-known 0 x 0 up to the declared minimum, and it is also \
         what a window legitimately sitting at its own minimum would report. Either the `Resized` \
         payload is being read again, or `min-height` has been raised to this window's real height. \
         Heights: {heights:?}\n\nThe whole trace:\n{}",
        trace.trim()
    );

    probe(&window, &start, &trace);
}

/// **Something outside this program resizes the window, and the program has to notice.**
///
/// Every assertion above reads a number this program chose. This one hands it a number it cannot
/// have known: the window is shrunk from outside by an amount computed after it was already running,
/// and the height it then reports must have moved by exactly that much.
///
/// Proved to discriminate: with `own_height_logical` replaced by `geometry::PREF_HEIGHT` the trace
/// shows no reaction at all, and every other assertion in this file still passes.
fn probe(window: &Window, start: &[Block], startup_trace: &str) {
    let m0 = start[0].measured;
    let needs = start[0].needs;
    let min_height = geometry_px("min-height");

    // The dump is printed only when the fit CHANGES (`main.rs`: `if changed`), and the only term a
    // resize moves is the too-short boolean. So the probe has to flip it, which means starting from
    // a window that is not short and shrinking below the threshold.
    let target = needs - 30.0;
    if m0 < needs || target <= min_height + 10.0 {
        eprintln!(
            "SKIPPED the external-resize half: this window is {m0:.1} logical against a {needs:.1} \
             threshold, so shrinking it cannot flip the too-short boolean and nothing would be \
             printed to read. The display is too short to host the probe."
        );
        return;
    }

    let outer0 = match ax(window.pid, "get size of window 1") {
        Ax::Said(s) => match pair(&s) {
            Some((_, h)) => h,
            None => {
                eprintln!("SKIPPED the external-resize half: window size read back as {s:?}");
                return;
            }
        },
        Ax::NotPermitted(e) => {
            // A machine fact, not a defect. On macOS the parent terminal needs Accessibility;
            // a CI runner does not have it and must not go red for that.
            eprintln!(
                "SKIPPED the external-resize half: this machine will not let a test drive another \
                 process's windows, so the strongest assertion in this file did not run. On macOS \
                 that is System Preferences → Privacy & Security → Accessibility, for whatever ran \
                 `cargo test`. osascript said: {e}"
            );
            return;
        }
        Ax::Unavailable(e) => {
            eprintln!(
                "SKIPPED the external-resize half: no way to drive a window from outside on this \
                 platform ({e}). The module header carries the retirement condition."
            );
            return;
        }
    };

    // Outer height, so this includes the title bar — which cancels, because the assertion is about
    // the CHANGE. Points are logical pixels on macOS, so the two deltas are the same units.
    let want_outer = (outer0 - (m0 - target)).round();
    let verb = format!("set size of window 1 to {{1180, {want_outer}}}");
    if let Ax::NotPermitted(e) | Ax::Unavailable(e) = ax(window.pid, &verb) {
        eprintln!("SKIPPED the external-resize half: the resize was refused ({e})");
        return;
    }

    let outer1 = match ax(window.pid, "get size of window 1") {
        Ax::Said(s) => pair(&s).map(|(_, h)| h),
        _ => None,
    };
    let outer1 = outer1.unwrap_or_else(|| {
        panic!(
            "asked for a {want_outer} px window and could not read back what happened, so this \
             assertion cannot tell a program that ignored the resize from a platform that refused \
             it"
        )
    });
    let moved = outer0 - outer1;
    assert!(
        moved.abs() >= 1.0,
        "asked to shrink the window from {outer0} to {want_outer} and the platform left it at \
         {outer1}. Could not drive the platform, so nothing here was measured — which is a failure \
         rather than a pass, because a probe that quietly does nothing is the instrument shape this \
         project has been burned by."
    );

    let want = m0 - moved;
    let trace = window.wait_for(REACTS_WITHIN, |bs| {
        bs.iter().filter(|b| b.platform.is_some()).count() > start.len()
    });
    let seen = on_screen(&trace);
    let after = &seen[start.len().min(seen.len())..];
    assert!(
        !after.is_empty(),
        "something outside this program shrank the window by {moved} px and the program printed \
         nothing, so it did not notice — it is still computing the fit from {m0:.1} logical against \
         a {needs:.1} threshold, which is a too-short window it does not know about. It is reading \
         a size from somewhere other than the platform. The whole trace:\n{}\n\nStartup was:\n{}",
        trace.trim(),
        startup_trace.trim()
    );
    for b in after {
        assert!(
            (b.measured - want).abs() < 1.0,
            "the window was shrunk by {moved} px from outside this program, so the fit should now \
             be computed from {want:.1} logical. It says {:.1}. The whole trace:\n{}",
            b.measured,
            trace.trim()
        );
    }
    assert!(
        after[after.len() - 1].too_short,
        "the window is {want:.1} logical against a {needs:.1} threshold and the program does not \
         think it is too short. §9.5's boolean is the one thing a resize is supposed to move. The \
         whole trace:\n{}",
        trace.trim()
    );

    // **And a move is not a resize.** `Moved` recomputes the fit from the window's own height too,
    // and it is the moment where reading a stale one goes unnoticed longest: nothing about the
    // window's size changed, so a correct program prints nothing at all — measured, not assumed: a
    // position nudge produces no block on this machine. An arm reading anything other than the
    // platform flips the boolean back and prints a block saying so, and the loop below is over
    // every block that appears after the resize.
    if let Ax::Said(s) = ax(window.pid, "get position of window 1") {
        if let Some((x, y)) = pair(&s) {
            let verb = format!("set position of window 1 to {{{x}, {}}}", y + 20.0);
            if let Ax::Said(_) = ax(window.pid, &verb) {
                std::thread::sleep(Duration::from_millis(300));
                let trace = window.trace();
                let seen = on_screen(&trace);
                for b in &seen[start.len().min(seen.len())..] {
                    assert!(
                        (b.measured - want).abs() < 1.0,
                        "moving the window changed the height the fit is computed from, from \
                         {want:.1} to {:.1}. Nothing about the window's size changed. The whole \
                         trace:\n{}",
                        b.measured,
                        trace.trim()
                    );
                }
            }
        }
    }

    eprintln!("the external-resize half ran: {m0:.1} -> {want:.1} logical, driven from outside");
}

/// **The verb column holds the verb this platform actually draws.**
///
/// §17.Q12 named `RAIL_VERB_W` as the one constant nobody had measured, and predicted its failure
/// exactly: *"the longest verb is `synthesise` and it appears at first-run step 1, the first thing
/// a new user ever sees this program do. If it elides, it elides there."* It shipped at 64 px,
/// derived from `geometry::BODY_ADVANCE` — a budget about a system UI face this program cannot
/// interrogate.
///
/// **The answer only exists in the renderer**, and Slint 1.17 exposes no Rust-side way to ask it.
/// So the measurement is taken by a `Text` (`verb-probe` in `ui/window.slint`, `visible: false` and
/// outside every layout), read back through `MainWindow.verb-width`, printed on `IPOD_LAYOUT`'s
/// `verb` line, and compared here — against the **real binary**, with the real font, on a real
/// display. Everything else about that constant is arithmetic about a number somebody chose.
///
/// It lives in this file rather than beside `the_rail_verb_column_holds_the_longest_verb` for the
/// reason the module header gives: `geometry.rs`'s own tests check the arithmetic and never launch
/// a window, and this project has shipped three defects that no amount of arithmetic could see.
#[test]
fn the_verb_column_holds_the_verb_this_platform_draws() {
    let window = match launch() {
        Started::NoWindowServer(trace) => {
            eprintln!(
                "SKIPPED: this machine declared it has no display, so there is no renderer to \
                 measure with. What the window printed:\n{}",
                trace.trim()
            );
            return;
        }
        Started::OnScreen(w) => w,
    };
    let trace = window.trace();
    let seen = on_screen(&trace);
    assert!(
        !seen.is_empty(),
        "the window never reported a size the platform measured. The whole trace:\n{}",
        trace.trim()
    );

    let column = geometry_px("rail-verb-w");
    let drawn: Vec<f64> = seen.iter().filter_map(|b| b.verb).collect();
    assert!(
        !drawn.is_empty(),
        "no block carries a `verb` line, so this test measured nothing — which is the shape \
         `AGENTS.md` §6 is about, not a pass. The whole trace:\n{}",
        trace.trim()
    );
    for w in &drawn {
        assert!(
            *w > 0.0,
            "the renderer measured `synthesise` at {w} px, which is not a measurement"
        );
        assert!(
            *w <= column,
            "`synthesise` draws {w:.1} px in this platform's face and the Rail's verb column is \
             {column:.1} px, so it ELIDES — at first-run step 1, which is the first thing a new \
             person ever sees this program do. `geometry::RAIL_VERB_W` is derived from \
             `BODY_ADVANCE`, a budget rather than a measurement, and this is the measurement. The \
             whole trace:\n{}",
            trace.trim()
        );
    }
    eprintln!(
        "`synthesise` draws {:.1} logical px against a {column:.1} px column — measured by the \
         renderer, on this display",
        drawn[0]
    );
}
