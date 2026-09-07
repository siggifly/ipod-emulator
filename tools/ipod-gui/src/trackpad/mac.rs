//! **The macOS adapter: `NSTouch`, the actuator, and taking the pointer.** `docs/GUI.md` §21.8.
//!
//! One implementation of [`Source`] and one of [`Detents`]; everything else about the mode is in the
//! file above and is the same on every platform. What is here is only what macOS does differently,
//! and each of the four facts below cost the spike a run to establish. None is guessable.
//!
//! 1. **`NSView.allowedTouchTypes` is mandatory and there is no fallback.** The default is `Direct`
//!    only. With it unset, an independent HID event tap saw **1611** touch-carrying events while
//!    this application saw **0** — by the responder chain and by a local monitor alike. So the one
//!    thing that has to reach Slint's own `NSView` is this one property, and [`NsTouch::arm`] is
//!    the whole of the reach.
//! 2. **A local `NSEvent` monitor is enough**, and no subclass or swizzle is needed. The spike ran
//!    both in one process: 900 monitor lines against 890 from a responder override, the same
//!    coordinates. A monitor also *can* swallow, which a responder cannot, and that is what stops a
//!    click landing on whatever the frozen cursor happens to be over.
//! 3. **`allTouches` raises an Objective-C exception on a non-gesture event, and an ObjC exception
//!    through a Rust frame aborts the process** (*"Rust cannot catch foreign exceptions"*). Every
//!    call to it here is behind a match on the event type. The spike hit this.
//! 4. **`CGAssociateMouseAndMouseCursorPosition` needs no Accessibility grant** and works only
//!    while frontmost, which is the right fence rather than a limitation: the moment this program
//!    is not the front application the cursor is the pointer again whether or not anything here
//!    noticed. The core's `WATCH` is what makes the *window's* state agree with the cursor's.
//!
//! And one that decided the shape of the feedback: **the haptic API offers three canned patterns
//! and nothing else** — `Generic`, `Alignment`, `LevelChange`. No amplitude, no duration, no
//! envelope. `Alignment` is the detent-shaped one and is spoken for. That leaves **two** for
//! everything else this file might ever want to say through a fingertip, which is the entire budget
//! the geometry marks are spent out of — see [`Shape`].

use std::cell::{Cell, RefCell};
use std::ptr::NonNull;
use std::rc::{Rc, Weak};

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObjectProtocol};
use objc2::sel;
use objc2_app_kit::{
    NSEvent, NSEventMask, NSEventModifierFlags, NSEventType, NSHapticFeedbackManager,
    NSHapticFeedbackPattern, NSHapticFeedbackPerformanceTime, NSHapticFeedbackPerformer, NSTouch,
    NSTouchPhase, NSTouchTypeMask, NSView,
};
use objc2_foundation::NSString;

use super::{
    note, verbose, Act, Contact, Detents, Frame, Left, Mark, Mode, Phase, Source, Surface,
};

/// **The actuator, as a sink for the whole window rather than for the mode.**
///
/// It is deliberately *not* installed by [`install`], and that is issue #27's fourth consequence
/// made structural: what fires it is the emulated piezo, so a wheel turned by a two-finger scroll or
/// by §16.8's arrow keys clicks exactly as one turned by a finger on the pad. Tying it to the
/// trackpad mode's installation would have made haptics a feature of a mode nobody has to engage.
pub fn actuator() -> Option<Box<dyn Detents>> {
    Some(Box::new(Actuator))
}

/// **The chord that makes the trackpad the wheel: ⌃⌘T.**
///
/// A chord rather than a letter because this is read *ahead of the responder chain* — before a
/// focused `TextInput` gets a look — so a bare letter would engage the mode in the middle of typing
/// a device's name. It is `⌃⌘`-shaped to sit beside `⌃⌘F`, the one other chord in §16.8 that is a
/// mode rather than an action, and it is a **virtual key code** rather than a character so a
/// non-US layout reaches the same physical key.
const KEY_T: u16 = 17;
/// `Esc`. The way out, read here rather than in the markup so that it works whatever has focus,
/// whatever is open, and whether or not Slint is drawing.
const KEY_ESC: u16 = 53;

