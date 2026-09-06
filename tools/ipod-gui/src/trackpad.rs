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

    /// **Say that a line drawn on the surface went past.**
    ///
    /// Not a detent and it must not feel like one: a detent says *the wheel turned a step*, and
    /// this says *the finger crossed a boundary*. Rate-limited by [`Feedback`] on the same
    /// actuator, and always **behind** the detent — see [`Ticks`] for why that ordering is the
    /// whole safety argument.
    fn mark(&self, m: Mark);
}

// ── The geometry a finger cannot see ────────────────────────────────────────────────────────────

/// **A line on the surface that is worth feeling.**
///
/// On a real 5G the hand is confined by the bezel and the centre button: **you feel where the ring
/// is**, and that is why a wheel can be worked without looking. On a rectangle of glass there is no
/// bezel, so the hand picks its own radius — 891 real contacts circle at a **median 18.7 mm**, well
/// outside a real wheel's 14 mm ring. The actuator is the only thing here that can put an edge back,
/// and this is the closed set of edges it is asked to.
///
/// **Only [`Edge::Centre`] is felt by default, and the argument is measured rather than aesthetic.**
/// See [`Felt`] for the numbers and `docs/GUI.md` §21.8.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Edge {
    /// **The ring meets the centre button**, at [`CENTRE`] — 12.58 mm on this machine's pad.
    ///
    /// The one boundary on the surface that **changes what the wheel does**: crossing it inward
    /// ends the contact, so the wheel stops turning, and today it does that in silence. It is also
    /// the only one on an axis the detents do not already use — a detent is *purely angular*
    /// (`wheel::position_at_angle` never reads a radius), so a **radial** mark cannot be mistaken
    /// for one having been caused by turning.
    Centre,
    /// **Where a real wheel's outer edge would be**, at r = 1.0 — 37 mm here.
    ///
    /// It changes nothing: [`Geometry`] puts `outer` at the pad's half-diagonal so that every
    /// corner is still the wheel, and this is a line drawn where the *drawn* device's case begins.
    /// Off by default, and the capture is why: across 899 samples the hand reached a **maximum
    /// radius of 31.4 mm** and so never crossed it once. It is also tangent to the top and bottom
    /// edges of a 121 × 74 mm pad, so it can only be crossed sideways — two arcs rather than a rim.
    Rim,
    /// **The eight edges of the four label bands**, where `wheel::quadrant` starts and stops naming
    /// a button — so a click at that spot starts and stops pressing a dome.
    ///
    /// Off by default, and the capture is why: **65 crossings against 790 detents** in the same
    /// 7.16 s, which is a pulse every 110 ms *on the same angular axis the detents are already
    /// using*. Eight boundaries a rotation is four times the centre edge's rate, and mixed into a
    /// detent stream rather than beside it.
    Band,
}

impl Edge {
    /// The name the diagnostic line and [`Felt::parse`] both use. No full stop.
    pub fn describe(self) -> &'static str {
        match self {
            Edge::Centre => "centre",
            Edge::Rim => "rim",
            Edge::Band => "bands",
        }
    }
}

/// **One crossing**, and which way it went.
///
/// `entering` is *towards the region the edge is named for*: into the centre button for
/// [`Edge::Centre`], out past the rim for [`Edge::Rim`], into a label's band for [`Edge::Band`].
/// The actuator may or may not use it — with three canned patterns and one of them spoken for
/// there is exactly one spare, so a direction costs the only remaining degree of freedom. It is in
/// the type because the log wants it either way.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Mark {
    pub edge: Edge,
    pub entering: bool,
}

/// **Which edges are felt.** `IPOD_TRACKPAD_EDGES`, and the default is [`Edge::Centre`] alone.
///
/// A set rather than a switch because **software cannot check that something feels right** — only a
/// hand can, and every round trip through a rebuild costs the operator a sitting. So the variants
/// are chosen at launch, next to `IPOD_TRACKPAD=1`, and several can be compared in one go:
///
/// ```text
/// IPOD_TRACKPAD_EDGES=off            nothing but detents — the control, and today's behaviour
/// IPOD_TRACKPAD_EDGES=centre         the default: the ring/centre boundary and nothing else
/// IPOD_TRACKPAD_EDGES=centre,rim     ... and where a real wheel's outer edge would be
/// IPOD_TRACKPAD_EDGES=centre,bands   ... and the eight label-band edges
/// IPOD_TRACKPAD_EDGES=all            every line this file knows how to draw
/// ```
///
/// **The default is one edge and the argument for it is arithmetic**, from the same 891-contact
/// capture [`CENTRE`] was measured from — 7.16 s of contact, replayed through this file's own
/// detector:
///
/// ```text
/// centre  16 crossings   2.2 a second, radial, in frames that carry no detent at all
/// rim      0 crossings   never reached: the hand's maximum radius was 31.4 mm of the 37 needed
/// bands   65 crossings   9 a second, angular, mixed into the 790 detents of the same gesture
/// ```
///
/// A surface that ticks constantly conveys less than one that ticks rarely, so the set that is on
/// by default is the smallest one that makes the geometry legible: **the boundary that changes
/// what the wheel does, on the axis the detents leave free.**
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Felt {
    centre: bool,
    rim: bool,
    band: bool,
}

impl Default for Felt {
    fn default() -> Self {
        Felt { centre: true, rim: false, band: false }
    }
}

