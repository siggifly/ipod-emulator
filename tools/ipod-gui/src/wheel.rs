//! Pointer angle -> click-wheel position, and the five button zones.
//!
//! Everything in this file is arithmetic on a 96-position ring, kept out of `main.rs` so it can be
//! tested without a window. The one thing worth stating loudly is which parts are *derived* and
//! which are *chosen*, because the difference decides what a wrong answer here would mean.
//!
//! **Derived.** 96 clicks per rotation, and that clockwise motion increases the value. Rockbox's
//! `button-clickwheel.c` gives both ("Highest wheel = 0x5F, clockwise increases"), and
//! [`ipod_machine::WHEEL_CLICKS_PER_ROTATION`] is where this project already records it. The wrap is
//! therefore modular in 96 and a rotation is a ring, not a range.
//!
//! **Chosen.** Where position 0 sits *physically*. Nothing in RetailOS, in the boot ROM, or in
//! Rockbox pins the wheel's zero to an angle on the bezel — the firmware only ever consumes
//! *differences* between successive frames (`0x000dd018`, the scroll accumulator, wraps at 0x60 and
//! folds the delta into `[state+0x10]`). So this file puts 0 at twelve o'clock and increases
//! clockwise because that is legible to a person dragging a mouse, and for no other reason. If a
//! future capture of a real 5G pins the zero somewhere else, only [`position_at_angle`] changes and
//! nothing downstream of it notices — which is the point of keeping the choice in one function.

use std::f32::consts::TAU;

use ipod_machine::{
    WHEEL_CLICKS_PER_ROTATION, WHEEL_LEFT, WHEEL_MENU, WHEEL_PLAY, WHEEL_RIGHT, WHEEL_SELECT,
};

/// 96, as a signed value — every calculation here is a difference on the ring.
pub const CLICKS: i32 = WHEEL_CLICKS_PER_ROTATION as i32;

/// The wheel position for a pointer at `(dx, dy)` from the wheel's centre, in screen coordinates
/// (x right, **y down**, which is what every GUI toolkit hands you and the opposite of the maths
/// convention).
///
/// Twelve o'clock is 0 and the value increases clockwise; see the module note on why that is a
/// choice rather than a measurement. A pointer exactly at the centre has no angle, so it answers 0
/// rather than whatever `atan2(0, 0)` happens to be — callers must not treat the centre as a
/// position, and [`WheelRing::hit`] never gives them one.
pub fn position_at_angle(dx: f32, dy: f32) -> u8 {
    // atan2(x, -y) puts zero at twelve o'clock and grows clockwise on a y-down axis: straight up is
    // (0, -1) -> atan2(0, 1) = 0; right is (1, 0) -> atan2(1, 0) = +pi/2.
    let theta = dx.atan2(-dy);
    let turns = theta / TAU;
    // rem_euclid on the float first so the rounding happens on a value already inside one turn --
    // rounding first can produce exactly 96, which is a position that does not exist.
    let clicks = (turns * CLICKS as f32).round() as i32;
    clicks.rem_euclid(CLICKS) as u8
}

/// The shortest signed number of clicks from `from` to `to` on the 96-ring.
///
/// Shortest-path rather than raw subtraction: a drag that crosses twelve o'clock moves one click,
/// and a naive `to - from` would report 95 of them in the other direction. The tie at exactly half
/// a rotation resolves to +48 — arbitrary, and unreachable in practice because the UI samples the
/// pointer far more often than once per half-turn.
pub fn shortest_delta(from: u8, to: u8) -> i32 {
    let raw = to as i32 - from as i32;
    (raw + CLICKS / 2).rem_euclid(CLICKS) - CLICKS / 2
}

/// The five buttons, in the order they are drawn: the ring's four printed labels, then the centre.
///
/// The mask is the streaming frame's bit order relative to bit 8, straight from `ipod-machine`;
/// nothing here re-derives it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Button {
    Menu,
    Next,
    Prev,
    Play,
    Select,
}

impl Button {
    pub const ALL: [Button; 5] = [
        Button::Menu,
        Button::Next,
        Button::Prev,
        Button::Play,
        Button::Select,
    ];

    /// A button by name, using the same spellings the command line's `--wheel` scripts accept —
    /// `ipod_machine::wheel_button` is the one place those live, so the two cannot drift.
    #[allow(dead_code)]  // retired when: something in the window names a button in text — §16.8's keys map letters to variants directly and the drawn labels raise a pointer stream, so nothing in this program spells a button out loud yet
    pub fn parse(name: &str) -> Option<Button> {
        let mask = ipod_machine::wheel_button(name.trim())?;
        Button::ALL.into_iter().find(|b| b.mask() == mask)
    }

    pub fn mask(self) -> u8 {
        match self {
            Button::Menu => WHEEL_MENU,
            Button::Next => WHEEL_RIGHT,
            Button::Prev => WHEEL_LEFT,
            Button::Play => WHEEL_PLAY,
            Button::Select => WHEEL_SELECT,
        }
    }

    /// What the bezel says, in words. Only Menu is *printed* as text on the real device; the other
    /// three are transport glyphs, drawn as geometry by `main::transport` rather than as
    /// codepoints, so these names exist for logs and tests rather than for the screen.
    #[allow(dead_code)]  // retired when: a refusal or a log names the control that was pressed — §7.4's held sentence is about the machine rather than about which button reached for it, so today nothing asks
    pub fn label(self) -> &'static str {
        match self {
            Button::Menu => "MENU",
            Button::Next => "next",
            Button::Prev => "previous",
            Button::Play => "play/pause",
            Button::Select => "select",
        }
    }

    /// Which quarter of the ring the label sits in, as a position range on the 96-ring. Menu is at
    /// twelve o'clock, next at three, play at six, prev at nine — the real bezel's layout.
    pub fn centre_click(self) -> Option<i32> {
        Some(match self {
            Button::Menu => 0,
            Button::Next => 24,
            Button::Play => 48,
            Button::Prev => 72,
            Button::Select => return None,
        })
    }
}

