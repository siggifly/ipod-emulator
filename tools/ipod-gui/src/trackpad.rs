//! **A trackpad, as the click wheel.** `docs/GUI.md` §21.8.
//!
//! An iPod's wheel is a capacitive surface that reports an **absolute angular position** — the
//! streaming frame carries one `position` byte and one touched bit, and the firmware consumes
//! differences between successive frames. A trackpad is a capacitive surface that reports absolute
//! positions. They are the same part under different plastic, and this file is the arithmetic
//! between them and the mode that decides which of the two the pad is.
//!
//! **One core and two platform halves**, because the intent is every trackpad on every platform and
//! the second implementation has to cost a file rather than a rewrite. [`Source`] is the touch
//! half, [`Detents`] the feedback half, [`Frame`] is what an adapter pushes, and [`Mode`] is
//! everything neither of them should have to write twice. `trackpad/mac.rs` is the first
//! implementation of both and today the only one.
//!
//! **The core never sees a platform unit.** A [`Contact`] is millimetres from the centre of the
//! surface, x right and **y down**, and both conversions that get there belong to the adapter
//! because both are platform facts and both are one character from being wrong:
//!
//! * **Orientation.** macOS's `normalizedPosition` has its origin at the lower-left, y **up** — the
//!   opposite of every GUI toolkit and of `wheel::position_at_angle`. Linux evdev and Windows are
//!   y-down. Get it wrong and the wheel runs backwards, with no compiler error.
//! * **Aspect.** A pad is not square — this machine's is 342.99 × 209.76 pt, 1.635:1, and an
//!   external Magic Trackpad reports a different size, so it is read per contact and never assumed.
//!   Polar arithmetic in a normalised 0…1 space puts the wheel on an **ellipse** and makes
//!   clicks-per-degree a function of angle.
//!
//! What is *chosen* rather than derived is [`Geometry`]: the wheel's radius is half the surface's
//! short axis, so the ring is the largest circle its height allows, and the proportions inside it
//! are the drawn wheel's own. The one departure is the outer edge — see there.
//!
//! **Nothing here names a toolkit type, and that is `AGENTS.md` §9 rather than taste.** The rule is
//! what makes the window replaceable, and this module is exactly the kind of thing that would erode
//! it: reaching a native view handle genuinely needs the windowing layer, and a mode that must
//! notice the window losing the foreground genuinely needs a clock. Both sit in `main.rs`.
//!
//! So the seams are drawn to keep them there. [`Mode::watch`] is the *question* and `main.rs` owns
//! the timer that asks it; `mac::View` is an already-resolved `NSView` and `main.rs` does the
//! translation. A second window implementation supplies those two things and changes nothing in
//! here — which is the portability the rule exists for, made concrete rather than asserted.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use crate::wheel::{Hit, WheelRing};

// ── The contract every platform's adapter meets ─────────────────────────────────────────────────

/// The touch surface, **in millimetres**.
///
/// Millimetres are honest here rather than aspirational: macOS reports `deviceSize` in points at 72
/// to the inch, so this machine's 342.99 × 209.76 pt is 121.0 × 74.0 mm — the pad you can put a
/// ruler on. Linux publishes `ABS_MT_POSITION_*` with a resolution in units per millimetre. Every
/// platform can produce this, which is what makes it the right thing to agree on.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Surface {
    pub w: f32,
    pub h: f32,
}

/// What a contact is doing.
///
/// **The lifecycle is part of the contract**, and that is the one thing this input has that a
/// scroll wheel does not: `wheel::Finger`'s scroll route has to *guess* when the hand left, with a
/// 300 ms timer, precisely because `PointerScrollEvent` carries a delta and nothing else. A finger
/// lifting is an event here, so nothing guesses.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Phase {
    Began,
    Moved,
    Stationary,
    Ended,
    Cancelled,
}

impl Phase {
    /// Whether the hand is still on the surface. `Ended` and `Cancelled` are reported in the same
    /// frame as the contacts that are still down, so this is what says which is which.
    pub fn touching(self) -> bool {
        matches!(self, Phase::Began | Phase::Moved | Phase::Stationary)
    }
}

/// One contact: **millimetres from the centre of the surface, x right and y down**.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Contact {
    pub x: f32,
    pub y: f32,
    pub phase: Phase,
}

/// **What an adapter pushes**: how big the surface is, and the one contact that owns the wheel.
///
/// `None` is the surface with nothing on it. The *adapter* picks the primary rather than the core,
/// because which of two fingers is the wheel's is a question about a platform's identity handles —
/// and because the core has one finger by construction, for the reason [`Pad`] gives.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Frame {
    pub surface: Surface,
    pub contact: Option<Contact>,
}

/// **The touch half of §21.8, as any platform must supply it.**
///
/// Delivery is not on this trait and deliberately: contacts are *pushed* from whatever event
/// mechanism the platform has, and an adapter owns its own. What is here is everything the core
/// has to be able to ask for and cannot do itself.
pub trait Source {
    /// A name, for the diagnostic line and the capability report. No full stop.
    fn describe(&self) -> &'static str;

    /// **Make contacts start arriving.** `false` while the platform is not ready — on macOS the
    /// property that has to be set lives on an `NSView` that does not exist until the event loop is
    /// running. It is never assumed: with it unset macOS delivers nothing at all, by any route.
    fn arm(&self) -> bool;

    /// Take the pointer, so the cursor stops following the finger, or give it back. `false` if it
    /// could not be taken — in which case the mode does not engage, because a mode that thinks it
    /// has the pointer and has not is how somebody's cursor goes missing.
    fn grab(&self, take: bool) -> bool;

    /// Whether the window this mode belongs to still owns the foreground.
    fn frontmost(&self) -> bool;

    /// Say, wherever the platform has somewhere to say it, that the mode is on. `""` clears it.
    fn announce(&self, note: &str);
}

/// **What tells a finger that a detent went past.**
///
/// A list rather than one implementation, and the reason is not portability. **A real 5G clicks
/// through a piezo — it makes a *sound*.** The linear actuator is what modders fit in its place, so
/// an audible click is the *faithful* behaviour and haptics is the enhancement. Adding the piezo's
/// click is one [`add_sink`] and nothing here changes; that click should come from the emulated
/// part rather than from the window counting steps, which is why nothing in this crate synthesises
/// audio.
pub trait Detents {
    /// A name, for the capability report. No full stop.
    fn describe(&self) -> &'static str;

    /// **Whether this build can produce one at all** — a stated capability rather than silence.
    ///
    /// It is a claim about the *build* and never about the hardware. macOS has no API that says
    /// whether a pad has an actuator and none that reports a failed pulse, so a pre-2015 trackpad
    /// answers `true` here and stays quiet. §21.8 is the only place that says so, and that gap is
    /// exactly what a second sink closes.
    fn present(&self) -> bool;

    /// Fire one. Already rate-limited by [`Feedback`]; this is called at most once per contact
    /// frame and never within [`TICK_FLOOR`] of the last.
    fn detent(&self);
}

// ── What this build can do ──────────────────────────────────────────────────────────────────────