impl Felt {
    /// **A comma-separated set, and an unknown word is refused rather than ignored.** A typo that
    /// silently selected the default would make the two arms of a comparison identical while
    /// reading as different, which is the shape `AGENTS.md` §6 is about — so it says so and keeps
    /// the default, which is the one behaviour that cannot mislead.
    pub fn parse(s: &str) -> Felt {
        let mut f = Felt { centre: false, rim: false, band: false };
        let s = s.trim();
        if s.is_empty() {
            return Felt::default();
        }
        for word in s.split(',').map(str::trim).filter(|w| !w.is_empty()) {
            match word {
                "off" | "none" => return Felt { centre: false, rim: false, band: false },
                "all" => return Felt { centre: true, rim: true, band: true },
                "centre" | "center" => f.centre = true,
                "rim" => f.rim = true,
                "band" | "bands" => f.band = true,
                other => {
                    // **Not behind `verbose()`, unlike everything else this file prints.** A
                    // misconfiguration is not a diagnostic: somebody comparing two variants by
                    // hand may well not have the log on, and a typo that silently kept the default
                    // would make two arms of their comparison behave identically while reading as
                    // different — which is the one outcome that would waste the sitting.
                    eprintln!("[trackpad] IPOD_TRACKPAD_EDGES: no edge is called {other:?} — using the default, {}", Felt::default().describe());
                    return Felt::default();
                }
            }
        }
        f
    }

    pub fn has(self, e: Edge) -> bool {
        match e {
            Edge::Centre => self.centre,
            Edge::Rim => self.rim,
            Edge::Band => self.band,
        }
    }

    /// What is felt, for the diagnostic line. `"no edges"` rather than an empty string.
    pub fn describe(self) -> String {
        let on: Vec<&str> =
            [Edge::Centre, Edge::Rim, Edge::Band].iter().copied().filter(|&e| self.has(e)).map(Edge::describe).collect();
        if on.is_empty() {
            "no edges".to_string()
        } else {
            on.join(" + ")
        }
    }

    /// Read once, because the answer cannot change while the process runs and a per-pulse `var_os`
    /// on the actuator's path would be a system call inside a haptic.
    pub fn from_env() -> Felt {
        static CACHE: std::sync::OnceLock<Felt> = std::sync::OnceLock::new();
        *CACHE.get_or_init(|| match std::env::var("IPOD_TRACKPAD_EDGES") {
            Ok(v) => Felt::parse(&v),
            Err(_) => Felt::default(),
        })
    }
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
///
/// # A dropped detent is worse than a missed edge, and this is where that is enforced
///
/// One actuator now has two callers, so they can contend — and the two failures are not equally
/// bad. A missed edge is a boundary you have to find by feel; **a dropped detent is a wheel that
/// stopped turning**, which is the input not working. So the priority is absolute rather than
/// weighted, and it is structural rather than a rule someone has to remember:
///
/// **[`Ticks::due`] reads `detent` and nothing else.** No field a [`Ticks::mark_due`] can write is
/// on its path, so no sequence of marks — none, one, a thousand in the same microsecond — can
/// change its answer for any input. A mark cannot delay, refuse or coalesce a detent because it
/// cannot reach the state the detent's decision is made from. `a_flood_of_edge_marks_cannot_refuse_a_single_detent`
/// is the test, and deleting one word — `self.detent` for `self.any` in `due` — is how to make it
/// go red.
///
/// The yielding is all in the other direction: a **mark** waits on `any`, so it stands off for
/// [`TICK_FLOOR`] after a pulse of *either* kind. Two pulses closer than the floor are one blur on
/// a single actuator, and it is the edge rather than the wheel that gives way.
#[derive(Default)]
pub struct Ticks {
    /// The last **detent**. The only clock [`Ticks::due`] consults, and no mark ever writes it.
    detent: Option<Instant>,
    /// The last pulse of either kind. Only a mark consults it, because only a mark yields.
    any: Option<Instant>,
    fired: u64,
    coalesced: u64,
    marks: u64,
    marks_lost: u64,
}

impl Ticks {
    /// Whether to actuate, for a frame that produced `clicks` detents.
    ///
    /// **Reads `self.detent` and nothing else** — see the type's note. That is not an optimisation
    /// and the extra field is not redundant: it is the whole of the guarantee that geometry
    /// feedback cannot starve the wheel.
    pub fn due(&mut self, now: Instant, clicks: u32) -> bool {
        if clicks == 0 {
            return false;
        }
        let ready = match self.detent {
            Some(t) => now.saturating_duration_since(t) >= TICK_FLOOR,
            None => true,
        };
        if !ready {
            self.coalesced += u64::from(clicks);
            return false;
        }
        self.detent = Some(now);
        self.any = Some(now);
        self.fired += 1;
        self.coalesced += u64::from(clicks - 1);
        true
    }

    /// Whether to actuate for an edge crossing. Yields to any pulse inside [`TICK_FLOOR`], and
    /// **never writes the clock a detent is judged against.**
    pub fn mark_due(&mut self, now: Instant) -> bool {
        let ready = match self.any {
            Some(t) => now.saturating_duration_since(t) >= TICK_FLOOR,
            None => true,
        };
        if !ready {
            self.marks_lost += 1;
            return false;
        }
        self.any = Some(now);
        self.marks += 1;
        true
    }