/// **Points to millimetres.** `NSTouch.deviceSize` is in points at 72 to the inch, so this
/// machine's 342.99 × 209.76 pt is 121.0 × 74.0 mm — the pad you can put a ruler on, and the unit
/// the core works in. Not a guess: the measured pad is 12.1 cm across.
const MM_PER_POINT: f64 = 25.4 / 72.0;

// The mechanism games use to take the mouse: input still flows, the cursor stops following it.
// Public CoreGraphics (`CGRemoteOperation.h`), no Accessibility grant, no entitlement. `boolean_t`
// is a C `int`, so it is declared as one rather than as a Rust `bool` of a different width.
#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGAssociateMouseAndMouseCursorPosition(connected: i32) -> i32;
}

// ── The two platform halves ─────────────────────────────────────────────────────────────────────

/// **How this file gets the view it must set one property on**, and the whole of what it asks of
/// the window.
///
/// `AGENTS.md` §9 puts the toolkit in `main.rs` and nowhere else, and that rule is what keeps the
/// window replaceable — so the translation from whatever the toolkit calls a window to an `NSView`
/// is `main.rs`'s, and this file names no toolkit type at all. What crosses the seam is an AppKit
/// object, which is correct for the macOS adapter and means a second window implementation has to
/// supply the same four lines rather than change anything here.
///
/// A closure rather than a stored view because the platform window does not exist when this is
/// installed — the event loop is not running yet — and the first engage is when it is wanted.
/// `None` until then, which is not an error.
pub type View = Rc<dyn Fn() -> Option<Retained<NSView>>>;

/// The touch half: the window's `NSView`, the property that makes contacts arrive, and the pointer.
struct NsTouch {
    reach: View,
    /// The view whose `allowedTouchTypes` was set — retained so the property is asked for once, and
    /// so the window is reachable for the title bar without asking again.
    view: RefCell<Option<Retained<NSView>>>,
}

impl Source for NsTouch {
    fn describe(&self) -> &'static str {
        "macOS NSTouch, indirect device"
    }

    /// Answers `false` while there is no view yet — the platform window does not exist until the
    /// event loop is running, which is after everything is wired. That is not an error; it is why
    /// this is done at the first engage rather than at install.
    fn arm(&self) -> bool {
        if self.view.borrow().is_some() {
            return true;
        }
        let Some(view) = (self.reach)() else {
            return false;
        };
        view.setAllowedTouchTypes(NSTouchTypeMask::Indirect);
        view.setWantsRestingTouches(false);
        note(&format!(
            "allowedTouchTypes = {:?} (Indirect = {:?})",
            view.allowedTouchTypes(),
            NSTouchTypeMask::Indirect
        ));
        *self.view.borrow_mut() = Some(view);
        true
    }

    fn grab(&self, take: bool) -> bool {
        // 0 is `kCGErrorSuccess`.
        let rc = unsafe { CGAssociateMouseAndMouseCursorPosition(i32::from(!take)) };
        if rc != 0 {
            note(&format!("CGAssociateMouseAndMouseCursorPosition({take}) -> CGError {rc}"));
        }
        rc == 0
    }

    fn frontmost(&self) -> bool {
        self.view
            .borrow()
            .as_ref()
            .and_then(|v| v.window())
            .is_some_and(|w| w.isKeyWindow())
    }

    /// The title bar's second line, which is where the mode says it is on and how to leave it.
    ///
    /// `NSWindow.subtitle` rather than the title: the title is the markup's (`window.slint` sets it
    /// once) and overwriting it would be this file reaching into a property it does not own. The
    /// subtitle is empty otherwise and belongs to nobody. It is macOS 11 and later, so the selector
    /// is asked for rather than assumed — on anything older the mode still works and says so only
    /// in the log.
    fn announce(&self, text: &str) {
        let view = self.view.borrow();
        let Some(win) = view.as_ref().and_then(|v| v.window()) else {
            return;
        };
        if win.respondsToSelector(sel!(setSubtitle:)) {
            win.setSubtitle(&NSString::from_str(text));
        }
    }
}