/// Whether absolute finger positions are readable here, and why not when they are not.
//
// **Exactly one variant is constructed per target** — [`support`] is a `const fn` of the target,
// not of the hardware — so the other is always "never constructed" in any single build. It is the
// other platforms' answer, and the enum is the closed set of them; dropping it would mean the
// honest gap could not be stated at all. Same shape and same reason as `client_height::WorkArea`.
#[allow(dead_code)] // retired when: a second platform publishes absolute touch positions to an ordinary application — the enum is the closed set, and dropping the unused arm would mean the gap could not be stated
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Support {
    /// macOS: `NSTouch` on an **indirect** device, which is what a trackpad is.
    Indirect,
    /// Everywhere else. Neither Windows nor X11 nor Wayland publishes per-finger absolute positions
    /// to an ordinary application: what arrives is a synthesised pointer and a scroll delta, which
    /// is the route `wheel::Finger::scrolled` already takes. A second [`Source`] is what changes
    /// this, and the rest of this file is written so that it is the only thing that has to.
    Unpublished,
}

impl Support {
    /// The one sentence the design and the diagnostic line both print. No full stop.
    pub fn describe(self) -> &'static str {
        match self {
            Support::Indirect => {
                "the trackpad reports each finger's absolute position, so it can be the wheel — \
                 macOS NSTouch, indirect devices"
            }
            Support::Unpublished => {
                "no absolute finger position is published on this platform, so a trackpad is a \
                 pointer and a scroll delta and nothing else"
            }
        }
    }

    pub fn available(self) -> bool {
        matches!(self, Support::Indirect)
    }
}

/// Which of the two this build can do. A `const fn` of the target, not of the hardware.
pub const fn support() -> Support {
    #[cfg(target_os = "macos")]
    {
        Support::Indirect
    }
    #[cfg(not(target_os = "macos"))]
    {
        Support::Unpublished
    }
}

// ── The wheel drawn on the surface ──────────────────────────────────────────────────────────────

/// **Where the centre button ends and the ring begins, as a fraction of half the surface's short
/// axis. 0.34 — measured rather than borrowed.**
///
/// The first version of this used the *drawn* wheel's own ratio, 0.465, on the reasoning that the
/// proportions of a 5G's 13 mm button in its 28 mm wheel should carry across. Replaying the spike's
/// capture — 891 real contacts from the hand that confirmed this feels right — says they must not:
///
/// ```text
/// natural circling radius, 491 contacts: min 12.6 mm, median 18.7 mm, max 31.4 mm
///   centre 0.465 (17.2 mm): 226 of 491 still on the ring, 265 swallowed by the button
///   centre 0.340 (12.6 mm): 491 of 491 still on the ring
/// ```
///
/// **The pad is 2.6 times the wheel**, so a proportional centre lands on the *median* of the circle
/// a hand actually draws, and more than half of a gesture stops turning anything. A ratio is the
/// right thing to copy between two objects of the same size and the wrong thing between two of
/// different sizes; what has to be preserved is that the button sits well inside the band a finger
/// sweeps, and 12.6 mm does that with the whole observed range to spare. In absolute terms it is
/// still a **25 mm target**, wider than the real device's own centre button.
///
/// **And there is no moulding.** On the drawn wheel a narrow dead band sits between the button and
/// the ring, because on the real part that gap is the bezel. On a rectangle of glass there is
/// nothing there to model, and a band that swallowed 69 of those 491 contacts would be a bezel
/// invented for a surface that has none.
pub const CENTRE: f32 = 0.34;

/// **The wheel a surface of this size is**, and the scale from millimetres into it.
///
/// The radius is half the surface's **short** axis, so the ring is the largest circle its height
/// allows. Inside it, [`CENTRE`]; outside it, everything — because **a rectangle of glass has no
/// case.** On the drawing, past the outer edge is the iPod's case and a press there is a press on
/// the case; on a trackpad there is nothing there but more trackpad. Left at 1.0 the ring would be
/// dead over most of a 1.635:1 pad — a finger near the left edge sits at r ≈ 1.6 — so `outer` is
/// the surface's half-diagonal and every corner is on the ring.
#[derive(Clone, Copy, Debug)]
pub struct Geometry {
    ring: WheelRing,
    /// Millimetres per unit of this surface's wheel radius. Half the short axis.
    radius: f32,
}

impl Geometry {
    /// `None` for a surface that cannot be a rectangle. The size is read off a contact rather than
    /// remembered, so this is asked per frame and must never panic or divide by zero.
    pub fn of(s: Surface) -> Option<Geometry> {
        if !s.w.is_finite() || !s.h.is_finite() || s.w <= 0.0 || s.h <= 0.0 {
            return None;
        }
        let short = s.w.min(s.h);
        let mut ring = WheelRing::new(0.0, 0.0, 1.0);
        ring.select = CENTRE;
        // `inner == select`, so the two zones meet with nothing between them: `WheelRing::hit`
        // tests `r < inner` only after the `Select` arm has taken everything at or below `select`,
        // which makes the gap exactly zero rather than nearly zero.
        ring.inner = CENTRE;
        // Half-diagonal over half-short-axis: 1.917 on this machine's pad, √2 on a square one.
        ring.outer = s.w.hypot(s.h) / short;
        Some(Geometry { ring, radius: short / 2.0 })
    }

    pub fn ring(&self) -> &WheelRing {
        &self.ring
    }

    /// Millimetres from the centre to **units of this surface's own wheel radius**. Both spaces are
    /// y-down; the adapter did the flip.
    pub fn unit(&self, c: Contact) -> (f32, f32) {
        (c.x / self.radius, c.y / self.radius)
    }
}

/// **Where the drawn wheel's ring is, in its own units.** Halfway between the `inner` and `outer`
/// of `WheelRing::new(_, _, 1.0)` — the same 0.76 `WheelRing::point_at` puts a label at. Read off
/// the type rather than written down, so the two cannot drift.
fn drawn_midline() -> f32 {
    let r = WheelRing::new(0.0, 0.0, 1.0);
    (r.inner + r.outer) * 0.5
}

/// **The contact's angle, expressed as a point on the *drawn* wheel's ring.**
///
/// This is the join between two geometries that are deliberately not the same one, and getting it
/// wrong is a defect that no test of either geometry alone can see. The pad's ring runs from
/// [`CENTRE`] out to the half-diagonal; the drawn wheel's runs from 0.52 to 1.0, and `main.rs`
/// hands what comes out of here straight to `wheel::Finger::touched(&unit_ring(), …)`. A contact in
/// the **corner** of the pad sits at r ≈ 1.6, which the drawn ring answers `Hit::None` for — so the
/// first version of this emitted the raw unit coordinates, and every gesture beginning outside the
/// pad's inscribed circle silently did nothing while `every_corner_of_the_surface_is_on_the_ring`
/// went on passing. Found by replaying a real capture, which is the only instrument that could see
/// it.
///
/// **Only the angle survives, and only the angle ever mattered**: `Finger::moved` reads nothing
/// else, and the radius a finger happens to be at is a fact about the pad rather than about the
/// wheel. Never reached for a contact at the exact centre, because [`Pad`] emits nothing there.
fn on_drawn_ring(x: f32, y: f32) -> (f32, f32) {
    let len = (x * x + y * y).sqrt();
    if len <= f32::EPSILON {
        // Unreachable through `Pad`, and answered rather than divided by: twelve o'clock is what
        // `wheel::position_at_angle` gives a centred pointer anyway.
        return (0.0, -drawn_midline());
    }
    let k = drawn_midline() / len;
    (x * k, y * k)
}

