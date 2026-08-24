//! Pointer angle -> click-wheel position, and the five button zones.
//!
//! Everything in this file is arithmetic on a 96-position ring, kept out of `main.rs` so it can be
//! tested without a window. The one thing worth stating loudly is which parts are *derived* and
//! which are *chosen*, because the difference decides what a wrong answer here would mean.
//!
//! **Derived.** 96 clicks per rotation, and that clockwise motion increases the value. Rockbox's
//! `button-clickwheel.c` gives both ("Highest wheel = 0x5F, clockwise increases"), and
//! [`eapp_loader::WHEEL_CLICKS_PER_ROTATION`] is where this project already records it. The wrap is
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

use eapp_loader::{
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
/// The mask is the streaming frame's bit order relative to bit 8, straight from `eapp-loader`;
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
    /// `eapp_loader::wheel_button` is the one place those live, so the two cannot drift.
    #[allow(dead_code)]  // retired when: something in the window names a button in text — §16.8's keys map letters to variants directly and the drawn labels raise a pointer stream, so nothing in this program spells a button out loud yet
    pub fn parse(name: &str) -> Option<Button> {
        let mask = eapp_loader::wheel_button(name.trim())?;
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
#[derive(Default)]
pub struct Finger {
    touch: Touch,
}

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
}

impl Finger {
    /// A pointer went down at `(x, y)`, in the wheel's own coordinates — x right, **y down**, from
    /// its centre.
    ///
    /// Answers nothing for a press the wheel does not own: the centre button is a control of its
    /// own with its own route, and the moulding between it and the ring is *"a press on neither —
    /// which is the honest answer, since on the hardware it is the moulding"*.
    pub fn pressed(&mut self, ring: &WheelRing, x: f32, y: f32) -> Vec<eapp_loader::WheelEvent> {
        let (at, button) = match ring.hit(x, y) {
            Hit::Ring(p) => (p, 0),
            Hit::RingButton(b, p) => (p, b.mask()),
            Hit::Select | Hit::None => return Vec::new(),
        };
        let mut out = Vec::new();
        if self.touch == Touch::Off {
            out.push(eapp_loader::WheelEvent::Touch);
        }
        self.touch = Touch::Pointer { at, button };
        if button != 0 {
            out.push(eapp_loader::WheelEvent::Button(button, true));
        }
        out
    }

    /// The pointer moved to `(x, y)` — one `Step` per detent crossed, the short way round.
    ///
    /// **Only the angle is read, and the radius is deliberately ignored.** A drag that wanders
    /// outside the ring's outer edge while turning is one a person means; a finger that has to stay
    /// inside a 58 px annulus to keep scrolling is a wheel that stops working when you press
    /// slightly too hard. Where the finger went **on** is decided by [`WheelRing::hit`], which does
    /// read the radius; where it has got to since is an angle.
    pub fn moved(&mut self, x: f32, y: f32) -> Vec<eapp_loader::WheelEvent> {
        let Touch::Pointer { at, button } = self.touch else {
            return Vec::new();
        };
        let to = position_at_angle(x, y);
        let d = shortest_delta(at, to);
        if d == 0 {
            return Vec::new();
        }
        self.touch = Touch::Pointer { at: to, button };
        vec![eapp_loader::WheelEvent::Step(d.signum() as i8); d.unsigned_abs() as usize]
    }

    /// The pointer lifted. Whatever it was holding comes up, then the finger leaves.
    ///
    /// The order is the hardware's: a button that came up *after* the touch ended would post a
    /// frame with the button still set and no finger on the wheel, which is a state the part cannot
    /// be in.
    pub fn released(&mut self) -> Vec<eapp_loader::WheelEvent> {
        let Touch::Pointer { button, .. } = self.touch else {
            return Vec::new();
        };
        self.touch = Touch::Off;
        let mut out = Vec::new();
        if button != 0 {
            out.push(eapp_loader::WheelEvent::Button(button, false));
        }
        out.push(eapp_loader::WheelEvent::Release);
        out
    }

    /// §16.8's `↑` `↓` (and `←` `→` over a machine): **one detent, by key**.
    ///
    /// The first one touches the wheel and the key's release lifts it, so holding the key down is
    /// one contact with a stream of clicks in it — which is what a scroll is. Auto-repeat is the
    /// repeat rate, and it is the platform's rather than one this program invents.
    pub fn keyed(&mut self, by: i8) -> Vec<eapp_loader::WheelEvent> {
        if by == 0 || matches!(self.touch, Touch::Pointer { .. }) {
            return Vec::new();
        }
        let mut out = Vec::new();
        if self.touch == Touch::Off {
            out.push(eapp_loader::WheelEvent::Touch);
            self.touch = Touch::Key;
        }
        out.push(eapp_loader::WheelEvent::Step(by.signum()));
        out
    }

    /// The key came up. Nothing happens if the contact was a pointer's — see the type's note on
    /// there being one finger.
    pub fn key_released(&mut self) -> Vec<eapp_loader::WheelEvent> {
        if self.touch != Touch::Key {
            return Vec::new();
        }
        self.touch = Touch::Off;
        vec![eapp_loader::WheelEvent::Release]
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

    use eapp_loader::WheelEvent::{Button as Btn, Release, Step, Touch};

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
}