/// The feedback half. **The enhancement, not the faithful one** — see [`Detents`]: a real 5G clicks
/// through a piezo and makes a sound, and this actuator is what modders fit in its place. The
/// faithful one is `click/mac.rs`, and both are driven off the same emulated register.
struct Actuator;

impl Detents for Actuator {
    fn describe(&self) -> &'static str {
        "the trackpad's actuator (Alignment for a click, LevelChange for an edge)"
    }

    /// **A claim about the build and never about the hardware.** Force Touch pads only — MacBook
    /// Pro 2015 and later, Air 2018 and later, Magic Trackpad 2 and later — and there is no API
    /// that says which you have, none that reports a failed pulse, and no way to find out but to
    /// ask a person. So this is `true` on macOS and an older pad is simply silent; §21.8 is the
    /// only place that says so, and a second sink is what closes the gap.
    fn present(&self) -> bool {
        true
    }

    /// **`Alignment`, and the tone is ignored on purpose.**
    ///
    /// The API takes a canned pattern and nothing else — no amplitude, no duration, no envelope —
    /// so there is no way to spend the wave or the length on it even if there were reason to.
    /// `Alignment` is the detent-shaped one, it is the one the operator has confirmed feels right,
    /// and it is not to be changed.
    fn click(&self, _: Option<crate::click::Tone>) {
        NSHapticFeedbackManager::defaultPerformer().performFeedbackPattern_performanceTime(
            NSHapticFeedbackPattern::Alignment,
            NSHapticFeedbackPerformanceTime::Now,
        );
    }

    /// **An edge, in the one pattern the detent does not use.**
    ///
    /// The whole design space is three canned patterns — no amplitude, no duration, no envelope —
    /// and `Alignment` is spoken for: it is the click, and the operator has confirmed it feels
    /// right, so it is not available and not to be changed. That leaves `LevelChange` and
    /// `Generic`, and `LevelChange` is the documented *"you crossed into something"* one, which is
    /// exactly what a boundary is.
    ///
    /// **Which of the two, and whether direction is spent on it, is the operator's to judge and
    /// not mine**: nothing in this program can tell whether two canned patterns are distinguishable
    /// through a fingertip. So the choice is a launch-time variant rather than a decision taken
    /// here — see [`Shape`].
    fn mark(&self, m: Mark) {
        NSHapticFeedbackManager::defaultPerformer()
            .performFeedbackPattern_performanceTime(Shape::from_env().pattern(m), NSHapticFeedbackPerformanceTime::Now);
    }
}

/// **What an edge feels like.** `IPOD_TRACKPAD_MARK`, and the default is `level`.
///
/// A variant rather than a constant because the question it answers — *can a hand tell these two
/// apart* — is not one software can ask. Three canned patterns exist, `Alignment` is the detent's,
/// and everything here is an arrangement of the other two.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Shape {
    /// `LevelChange` whichever way the line was crossed. The default, and the least to learn.
    Level,
    /// `Generic` instead — the third pattern, in case `LevelChange` reads as a detent on this pad.
    Generic,
    /// **The two spare patterns spent on direction**: `LevelChange` going in, `Generic` coming out.
    /// The only way to say *which side you are now on* with an API that has no amplitude — and it
    /// costs the last degree of freedom there is, which is why it is not the default.
    Directional,
}

impl Shape {
    fn from_env() -> Shape {
        static CACHE: std::sync::OnceLock<Shape> = std::sync::OnceLock::new();
        *CACHE.get_or_init(|| match std::env::var("IPOD_TRACKPAD_MARK").as_deref() {
            Ok("generic") => Shape::Generic,
            Ok("directional") => Shape::Directional,
            Ok("level") | Err(_) => Shape::Level,
            Ok(other) => {
                // Unconditional for the reason `Felt::parse`'s is — a typo must not make two arms
                // of a comparison behave the same while reading as different.
                eprintln!("[trackpad] IPOD_TRACKPAD_MARK: no shape is called {other:?} — using level");
                Shape::Level
            }
        })
    }

    fn pattern(self, m: Mark) -> NSHapticFeedbackPattern {
        match self {
            Shape::Level => NSHapticFeedbackPattern::LevelChange,
            Shape::Generic => NSHapticFeedbackPattern::Generic,
            Shape::Directional if m.entering => NSHapticFeedbackPattern::LevelChange,
            Shape::Directional => NSHapticFeedbackPattern::Generic,
        }
    }
}