// ── The actuator, and what it may be asked for ──────────────────────────────────────────────────

/// **The shortest gap between two detents. 6 ms.**
///
/// Apple's own `NSAlignmentFeedbackFilter` withholds feedback when the thing being aligned moves
/// too fast, which is the platform saying out loud that the actuator has a rate. There is no
/// published figure, so this is the one the spike used — 6 ms, ~167 Hz. It sits above the ~39
/// detents a second an ordinary spin produced and *below* the pad's own 124 Hz mean sample rate,
/// which is what makes it **never bite at any speed a hand reaches**: frames arrive about 8 ms
/// apart, so two consecutive ones are never inside the floor and the only thing ever coalesced is a
/// second detent inside a single frame. 32 of 278 were, at ordinary speed.
pub const TICK_FLOOR: Duration = Duration::from_millis(6);

/// The rate limiter, and the counters that say what it did.
///
/// **At most one pulse per frame**, which is not a second rule but the same one: within one event
/// the clock does not advance, so the floor above already refuses the second. Saying so here is
/// cheaper than a loop that can only ever run once, and it is what the spike measured rather than
/// what its code implied.
#[derive(Default)]
pub struct Ticks {
    last: Option<Instant>,
    fired: u64,
    coalesced: u64,
}

impl Ticks {
    /// Whether to actuate, for a frame that produced `clicks` detents.
    pub fn due(&mut self, now: Instant, clicks: u32) -> bool {
        if clicks == 0 {
            return false;
        }
        let ready = match self.last {
            Some(t) => now.saturating_duration_since(t) >= TICK_FLOOR,
            None => true,
        };
        if !ready {
            self.coalesced += u64::from(clicks);
            return false;
        }
        self.last = Some(now);
        self.fired += 1;
        self.coalesced += u64::from(clicks - 1);
        true
    }

    /// Pulses fired, and detents that went unfelt.
    pub fn counts(&self) -> (u64, u64) {
        (self.fired, self.coalesced)
    }
}

/// **Every sink, behind the one rate limit.** Adding the piezo's audible click is `add`.
#[derive(Default)]
pub struct Feedback {
    ticks: Ticks,
    sinks: Vec<Box<dyn Detents>>,
}

impl Feedback {
    pub fn add(&mut self, sink: Box<dyn Detents>) {
        self.sinks.push(sink);
    }

    /// One detent for a frame the machine took `clicks` steps from. Rate-limited once, for all of
    /// them: two sinks firing on different schedules would be two clicks out of step.
    pub fn fire(&mut self, clicks: u32) {
        if !self.ticks.due(Instant::now(), clicks) {
            return;
        }
        for s in &self.sinks {
            if s.present() {
                s.detent();
            }
        }
    }

    /// **What will actually be felt or heard**, stated rather than assumed. `"nothing"` when there
    /// is no sink, which is the honest answer on a build with no platform half.
    pub fn describe(&self) -> String {
        let names: Vec<&str> = self.sinks.iter().filter(|s| s.present()).map(|s| s.describe()).collect();
        if names.is_empty() {
            "nothing — a detent is felt by no one on this build".to_string()
        } else {
            names.join(" and ")
        }
    }

    pub fn counts(&self) -> (u64, u64) {
        self.ticks.counts()
    }
}

thread_local! {
    /// **One set of sinks for the process**, because there is one hand on one surface.
    ///
    /// Reached by [`detent`] from `main.rs`'s own handler rather than owned by [`Mode`], and that
    /// is the whole reason it is a thread-local: the handler is built before the mode exists, and
    /// it is the handler — not the pad — that knows how many steps the *machine* took. With no
    /// machine on the bench the wheel does not turn, and an actuator clicking against an empty
    /// bench is a lie told through somebody's fingertip.
    static FEEDBACK: RefCell<Feedback> = RefCell::new(Feedback::default());
}

/// Register a detent sink. Called once per sink at install; a second one is one more call.
pub fn add_sink(sink: Box<dyn Detents>) {
    FEEDBACK.with(|f| f.borrow_mut().add(sink));
}

/// Ask for a detent, for a frame the machine took `clicks` steps from.
pub fn detent(clicks: u32) {
    if clicks == 0 {
        return;
    }
    FEEDBACK.with(|f| f.borrow_mut().fire(clicks));
}

/// What a detent will be felt or heard as, and how many have been.
pub fn feedback_state() -> (String, u64, u64) {
    FEEDBACK.with(|f| {
        let f = f.borrow();
        let (fired, coalesced) = f.counts();
        (f.describe(), fired, coalesced)
    })
}

// ── The finger on the surface ───────────────────────────────────────────────────────────────────

/// What the surface did, in the window's own terms. `main.rs` is the only thing that knows what a
/// machine is; this is everything it needs to be told.
//
// No `Eq`: two of the five carry `f32`s, and the only comparison this type is asked for is a test's
// `assert_eq!` against a literal. `Hit` has `Eq` and keeps it.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Act {
    /// A finger arrived on the ring, at these unit-radius coordinates. **Contact, not a press** —
    /// see `wheel::Finger::touched`.
    Down(f32, f32),
    /// It is still down, and it is there now.
    Moved(f32, f32),
    /// It left the ring, either by lifting or by moving into the centre.
    Up,
    /// The surface was **clicked** while the finger was over this control. A touch surface reports
    /// contact and pressure on different events, so this is the press the ring's four labels and
    /// the centre button want — and resting on a label is not one.
    Press(Hit),
    /// The click came up, on whatever it went down on.
    Release(Hit),
}

/// **The one finger this surface has**, and what a click on it is holding.
///
/// One finger because the emulated part has one: two touches on a 5G's wheel are not two positions,
/// and the streaming frame has one `position` byte and one touched bit to say so. This machine's
/// pad reported a maximum of one simultaneous touch across 891 events with resting touches refused;
/// where a second one does arrive, which is primary is the adapter's to decide.
#[derive(Default)]
pub struct Pad {
    /// The surface's wheel, from the last frame — a **click** carries no contact of its own, so the
    /// geometry it is resolved against has to be the one the finger last reported. Which is also
    /// the only correct answer, since it is the same surface.
    geom: Option<Geometry>,
    /// Where the finger was last, in unit-radius coordinates, so a click knows what it is on. Set
    /// by every frame including the ones that are not on the ring, because the centre button is
    /// exactly such a place and it is the one people click.
    at: Option<(f32, f32)>,
    /// Whether the wheel has a contact from this surface right now.
    on_ring: bool,
    /// What the click put down, so the release takes the same thing up. A release for a press that
    /// was never sent is `GUI.md` §7.4's stuck finger, in the other direction.
    held: Option<Hit>,
}