    /// Pulses fired, and detents that went unfelt.
    pub fn counts(&self) -> (u64, u64) {
        (self.fired, self.coalesced)
    }

    /// Edges felt, and edges that gave way to a pulse already going out.
    pub fn mark_counts(&self) -> (u64, u64) {
        (self.marks, self.marks_lost)
    }
}

/// **Every sink, behind the one rate limit.** Adding the piezo's audible click is `add`.
pub struct Feedback {
    ticks: Ticks,
    sinks: Vec<Box<dyn Detents>>,
    /// Which edges reach the sinks at all. Read from the environment once, so that a comparison
    /// between two variants is a relaunch rather than a rebuild.
    felt: Felt,
}

impl Default for Feedback {
    fn default() -> Self {
        Feedback { ticks: Ticks::default(), sinks: Vec::new(), felt: Felt::from_env() }
    }
}

impl Feedback {
    pub fn add(&mut self, sink: Box<dyn Detents>) {
        self.sinks.push(sink);
    }

    /// Choose the edges by hand rather than from the environment. The tests', and a drawn control's
    /// if §21 ever grows one.
    #[allow(dead_code)] // retired when: a drawn control in §21 chooses the edges — `Handle::toggle` is the seam it would arrive through, and until then the environment is the only caller outside the tests
    pub fn feel(&mut self, felt: Felt) {
        self.felt = felt;
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

    /// One edge crossing, if that edge is one of the ones being felt.
    ///
    /// **Behind the detent by construction** — see [`Ticks`]. A crossing that arrives while the
    /// actuator is mid-pulse is dropped and counted rather than queued: a queued mark would arrive
    /// after the finger had moved on, which is a boundary reported in the wrong place.
    pub fn mark(&mut self, m: Mark) {
        if !self.felt.has(m.edge) {
            return;
        }
        if !self.ticks.mark_due(Instant::now()) {
            return;
        }
        for s in &self.sinks {
            if s.present() {
                s.mark(m);
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

    pub fn mark_counts(&self) -> (u64, u64) {
        self.ticks.mark_counts()
    }

    pub fn felt(&self) -> Felt {
        self.felt
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

/// Ask for an edge crossing to be felt. Refused silently when that edge is not one of the ones
/// being felt, which is the ordinary case for two of the three.
pub fn mark(m: Mark) {
    FEEDBACK.with(|f| f.borrow_mut().mark(m));
}

/// What a detent will be felt or heard as, and how many have been.
pub fn feedback_state() -> (String, u64, u64) {
    FEEDBACK.with(|f| {
        let f = f.borrow();
        let (fired, coalesced) = f.counts();
        (f.describe(), fired, coalesced)
    })
}

/// Which edges are being felt, how many were, and how many gave way to a pulse already going out.
pub fn mark_state() -> (String, u64, u64) {
    FEEDBACK.with(|f| {
        let f = f.borrow();
        let (marks, lost) = f.mark_counts();
        (f.felt().describe(), marks, lost)
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
    /// **A line drawn on the surface went past.** Reported for every [`Edge`] whether or not it is
    /// one of the ones being felt, because the diagnostic line wants the geometry either way and
    /// [`Feedback::mark`] is the one place that should decide what reaches a fingertip.
    Mark(Mark),
}

/// **How far past a boundary the finger must travel before that boundary can speak again. 1.5 mm.**
///
/// The engineering risk in the whole of this is **chatter**: the pad samples at ~124 Hz, so a hand
/// resting near a line, or drifting across one, would cross it many times a second and turn the
/// actuator into a machine gun. Hysteresis is the answer, and this is the distance — chosen from
/// the same 891-contact capture [`CENTRE`] was measured from, which bounds it from **both** sides:
///
/// ```text
/// from below   a hand trying to hold still wanders radially. The stillest 265 ms in the capture
///              — 33 consecutive frames moving under 0.35 mm each — drifted over a 1.10 mm band.
///              Anything at or under that lets a resting finger re-arm itself and chatter.
/// from above   at 1.9 mm the capture's 16 real crossings are all still there; at 2.0 mm one is
///              gone and at 2.9 mm two are. So the band may not exceed ~1.9 mm without starting to
///              swallow crossings a hand actually made.
/// ```
///
/// 1.5 mm is the middle of `1.10 … 1.9`, with ~35% of margin over the measured wander and ~20%
/// under the measured cost. It is also small against the 6.1 mm between the boundary and the
/// **median radius a hand circles at**, so it narrows nothing anyone is aiming for.
///
/// **Millimetres rather than a fraction of the radius**, and that is the same argument [`CENTRE`]
/// makes in reverse: this is a fact about *fingers* — how still a hand can hold — and not about
/// pads, so it must not scale when an external Magic Trackpad reports a different size.
///
/// **It is an arming band and not a shifted threshold**, which matters: a Schmitt trigger would
/// move the boundary, and the boundary is not this feature's to move — crossing [`CENTRE`] is what
/// takes the finger off the wheel. So the mark fires on the *true* crossing and is then mute until
/// the finger is this far clear of the line. One tick per crossing a hand meant, none for a wobble,
/// and the wheel behaves exactly as it did before.
pub const ARM_MM: f32 = 1.5;

/// **The same idea on the angular axis, for [`Edge::Band`]. 2 clicks — 7.5°.**
///
/// Less carefully derived than [`ARM_MM`] and deliberately so: bands are off by default, and the
/// capture cannot pin this one the way it pins the radial band. Angular position is *intrinsically*
/// noisy near the middle of the pad — at 10 mm out one click is 0.68 mm of arc, so the same hand
/// wander that is 1.1 mm radially is worth a click and a half — which is one more reason this edge
/// is the weaker idea rather than a reason to tune it.
pub const ARM_CLICKS: f32 = 2.0;

/// **One boundary at a fixed radius, and the crossings of it worth feeling.**
///
/// Everything is in units of the wheel's own radius except the arming band, which arrives already
/// converted — see [`ARM_MM`] for why that one is millimetres.
#[derive(Clone, Copy, Debug)]
struct Radial {
    /// Which side the finger was on last. `None` before the first frame of a contact, because **a
    /// finger arriving is not a crossing** — it has no previous side to have come from.
    outside: Option<bool>,
    armed: bool,
}

impl Default for Radial {
    fn default() -> Self {
        Radial { outside: None, armed: true }
    }
}

impl Radial {
    /// `Some(entering)` when this frame crossed `at` and the crossing is one to report.
    fn crossed(&mut self, r: f32, at: f32, arm: f32) -> Option<bool> {
        let outside = r >= at;
        let out = match self.outside.replace(outside) {
            Some(was) if was != outside && self.armed => {
                self.armed = false;
                Some(!outside)
            }
            _ => None,
        };
        // Re-arm on the way through, and **after** the test rather than before: a frame that
        // crosses the line from well outside the band to well outside it on the other side is one
        // crossing that fires and then immediately re-arms for the next, which is right.
        if (r - at).abs() > arm {
            self.armed = true;
        }
        out
    }
}

/// **The eight edges of the four label bands**, on the angular axis.
///
/// A single `bool` is enough because the bands do not touch: `wheel::quadrant` gives each label the
/// 16 clicks centred on its quadrant, so there is a dead band either side and a finger can never go
/// from one label straight into another without passing through nothing.
#[derive(Clone, Copy, Debug)]
struct Angular {
    inside: Option<bool>,
    armed: bool,
}

/// **Armed to begin with, exactly as [`Radial`] is**, and written out rather than derived: a
/// derived `Default` gives `false`, which would swallow the first crossing of a contact that
/// happened to land within [`ARM_CLICKS`] of a band edge. That is a real difference in behaviour
/// and it should be a decision rather than a consequence of which trait was derived.
impl Default for Angular {
    fn default() -> Self {
        Angular { inside: None, armed: true }
    }
}

impl Angular {
    /// `Some(entering)` when this frame crossed into or out of a label's band.
    fn crossed(&mut self, pos: u8) -> Option<bool> {
        let inside = crate::wheel::quadrant(pos).is_some();
        let out = match self.inside.replace(inside) {
            Some(was) if was != inside && self.armed => {
                self.armed = false;
                Some(inside)
            }
            _ => None,
        };
        // How far this position is from the nearest band edge, which sits at 8.5 clicks from a
        // label's centre — `quadrant` takes `<= 8`, so the line is between click 8 and click 9.
        let nearest = crate::wheel::Button::ALL
            .iter()
            .filter_map(|b| b.centre_click())
            .map(|c| crate::wheel::shortest_delta(pos, c as u8).abs())
            .min()
            .unwrap_or(0);
        if (nearest as f32 - 8.5).abs() > ARM_CLICKS {
            self.armed = true;
        }
        out
    }

    fn forget(&mut self) {
        *self = Angular::default();
    }
}

/// **Every boundary the surface has, and what the finger has done to them.**
///
/// Held by [`Pad`] and reset whenever the surface empties: a finger that lifts inside the centre
/// button and lands again on the ring has not crossed anything, and reporting that as a crossing
/// would put an edge where the hand felt none.
#[derive(Clone, Copy, Debug, Default)]
struct Edges {
    centre: Radial,
    rim: Radial,
    band: Angular,
}

impl Edges {
    /// The crossings in one frame, for a contact at `(x, y)` in units of the wheel's radius.
    /// `mm_per_unit` is [`Geometry`]'s scale, and the only thing the arming band needs it for.
    fn crossed(&mut self, x: f32, y: f32, mm_per_unit: f32, on_ring: bool) -> Vec<Act> {
        let mut out = Vec::new();
        let r = (x * x + y * y).sqrt();
        // The band in millimetres, in the space the radii are actually in. `Geometry::of` refuses
        // a surface that is not finite and positive, and the radius is half its short axis, so
        // this divisor cannot be zero and there is no branch here for a case that cannot arise.
        let arm = ARM_MM / mm_per_unit;
        if let Some(entering) = self.centre.crossed(r, CENTRE, arm) {
            out.push(Act::Mark(Mark { edge: Edge::Centre, entering }));
        }
        // The rim is named for what is *outside* it, so entering it is going out.
        if let Some(inward) = self.rim.crossed(r, 1.0, arm) {
            out.push(Act::Mark(Mark { edge: Edge::Rim, entering: !inward }));
        }
        // **Angles only mean anything on the ring.** Near the middle of the pad `atan2` swings
        // wildly for a millimetre of movement, so a band edge there would be noise rather than
        // geometry — and a label's band is a thing on the ring in the first place.
        if on_ring {
            if let Some(entering) = self.band.crossed(crate::wheel::position_at_angle(x, y)) {
                out.push(Act::Mark(Mark { edge: Edge::Band, entering }));
            }
        } else {
            self.band.forget();
        }
        out
    }
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
    /// The boundaries, and which side of each the finger is on. See [`Edges`].
    edges: Edges,
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
        let hit = g.ring().hit(x, y);
        let mut out = match hit {
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
        };
        // **After what the wheel did, because it reports on it.** The order also puts the machine's
        // act first in the frame that crosses [`CENTRE`], which is the frame the hand most needs to
        // be told about: the contact ends and *then* the edge says why.
        out.extend(self.edges.crossed(x, y, g.radius, self.on_ring));
        out
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
        // **A lift forgets every boundary.** A finger that goes up inside the centre button and
        // comes down on the ring has crossed nothing — the line is between two places on the
        // surface, not between two contacts — and reporting that as a crossing would put an edge
        // where the hand felt none.
        self.edges = Edges::default();
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
        note(&format!(
            "engaged — {}, detents: {}, edges felt: {}",
            self.source.describe(),
            feedback_state().0,
            mark_state().0
        ));
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
        let (edges, marks, lost) = mark_state();
        note(&format!(
            "released ({}) — {} frames, {fired} detents felt, {coalesced} coalesced, \
             {marks} edges felt ({edges}), {lost} gave way",
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
            "{}, armed: {armed}, detents: {}, edges felt: {}",
            self.source.describe(),
            feedback_state().0,
            mark_state().0
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

    /// Half the pad's short axis — one unit of the wheel's radius, in millimetres. 37.0 here.
    const RADIUS_MM: f32 = PAD.h / 2.0;

    /// **The centre boundary in the unit a ruler measures.** 12.58 mm on this pad.
    const BOUNDARY_MM: f32 = CENTRE * RADIUS_MM;

    /// A contact at a wheel position and a radius **in millimetres**, which is the unit every
    /// number about a finger is in.
    fn at_mm(click: f64, mm: f32) -> Contact {
        at(click, mm / RADIUS_MM)
    }

    fn marks_of(acts: &[Act], edge: Edge) -> usize {
        acts.iter().filter(|a| matches!(a, Act::Mark(m) if m.edge == edge)).count()
    }

    /// **The half of a frame the machine sees.** A geometry mark goes to a fingertip and never to
    /// the emulated part, so the tests that are about *what the wheel did* assert on this and stay
    /// exactly as strong as they were before edges existed.
    fn wheel_acts(acts: Vec<Act>) -> Vec<Act> {
        acts.into_iter().filter(|a| !matches!(a, Act::Mark(_))).collect()
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
            // `wheel_acts` because a quarter turn also crosses two label-band edges, which are a
            // fact about a fingertip rather than about the wheel — see `Edge::Band`.
            assert_eq!(wheel_acts(pad.frame(frame(p))), vec![Act::Moved(u.0, u.1)]);
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
        // **And the finger is told.** The frame that takes the contact off the wheel is the frame
        // that crosses `CENTRE`, which is the whole of why that edge is the one felt by default.
        assert_eq!(
            pad.frame(frame(middle)),
            vec![Act::Up, Act::Mark(Mark { edge: Edge::Centre, entering: true })]
        );
        assert_eq!(pad.frame(frame(middle)), Vec::new(), "already off");
        assert_eq!(
            pad.frame(frame(edge)),
            vec![
                Act::Down(drawn(edge).0, drawn(edge).1),
                Act::Mark(Mark { edge: Edge::Centre, entering: false }),
            ]
        );
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

    /// **A dropped detent is worse than a missed edge, and this is the proof that it cannot
    /// happen.**
    ///
    /// One actuator now has two callers. A missed edge is a boundary you have to find by feel; a
    /// dropped detent is *the wheel not turning*, which is the input being broken. So the priority
    /// is structural rather than weighted: [`Ticks::due`] reads `self.detent`, which no mark ever
    /// writes, so no number of marks at any instant can reach the state its decision is made from.
    ///
    /// Two hundred of them in the same microsecond — far past anything a hand could cause — and the
    /// detent that follows in that same instant still fires.
    ///
    /// **How to make it go red:** have `Ticks::due` test `self.any` instead of `self.detent`. The
    /// first mark takes the slot and the detent is refused, which on a real pad is a wheel that
    /// stops turning when a finger wanders near a line.
    #[test]
    fn a_flood_of_edge_marks_cannot_refuse_a_single_detent() {
        let mut t = Ticks::default();
        let t0 = Instant::now();
        for _ in 0..200 {
            t.mark_due(t0);
        }
        assert!(t.due(t0, 1), "a detent was refused after a burst of edge marks");
        // And at every speed a hand reaches, not just the first one.
        for i in 1..20 {
            let now = t0 + Duration::from_micros(8060 * i);
            for _ in 0..8 {
                t.mark_due(now);
            }
            assert!(t.due(now, 1), "a detent was refused at frame {i}");
        }
        assert_eq!(t.counts().0, 20, "every frame's detent was felt");
    }

    /// The yielding, in the direction it is supposed to go: an **edge** stands off for
    /// [`TICK_FLOOR`] after a pulse of either kind, because two pulses closer than that are one
    /// blur on a single actuator and it is the edge rather than the wheel that gives way.
    #[test]
    fn an_edge_gives_way_to_a_pulse_already_going_out() {
        let mut t = Ticks::default();
        let t0 = Instant::now();
        assert!(t.due(t0, 1), "the detent");
        assert!(!t.mark_due(t0 + Duration::from_millis(1)), "an edge cut in on a detent");
        assert!(t.mark_due(t0 + TICK_FLOOR), "and one floor later it is allowed");
        // A second edge inside the floor of the first gives way too — one actuator, one rate.
        assert!(!t.mark_due(t0 + TICK_FLOOR + Duration::from_millis(1)));
        assert_eq!(t.mark_counts(), (1, 2), "one edge felt, two that gave way");
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
            fn mark(&self, _: Mark) {
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

    // ── The geometry a finger cannot see ────────────────────────────────────────────────────────

    /// **Chatter is the engineering risk in the whole of this, and [`ARM_MM`] is the answer.**
    ///
    /// The pad samples at ~124 Hz, so a hand resting near the centre boundary — or drifting across
    /// it — crosses it many times a second, and without hysteresis that is a machine gun rather
    /// than a boundary. Here the finger oscillates ±0.5 mm about the line for 40 frames, which is a
    /// third of a second and **larger than anything a still hand does**: the stillest 265 ms in the
    /// 891-contact capture wandered over a 1.10 mm band, and one frame of it moved 0.3 mm.
    ///
    /// **The control is the second half and is not decoration.** A wobble that produced one mark
    /// looks identical to a wobble the detector never saw, so the same 40 frames are run through a
    /// bare [`Radial`] with **no** arming band, where they must produce 40. That is the instrument
    /// making a non-zero before its zero is believed, and it is what would have caught a detector
    /// that had silently stopped detecting.
    ///
    /// **How to make it go red:** set [`ARM_MM`] to 0.0. The first assertion gets 40.
    #[test]
    fn a_finger_wobbling_on_the_centre_boundary_is_one_edge_and_not_forty() {
        let wobble: Vec<f32> =
            (0..40).map(|i| BOUNDARY_MM + if i % 2 == 0 { -0.5 } else { 0.5 }).collect();

        let mut pad = Pad::default();
        // Start clear of the band, so the detector is armed and knows which side it began on.
        pad.frame(frame(at_mm(0.0, BOUNDARY_MM + 5.0)));
        let felt: usize =
            wobble.iter().map(|&r| marks_of(&pad.frame(frame(at_mm(0.0, r))), Edge::Centre)).sum();
        assert_eq!(felt, 1, "a 1 mm wobble on the line was felt {felt} times");

        // The control: the same 40 crossings with no arming band at all, primed from the same
        // place so that the two arms differ in exactly one thing.
        let mut bare = Radial::default();
        bare.crossed((BOUNDARY_MM + 5.0) / RADIUS_MM, CENTRE, 0.0);
        let n = wobble.iter().filter(|&&r| bare.crossed(r / RADIUS_MM, CENTRE, 0.0).is_some()).count();
        assert_eq!(n, 40, "the control never fired, so the first assertion measured nothing");
    }

    /// **The arming band is a gate on the mark and never a move of the boundary.**
    ///
    /// Crossing [`CENTRE`] is what takes the finger off the wheel, and that behaviour is not this
    /// feature's to change — a Schmitt trigger would have shifted it by [`ARM_MM`] in each
    /// direction. So both halves must land on the line: `Act::Up` at 12.58 mm and **the mark in the
    /// same frame**, rather than 1.5 mm further in once the band was cleared. A boundary announced
    /// late is a boundary reported in the wrong place.
    ///
    /// **How to make it go red:** make [`Radial`] a Schmitt trigger — test `r >= at + arm` going out
    /// and `r < at - arm` coming in, instead of arming on the way through. The `Act::Up` still
    /// arrives at the line and the mark no longer does.
    #[test]
    fn the_wheel_and_the_mark_both_land_on_the_line_and_not_on_the_arming_band() {
        let mut pad = Pad::default();
        pad.frame(frame(at_mm(0.0, BOUNDARY_MM + 5.0)));
        assert!(pad.on_ring());
        // One hundredth of a millimetre inside the line is inside the button, arming band or no —
        // and it is a hundred and fifty times finer than the band, so nothing here is a rounding.
        let acts = pad.frame(frame(at_mm(0.0, BOUNDARY_MM - 0.01)));
        assert!(acts.contains(&Act::Up), "the contact did not end at the line: {acts:?}");
        assert_eq!(marks_of(&acts, Edge::Centre), 1, "the mark did not land on the line: {acts:?}");
        assert!(!pad.on_ring());
    }

    /// **Why the centre edge can never starve a detent, said at the level where it is a property of
    /// the geometry rather than of the rate limiter.**
    ///
    /// A detent comes from `Act::Moved`, and moving *across* [`CENTRE`] is precisely the frame in
    /// which the contact starts or ends — `Act::Down` or `Act::Up`, never `Act::Moved`. So the two
    /// do not merely take turns: **they cannot occur in the same frame at all**, which is what
    /// makes this the edge that is on by default. A radial line is invisible to
    /// `wheel::position_at_angle`, which never reads a radius.
    ///
    /// **How to make it go red:** have `Pad::frame` emit `Act::Moved` for a contact in the centre
    /// as well as on the ring.
    #[test]
    fn the_centre_edge_never_shares_a_frame_with_a_detent() {
        let mut crossings = 0;
        for click in [0.0, 7.0, 24.0, 41.0, 60.0, 84.0] {
            let mut pad = Pad::default();
            // In from well outside the band to well inside it, and back out, a third of a
            // millimetre at a time — finer than the pad's own resolution at any speed.
            let sweep = (0..200).map(|i| 30.0 - f32::from(i as u16) * 0.3);
            for r in sweep.chain((0..200).map(|i| f32::from(i as u16).mul_add(0.3, 0.6))) {
                let acts = pad.frame(frame(at_mm(click, r.max(0.6))));
                let m = marks_of(&acts, Edge::Centre);
                crossings += m;
                assert!(
                    m == 0 || !acts.iter().any(|a| matches!(a, Act::Moved(_, _))),
                    "a centre edge and a detent in one frame at {click}: {acts:?}"
                );
            }
        }
        // The control: a sweep that crossed nothing proves nothing about frames that do.
        assert_eq!(crossings, 12, "six sweeps in and out is twelve crossings, not {crossings}");
    }

    /// **A band edge does share its frame with a detent, and that is the argument against it stated
    /// as a test rather than as an opinion.**
    ///
    /// `Edge::Band` is angular, which is the axis the detents already use, so a crossing arrives
    /// *inside* a turn rather than beside one. That is why it is off by default and why
    /// [`Ticks`]'s priority rule has to be structural: on this edge the contention is real.
    #[test]
    fn a_band_edge_does_share_its_frame_with_a_detent_which_is_why_it_is_not_on() {
        let mut pad = Pad::default();
        pad.frame(frame(at_mm(0.0, 25.0)));
        let mut shared = 0;
        for click in 1..=30 {
            let acts = pad.frame(frame(at_mm(f64::from(click), 25.0)));
            if marks_of(&acts, Edge::Band) > 0 && acts.iter().any(|a| matches!(a, Act::Moved(_, _))) {
                shared += 1;
            }
        }
        assert!(shared > 0, "the band edge never met a detent, so this test measured nothing");
        // Leaving MENU's band at 8/9 and entering Next's at 16/17 — two crossings in a quarter turn.
        assert_eq!(shared, 2, "a quarter turn crosses two band edges, not {shared}");
    }

    /// **A contact that begins right on a boundary still gets its first crossing.**
    ///
    /// Both detectors start *armed*, and for [`Angular`] that had to be written out — a derived
    /// `Default` gives `false`, which silently swallows the first crossing whenever a finger
    /// happens to land within [`ARM_CLICKS`] of a band edge. Click 8 is exactly that: the last
    /// click `wheel::quadrant` still calls MENU, half a click from where it stops.
    ///
    /// **How to make it go red:** derive `Default` for `Angular` instead of writing it.
    #[test]
    fn a_contact_that_begins_on_a_boundary_still_reports_leaving_it() {
        let mut pad = Pad::default();
        pad.frame(frame(at_mm(8.0, 25.0)));
        let acts = pad.frame(frame(at_mm(12.0, 25.0)));
        assert_eq!(marks_of(&acts, Edge::Band), 1, "the first crossing was swallowed: {acts:?}");
    }

    /// A finger that lifts inside the centre button and comes down on the ring has crossed nothing:
    /// the line is between two places on the surface, not between two contacts. Without the reset a
    /// new gesture would open with an edge the hand never felt.
    #[test]
    fn a_lift_and_a_landing_on_the_other_side_is_not_a_crossing() {
        let mut pad = Pad::default();
        pad.frame(frame(at_mm(0.0, 5.0)));
        assert_eq!(marks_of(&pad.frame(Frame { surface: PAD, contact: None }), Edge::Centre), 0);
        let acts = pad.frame(frame(at_mm(0.0, 25.0)));
        assert_eq!(marks_of(&acts, Edge::Centre), 0, "a new contact was reported as a crossing");
        // And the contact is a real one, so this is not a test of a pad that emitted nothing.
        assert!(matches!(acts.first(), Some(Act::Down(_, _))), "{acts:?}");
    }

    /// **The hand a wheel is worked with is never told about the default edge — and would be told
    /// about the other one eight times a turn.** This is the whole argument for the smallest set,
    /// as arithmetic rather than as an opinion.
    ///
    /// One full turn at the **median radius a hand actually circles at**, 18.7 mm, which is 6.1 mm
    /// clear of the 12.58 mm line. The boundary that is on says nothing at all; the boundary that
    /// is off would speak on 8 of those frames, in the middle of the detent stream, on the same
    /// angular axis the detents are already using. **Over-buzzing is the likely failure of a
    /// feature like this, and a surface that ticks constantly conveys less than one that ticks
    /// rarely.**
    #[test]
    fn an_ordinary_turn_says_nothing_on_the_edge_that_is_on_and_eight_things_on_the_one_that_is_not()
    {
        let mut pad = Pad::default();
        let (mut on, mut off, mut turned) = (0, 0, 0);
        for i in 0..=96 {
            let acts = pad.frame(frame(at_mm(f64::from(i), 18.7)));
            on += marks_of(&acts, Edge::Centre) + marks_of(&acts, Edge::Rim);
            off += marks_of(&acts, Edge::Band);
            turned += acts.iter().filter(|a| matches!(a, Act::Moved(_, _))).count();
        }
        assert!(turned > 90, "the wheel barely turned, so this measured nothing: {turned}");
        assert_eq!(on, 0, "a turn at the median circling radius crossed {on} of the felt edges");
        assert_eq!(off, 8, "and eight label-band edges, which is why they are not felt");
    }

    /// **The rim was never reached by the hand this was measured from**, which is the argument for
    /// it being off. 31.4 mm was the furthest of 899 samples and the line is at 37.0.
    #[test]
    fn the_rim_sits_outside_the_radius_the_hand_was_measured_to_reach() {
        let g = Geometry::of(PAD).expect("a surface");
        assert!((g.radius - 37.0).abs() < 0.01, "the rim is at {} mm", g.radius);
        let mut pad = Pad::default();
        pad.frame(frame(at_mm(0.0, 20.0)));
        // The furthest the capture's hand went.
        assert_eq!(marks_of(&pad.frame(frame(at_mm(24.0, 31.4))), Edge::Rim), 0);
        // And it is a real boundary when a finger does go out there — into the corner of the pad.
        assert_eq!(marks_of(&pad.frame(frame(at_mm(24.0, 45.0))), Edge::Rim), 1);
    }

    /// **Which edges are felt is a launch-time choice**, because software cannot check that
    /// something feels right and every round trip through a rebuild costs the operator a sitting.
    /// An unknown word keeps the default rather than silently selecting nothing — two arms of a
    /// comparison that differ only in a typo would read as different and behave the same.
    #[test]
    fn the_edges_being_felt_are_a_set_and_an_unknown_word_keeps_the_default() {
        assert!(Felt::default().has(Edge::Centre), "the default is the centre boundary");
        assert!(!Felt::default().has(Edge::Rim));
        assert!(!Felt::default().has(Edge::Band));
        assert_eq!(Felt::parse("off"), Felt { centre: false, rim: false, band: false });
        assert_eq!(Felt::parse("all"), Felt { centre: true, rim: true, band: true });
        assert_eq!(Felt::parse("centre,bands"), Felt { centre: true, rim: false, band: true });
        assert_eq!(Felt::parse(" rim , center "), Felt { centre: true, rim: true, band: false });
        assert_eq!(Felt::parse("wheel"), Felt::default(), "an unknown word kept the default");
        assert_eq!(Felt::parse(""), Felt::default());
        assert_eq!(Felt::default().describe(), "centre");
        assert_eq!(Felt::parse("all").describe(), "centre + rim + bands");
        assert_eq!(Felt::parse("off").describe(), "no edges");
    }

    /// **An edge that is not being felt does not reach a fingertip**, which is what makes `off` a
    /// real control arm rather than a differently-worded default. The `Pad` reports every crossing
    /// either way, because the diagnostic line wants the geometry whether or not the actuator does.
    #[test]
    fn only_the_edges_being_felt_reach_the_sinks() {
        struct Counter(&'static AtomicU32);
        impl Detents for Counter {
            fn describe(&self) -> &'static str {
                "a counter"
            }
            fn present(&self) -> bool {
                true
            }
            fn detent(&self) {}
            fn mark(&self, _: Mark) {
                self.0.fetch_add(1, Ordering::Relaxed);
            }
        }
        static SEEN: AtomicU32 = AtomicU32::new(0);

        let mut f = Feedback::default();
        f.add(Box::new(Counter(&SEEN)));
        f.feel(Felt::default());
        f.mark(Mark { edge: Edge::Rim, entering: true });
        f.mark(Mark { edge: Edge::Band, entering: true });
        assert_eq!(SEEN.load(Ordering::Relaxed), 0, "an edge nobody asked for was felt");
        f.mark(Mark { edge: Edge::Centre, entering: true });
        assert_eq!(SEEN.load(Ordering::Relaxed), 1, "the edge that is on was not");
        // And `off` reaches nothing at all.
        f.feel(Felt::parse("off"));
        f.mark(Mark { edge: Edge::Centre, entering: false });
        assert_eq!(SEEN.load(Ordering::Relaxed), 1);
    }

    /// Which way the line was crossed, since the one spare pattern can be spent on saying so.
    /// `entering` is *towards the region the edge is named for* — into the button, out past the rim.
    #[test]
    fn a_crossing_says_which_way_it_went() {
        let mut pad = Pad::default();
        pad.frame(frame(at_mm(0.0, 25.0)));
        let inward = pad.frame(frame(at_mm(0.0, 5.0)));
        assert!(inward.contains(&Act::Mark(Mark { edge: Edge::Centre, entering: true })), "{inward:?}");
        let outward = pad.frame(frame(at_mm(0.0, 25.0)));
        assert!(
            outward.contains(&Act::Mark(Mark { edge: Edge::Centre, entering: false })),
            "{outward:?}"
        );
        // The rim is named for what is outside it, so going out is going in.
        let mut pad = Pad::default();
        pad.frame(frame(at_mm(0.0, 25.0)));
        let out = pad.frame(frame(at_mm(0.0, 45.0)));
        assert!(out.contains(&Act::Mark(Mark { edge: Edge::Rim, entering: true })), "{out:?}");
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