// ── The event plumbing ──────────────────────────────────────────────────────────────────────────

/// What the monitor keeps between events. Everything about *the wheel* is in [`Mode`]; this is
/// only what an `NSEvent` stream needs and no other platform would.
struct Plumbing {
    mode: Rc<Mode>,
    /// Which touch is the wheel's, by the address of its `identity` object. `0` is none.
    ///
    /// Held so that a **second finger cannot take the wheel over** mid-gesture: the tracked one is
    /// preferred while it is still down, and only when it is gone does the lowest address win. The
    /// address is stable for the life of a touch because the `NSTouch` holds the identity object;
    /// it is not stable across gestures, which is why it is cleared whenever the surface empties.
    tracked: Cell<usize>,
    /// The key code whose *down* was swallowed, so its *up* goes with it and nothing downstream
    /// sees half a keystroke. `0` is none.
    swallowed: Cell<u16>,
    /// Whether `Mode::probe` has run. Once, on the first event this monitor ever sees — which is
    /// also the moment that event proves the monitor is live, so one line reports both.
    probed: Cell<bool>,
}

impl Plumbing {
    /// One gesture event's touches, turned into a [`Frame`].
    ///
    /// **Called only from the gesture family**, because `allTouches` raises on anything else and an
    /// ObjC exception through this frame would abort the process.
    fn touches(&self, event: &NSEvent) {
        let set = event.allTouches();
        let mut live: Vec<Retained<NSTouch>> = Vec::new();
        let mut size = None;
        for t in set.iter() {
            size = size.or_else(|| surface_of(t.deviceSize()));
            // `Touching` is `Began | Moved | Stationary`. An ended or cancelled touch is reported
            // in the same set as the ones still down, so the phase is what says the hand is there.
            if t.phase().intersects(NSTouchPhase::Touching) && !t.isResting() {
                live.push(t);
            }
        }
        // No size means no touch carried one, which means there is nothing to say about a surface.
        let Some(surface) = size else {
            return;
        };

        let want = self.tracked.get();
        let primary = live
            .iter()
            .find(|t| ident(t) == want)
            .or_else(|| live.iter().min_by_key(|t| ident(t)));
        let Some(primary) = primary else {
            self.tracked.set(0);
            self.mode.frame(Frame { surface, contact: None });
            return;
        };
        self.tracked.set(ident(primary));

        let np = primary.normalizedPosition();
        let Some(contact) = contact_of(np.x, np.y, surface, phase_of(primary.phase())) else {
            return;
        };
        if verbose() {
            eprintln!(
                "[trackpad] n={} norm=({:.4},{:.4}) dev={:.2}x{:.2}mm mm=({:+.2},{:+.2})",
                live.len(),
                np.x,
                np.y,
                surface.w,
                surface.h,
                contact.x,
                contact.y
            );
        }
        self.mode.frame(Frame { surface, contact: Some(contact) });
    }