impl Pad {
    /// One frame from the surface. Answers the edges it crossed, if any.
    pub fn frame(&mut self, f: Frame) -> Vec<Act> {
        let Some(g) = Geometry::of(f.surface) else {
            return Vec::new();
        };
        self.geom = Some(g);
        // An ended or cancelled contact is the hand leaving, and it is an *event* rather than a
        // timeout — which is the one thing this input has that a scroll wheel does not.
        let Some(c) = f.contact.filter(|c| c.phase.touching()) else {
            return self.empty();
        };
        let (x, y) = g.unit(c);
        if !x.is_finite() || !y.is_finite() {
            return Vec::new();
        }
        // **What is remembered is the pad's own coordinate and what is emitted is the drawn
        // wheel's.** A click is resolved against `self.geom`, which is this surface's ring, so
        // `at` has to stay in that space; everything downstream of an `Act` is the drawn wheel's,
        // so that is what goes out. See [`on_drawn_ring`] for the defect this separation fixes.
        let was = self.at.replace((x, y));
        match g.ring().hit(x, y) {
            Hit::Ring(_) | Hit::RingButton(_, _) => {
                let (dx, dy) = on_drawn_ring(x, y);
                if self.on_ring {
                    // A frame that did not move is not an edge. A surface sends stationary contacts
                    // at the full rate and every one of them would otherwise be a `Moved`.
                    if was == Some((x, y)) {
                        Vec::new()
                    } else {
                        vec![Act::Moved(dx, dy)]
                    }
                } else {
                    self.on_ring = true;
                    vec![Act::Down(dx, dy)]
                }
            }
            // The centre is not the ring, so the wheel's contact ends there — and the position is
            // still remembered, because that is where a click lands.
            Hit::Select | Hit::None => self.leave_ring(),
        }
    }

    /// Nothing is on the surface. Ends the click first and the contact second, which is the
    /// hardware's order: a frame with a button set and no finger on the wheel is a state the part
    /// cannot be in.
    pub fn empty(&mut self) -> Vec<Act> {
        let mut out = Vec::new();
        if let Some(h) = self.held.take() {
            out.push(Act::Release(h));
        }
        out.extend(self.leave_ring());
        self.at = None;
        out
    }

    /// The surface was pressed. The control under the finger goes down; **the contact does not move
    /// and does not end**, because a finger that presses harder has not gone anywhere.
    ///
    /// **Which control is decided by where the finger is, and that is the hardware rather than an
    /// approximation.** A 5G's wheel is not one button: four dome switches sit under the ring at
    /// the cardinal points — MENU at twelve, NEXT at three, PLAY at six, PREV at nine — with the
    /// centre button separate underneath. So the finger selects the dome and the click presses it,
    /// which is mechanically what the part does.
    ///
    /// **And between two domes there is no switch, so a click there presses nothing.** They are
    /// four discrete domes rather than a continuous ring; a press at 45° is not a button on real
    /// hardware and must not become one here. That rule is `wheel::quadrant`'s and predates this
    /// file: each label owns the 16 clicks centred on its quadrant — two thirds of the quarter,
    /// because the membrane under a label is one switch across most of it — leaving a dead band
    /// either side. **Nothing is invented**: no fifth position, and no nearest-dome rounding across
    /// the band.
    pub fn clicked(&mut self) -> Vec<Act> {
        if self.held.is_some() {
            return Vec::new();
        }
        let (Some(g), Some((x, y))) = (self.geom, self.at) else {
            return Vec::new();
        };
        let hit = g.ring().hit(x, y);
        // Bare ring is not a control. On the real part the membrane under a label is one switch
        // across most of its quarter, and the band between two of them is under nothing.
        if matches!(hit, Hit::None | Hit::Ring(_)) {
            return Vec::new();
        }
        self.held = Some(hit);
        vec![Act::Press(hit)]
    }

    /// The surface came up. Only for a press this went down for.
    pub fn unclicked(&mut self) -> Vec<Act> {
        match self.held.take() {
            Some(h) => vec![Act::Release(h)],
            None => Vec::new(),
        }
    }

    /// Whether a contact is on the ring right now — the diagnostic line's, and the tests'.
    pub fn on_ring(&self) -> bool {
        self.on_ring
    }

    fn leave_ring(&mut self) -> Vec<Act> {
        if self.on_ring {
            self.on_ring = false;
            vec![Act::Up]
        } else {
            Vec::new()
        }
    }
}

// ── The mode ────────────────────────────────────────────────────────────────────────────────────

/// Why the trackpad stopped being the wheel. Every one of these gives the cursor back.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Left {
    /// `Esc`, which is the one that must always work.
    Esc,
    /// The chord again.
    Chord,
    /// A **force click** — the deliberate second escape. See [`Left::Force`]'s note below.
    ///
    /// It is an *emulator* control and never an iPod one, and that is exactly why it can be one: a
    /// click wheel has no second pressure stage, so a force click cannot be mistaken for anything
    /// the emulated part does. Inventing a wheel input out of it would be the mistake `GUI.md`
    /// §21.5 refuses for MENU+SELECT; using it for the emulator is the same rule kept.
    Force,
    /// The window stopped owning the foreground.
    Focus,
    /// The window went, or the process is leaving.
    Quit,
    /// It was engaged and no contact ever arrived. See [`ARM_GRACE`].
    NoTrackpad,
}

impl Left {
    pub fn describe(self) -> &'static str {
        match self {
            Left::Esc => "esc",
            Left::Chord => "the chord again",
            Left::Force => "a force click",
            Left::Focus => "the window is not frontmost",
            Left::Quit => "the window is going",
            Left::NoTrackpad => "no trackpad reported a contact",
        }
    }
}

/// **How long a mode nothing has touched stays on. 3 s.**
///
/// The control that turns a zero into an observation, which `AGENTS.md` §6 is entirely about: a Mac
/// with no trackpad at all — a mini, an iMac, a laptop closed under an external mouse — will engage
/// this mode happily and then report nothing for ever, which reads as *broken* rather than as
/// *there is no trackpad here*. So a mode that has never, in the life of the process, seen a single
/// contact gives up after this and says which of the two it was. Once one has arrived the grace is
/// over for good: a person who engages the mode and then pauses to think is not a person without a
/// trackpad.
pub const ARM_GRACE: Duration = Duration::from_secs(3);

/// **How often the mode checks that it still owns the foreground. 200 ms.**
///
/// It cannot be an event: nothing is delivered to an application that is not frontmost, so the one
/// state that must end the mode is the one state that sends nothing. Fast enough that a person who
/// ⌘-tabs away and back does not catch it, and slow enough to be free — it is one question asked of
/// the [`Source`], and only while the mode is on.
pub const WATCH: Duration = Duration::from_millis(200);

/// The sentence the platform announces while the pad is the wheel. It names the way out, because a
/// person who cannot get their cursor back will force-quit and never open this program again.
pub const ANNOUNCE: &str = "the trackpad is the click wheel — esc gives the cursor back";

/// **The mode itself**, and the half of §21.8 that is the same on every platform.
pub struct Mode {
    source: Box<dyn Source>,
    act: Rc<dyn Fn(Act)>,
    engaged: Cell<bool>,
    pad: RefCell<Pad>,
    /// Whether a contact has **ever** arrived in this process. The control for [`ARM_GRACE`].
    seen: Cell<bool>,
    engaged_at: Cell<Option<Instant>>,
    frames: Cell<u64>,
}

impl Mode {
    pub fn new(source: Box<dyn Source>, act: Rc<dyn Fn(Act)>) -> Rc<Mode> {
        Rc::new(Mode {
            source,
            act,
            engaged: Cell::new(false),
            pad: RefCell::new(Pad::default()),
            seen: Cell::new(false),
            engaged_at: Cell::new(None),
            frames: Cell::new(0),
        })
    }