/// The wheel's geometry in whatever coordinate space the caller draws in: a centre, the ring's two
/// radii, and the select button's radius.
///
/// Proportions are the real device's, measured off Apple's published dimensions for the 5G: the
/// wheel is 27 mm across the outer ring and the select button 13 mm, on a 61.8 mm-wide case. They
/// are held as ratios of the wheel's outer radius so the whole device scales with the window.
#[derive(Clone, Copy, Debug)]
pub struct WheelRing {
    pub cx: f32,
    pub cy: f32,
    pub outer: f32,
    pub inner: f32,
    pub select: f32,
}

/// What a press at a point means.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Hit {
    /// On the ring, at this position. A drag from here rotates.
    Ring(u8),
    /// On one of the four printed labels — still on the ring, so it also carries a position.
    RingButton(Button, u8),
    Select,
    /// Off the wheel entirely.
    None,
}

impl WheelRing {
    /// The 5G's own proportions: a ~28 mm wheel with a ~13 mm select button in it, so the button is
    /// 13/28 of the diameter and the ring's inner edge sits just outside it. The narrow gap between
    /// `select` and `inner` is the bezel line on the real part, and a press landing in it is a press
    /// on neither — which is the honest answer, since on the hardware it is the moulding.
    pub fn new(cx: f32, cy: f32, outer: f32) -> Self {
        WheelRing {
            cx,
            cy,
            outer,
            inner: outer * 0.52,
            select: outer * 0.465,
        }
    }

    /// Where a point falls. The ring's four labels are wide zones centred on the printed glyph
    /// rather than small dots at it, because that is how the real part is wired — the membrane
    /// under a label is one switch across most of its quarter. See [`quadrant`] for the width and
    /// for why there is a dead band between them.
    pub fn hit(&self, x: f32, y: f32) -> Hit {
        let (dx, dy) = (x - self.cx, y - self.cy);
        let r = (dx * dx + dy * dy).sqrt();
        if r <= self.select {
            return Hit::Select;
        }
        if r < self.inner || r > self.outer {
            return Hit::None;
        }
        let p = position_at_angle(dx, dy);
        match quadrant(p) {
            Some(b) => Hit::RingButton(b, p),
            None => Hit::Ring(p),
        }
    }

    /// The point on the ring's midline at a given position — where a label is drawn, and where the
    /// touch indicator goes.
    #[allow(dead_code)]  // retired when: something in the window draws AT a wheel position — §7.4 puts the backlog on the cradle label and says explicitly it is *never on the wheel itself*, so the indicator this was written for is a thing the design declined
    pub fn point_at(&self, pos: u8) -> (f32, f32) {
        let mid = (self.inner + self.outer) * 0.5;
        let theta = pos as f32 / CLICKS as f32 * TAU;
        (self.cx + mid * theta.sin(), self.cy - mid * theta.cos())
    }
}

/// Which printed label owns a position, or `None` between two of them.
///
/// Each label owns the 16 clicks centred on its quadrant — two thirds of the quarter — leaving a
/// dead band either side. The band is not decoration: without it a drag that starts anywhere on the
/// ring is also a button press, and a wheel where you cannot scroll without pressing Menu is not a
/// wheel. The real membrane has the same property for the same reason.
pub fn quadrant(pos: u8) -> Option<Button> {
    for b in Button::ALL {
        let Some(c) = b.centre_click() else { continue };
        if shortest_delta(pos, c as u8).abs() <= 8 {
            return Some(b);
        }
    }
    None
}

/// **The window's one finger on the drawn wheel**, and the events it produces.
///
/// Everything above this is arithmetic on a ring; this is the small piece of state that turns
/// *where the pointer is* into *what the wheel did*, and it lives here for the same reason the
/// arithmetic does — none of it needs a window to be true, and `main.rs` has no business holding a
/// position between two pointer events.
///
/// **One finger, and that is the hardware rather than a simplification.** A 5G's wheel is one
/// capacitive surface: two touches on it are not two positions, and the streaming frame has one
/// `position` byte and one touched bit to say so. So a key step arriving while a pointer is down is
/// **refused** rather than queued — the pointer owns the wheel until it lifts, and the alternative
/// is two writers of one `at` disagreeing about where the finger is.
///
/// **`Touch` is emitted once and `Release` once**, whichever control started the contact. A drag
/// that begins on the MENU label is a button press *and* a touch at that position, which is what
/// [`WheelRing::hit`] answers and what the real membrane does; the dead band either side of each
/// label is what stops every drag being one.
///
/// **A scroll gesture is a contact too**, and it is the one whose *end* nobody sends: a pointer
/// lifts and a key comes up, but `PointerScrollEvent` carries a delta and nothing else — Slint's
/// markup type is `{ delta-x, delta-y, modifiers }` and winit's `phase` is dropped before it gets
/// there (`i-slint-core-1.17.1/items/input_items.rs:196`). So the contact is closed by a timer
/// instead, [`SCROLL_LIFT`] after the last delta, and [`Finger::scroll_lifted`] is the edge the
/// window's own timer supplies. Nothing here reads a clock; the *gesture's* end is a fact about a
/// hand and belongs to the window, and every event this produces is anchored in the machine's
/// simulated time by `emu::drain` like every other one.
#[derive(Default)]
pub struct Finger {
    touch: Touch,
    /// Scroll delta that has not yet added up to a whole click, in logical pixels, signed the same
    /// way a `Step` is. Held here rather than inside [`Touch::Scroll`] so that enum stays `Eq` —
    /// every other rule in this file is written as a comparison against a variant.
    residue: f32,
}