    /// **The monitor's whole body.** Returns the event to let it through, or null to swallow it.
    ///
    /// While the mode is **off** this returns on the first comparison for everything but the chord,
    /// which is what makes it free to leave installed for the life of the window.
    fn dispatch(&self, ev: NonNull<NSEvent>) -> *mut NSEvent {
        let pass = ev.as_ptr();
        // SAFETY: AppKit hands the monitor a live event and keeps it alive for the call. The
        // reference never leaves this function.
        let event = unsafe { ev.as_ref() };
        let ty = event.r#type();

        // **The first event this monitor sees proves the monitor is live**, and it is the only
        // moment at which that can be observed without a hand. `Mode::probe` reports the other
        // half — whether the surface could be armed — and both go in one line under
        // `IPOD_TRACKPAD=1`. See `Mode::probe` for why a zero here would otherwise be unreadable.
        if verbose() && !self.probed.get() {
            self.probed.set(true);
            note(&format!("monitor is live — first event is NSEventType {}", ty.0));
            self.mode.probe();
        }

        // ── The keys, before anything else can claim them ──
        //
        // Ahead of the responder chain deliberately: `Esc` has to work whatever has focus and
        // whatever is open, because a person whose cursor has stopped moving needs one key to work
        // and does not care which control believes it owns the keyboard.
        if ty == NSEventType::KeyDown {
            let code = event.keyCode();
            if code == KEY_T && is_chord(event.modifierFlags()) {
                self.swallowed.set(KEY_T);
                self.mode.toggle();
                return std::ptr::null_mut();
            }
            if code == KEY_ESC && self.mode.engaged() {
                self.swallowed.set(KEY_ESC);
                self.mode.leave(Left::Esc);
                return std::ptr::null_mut();
            }
            return pass;
        }
        if ty == NSEventType::KeyUp {
            // Only the up of a down this swallowed. Half a keystroke reaching Slint is how a key
            // comes to be held down for ever — §16.8's stuck finger, on the keyboard.
            if self.swallowed.get() != 0 && event.keyCode() == self.swallowed.get() {
                self.swallowed.set(0);
                return std::ptr::null_mut();
            }
            return pass;
        }

        if !self.mode.engaged() {
            return pass;
        }

        match ty {
            NSEventType::LeftMouseDown => {
                self.mode.click(true);
                std::ptr::null_mut()
            }
            NSEventType::LeftMouseUp => {
                self.mode.click(false);
                std::ptr::null_mut()
            }
            // A drag is a click that has not let go yet: it belongs to the wheel, not to whatever
            // the frozen cursor is sitting on.
            NSEventType::LeftMouseDragged => std::ptr::null_mut(),
            // **The second way out, and the one that cannot be mistaken for a gesture.**
            //
            // `NSEvent.stage` is 1 for an ordinary click and 2 for a force click on Force Touch
            // hardware. Stage 2 is deliberately mapped to **nothing on the iPod**: a click wheel
            // has no second pressure stage, and giving it one would be inventing input the part
            // does not have — the rule `GUI.md` §21.5 keeps when it refuses to let MENU+SELECT
            // restart the machine. Which is exactly what makes it the right *emulator* control:
            // with the pointer taken, the way out has to be something the wheel can never mean.
            //
            // A force click passes through stage 1 on the way, so the ordinary click it began as
            // has already pressed whatever was under the finger. `Mode::leave` takes that up first
            // — `Pad::empty` releases the held press before the contact — so the cost is one brief
            // button press on the way out, and never a button left down.
            NSEventType::Pressure => {
                if event.stage() >= 2 {
                    self.mode.leave(Left::Force);
                }
                std::ptr::null_mut()
            }
            // **Two fingers would otherwise turn the wheel twice.** Slint delivers a scroll to
            // `wheel::Finger::scrolled` over the drawn ring, and the pad is already driving the
            // same finger absolutely. One surface, one contact.
            NSEventType::ScrollWheel => std::ptr::null_mut(),
            NSEventType::Gesture
            | NSEventType::Magnify
            | NSEventType::Swipe
            | NSEventType::Rotate
            | NSEventType::BeginGesture
            | NSEventType::EndGesture
            | NSEventType::SmartMagnify => {
                self.touches(event);
                std::ptr::null_mut()
            }
            _ => pass,
        }
    }
}

/// **The wiring, and the monitor's life.** Dropping this leaves the mode and removes the monitor,
/// which is what makes the cursor come back on any exit the process actually reaches — the window
/// closing, `Wiring` going, a panic unwinding through `main`.
pub struct Handle {
    plumbing: Rc<Plumbing>,
    monitor: Retained<AnyObject>,
}

impl Handle {
    /// The chord, reachable without one — so a control or a menu item can be given this later
    /// without anything else changing. There is no matching `release`: the ways *out* are `Esc`,
    /// the foreground going, and this struct being dropped, and all three are already wired.
    #[allow(dead_code)] // retired when: §21 draws a control for the mode — today the chord is the only way in and it is read inside the monitor, which is what makes it work with any focus
    pub fn toggle(&self) {
        self.plumbing.mode.toggle();
    }