    pub fn engaged(&self) -> bool {
        self.engaged.get()
    }

    pub fn toggle(self: &Rc<Self>) {
        if self.engaged.get() {
            self.leave(Left::Chord);
        } else {
            self.engage();
        }
    }

    pub fn engage(self: &Rc<Self>) {
        if self.engaged.get() {
            return;
        }
        if !self.source.arm() {
            note("the surface could not be armed — there is no window yet");
            return;
        }
        // A mode that believes it has the pointer and has not is how somebody's cursor goes
        // missing, so a refusal leaves it off rather than on.
        if !self.source.grab(true) {
            note("the pointer could not be taken — mode not engaged");
            return;
        }
        self.engaged.set(true);
        self.engaged_at.set(Some(Instant::now()));
        self.source.announce(ANNOUNCE);
        note(&format!("engaged — {}, detents: {}", self.source.describe(), feedback_state().0));
    }

    /// **The way out, and every way out runs through here.**
    ///
    /// The order is the hardware's: the finger comes off the wheel *before* the pointer comes back,
    /// so the machine is never left with a contact — or a button — that nothing will lift.
    pub fn leave(&self, why: Left) {
        if !self.engaged.get() {
            return;
        }
        self.engaged.set(false);
        self.engaged_at.set(None);
        let acts = self.pad.borrow_mut().empty();
        for a in acts {
            (self.act)(a);
        }
        self.source.grab(false);
        self.source.announce("");
        let (_, fired, coalesced) = feedback_state();
        note(&format!(
            "released ({}) — {} frames, {fired} detents felt, {coalesced} coalesced",
            why.describe(),
            self.frames.get()
        ));
    }

    /// **The control that makes the instrument produce a non-zero**, and the reason it is not a
    /// test: if a person engages the mode and nothing happens, the first question is whether the
    /// surface was ever armed at all — and on macOS, an unarmed surface delivers *nothing*, by
    /// every route, which reads as a wheel that does not work rather than as a property that was
    /// never set. So under `IPOD_TRACKPAD=1` the adapter arms once, early, and says whether it
    /// could, before anybody has pressed anything.
    ///
    /// Arming is not engaging. Nothing is taken, nothing is announced, and the monitor goes on
    /// ignoring contacts until the chord; all this does is make them arrive.
    pub fn probe(&self) -> bool {
        let armed = self.source.arm();
        note(&format!(
            "{}, armed: {armed}, detents: {}",
            self.source.describe(),
            feedback_state().0
        ));
        armed
    }

    /// One frame from the adapter.
    pub fn frame(&self, f: Frame) {
        if f.contact.is_some_and(|c| c.phase.touching()) {
            self.seen.set(true);
            self.frames.set(self.frames.get() + 1);
        }
        let acts = self.pad.borrow_mut().frame(f);
        if verbose() && !acts.is_empty() {
            // The wheel position is the one number the acts do not already carry, and it is the one
            // a person reading this log is checking: it is what the machine will be stepped to.
            let pos = Geometry::of(f.surface).zip(f.contact).map(|(g, c)| {
                let (x, y) = g.unit(c);
                crate::wheel::position_at_angle(x, y)
            });
            eprintln!(
                "[trackpad] {f:?} on_ring={} pos={pos:?} -> {acts:?}",
                self.pad.borrow().on_ring()
            );
        }
        for a in acts {
            (self.act)(a);
        }
    }

    /// A click edge from the adapter.
    ///
    /// **Logged even when it did nothing**, unlike a contact frame, and for two reasons that are
    /// both about not being able to press this with a hand from here. A click on bare ring is
    /// *correctly* silent, so a silent log would leave *the click never arrived* and *the click
    /// arrived and pressed nothing* looking identical — which is the shape `AGENTS.md` §6 forbids.
    /// And it is the instrument that answers the open question about **tap-to-click**: a person who
    /// has it on and drags around the ring would see `click down` lines they did not make, and
    /// there is no other way to find that out without changing somebody's system settings.
    pub fn click(&self, down: bool) {
        let acts = if down {
            self.pad.borrow_mut().clicked()
        } else {
            self.pad.borrow_mut().unclicked()
        };
        if verbose() {
            eprintln!("[trackpad] click {} -> {acts:?}", if down { "down" } else { "up" });
        }
        for a in acts {
            (self.act)(a);
        }
    }

    /// **[`WATCH`]'s two questions, asked by the window's own timer**: does this window still own
    /// the foreground, and did a trackpad ever answer.
    ///
    /// The *timer* is deliberately not here. `AGENTS.md` §9 puts the toolkit in `main.rs`, and a
    /// `slint::Timer` is the toolkit; an input adapter has no business owning one. So this is the
    /// tick and `main.rs` is what arranges for it to be called — which is also why it is free to
    /// run while the mode is off, and returns on the first comparison when it is.
    pub fn watch(&self) {
        if !self.engaged.get() {
            return;
        }
        if !self.source.frontmost() {
            self.leave(Left::Focus);
            return;
        }
        if !self.seen.get() && self.engaged_at.get().is_some_and(|t| t.elapsed() >= ARM_GRACE) {
            self.leave(Left::NoTrackpad);
        }
    }
}

/// Whether the diagnostic line is on. `IPOD_TRACKPAD=1`.
///
/// It is the instrument this work has instead of a finger in a test: the numbers in §21.8 — frames,
/// detents felt, detents coalesced, the position derived from each contact — are what this prints,
/// and they are how anything about a hand on a pad is checked at all.
pub fn verbose() -> bool {
    std::env::var_os("IPOD_TRACKPAD").is_some_and(|v| v == "1")
}

/// One line on stderr under that gate, and nothing otherwise.
pub fn note(s: &str) {
    if verbose() {
        eprintln!("[trackpad] {s}");
    }
}

// ── macOS ───────────────────────────────────────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
mod mac;

#[cfg(target_os = "macos")]
pub use mac::{install, Handle, View};

/// Everywhere else there is nothing to install, and the caller is told so rather than left to
/// wonder. `main.rs` reads [`support`] before it ever gets here, so this is the belt to that
/// braces — and everything above stays compiled and tested on every platform rather than only on
/// the one that has an adapter today.
#[cfg(not(target_os = "macos"))]
pub struct Handle;