/// **Logical pixels of scroll per detent. 60, and it is Slint's own figure rather than a taste.**
///
/// Slint's winit backend turns a notched wheel's `MouseScrollDelta::LineDelta(_, ±1)` into **±60
/// logical pixels** (`i-slint-backend-winit-1.17.1/event_loop.rs:403`) and passes a trackpad's
/// `PixelDelta` through in logical pixels unchanged (`:404-406`). Taking 60 as one detent therefore
/// makes **one notch of a mouse wheel one click of this one** — the mapping a person can predict
/// without being told it, and the true one for the part being drawn, where one detent is one item.
///
/// A trackpad has no notches, so [`Finger::residue`] carries what is left over between events: at
/// 120 Hz a gentle drag delivers ten or twelve pixels a frame, and without the residue every one of
/// them would round to nothing and the wheel would be dead to the device most people have.
///
/// **It is also what makes §7.4's "momentum scrolling is not viable and is not offered" a figure
/// rather than a hope.** A whole rotation is 96 clicks, which is `emu::MAX_QUEUE` and about two
/// seconds of the wheel's own drain — and at 60 px a click it costs **5 760 px** of scrolling to
/// ask for one. Past that `Link::push` drops the surplus and counts it in `input_dropped`; there is
/// deliberately no second cap here, because the queue is the physical statement and a cap invented
/// beside it would be a second one to disagree with it.
pub const PX_PER_CLICK: f32 = 60.0;

/// **How long after the last delta the finger leaves the wheel. 300 ms.**
///
/// It has to be longer than the gap *inside* one gesture and shorter than a person minds. A
/// trackpad's deltas arrive at the display's refresh rate — 8 to 17 ms apart — and a notched mouse
/// turned deliberately, one click at a time, leaves a couple of hundred milliseconds between
/// notches; shorter than that and one gesture becomes a string of separate contacts, which is a lie
/// about a hand that never left the wheel and costs three events per click instead of one.
///
/// What it buys the other way is nothing at all: a finger resting on the wheel after you stop
/// turning it is the state the real part spends most of its life in, and the frame says so with one
/// bit. The number is the same 300 ms as `emu::MIN_BUTTON_HOLD_USEC`, and by coincidence rather
/// than derivation — that one is bounded from below by Apple's 150 ms diagnostics poll, and this
/// one by a hand.
pub const SCROLL_LIFT: std::time::Duration = std::time::Duration::from_millis(300);

/// One whole rotation's worth of scrolling — 5 760 logical pixels, and the most a single event is
/// allowed to mean. See [`Finger::scrolled`], which is the only place it is used and where the
/// argument for it being a sanity bound rather than a cap lives.
const ONE_TURN_PX: f32 = CLICKS as f32 * PX_PER_CLICK;

/// What is on the wheel right now.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
enum Touch {
    /// Nothing.
    #[default]
    Off,
    /// A pointer, at this ring position, holding this button mask — `0` for bare ring.
    Pointer { at: u8, button: u8 },
    /// A key. It has a direction and **no position**: §16.8's `↑` `↓` ask for one detent, not for
    /// somewhere to be.
    Key,
    /// A scroll gesture, which has a direction and no position for the same reason a key does — a
    /// wheel event says *how far*, never *where*. What is left over between two of them is
    /// [`Finger::residue`].
    Scroll,
}

impl Finger {
    /// A pointer went down at `(x, y)`, in the wheel's own coordinates — x right, **y down**, from
    /// its centre.
    ///
    /// Answers nothing for a press the wheel does not own: the centre button is a control of its
    /// own with its own route, and the moulding between it and the ring is *"a press on neither —
    /// which is the honest answer, since on the hardware it is the moulding"*.
    ///
    /// **A pointer arriving on a contact somebody else started takes it over rather than opening a
    /// second one**, which is what the `Touch::Off` test says and what one capacitive surface can
    /// mean: put a finger down while a scroll is still settling and the wheel does not leave and
    /// come back, it simply has a position now.
    pub fn pressed(&mut self, ring: &WheelRing, x: f32, y: f32) -> Vec<ipod_machine::WheelEvent> {
        let (at, button) = match ring.hit(x, y) {
            Hit::Ring(p) => (p, 0),
            Hit::RingButton(b, p) => (p, b.mask()),
            Hit::Select | Hit::None => return Vec::new(),
        };
        let mut out = Vec::new();
        if self.touch == Touch::Off {
            out.push(ipod_machine::WheelEvent::Touch);
        }
        self.touch = Touch::Pointer { at, button };
        self.residue = 0.0;
        if button != 0 {
            out.push(ipod_machine::WheelEvent::Button(button, true));
        }
        out
    }

    /// **A finger arrived on the surface at `(x, y)` — contact, and not a press.**
    ///
    /// The difference from [`Finger::pressed`] is the whole of why this exists, and it is a
    /// property of the input device rather than a policy: a mouse has no way to *rest* on a
    /// wheel, so a pointer going down is the contact and the press at once and `pressed` sends the
    /// label's `Button` along with the `Touch`. A capacitive surface says the two separately, and
    /// so does the part being emulated — a thumb sitting on MENU is a finger on the wheel, not
    /// MENU pressed. §21.8's trackpad reports contact and click on different events, so it uses
    /// this one and raises the button from the click.
    ///
    /// The centre and the moulding answer nothing, exactly as they do for a pointer: the centre
    /// button is a control of its own with its own route, and the gap between it and the ring is a
    /// press on neither.
    pub fn touched(&mut self, ring: &WheelRing, x: f32, y: f32) -> Vec<ipod_machine::WheelEvent> {
        let at = match ring.hit(x, y) {
            Hit::Ring(p) | Hit::RingButton(_, p) => p,
            Hit::Select | Hit::None => return Vec::new(),
        };
        let mut out = Vec::new();
        if self.touch == Touch::Off {
            out.push(ipod_machine::WheelEvent::Touch);
        }
        self.touch = Touch::Pointer { at, button: 0 };
        self.residue = 0.0;
        out
    }