    /// **The `WATCH` tick, for `main.rs`'s timer to drive.**
    ///
    /// A `slint::Timer` is the toolkit and `AGENTS.md` §9 puts the toolkit in `main.rs`, so the
    /// mode owns the *question* and the window owns the clock. It is free to run while the mode is
    /// off — `Mode::watch` returns on the first comparison — which is why one always-on timer is
    /// simpler and cheaper than one started and stopped per engage.
    pub fn ticker(&self) -> Rc<dyn Fn()> {
        let mode = self.plumbing.mode.clone();
        Rc::new(move || mode.watch())
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        self.plumbing.mode.leave(Left::Quit);
        // SAFETY: this is the object `addLocalMonitorForEventsMatchingMask:handler:` returned and
        // it has been removed from nowhere else — `Handle` is the only owner and it is not `Clone`.
        unsafe { NSEvent::removeMonitor(&self.monitor) };
    }
}

/// Install the adapter: one [`Source`], one [`Detents`], and the monitor that feeds them.
///
/// `None` if AppKit refuses a monitor, which it does not do in practice — and which is reported
/// rather than assumed away, because a monitor that is not there is a mode that reports nothing,
/// and *nothing* is the reading `AGENTS.md` §6 says never to believe on its own.
pub fn install(reach: View, act: Rc<dyn Fn(Act)>) -> Option<Handle> {
    let source = NsTouch { reach, view: RefCell::new(None) };
    let plumbing = Rc::new(Plumbing {
        mode: Mode::new(Box::new(source), act),
        tracked: Cell::new(0),
        swallowed: Cell::new(0),
        probed: Cell::new(false),
    });

    let weak: Weak<Plumbing> = Rc::downgrade(&plumbing);
    let block = RcBlock::new(move |ev: NonNull<NSEvent>| -> *mut NSEvent {
        match weak.upgrade() {
            Some(p) => p.dispatch(ev),
            None => ev.as_ptr(),
        }
    });

    // The keys always; the rest only ever looked at while the mode is on, and refused on one
    // comparison while it is not.
    let mask = NSEventMask::KeyDown
        | NSEventMask::KeyUp
        | NSEventMask::LeftMouseDown
        | NSEventMask::LeftMouseUp
        | NSEventMask::LeftMouseDragged
        | NSEventMask::Pressure
        | NSEventMask::ScrollWheel
        | NSEventMask::Gesture
        | NSEventMask::Magnify
        | NSEventMask::Swipe
        | NSEventMask::Rotate
        | NSEventMask::BeginGesture
        | NSEventMask::EndGesture
        | NSEventMask::SmartMagnify;

    // SAFETY: the block is `'static` — it holds a `Weak`, nothing borrowed — and AppKit copies it.
    // The returned object is the monitor token, owned by `Handle` and removed exactly once.
    let monitor = unsafe { NSEvent::addLocalMonitorForEventsMatchingMask_handler(mask, &block) }?;
    note("local monitor installed — ctrl-cmd-T engages, esc releases");
    Some(Handle { plumbing, monitor })
}

// ── The two conversions, which are this file's whole reason for existing ─────────────────────────

/// `NSTouch.deviceSize` — points at 72 to the inch — as the core's millimetres.
///
/// `None` for a size that cannot be a rectangle. It is read off a touch on every event rather than
/// remembered, because an external Magic Trackpad is a different size from the built-in one and a
/// person may be using either.
fn surface_of(size: objc2_foundation::NSSize) -> Option<Surface> {
    if !size.width.is_finite() || !size.height.is_finite() || size.width <= 0.0 || size.height <= 0.0
    {
        return None;
    }
    Some(Surface {
        w: (size.width * MM_PER_POINT) as f32,
        h: (size.height * MM_PER_POINT) as f32,
    })
}

/// **`normalizedPosition` → the core's contact.** The one conversion that decides whether the wheel
/// runs forwards, and the reason the core is given millimetres rather than a normalised pair.
///
/// `normalizedPosition` is 0…1 with its origin at the **lower-left**, y **up**. Every angle in
/// `wheel.rs` is y **down**. The minus below is the whole of that, and getting it wrong runs the
/// wheel backwards with no compiler error.
fn contact_of(nx: f64, ny: f64, s: Surface, phase: Phase) -> Option<Contact> {
    if !nx.is_finite() || !ny.is_finite() {
        return None;
    }
    Some(Contact {
        x: ((nx - 0.5) * f64::from(s.w)) as f32,
        y: (-(ny - 0.5) * f64::from(s.h)) as f32,
        phase,
    })
}