#[cfg(not(target_os = "macos"))]
impl Handle {
    /// The tick `main.rs`'s timer drives on macOS. There is no mode here, so there is nothing to
    /// ask — but the shape is the same so the call site is not two call sites.
    pub fn ticker(&self) -> Rc<dyn Fn()> {
        Rc::new(|| {})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wheel::{self, Button};
    use std::sync::atomic::{AtomicU32, Ordering};

    /// This machine's pad, in the unit the core works in: 342.99 × 209.76 pt at 72 to the inch.
    const PAD: Surface = Surface { w: 121.0, h: 74.0 };

    fn at(c: f64, r_frac: f32) -> Contact {
        let theta = c / 96.0 * std::f64::consts::TAU;
        let r = r_frac * PAD.h / 2.0;
        Contact {
            x: r * theta.sin() as f32,
            // y DOWN: twelve o'clock is negative y, which is `wheel.rs`'s convention and the
            // adapter's job to arrive at.
            y: -r * theta.cos() as f32,
            phase: Phase::Moved,
        }
    }

    fn frame(c: Contact) -> Frame {
        Frame { surface: PAD, contact: Some(c) }
    }

    fn unit(c: Contact) -> (f32, f32) {
        Geometry::of(PAD).expect("a surface").unit(c)
    }

    /// Where a contact's `Act` puts it: on the **drawn** wheel's ring, at the contact's own angle.
    fn drawn(c: Contact) -> (f32, f32) {
        let (x, y) = unit(c);
        on_drawn_ring(x, y)
    }

    /// How many detents a move is worth — the same two functions `wheel::Finger::moved` asks, so a
    /// disagreement between this and the machine would be a bug in one of them rather than in the
    /// test's arithmetic.
    fn detents(from: (f32, f32), to: (f32, f32)) -> u32 {
        let a = wheel::position_at_angle(from.0, from.1);
        let b = wheel::position_at_angle(to.0, to.1);
        wheel::shortest_delta(a, b).unsigned_abs()
    }

    /// **The core's axis, stated once.** A contact is y-down, so the top of the surface is negative
    /// y and wheel position 0. The *flip* that gets there from a platform's axis is the adapter's
    /// and is tested in `mac.rs`; this is the contract it has to meet.
    #[test]
    fn the_top_of_the_surface_is_twelve_oclock_and_the_value_grows_clockwise() {
        let pos = |x: f32, y: f32| {
            let (ux, uy) = unit(Contact { x, y, phase: Phase::Moved });
            wheel::position_at_angle(ux, uy)
        };
        assert_eq!(pos(0.0, -30.0), 0, "30 mm above the centre");
        assert_eq!(pos(30.0, 0.0), 24, "30 mm right — a quarter turn clockwise");
        assert_eq!(pos(0.0, 30.0), 48, "below");
        assert_eq!(pos(-30.0, 0.0), 72, "left");
    }

    /// **The property the aspect correction exists for.** 96 evenly spaced angles must come back as
    /// 96 different clicks, which is only true because the core works in a physically proportioned
    /// space. It is stated here as the core's guarantee and enforced in `mac.rs` on the conversion
    /// that could break it.
    ///
    /// **How to make it go red:** have `Geometry::unit` divide x by half the *width* and y by half
    /// the *height*. On a 1.635:1 pad the sweep visits 14 positions twice and misses 14 — an
    /// ellipse, and clicks-per-degree a function of angle.
    #[test]
    fn a_circle_on_the_surface_visits_every_click_exactly_once() {
        let mut seen = [0u32; 96];
        for i in 0..96 {
            let (x, y) = unit(at(f64::from(i), 0.8));
            seen[wheel::position_at_angle(x, y) as usize] += 1;
        }
        assert!(seen.iter().all(|&n| n == 1), "not a bijection: {seen:?}");
    }

    /// The corners of the surface are still the wheel. On the drawing, past the outer edge is the
    /// case; on a rectangle of glass there is no case, and a ring that stopped at r = 1 would be
    /// dead over most of a 1.635:1 pad.
    ///
    /// **How to make it go red:** leave `Geometry::of`'s `outer` at `WheelRing::new`'s 1.0. All four
    /// corners answer `Hit::None`.
    #[test]
    fn every_corner_of_the_surface_is_on_the_ring() {
        let g = Geometry::of(PAD).expect("a surface");
        for (x, y) in [
            (-PAD.w / 2.0, -PAD.h / 2.0),
            (PAD.w / 2.0, -PAD.h / 2.0),
            (-PAD.w / 2.0, PAD.h / 2.0),
            (PAD.w / 2.0, PAD.h / 2.0),
            (-PAD.w / 2.0 + 1.0, 0.0),
        ] {
            let (ux, uy) = g.unit(Contact { x, y, phase: Phase::Moved });
            assert!(
                !matches!(g.ring().hit(ux, uy), Hit::None),
                "({x}, {y}) mm fell off the wheel at r = {:.3}",
                (ux * ux + uy * uy).sqrt()
            );
        }
    }

    /// **The join between the two geometries, and the defect no test of either one could see.**
    ///
    /// The pad's ring runs from `CENTRE` out to the half-diagonal; the *drawn* wheel's runs from
    /// 0.52 to 1.0, and `main.rs` hands what an `Act` carries straight to
    /// `wheel::Finger::touched(&unit_ring(), …)`. So every contact this pad accepts has to come out
    /// as a point the **drawn** ring also accepts — at the same angle — or the gesture reaches the
    /// machine as nothing at all.
    ///
    /// It was not: the first version emitted the raw unit coordinates, so a contact in a corner
    /// (r ≈ 1.6) was `Hit::None` to the drawn ring and was dropped in silence, while
    /// `every_corner_of_the_surface_is_on_the_ring` passed. Found by replaying a real capture.
    ///
    /// **How to make it go red:** have `Pad::frame` emit `(x, y)` instead of `on_drawn_ring(x, y)`.
    /// The four corners and everything past r = 1.0 stop reaching the wheel.
    #[test]
    fn every_contact_the_pad_accepts_lands_on_the_drawn_wheels_ring() {
        let drawn_ring = WheelRing::new(0.0, 0.0, 1.0);
        let g = Geometry::of(PAD).expect("a surface");
        let mut checked = 0;

        // A grid over the whole pad, corners included, in millimetres.
        for i in 0..=40 {
            for j in 0..=40 {
                let c = Contact {
                    x: (f64::from(i) / 40.0 - 0.5) as f32 * PAD.w,
                    y: (f64::from(j) / 40.0 - 0.5) as f32 * PAD.h,
                    phase: Phase::Moved,
                };
                let (ux, uy) = g.unit(c);
                let on_pad_ring = !matches!(g.ring().hit(ux, uy), Hit::Select | Hit::None);
                // A fresh pad per point, so this is the `Down` edge every time.
                let mut pad = Pad::default();
                let acts = pad.frame(frame(c));
                let Some(Act::Down(dx, dy)) = acts.first().copied() else {
                    assert!(!on_pad_ring, "the pad accepted {c:?} and emitted {acts:?}");
                    continue;
                };
                assert!(on_pad_ring, "an act for a contact the pad's own ring refuses");
                assert!(
                    !matches!(drawn_ring.hit(dx, dy), Hit::None | Hit::Select),
                    "({:.1}, {:.1}) mm came out at r = {:.3}, which the drawn ring drops",
                    c.x,
                    c.y,
                    (dx * dx + dy * dy).sqrt()
                );
                // And the angle — the only thing that was ever supposed to survive.
                assert_eq!(
                    wheel::position_at_angle(dx, dy),
                    wheel::position_at_angle(ux, uy),
                    "the projection moved the contact's angle"
                );
                checked += 1;
            }
        }
        // The control: a sweep that accepted nothing looks exactly like a sweep that found no fault.
        assert!(checked > 1000, "only {checked} contacts were on the ring, which is not this pad");
    }

    /// The centre of the surface is the centre button, and the gap around it is neither control —
    /// the drawn wheel's own proportions, carried over unchanged.
    #[test]
    fn the_middle_of_the_surface_is_the_centre_button() {
        let g = Geometry::of(PAD).expect("a surface");
        let (x, y) = g.unit(Contact { x: 0.0, y: 0.0, phase: Phase::Moved });
        assert_eq!(g.ring().hit(x, y), Hit::Select);
        assert_eq!(g.ring().hit(0.0, -CENTRE), Hit::Select, "the edge of the button is the button");
        // **And nothing between the two.** The drawn wheel has a bezel there and a surface of glass
        // does not; a gap of even a hundredth swallowed 69 of the 491 real contacts `CENTRE` is
        // measured from.
        //
        // **How to make it go red:** give `Geometry::of` an `inner` above its `select`.
        // Twelve o'clock, so the ring answers with its MENU label rather than bare ring — what is
        // asserted is that it is the ring at all.
        assert_eq!(
            g.ring().hit(0.0, -(CENTRE + 0.001)),
            Hit::RingButton(Button::Menu, 0),
            "there is a dead band just outside the centre button"
        );
    }

    /// A finger put down, dragged a quarter turn clockwise and lifted: one contact, the moves in
    /// between, one lift. The `Down` is what carries §21.8's whole claim — that it is a contact and
    /// not a press — and `wheel::Finger::touched` is where that half is proved.
    #[test]
    fn a_finger_arrives_turns_and_leaves() {
        let mut pad = Pad::default();
        let start = at(0.0, 0.8);

        assert_eq!(pad.frame(frame(start)), vec![Act::Down(drawn(start).0, drawn(start).1)]);
        assert!(pad.on_ring());
        // The same frame again is not an edge — a surface sends stationary contacts at full rate.
        assert_eq!(pad.frame(frame(Contact { phase: Phase::Stationary, ..start })), Vec::new());

        let mut turned = 0;
        let mut prev = drawn(start);
        for c in 1..=24 {
            let p = at(f64::from(c), 0.8);
            let u = drawn(p);
            assert_eq!(pad.frame(frame(p)), vec![Act::Moved(u.0, u.1)]);
            turned += detents(prev, u);
            prev = u;
        }
        assert_eq!(turned, 24, "a quarter turn is 24 detents");
        assert_eq!(pad.frame(Frame { surface: PAD, contact: None }), vec![Act::Up]);
        assert!(!pad.on_ring());
    }

    /// **A lift is an event, not a timeout** — the one thing this input has that a scroll wheel does
    /// not, and the reason `Phase` is in the contract at all.
    ///
    /// **How to make it go red:** drop the `.filter(|c| c.phase.touching())` in `Pad::frame`. The
    /// ended contact reads as a live one, the wheel keeps a finger the hand has taken away, and the
    /// next `Down` sends no `Touch` because the window still believes the old one.
    #[test]
    fn an_ended_contact_takes_the_finger_off_the_wheel() {
        let mut pad = Pad::default();
        let p = at(0.0, 0.8);
        assert_eq!(pad.frame(frame(p)).len(), 1, "the contact arrived");
        assert_eq!(pad.frame(frame(Contact { phase: Phase::Ended, ..p })), vec![Act::Up]);
        assert!(!pad.on_ring());
        // And a cancelled one does the same — `GUI.md` §7.4's cancelled contact, on this surface.
        assert_eq!(pad.frame(frame(p)).len(), 1);
        assert_eq!(pad.frame(frame(Contact { phase: Phase::Cancelled, ..p })), vec![Act::Up]);
    }

    /// **A finger that wanders into the centre leaves the wheel**, and coming back out is a new
    /// contact rather than a jump. Without the `Up` the machine keeps a finger on a wheel the hand
    /// is not on — the stuck finger, reached sideways.
    #[test]
    fn crossing_the_centre_lifts_the_finger_and_coming_back_is_a_new_contact() {
        let mut pad = Pad::default();
        let edge = at(0.0, 0.9);
        let middle = Contact { x: 0.0, y: 0.0, phase: Phase::Moved };

        assert_eq!(pad.frame(frame(edge)).len(), 1);
        assert_eq!(pad.frame(frame(middle)), vec![Act::Up]);
        assert_eq!(pad.frame(frame(middle)), Vec::new(), "already off");
        assert_eq!(pad.frame(frame(edge)), vec![Act::Down(drawn(edge).0, drawn(edge).1)]);
    }

    /// **The click is the press, and resting is not.** A finger sitting on the MENU label sends no
    /// button; clicking there does, and the release takes up what the press put down.
    ///
    /// **How to make it go red:** have `Pad::frame` answer `Press` for a `Hit::RingButton`. The
    /// first assertion gets a press nobody made — which on the emulated iPod is the main menu
    /// appearing because somebody rested a thumb at twelve o'clock.
    #[test]
    fn resting_on_a_label_presses_nothing_and_clicking_on_it_does() {
        let mut pad = Pad::default();
        let top = at(0.0, 0.9);
        let d = drawn(top);

        assert_eq!(pad.frame(frame(top)), vec![Act::Down(d.0, d.1)]);
        let u = unit(top);
        let g = Geometry::of(PAD).expect("a surface");
        assert_eq!(g.ring().hit(u.0, u.1), Hit::RingButton(Button::Menu, 0));

        let down = pad.clicked();
        assert!(matches!(down[..], [Act::Press(Hit::RingButton(Button::Menu, _))]), "{down:?}");
        assert_eq!(pad.clicked(), Vec::new(), "a second press with no release between");
        assert!(matches!(pad.unclicked()[..], [Act::Release(Hit::RingButton(Button::Menu, _))]));
        assert_eq!(pad.unclicked(), Vec::new(), "a release for a press that never happened");
    }

    /// A click in the middle is the centre button, and one on bare ring is on nothing — the dead
    /// band either side of each label, which is the same band that stops every drag being a press.
    #[test]
    fn a_click_in_the_middle_is_the_centre_button_and_one_between_labels_is_nothing() {
        let mut pad = Pad::default();

        pad.frame(frame(Contact { x: 0.0, y: 0.0, phase: Phase::Began }));
        assert_eq!(pad.clicked(), vec![Act::Press(Hit::Select)]);
        assert_eq!(pad.unclicked(), vec![Act::Release(Hit::Select)]);

        // Position 12 — halfway between MENU and Next, which `wheel::quadrant` gives to neither.
        pad.frame(frame(at(12.0, 0.8)));
        assert_eq!(pad.clicked(), Vec::new(), "bare ring is under no switch");
    }

    /// **Four dome switches, and nothing between them.** The 5G's ring is not one button: four
    /// domes sit at the cardinal points with the centre button separate, so where the finger is
    /// selects the switch and the click presses it. Between two domes the real part has no switch,
    /// and a press at 45 degrees must not become one here — no fifth position, and no rounding to
    /// the nearest dome across the band.
    ///
    /// **How to make it go red:** widen `wheel::quadrant`'s `<= 8` to `<= 12`, which is what
    /// "nearest dome" would mean. The four band assertions start naming a button.
    #[test]
    fn all_four_domes_click_and_the_bands_between_them_do_not() {
        let mut pad = Pad::default();
        for (click, want) in [
            (0.0, Button::Menu),
            (24.0, Button::Next),
            (48.0, Button::Play),
            (72.0, Button::Prev),
        ] {
            pad.frame(frame(at(click, 0.8)));
            let down = pad.clicked();
            assert!(
                matches!(down[..], [Act::Press(Hit::RingButton(b, _))] if b == want),
                "at {click} the click gave {down:?}, not {want:?}"
            );
            pad.unclicked();
        }
        // Halfway between each pair. `wheel::quadrant` gives a label the 16 clicks centred on its
        // quadrant, so 12, 36, 60 and 84 are under no dome at all.
        for band in [12.0, 36.0, 60.0, 84.0] {
            pad.frame(frame(at(band, 0.8)));
            assert_eq!(pad.clicked(), Vec::new(), "a click at {band} pressed a dome that is not there");
        }
    }

    /// Lifting with the click still down takes the press up first. A button that came up *after*
    /// the touch ended would post a frame with the button set and no finger on the wheel.
    #[test]
    fn lifting_with_the_click_down_releases_the_button_before_the_contact() {
        let mut pad = Pad::default();
        pad.frame(frame(at(0.0, 0.9)));
        pad.clicked();
        let out = pad.empty();
        assert!(
            matches!(out[..], [Act::Release(Hit::RingButton(Button::Menu, _)), Act::Up]),
            "{out:?}"
        );
    }

    /// **The rate limit, at the speed that matters.** At the pad's own 124 Hz mean the floor never
    /// bites — every frame gets its pulse.
    ///
    /// **How to make it go red:** raise [`TICK_FLOOR`] to 10 ms. The 8 ms arm below stops firing,
    /// which is the actuator going quiet at ordinary speed.
    #[test]
    fn the_floor_never_bites_at_the_speed_the_pad_samples_at() {
        let mut t = Ticks::default();
        let t0 = Instant::now();
        // 8.06 ms is 124 Hz, this machine's measured mean.
        for i in 0..20 {
            assert!(t.due(t0 + Duration::from_micros(8060 * i), 1), "a frame at 124 Hz was refused");
        }
        assert_eq!(t.counts(), (20, 0));
    }

    /// Two detents inside one frame: one is felt, one is not, and the one that is not is counted
    /// rather than forgotten.
    #[test]
    fn detents_inside_one_frame_are_coalesced_and_counted() {
        let mut t = Ticks::default();
        let t0 = Instant::now();
        assert!(t.due(t0, 3), "the first of three");
        assert!(!t.due(t0, 1), "a second pulse in the same instant");
        assert!(t.due(t0 + TICK_FLOOR, 1), "and one floor later it is due again");
        assert_eq!(t.counts(), (2, 3), "two fired; two from the triple and the one refused");
    }

    #[test]
    fn nothing_is_asked_of_the_actuator_for_a_frame_that_turned_nothing() {
        let mut t = Ticks::default();
        assert!(!t.due(Instant::now(), 0));
        assert_eq!(t.counts(), (0, 0));
    }

    /// **The seam, exercised.** A second sink — the piezo's audible click, when it exists — is one
    /// `add` and nothing else changes: both fire, on one rate limit, so two clicks cannot come out
    /// of step with each other. A sink that says it is not present is not asked.
    ///
    /// **How to make it go red:** give `Feedback::fire` one sink instead of the list, or drop its
    /// `present()` guard.
    #[test]
    fn every_present_sink_fires_on_the_one_rate_limit() {
        struct Counter(&'static str, bool, &'static AtomicU32);
        impl Detents for Counter {
            fn describe(&self) -> &'static str {
                self.0
            }
            fn present(&self) -> bool {
                self.1
            }
            fn detent(&self) {
                self.2.fetch_add(1, Ordering::Relaxed);
            }
        }
        static HAPTIC: AtomicU32 = AtomicU32::new(0);
        static PIEZO: AtomicU32 = AtomicU32::new(0);
        static ABSENT: AtomicU32 = AtomicU32::new(0);

        let mut f = Feedback::default();
        assert_eq!(f.describe(), "nothing — a detent is felt by no one on this build");
        f.add(Box::new(Counter("the actuator", true, &HAPTIC)));
        f.add(Box::new(Counter("the piezo", true, &PIEZO)));
        f.add(Box::new(Counter("something absent", false, &ABSENT)));
        assert_eq!(f.describe(), "the actuator and the piezo");

        f.fire(1);
        f.fire(1); // inside the floor, so refused for both together
        assert_eq!(HAPTIC.load(Ordering::Relaxed), 1);
        assert_eq!(PIEZO.load(Ordering::Relaxed), 1);
        assert_eq!(ABSENT.load(Ordering::Relaxed), 0, "a sink that is not present was asked");
        assert_eq!(f.counts(), (1, 1));
    }