    /// The pointer moved to `(x, y)` — one `Step` per detent crossed, the short way round.
    ///
    /// **Only the angle is read, and the radius is deliberately ignored.** A drag that wanders
    /// outside the ring's outer edge while turning is one a person means; a finger that has to stay
    /// inside a 58 px annulus to keep scrolling is a wheel that stops working when you press
    /// slightly too hard. Where the finger went **on** is decided by [`WheelRing::hit`], which does
    /// read the radius; where it has got to since is an angle.
    pub fn moved(&mut self, x: f32, y: f32) -> Vec<ipod_machine::WheelEvent> {
        let Touch::Pointer { at, button } = self.touch else {
            return Vec::new();
        };
        let to = position_at_angle(x, y);
        let d = shortest_delta(at, to);
        if d == 0 {
            return Vec::new();
        }
        self.touch = Touch::Pointer { at: to, button };
        vec![ipod_machine::WheelEvent::Step(d.signum() as i8); d.unsigned_abs() as usize]
    }

    /// The pointer lifted. Whatever it was holding comes up, then the finger leaves.
    ///
    /// The order is the hardware's: a button that came up *after* the touch ended would post a
    /// frame with the button still set and no finger on the wheel, which is a state the part cannot
    /// be in.
    pub fn released(&mut self) -> Vec<ipod_machine::WheelEvent> {
        let Touch::Pointer { button, .. } = self.touch else {
            return Vec::new();
        };
        self.touch = Touch::Off;
        let mut out = Vec::new();
        if button != 0 {
            out.push(ipod_machine::WheelEvent::Button(button, false));
        }
        out.push(ipod_machine::WheelEvent::Release);
        out
    }

    /// §16.8's `↑` `↓` (and `←` `→` over a machine): **one detent, by key**.
    ///
    /// The first one touches the wheel and the key's release lifts it, so holding the key down is
    /// one contact with a stream of clicks in it — which is what a scroll is. Auto-repeat is the
    /// repeat rate, and it is the platform's rather than one this program invents.
    ///
    /// **A key arriving on a settling scroll takes the contact over**, the way a pointer does: the
    /// state becomes `Key` whichever way the finger got here, so the key's own release is what
    /// lifts it. Writing that assignment inside the `Touch::Off` arm — which is where it was, when
    /// `Off` and `Pointer` were the only other states — would leave the wheel held by a scroll
    /// timer that has already been disarmed, and nothing would ever send the `Release`.
    pub fn keyed(&mut self, by: i8) -> Vec<ipod_machine::WheelEvent> {
        if by == 0 || matches!(self.touch, Touch::Pointer { .. }) {
            return Vec::new();
        }
        let mut out = Vec::new();
        if self.touch == Touch::Off {
            out.push(ipod_machine::WheelEvent::Touch);
        }
        self.touch = Touch::Key;
        self.residue = 0.0;
        out.push(ipod_machine::WheelEvent::Step(by.signum()));
        out
    }

    /// The key came up. Nothing happens if the contact was a pointer's — see the type's note on
    /// there being one finger.
    pub fn key_released(&mut self) -> Vec<ipod_machine::WheelEvent> {
        if self.touch != Touch::Key {
            return Vec::new();
        }
        self.touch = Touch::Off;
        vec![ipod_machine::WheelEvent::Release]
    }

    /// **A mouse wheel or a trackpad, over the drawn ring** — [`PX_PER_CLICK`] logical pixels a
    /// detent, with the remainder carried to the next event.
    ///
    /// **`delta_y` and not `delta_x`.** A menu is a vertical list, so the vertical axis is the one
    /// a person means; and on macOS ⇧-scroll is delivered as horizontal delta, so honouring x would
    /// make a chord this program has never defined turn the emulated wheel — which is the same rule
    /// §16.8's modifier guard states for keys.
    ///
    /// **The sign is the platform's rather than a choice.** Slint adds `delta_y` to a `Flickable`'s
    /// `viewport_y`, which runs from `0` at the top to a negative value at the bottom
    /// (`i-slint-core-1.17.1/items/flickable.rs:480`, `ensure_in_bound`), so scrolling *down* a list
    /// is a **negative** `delta_y`. Down a list is clockwise on this wheel, and clockwise is
    /// `Step(+1)` — see this module's note on why that direction is derived and not chosen.
    ///
    /// Refused while a pointer or a key owns the wheel, for the reason the type's own note gives:
    /// one capacitive surface, one finger, and two writers of one contact is how they come to
    /// disagree about whether it is there.
    pub fn scrolled(&mut self, delta_y: f32) -> Vec<ipod_machine::WheelEvent> {
        if !delta_y.is_finite() || matches!(self.touch, Touch::Pointer { .. } | Touch::Key) {
            return Vec::new();
        }
        let mut out = Vec::new();
        if self.touch == Touch::Off {
            out.push(ipod_machine::WheelEvent::Touch);
        }
        self.touch = Touch::Scroll;
        // **One event cannot mean more than one turn**, and this is a sanity bound on the number
        // rather than a policy about the wheel. `moved` gets the same bound for free — a ring's
        // shortest path is at most half a turn, so a drag can never ask for more than 48 clicks in
        // one sample however wild the pointer is — and a scroll has no geometry to get it from. A
        // delta of 10^9 px is not a gesture a hand made; without this it is a `Vec` of sixteen
        // million events built before `Link::push` ever sees one.
        //
        // It is deliberately **not** a cap on how fast the wheel may be turned: at one turn a
        // frame this is still an order of magnitude more than `emu::MAX_QUEUE` can take, so the
        // surplus still reaches `push`, is still dropped there, and is still counted in
        // `input_dropped` — which §7.4 wants, because *a refused step is a lie about what you did*
        // and a step this function had quietly swallowed could never be refused out loud.
        self.residue -= delta_y.clamp(-ONE_TURN_PX, ONE_TURN_PX);
        let clicks = (self.residue / PX_PER_CLICK) as i32;
        self.residue -= clicks as f32 * PX_PER_CLICK;
        if clicks != 0 {
            out.extend(std::iter::repeat_n(
                ipod_machine::WheelEvent::Step(clicks.signum() as i8),
                clicks.unsigned_abs() as usize,
            ));
        }
        out
    }