/// AppKit's phase bits as the core's closed set. `NSTouchPhase` is a bitflags type and a touch
/// carries exactly one of them; anything unrecognised is treated as the hand having gone, which is
/// the safe direction — a wheel with no finger on it is a state the part can be in.
fn phase_of(p: NSTouchPhase) -> Phase {
    if p.contains(NSTouchPhase::Began) {
        Phase::Began
    } else if p.contains(NSTouchPhase::Moved) {
        Phase::Moved
    } else if p.contains(NSTouchPhase::Stationary) {
        Phase::Stationary
    } else if p.contains(NSTouchPhase::Cancelled) {
        Phase::Cancelled
    } else {
        Phase::Ended
    }
}

/// The address of a touch's identity object — stable for the life of that touch, because the
/// `NSTouch` holds it, and deliberately not compared across gestures.
fn ident(t: &NSTouch) -> usize {
    let id = t.identity();
    Retained::as_ptr(&id) as usize
}

/// ⌃⌘ exactly — not ⌃⌥⌘, not ⇧⌃⌘. A chord this program has not defined must not engage a mode.
fn is_chord(flags: NSEventModifierFlags) -> bool {
    let f = flags & NSEventModifierFlags::DeviceIndependentFlagsMask;
    f.contains(NSEventModifierFlags::Control | NSEventModifierFlags::Command)
        && !f.intersects(NSEventModifierFlags::Option | NSEventModifierFlags::Shift)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wheel;
    use objc2_foundation::NSSize;

    /// This machine's pad, as the spike measured it: `device size (points): NSSize { width: 342.99,
    /// height: 209.76 }`, across 891 touch events.
    const PAD: NSSize = NSSize { width: 342.99, height: 209.76 };

    /// **Points at 72 to the inch are millimetres you can measure.** 342.99 pt is 12.1 cm, which is
    /// the pad on this machine, and it is what makes the core's unit honest rather than a name.
    #[test]
    fn device_size_in_points_is_millimetres_at_seventy_two_to_the_inch() {
        let s = surface_of(PAD).expect("a pad with a size");
        assert!((s.w - 121.0).abs() < 0.1, "{} mm across", s.w);
        assert!((s.h - 74.0).abs() < 0.1, "{} mm down", s.h);
        // A ratio the core relies on for its circle, preserved by the conversion.
        assert!(((s.w / s.h) - (PAD.width / PAD.height) as f32).abs() < 1e-4);
    }

    /// **The y flip, and it is why this test exists at all.** `normalizedPosition` grows upwards;
    /// every angle in `wheel.rs` grows downwards. The top of the pad is twelve o'clock and must be
    /// wheel position 0.
    ///
    /// **How to make it go red:** drop the minus in [`contact_of`]'s `y`. The top of the pad becomes
    /// 48 and the bottom 0 — the wheel runs backwards, and no compiler notices.
    #[test]
    fn the_top_of_the_pad_is_twelve_oclock_and_the_value_grows_clockwise() {
        let s = surface_of(PAD).expect("a pad with a size");
        let g = super::super::Geometry::of(s).expect("a wheel");
        let pos = |nx, ny| {
            let c = contact_of(nx, ny, s, Phase::Moved).expect("a contact");
            let (x, y) = g.unit(c);
            wheel::position_at_angle(x, y)
        };
        assert_eq!(pos(0.5, 1.0), 0, "the top edge, mid-width");
        assert_eq!(pos(1.0, 0.5), 24, "the right edge — a quarter turn clockwise");
        assert_eq!(pos(0.5, 0.0), 48, "the bottom edge");
        assert_eq!(pos(0.0, 0.5), 72, "the left edge");
    }

    /// **The aspect correction, from the platform's own numbers.** 96 evenly spaced angles on a
    /// circle drawn in *physical* space must come back as 96 different clicks after the conversion.
    ///
    /// **How to make it go red:** take the `* s.w` / `* s.h` out of [`contact_of`] and hand the
    /// normalised offsets straight through. On this 1.635:1 pad the sweep visits 14 positions twice
    /// and misses 14 — an ellipse, and clicks-per-degree a function of angle.
    #[test]
    fn a_circle_traced_on_the_real_pad_visits_every_click_exactly_once() {
        let s = surface_of(PAD).expect("a pad with a size");
        let g = super::super::Geometry::of(s).expect("a wheel");
        let mut seen = [0u32; 96];
        for i in 0..96 {
            let theta = f64::from(i) / 96.0 * std::f64::consts::TAU;
            // A circle of 0.8 of the half-short axis, expressed the way the pad reports one.
            let r = 0.8 * PAD.height / 2.0;
            let c = contact_of(
                0.5 + r * theta.sin() / PAD.width,
                0.5 + r * theta.cos() / PAD.height,
                s,
                Phase::Moved,
            )
            .expect("a contact");
            let (x, y) = g.unit(c);
            seen[wheel::position_at_angle(x, y) as usize] += 1;
        }
        assert!(seen.iter().all(|&n| n == 1), "not a bijection: {seen:?}");
    }

    /// A size or a position that cannot be one answers `None` rather than an infinity. Both are
    /// read off a touch on every event, so both are asked at the pad's own rate and must never
    /// panic or divide by zero.
    #[test]
    fn a_pad_with_no_size_and_a_position_that_is_not_one_are_refused() {
        assert!(surface_of(NSSize { width: 0.0, height: 209.76 }).is_none());
        assert!(surface_of(NSSize { width: 342.99, height: -1.0 }).is_none());
        assert!(surface_of(NSSize { width: f64::NAN, height: 209.76 }).is_none());
        let s = surface_of(PAD).expect("a pad with a size");
        assert!(contact_of(f64::NAN, 0.5, s, Phase::Moved).is_none());
        assert!(contact_of(0.5, f64::INFINITY, s, Phase::Moved).is_none());
    }

    /// AppKit's five phases as the core's five, and **anything unrecognised is the hand having
    /// gone** — the safe direction, because a wheel with no finger on it is a state the part can be
    /// in and a finger that never lifts is not.
    #[test]
    fn every_touch_phase_maps_to_one_the_core_knows() {
        assert_eq!(phase_of(NSTouchPhase::Began), Phase::Began);
        assert_eq!(phase_of(NSTouchPhase::Moved), Phase::Moved);
        assert_eq!(phase_of(NSTouchPhase::Stationary), Phase::Stationary);
        assert_eq!(phase_of(NSTouchPhase::Ended), Phase::Ended);
        assert_eq!(phase_of(NSTouchPhase::Cancelled), Phase::Cancelled);
        assert_eq!(phase_of(NSTouchPhase::empty()), Phase::Ended, "nothing set is nothing there");
        assert!(phase_of(NSTouchPhase::Began).touching());
        assert!(!phase_of(NSTouchPhase::Ended).touching());
    }

    /// **⌃⌘ exactly.** A chord this program has not defined must not engage a mode, and the one that
    /// nearly did is `⌃⌥⌘T` — three modifiers, of which a `contains` check alone would accept two.
    ///
    /// **How to make it go red:** drop the `!f.intersects(…)` in [`is_chord`]. `⌃⌥⌘T` and `⇧⌃⌘T`
    /// both start engaging the mode.
    #[test]
    fn only_control_and_command_together_are_the_chord() {
        use NSEventModifierFlags as M;
        assert!(is_chord(M::Control | M::Command));
        // The one that matters: a real ⌃⌘T also carries the numeric-pad/function bits on some
        // keyboards, and masking with `DeviceIndependentFlagsMask` is not enough on its own —
        // `Function` survives it. So the guard names what must be ABSENT rather than what the whole
        // word must equal.
        assert!(is_chord(M::Control | M::Command | M::Function));
        assert!(!is_chord(M::Control));
        assert!(!is_chord(M::Command));
        assert!(!is_chord(M::Control | M::Command | M::Option), "⌃⌥⌘ is not this chord");
        assert!(!is_chord(M::Control | M::Command | M::Shift), "⇧⌃⌘ is not this chord");
        assert!(!is_chord(M::empty()));
    }
}