    /// A surface that cannot be a rectangle answers `None` rather than an infinity. The size is read
    /// off a contact on every frame, so this is asked at the surface's own rate and must never panic
    /// or divide by zero — and a frame carrying one produces no acts rather than a wrong one.
    #[test]
    fn a_surface_with_no_size_is_refused_rather_than_divided_by() {
        for s in [
            Surface { w: 0.0, h: 74.0 },
            Surface { w: 121.0, h: -1.0 },
            Surface { w: f32::NAN, h: 74.0 },
            Surface { w: 121.0, h: f32::INFINITY },
        ] {
            assert!(Geometry::of(s).is_none(), "{s:?}");
            let mut pad = Pad::default();
            let c = Contact { x: 0.0, y: -30.0, phase: Phase::Began };
            assert_eq!(pad.frame(Frame { surface: s, contact: Some(c) }), Vec::new());
            assert!(!pad.on_ring());
        }
    }

    /// **The three numbers a surface's wheel is made of**, on a square one where the arithmetic is
    /// checkable by hand: the button at [`CENTRE`], nothing between it and the ring, and an outer
    /// edge at the half-diagonal — √2 half-widths on a square — so the corners are on the ring.
    #[test]
    fn a_surfaces_wheel_is_a_button_a_ring_and_no_gap_between_them() {
        let g = Geometry::of(Surface { w: 100.0, h: 100.0 }).expect("a square surface");
        assert!((g.ring().outer - std::f32::consts::SQRT_2).abs() < 1e-5, "{}", g.ring().outer);
        assert!((g.ring().select - CENTRE).abs() < 1e-6);
        assert_eq!(g.ring().inner, g.ring().select, "a bezel invented for a surface with none");
        assert!((g.radius - 50.0).abs() < 1e-5, "half the short axis, in millimetres");
        // And on this machine's pad, the button in the unit a ruler measures.
        let pad = Geometry::of(PAD).expect("a surface");
        let mm = pad.ring().select * pad.radius;
        assert!((mm - 12.6).abs() < 0.1, "the centre button is {mm:.1} mm, not 12.6");
    }

    /// This build's answer to *can the trackpad be the wheel*, and it is a fact about the target
    /// rather than about the hardware — which is exactly why the mode has [`ARM_GRACE`].
    #[test]
    fn support_is_a_fact_about_the_target() {
        assert_eq!(support().available(), cfg!(target_os = "macos"));
        assert!(!support().describe().is_empty());
        for l in [Left::Esc, Left::Chord, Left::Force, Left::Focus, Left::Quit, Left::NoTrackpad] {
            assert!(!l.describe().is_empty());
        }
    }
}