    /// **The scroll gesture went quiet, so the finger leaves.** [`SCROLL_LIFT`] after the last
    /// delta, timed by the window.
    ///
    /// It is the one edge this file cannot produce for itself: a pointer sends an up and a key
    /// sends a release, and a wheel event sends neither. Nothing happens if some other contact has
    /// taken the wheel over in the meantime, which is what makes a timer that has already been
    /// overtaken harmless rather than something the window has to remember to cancel.
    ///
    /// The remainder goes with the contact. Half a click of a gesture that ended is not the first
    /// half of the next one.
    pub fn scroll_lifted(&mut self) -> Vec<ipod_machine::WheelEvent> {
        if self.touch != Touch::Scroll {
            return Vec::new();
        }
        self.touch = Touch::Off;
        self.residue = 0.0;
        vec![ipod_machine::WheelEvent::Release]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn twelve_oclock_is_zero_and_the_value_grows_clockwise() {
        assert_eq!(position_at_angle(0.0, -1.0), 0, "straight up");
        assert_eq!(position_at_angle(1.0, 0.0), 24, "a quarter turn clockwise");
        assert_eq!(position_at_angle(0.0, 1.0), 48, "half a turn");
        assert_eq!(position_at_angle(-1.0, 0.0), 72, "three quarters");
    }

    /// The failure this test exists for: rounding a value of exactly one turn to 96, a position the
    /// device does not have. A frame carrying 96 would decode as position 0 with bit 22 set, which
    /// is not a position at all.
    #[test]
    fn no_angle_ever_produces_ninety_six() {
        for i in 0..3600 {
            let theta = i as f32 / 3600.0 * TAU;
            let p = position_at_angle(theta.sin(), -theta.cos());
            assert!(p < 96, "angle {i} tenths produced position {p}");
        }
    }

    /// The wrap. Each click owns the half-click either side of its own angle, so the boundary
    /// between 95 and 0 sits half a click anticlockwise of twelve o'clock — and this is the test
    /// that would catch a `%` where a `rem_euclid` belongs, which on a negative angle gives -1 and
    /// then panics or silently becomes 255.
    #[test]
    fn the_wrap_at_ninety_five_to_zero_is_where_it_belongs() {
        let click = |c: f32| {
            let theta = c / 96.0 * TAU;
            position_at_angle(theta.sin(), -theta.cos())
        };
        assert_eq!(click(-0.45), 0, "just inside 0's arc, anticlockwise");
        assert_eq!(click(-0.55), 95, "just past the boundary");
        assert_eq!(click(0.45), 0, "just inside 0's arc, clockwise");
        assert_eq!(click(-1.0), 95);
        assert_eq!(click(-96.0), 0, "a whole turn back is the same place");
        assert_eq!(click(-97.0), 95);
        assert_eq!(click(1000.0), (1000 % 96) as u8);
    }

    #[test]
    fn a_full_sweep_visits_every_position_exactly_once() {
        let mut seen = [0u32; 96];
        for i in 0..96 {
            // Sample the centre of each click's arc, which is where its own angle is.
            let theta = i as f32 / 96.0 * TAU;
            seen[position_at_angle(theta.sin(), -theta.cos()) as usize] += 1;
        }
        assert!(seen.iter().all(|&n| n == 1), "not a bijection: {seen:?}");
    }

    #[test]
    fn shortest_delta_takes_the_short_way_round() {
        assert_eq!(shortest_delta(0, 1), 1);
        assert_eq!(shortest_delta(1, 0), -1);
        assert_eq!(shortest_delta(0, 0), 0);
        // The whole reason this function exists: 95 -> 0 is one click clockwise, not 95 back.
        assert_eq!(shortest_delta(95, 0), 1);
        assert_eq!(shortest_delta(0, 95), -1);
        assert_eq!(shortest_delta(90, 5), 11);
        assert_eq!(shortest_delta(5, 90), -11);
    }

    /// Walking the delta from any position must land exactly on the target, for every pair. This is
    /// the property the emulator side depends on: it applies `Step(+1)`/`Step(-1)` `|delta|` times
    /// and must end up where the pointer is.
    #[test]
    fn walking_the_delta_lands_on_the_target_for_every_pair() {
        for from in 0..96u8 {
            for to in 0..96u8 {
                let d = shortest_delta(from, to);
                assert!(d.abs() <= 48, "{from}->{to} took the long way: {d}");
                let landed = (from as i32 + d).rem_euclid(CLICKS) as u8;
                assert_eq!(landed, to, "{from} + {d} should be {to}");
            }
        }
    }

    #[test]
    fn the_four_labels_sit_at_the_compass_points_and_do_not_overlap() {
        assert_eq!(quadrant(0), Some(Button::Menu));
        assert_eq!(quadrant(24), Some(Button::Next));
        assert_eq!(quadrant(48), Some(Button::Play));
        assert_eq!(quadrant(72), Some(Button::Prev));
        // Halfway between two labels is neither -- the dead band a drag needs.
        assert_eq!(quadrant(12), None);
        assert_eq!(quadrant(36), None);
        assert_eq!(quadrant(60), None);
        assert_eq!(quadrant(84), None);
        // And the wrap: 95 is one click anticlockwise of Menu's centre, so it is still Menu.
        assert_eq!(quadrant(95), Some(Button::Menu));
        assert_eq!(quadrant(88), Some(Button::Menu));
        assert_eq!(
            quadrant(87),
            None,
            "the eighth click out is the edge of the band"
        );
    }

    #[test]
    fn masks_match_the_emulators_own_constants() {
        assert_eq!(Button::Select.mask(), WHEEL_SELECT);
        assert_eq!(Button::Next.mask(), WHEEL_RIGHT);
        assert_eq!(Button::Prev.mask(), WHEEL_LEFT);
        assert_eq!(Button::Play.mask(), WHEEL_PLAY);
        assert_eq!(Button::Menu.mask(), WHEEL_MENU);
        // Five distinct bits, none of them overlapping -- a typo here would make two buttons one.
        let mut all = 0u8;
        for b in Button::ALL {
            assert_eq!(all & b.mask(), 0, "{b:?} collides");
            all |= b.mask();
        }
        assert_eq!(all, 0x1f);
    }

    #[test]
    fn the_centre_is_select_and_outside_the_ring_is_nothing() {
        let w = WheelRing::new(100.0, 100.0, 50.0);
        assert_eq!(w.hit(100.0, 100.0), Hit::Select);
        assert_eq!(
            w.hit(100.0, 80.0),
            Hit::Select,
            "inside the select radius (23 of 23.25)"
        );
        assert_eq!(w.hit(100.0, 40.0), Hit::None, "outside the outer radius");
        assert_eq!(
            w.hit(100.0, 75.25),
            Hit::None,
            "the bezel line between button and ring"
        );
        // Twelve o'clock on the ring itself is the Menu label.
        assert_eq!(w.hit(100.0, 62.0), Hit::RingButton(Button::Menu, 0));
        // And 45 degrees round from it is bare ring.
        let d = 38.0f32 / 2.0f32.sqrt();
        assert_eq!(w.hit(100.0 + d, 100.0 - d), Hit::Ring(12));
    }

    #[test]
    fn point_at_is_the_inverse_of_the_angle_mapping() {
        let w = WheelRing::new(0.0, 0.0, 100.0);
        for p in 0..96u8 {
            let (x, y) = w.point_at(p);
            assert_eq!(position_at_angle(x, y), p, "round trip failed at {p}");
        }
    }
    // ── The finger ──────────────────────────────────────────────────────────────────────────────

    use ipod_machine::WheelEvent::{Button as Btn, Release, Step, Touch};

    /// The unit ring the window uses: the pointer arrives in units of the wheel's outer radius, so
    /// the whole of `main.rs` needs no idea how big the drawing is.
    fn unit() -> WheelRing {
        WheelRing::new(0.0, 0.0, 1.0)
    }

    /// A point on the ring's midline at `pos`, in unit-radius coordinates.
    fn on_ring(pos: u8) -> (f32, f32) {
        let (x, y) = unit().point_at(pos);
        (x, y)
    }

    #[test]
    fn a_press_on_bare_ring_touches_the_wheel_and_presses_nothing() {
        let mut f = Finger::default();
        let (x, y) = on_ring(12);
        assert_eq!(f.pressed(&unit(), x, y), vec![Touch]);
        assert_eq!(f.released(), vec![Release]);
    }

    /// §7.4: the four labels are the machine's buttons, and a press on one is a touch **and** a
    /// button — which is what `hit` answers and what the membrane does.
    #[test]
    fn a_press_on_a_printed_label_is_a_touch_and_that_button() {
        let mut f = Finger::default();
        let (x, y) = on_ring(0);
        assert_eq!(f.pressed(&unit(), x, y), vec![Touch, Btn(WHEEL_MENU, true)]);
        // The release comes up before the finger leaves: a frame with a button set and no finger on
        // the wheel is a state the part cannot be in.
        assert_eq!(f.released(), vec![Btn(WHEEL_MENU, false), Release]);
    }

    /// The one that would catch a `hit` answering `Select` for the moulding, or a `main.rs` that
    /// sent `Touch` for a press on the case.
    #[test]
    fn a_press_the_ring_does_not_own_produces_nothing_and_leaves_no_finger_behind() {
        let mut f = Finger::default();
        // The centre button — its own control, with its own route.
        assert!(f.pressed(&unit(), 0.0, 0.0).is_empty());
        // The moulding between the button and the ring: `select` is 0.465 and `inner` is 0.52.
        assert!(f.pressed(&unit(), 0.0, -0.49).is_empty());
        // Off the wheel entirely.
        assert!(f.pressed(&unit(), 0.0, -2.0).is_empty());
        assert!(f.released().is_empty(), "a press that did nothing left a finger on the wheel");
    }

    /// **§21.8's trackpad, and the one thing it does that a pointer cannot.** A finger resting on
    /// the MENU label is a finger on the wheel; it is not MENU pressed. The control that makes
    /// this fail is `pressed` in place of `touched` — it answers `[Touch, Btn(MENU, true)]` and
    /// the emulated iPod goes to the main menu because somebody put a thumb down at twelve
    /// o'clock.
    #[test]
    fn a_contact_on_a_printed_label_is_a_touch_and_no_button() {
        let mut f = Finger::default();
        let (x, y) = on_ring(0);
        assert_eq!(f.touched(&unit(), x, y), vec![Touch]);
        assert_eq!(f.released(), vec![Release], "a contact that pressed nothing released something");
    }

    /// A contact turns the wheel exactly as a press does — the difference is the button, and
    /// nothing else. Two turns from the same start must give the same steps.
    #[test]
    fn a_contact_turns_the_wheel_the_same_way_a_press_does() {
        let steps = |f: &mut Finger| {
            let (x, y) = on_ring(9);
            f.moved(x, y)
        };
        let (x, y) = on_ring(6);

        let mut pressed = Finger::default();
        pressed.pressed(&unit(), x, y);
        let mut touched = Finger::default();
        touched.touched(&unit(), x, y);

        assert_eq!(steps(&mut pressed), vec![Step(1); 3]);
        assert_eq!(steps(&mut touched), vec![Step(1); 3]);
    }

    /// The same three places a press answers nothing for, asked of a contact: the centre is its
    /// own control, the moulding is neither, and off the ring is off it.
    #[test]
    fn a_contact_the_ring_does_not_own_leaves_no_finger_behind() {
        let mut f = Finger::default();
        assert!(f.touched(&unit(), 0.0, 0.0).is_empty(), "the centre button is not the wheel");
        assert!(f.touched(&unit(), 0.0, -0.49).is_empty(), "the moulding is neither control");
        assert!(f.touched(&unit(), 0.0, -2.0).is_empty(), "off the ring entirely");
        assert!(f.released().is_empty(), "a contact that did nothing left a finger on the wheel");
    }

    #[test]
    fn a_drag_is_one_step_per_detent_the_short_way_round() {
        let mut f = Finger::default();
        let (x, y) = on_ring(12);
        f.pressed(&unit(), x, y);
        let (x, y) = on_ring(15);
        assert_eq!(f.moved(x, y), vec![Step(1); 3]);
        // …and it is now AT 15, so the next move is relative to there rather than to where the
        // press was. A `moved` that forgot to advance `at` would send three more.
        let (x, y) = on_ring(16);
        assert_eq!(f.moved(x, y), vec![Step(1)]);
        // Across twelve o'clock the short way is one click, not ninety-five.
        let (x, y) = on_ring(95);
        f.moved(x, y);
        let (x, y) = on_ring(0);
        assert_eq!(f.moved(x, y), vec![Step(1)]);
    }

    /// The radius is deliberately not read after the press — see [`Finger::moved`].
    #[test]
    fn a_drag_that_wanders_off_the_ring_keeps_turning() {
        let mut f = Finger::default();
        let (x, y) = on_ring(0);
        f.pressed(&unit(), x, y);
        // Three o'clock, four radii out. The angle is 24 clicks round; the radius is nonsense.
        assert_eq!(f.moved(4.0, 0.0), vec![Step(1); 24]);
    }

    #[test]
    fn nothing_moves_the_wheel_while_no_pointer_is_down() {
        let mut f = Finger::default();
        assert!(f.moved(1.0, 0.0).is_empty(), "a hover turned the wheel");
        assert!(f.released().is_empty(), "a release with no press said something");
    }

    /// §16.8's `↑` `↓`: one contact, a stream of clicks in it, and the key's release lifts it.
    #[test]
    fn a_held_key_is_one_touch_with_a_stream_of_clicks_in_it() {
        let mut f = Finger::default();
        assert_eq!(f.keyed(1), vec![Touch, Step(1)]);
        assert_eq!(f.keyed(1), vec![Step(1)], "auto-repeat touched the wheel a second time");
        assert_eq!(f.keyed(-1), vec![Step(-1)]);
        assert_eq!(f.key_released(), vec![Release]);
        assert!(f.key_released().is_empty(), "the key came up twice and the wheel noticed twice");
    }

    /// One finger. The pointer owns the wheel until it lifts, because two writers of one `at` is
    /// two answers to where the finger is.
    #[test]
    fn a_key_is_refused_while_a_pointer_is_down_and_does_not_lift_it() {
        let mut f = Finger::default();
        let (x, y) = on_ring(12);
        assert_eq!(f.pressed(&unit(), x, y), vec![Touch]);
        assert!(f.keyed(1).is_empty(), "a key stepped the wheel out from under a drag");
        assert!(f.key_released().is_empty(), "a key release ended the pointer's touch");
        // …and the drag is still live: it still knows it is at 12.
        let (x, y) = on_ring(13);
        assert_eq!(f.moved(x, y), vec![Step(1)]);
        assert_eq!(f.released(), vec![Release]);
    }

    /// The other order: a key is holding the wheel and a pointer comes down on it. The contact is
    /// already made, so `Touch` is not sent twice — and the pointer takes it over.
    #[test]
    fn a_pointer_takes_over_a_wheel_a_key_was_already_holding() {
        let mut f = Finger::default();
        assert_eq!(f.keyed(1), vec![Touch, Step(1)]);
        let (x, y) = on_ring(0);
        assert_eq!(
            f.pressed(&unit(), x, y),
            vec![Btn(WHEEL_MENU, true)],
            "the wheel was touched a second time without ever being released"
        );
        assert_eq!(f.released(), vec![Btn(WHEEL_MENU, false), Release]);
    }

    // ── The scroll ──────────────────────────────────────────────────────────────────────────────

    /// **One notch of a mouse wheel is one detent**, which is the whole of [`PX_PER_CLICK`]'s
    /// argument, and the sign is the platform's.
    ///
    /// 60 is not a number typed here: Slint's winit backend multiplies a `LineDelta` by exactly 60
    /// to get logical pixels, so a notch arrives as ±60 and this asserts the round trip a person
    /// makes when they turn a wheel one click.
    #[test]
    fn one_notch_of_a_mouse_wheel_is_one_click_of_this_one() {
        let mut f = Finger::default();
        // Down the list is a negative `delta_y` and clockwise on the ring.
        assert_eq!(f.scrolled(-PX_PER_CLICK), vec![Touch, Step(1)]);
        assert_eq!(f.scrolled(-PX_PER_CLICK), vec![Step(1)], "the second notch touched again");
        assert_eq!(f.scrolled(PX_PER_CLICK * 3.0), vec![Step(-1); 3], "up the list is anticlockwise");
        assert_eq!(f.scroll_lifted(), vec![Release]);
        assert!(f.scroll_lifted().is_empty(), "the contact was lifted twice");
    }

    /// **A trackpad's remainder is carried rather than thrown away**, which is the difference
    /// between a wheel that works with the device most people have and one that is dead to it.
    ///
    /// Twelve pixels a frame is a gentle two-finger drag at 120 Hz. Five of them make a click; the
    /// four before it make nothing but must not be lost, and the sixth must not make a second one.
    #[test]
    fn a_trackpads_pixels_accumulate_into_whole_clicks_and_lose_nothing() {
        let mut f = Finger::default();
        assert_eq!(f.scrolled(-12.0), vec![Touch], "a first frame that is not yet a click");
        for _ in 0..3 {
            assert!(f.scrolled(-12.0).is_empty());
        }
        assert_eq!(f.scrolled(-12.0), vec![Step(1)], "five twelves are sixty and nothing arrived");
        assert!(f.scrolled(-12.0).is_empty(), "the residue was not spent");

        // …and the remainder does not survive the gesture that produced it: half a click of
        // something that ended is not the first half of the next one.
        f.scrolled(-30.0);
        assert_eq!(f.scroll_lifted(), vec![Release]);
        assert_eq!(f.scrolled(-30.0), vec![Touch], "half a click was carried across two contacts");
    }

    /// **A big delta is a whole run of clicks, in one event**, up to one turn — which is where the
    /// number stops being a gesture. A momentum flick is not otherwise special-cased:
    /// `emu::Link::push` is where a backlog stops and where a dropped step gets counted, and a
    /// policy cap beside it would be a second answer to one question.
    #[test]
    fn one_large_delta_is_as_many_clicks_as_it_paid_for_up_to_one_turn() {
        let mut f = Finger::default();
        let out = f.scrolled(-PX_PER_CLICK * CLICKS as f32);
        assert_eq!(out.len(), CLICKS as usize + 1, "a whole rotation is 96 clicks and one touch");
        assert_eq!(out[0], Touch);
        assert!(out[1..].iter().all(|e| *e == Step(1)));
        f.scroll_lifted();

        // Past a turn the number is not a hand's, and the bound is on the allocation rather than on
        // the wheel. **Ten turns first and `f32::MAX` second, in that order deliberately**: this
        // test has to be runnable with the clamp taken out, and unclamped the second line builds a
        // `Vec` of two billion events. Ten turns is red and cheap; the line under it is the case
        // the bound actually exists for and is only ever reached green.
        let ten = f.scrolled(-PX_PER_CLICK * CLICKS as f32 * 10.0);
        assert_eq!(ten.len(), CLICKS as usize + 1, "ten turns in one event was not bounded to one");
        f.scroll_lifted();
        let huge = f.scrolled(-f32::MAX);
        assert_eq!(huge.len(), CLICKS as usize + 1, "an impossible delta was not bounded");
        // …and the surplus is not banked either, or the next gesture would start owing it.
        assert!(f.scrolled(0.0).is_empty(), "a clamped delta left a residue behind it");
    }

    /// One finger, and the scroll is not exempt from it. A pointer or a key holding the wheel owns
    /// it, and a scroll that stepped it out from under a drag is two writers of one contact.
    #[test]
    fn a_scroll_is_refused_while_a_pointer_or_a_key_holds_the_wheel() {
        let mut f = Finger::default();
        let (x, y) = on_ring(12);
        f.pressed(&unit(), x, y);
        assert!(f.scrolled(-600.0).is_empty(), "a scroll turned the wheel during a drag");
        assert!(f.scroll_lifted().is_empty(), "a scroll's lift ended a pointer's contact");
        assert_eq!(f.released(), vec![Release]);

        assert_eq!(f.keyed(1), vec![Touch, Step(1)]);
        assert!(f.scrolled(-600.0).is_empty(), "a scroll turned the wheel under a held key");
        assert!(f.scroll_lifted().is_empty(), "a scroll's lift ended a key's contact");
        assert_eq!(f.key_released(), vec![Release]);
    }

    /// **A contact handed from a scroll to a pointer or a key is still one contact** — the finger
    /// does not leave and come back, and whoever took it over is who lifts it.
    ///
    /// The second half is the one that would go wrong silently: with `Touch::Key` assigned only on
    /// the `Off` arm, a key arriving on a settling scroll would step the wheel and leave the state
    /// reading `Scroll`, so `key_released` would answer nothing and the `Release` would depend on a
    /// timer the window had already disarmed.
    #[test]
    fn a_pointer_or_a_key_takes_over_a_scrolls_contact_and_is_what_lifts_it() {
        let mut f = Finger::default();
        assert_eq!(f.scrolled(-PX_PER_CLICK), vec![Touch, Step(1)]);
        let (x, y) = on_ring(0);
        assert_eq!(
            f.pressed(&unit(), x, y),
            vec![Btn(WHEEL_MENU, true)],
            "the wheel was touched a second time without ever being released"
        );
        assert!(f.scroll_lifted().is_empty(), "a stale scroll timer released a live drag");
        assert_eq!(f.released(), vec![Btn(WHEEL_MENU, false), Release]);

        assert_eq!(f.scrolled(-PX_PER_CLICK), vec![Touch, Step(1)]);
        assert_eq!(f.keyed(-1), vec![Step(-1)], "the key opened a second contact");
        assert!(f.scroll_lifted().is_empty(), "a stale scroll timer released a held key");
        assert_eq!(f.key_released(), vec![Release], "the key that took the wheel could not lift it");
    }

    /// A delta of zero is a scroll event a trackpad sends at the end of a gesture, and it must not
    /// be a click. It still counts as contact, which is what the gesture is.
    #[test]
    fn a_zero_delta_touches_the_wheel_and_turns_nothing() {
        let mut f = Finger::default();
        assert_eq!(f.scrolled(0.0), vec![Touch]);
        assert!(f.scrolled(0.0).is_empty());
        assert_eq!(f.scroll_lifted(), vec![Release]);
        // And a value that is not a number cannot become a click count.
        assert!(f.scrolled(f32::NAN).is_empty());
        assert!(f.scrolled(f32::INFINITY).is_empty());
        assert!(f.scroll_lifted().is_empty(), "a refused delta still opened a contact");
    }
}
